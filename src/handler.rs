use crate::catalog::{Catalog, Schema};
use crate::executor::{build_executor, ExecutionContext};
use crate::planner::{bind_statement, SQLStatement};
use crate::session::SessionState;
use crate::storage::Tuple;
use crate::tx::{TransactionManager, TxStatus, UndoRecord};
use anyhow::{anyhow, Context, Result};
use comfy_table::Table;
use std::collections::HashMap;

/// 全局共享数据库状态 (放在 Arc<Mutex<DatabaseState>>)
pub struct DatabaseState {
    pub catalog: Catalog,
    pub tables: HashMap<String, Vec<Tuple>>,
    pub tx_mgr: TransactionManager,
}

/// 处理单条 SQL 或特殊命令, 返回响应文本
pub fn handle_statement(db: &mut DatabaseState, session: &mut SessionState, sql: &str) -> String {
    let trimmed = sql.trim();

    // 特殊 REPL 命令
    if trimmed.eq_ignore_ascii_case("/crash") {
        println!("[CRASH] 模拟崩溃, 进程立即退出 (redo buffer 未刷盘部分丢失)");
        std::process::exit(1);
    }
    if trimmed.eq_ignore_ascii_case("/status") {
        let active: Vec<u64> = db
            .tx_mgr
            .statuses
            .iter()
            .filter(|(_, s)| **s == TxStatus::Active)
            .map(|(k, _)| *k)
            .collect();
        return format!(
            "session={} tx_id={:?} cr_scn={} global_scn={} active_txs={:?}",
            session.session_id,
            session.current_tx_id,
            session.cr_scn,
            db.tx_mgr.scn.get(),
            active
        );
    }

    match inner_handle(db, session, sql) {
        Ok(msg) => msg,
        Err(e) => format!("Error: {}", e),
    }
}

fn inner_handle(db: &mut DatabaseState, session: &mut SessionState, sql: &str) -> Result<String> {
    let upper = sql.trim().to_uppercase();

    // 事务控制语句: 直接匹配关键字
    if upper.starts_with("BEGIN") || upper.starts_with("START TRANSACTION") {
        if session.current_tx_id.is_some() {
            return Err(anyhow!("已有活跃事务, 请先 COMMIT 或 ROLLBACK"));
        }
        let tx_id = db.tx_mgr.begin();
        session.current_tx_id = Some(tx_id);
        return Ok(format!("BEGIN (tx_id={})", tx_id));
    }
    if upper == "COMMIT" {
        let tx_id = session.current_tx_id.ok_or_else(|| anyhow!("没有活跃事务"))?;
        db.tx_mgr.commit(tx_id);
        session.current_tx_id = None;
        return Ok(format!("COMMIT (tx_id={})", tx_id));
    }
    if upper == "ROLLBACK" {
        let tx_id = session.current_tx_id.ok_or_else(|| anyhow!("没有活跃事务"))?;
        // 执行 Undo 回滚: 逆序移除插入的行
        let undo_records = db.tx_mgr.undo_segments.remove(&tx_id).unwrap_or_default();
        for rec in undo_records.iter().rev() {
            let UndoRecord::Insert { table_name, row_idx } = rec;
            if let Some(table) = db.tables.get_mut(table_name) {
                if *row_idx < table.len() {
                    table.remove(*row_idx);
                }
            }
        }
        db.tx_mgr.rollback(tx_id);
        session.current_tx_id = None;
        return Ok(format!("ROLLBACK (tx_id={})", tx_id));
    }

    // 普通 SQL: 走 planner
    let statement = bind_statement(sql, &db.catalog)?;
    match statement {
        SQLStatement::CreateTable { table_name, schema } => {
            let name_lower = table_name.to_lowercase();
            if db.catalog.get_schema(&name_lower).is_some() {
                return Err(anyhow!("Table '{}' already exists", table_name));
            }
            db.catalog.add_table(name_lower.clone(), schema);
            // 用 entry().or_insert_with() 避免覆盖 recovery 重建的数据
            db.tables.entry(name_lower).or_insert_with(Vec::new);
            Ok(format!("Table '{}' created successfully.", table_name))
        }
        SQLStatement::Insert { table_name, rows } => {
            let name_lower = table_name.to_lowercase();
            let schema = db
                .catalog
                .get_schema(&name_lower)
                .ok_or_else(|| anyhow!("Table '{}' not found", table_name))?
                .clone();
            let tx_id = session.current_tx_id;
            let scn = db.tx_mgr.scn.get();
            let mut inserted_count = 0;

            let target_table = db
                .tables
                .get_mut(&name_lower)
                .ok_or_else(|| anyhow!("Table storage not initialized for '{}'", table_name))?;

            for row in rows {
                if row.len() != schema.columns.len() {
                    return Err(anyhow!(
                        "Insert column count mismatch: table has {}, query provided {}",
                        schema.columns.len(),
                        row.len()
                    ));
                }
                let dummy_tuple = Tuple::new(vec![]);
                let dummy_schema = Schema::new(vec![]);
                let mut vals = Vec::new();
                for (i, expr) in row.iter().enumerate() {
                    let val = expr
                        .eval(&dummy_tuple, &dummy_schema)
                        .context(format!("Failed to evaluate insert value at position {}", i))?;
                    vals.push(val);
                }
                let row_idx = target_table.len();
                if let Some(tid) = tx_id {
                    db.tx_mgr.record_insert(tid, &name_lower, vals.clone(), row_idx);
                }
                target_table.push(Tuple::new_with_meta(vals, tx_id, scn));
                inserted_count += 1;
            }
            Ok(format!("Inserted {} row(s).", inserted_count))
        }
        SQLStatement::Query(logical_plan) => {
            // 查询开始时捕获 CR SCN (Consistent Read)
            session.cr_scn = db.tx_mgr.scn.get();
            let mut physical_plan = build_executor(&logical_plan, &db.catalog)?;
            let ctx = ExecutionContext {
                tables: &db.tables,
                tx_mgr: &db.tx_mgr,
                session_tx_id: session.current_tx_id,
                cr_scn: session.cr_scn,
            };
            physical_plan.init(&ctx)?;
            let query_schema = logical_plan.schema(&db.catalog)?;
            let mut table = Table::new();
            let headers: Vec<String> = query_schema.columns.iter().map(|c| c.name.clone()).collect();
            table.set_header(headers);
            let mut row_count = 0;
            while let Some(tuple) = physical_plan.next(&ctx)? {
                let row_vals: Vec<String> = tuple.values.iter().map(|v| format!("{}", v)).collect();
                table.add_row(row_vals);
                row_count += 1;
            }
            physical_plan.close(&ctx)?;
            let mut out = String::new();
            if row_count > 0 {
                out.push_str(&format!("{}\n", table));
            }
            out.push_str(&format!("{} row(s) in set.", row_count));
            Ok(out)
        }
    }
}

/// 启动时加载 demo 数据 (仅在无 redo.log 时调用)
pub fn setup_demo_data(db: &mut DatabaseState) -> Result<()> {
    let demo_sqls = [
        "CREATE TABLE users (id INT, name VARCHAR, age INT, score FLOAT)",
        "INSERT INTO users VALUES (1, 'Alice', 25, 95.5), (2, 'Bob', 30, 88.0), (3, 'Charlie', 22, 92.0), (4, 'David', 30, 75.5), (5, 'Eva', 25, 99.0)",
        "CREATE TABLE categories (id INT, category_name VARCHAR)",
        "INSERT INTO categories VALUES (10, 'Electronics'), (20, 'Books'), (30, 'Clothing')",
        "CREATE TABLE products (id INT, product_name VARCHAR, category_id INT)",
        "INSERT INTO products VALUES (501, 'Laptop', 10), (502, 'Phone', 10), (503, 'Rust Programming Book', 20), (504, 'T-Shirt', 30)",
        "CREATE TABLE orders (id INT, user_id INT, product_id INT, amount FLOAT)",
        "INSERT INTO orders VALUES (101, 1, 501, 1500.0), (102, 2, 503, 88.0), (103, 1, 504, 45.0), (104, 3, 502, 999.0), (105, 9, 501, 999.0)",
    ];
    let mut tmp_session = SessionState::new();
    for sql in &demo_sqls {
        inner_handle(db, &mut tmp_session, sql)?;
    }
    println!("[BOOT] Demo data loaded.");
    println!("[BOOT] Tables: users, categories, products, orders");
    Ok(())
}
