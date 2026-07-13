use crate::catalog::Catalog;
use crate::storage::Tuple;
use crate::tx::{RedoRecord, TransactionManager, TxStatus};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

/// 启动时 Instance Recovery:
/// 1. Roll-Forward: 重放所有 redo 记录, 重建内存表 + 事务状态
/// 2. Roll-Back: 删除未提交 (或已回滚) 事务写入的元组
pub fn recover(
    catalog: &mut Catalog,
    tables: &mut HashMap<String, Vec<Tuple>>,
    tx_mgr: &mut TransactionManager,
    redo_path: &str,
) -> Result<()> {
    let path = redo_path;
    if !std::path::Path::new(path).exists() {
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records: Vec<RedoRecord> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RedoRecord>(&line) {
            Ok(rec) => records.push(rec),
            Err(e) => {
                println!("[RECOVERY] 警告: 跳过无法解析的行: {} ({})", line, e);
            }
        }
    }

    if records.is_empty() {
        return Ok(());
    }
    println!("[RECOVERY] 读到 {} 条 redo 记录, 开始 Instance Recovery", records.len());

    // 第一遍: Roll-Forward
    let mut committed: HashSet<u64> = HashSet::new();
    let mut rolled_back: HashSet<u64> = HashSet::new();
    let mut max_scn: u64 = 0;

    for rec in &records {
        match rec {
            RedoRecord::Begin { tx_id, scn } => {
                tx_mgr.statuses.insert(*tx_id, TxStatus::Active);
                max_scn = max_scn.max(*scn);
            }
            RedoRecord::Insert { tx_id, scn, table_name, values, row_idx: _ } => {
                // recovery 时直接追加, 不按 row_idx 定位 (demo 数据不在 redo 中, row_idx 无意义)
                let table = tables.entry(table_name.clone()).or_insert_with(Vec::new);
                table.push(Tuple::new_with_meta(values.clone(), Some(*tx_id), *scn));
                max_scn = max_scn.max(*scn);
            }
            RedoRecord::Commit { tx_id, scn } => {
                committed.insert(*tx_id);
                tx_mgr.statuses.insert(*tx_id, TxStatus::Committed);
                tx_mgr.commit_scns.insert(*tx_id, *scn);
                max_scn = max_scn.max(*scn);
            }
            RedoRecord::Rollback { tx_id, scn } => {
                rolled_back.insert(*tx_id);
                tx_mgr.statuses.insert(*tx_id, TxStatus::RolledBack);
                max_scn = max_scn.max(*scn);
            }
        }
    }

    // 推进 SCN 到最大值之后
    tx_mgr.scn.0.store(max_scn + 1, std::sync::atomic::Ordering::SeqCst);

    // 第二遍: Roll-Back, 删除未提交事务的元组
    let mut removed = 0usize;
    let mut kept = 0usize;
    for table in tables.values_mut() {
        let original = table.len();
        table.retain(|t| match t.tx_id {
            None => true,
            Some(row_tx) => {
                let keep = committed.contains(&row_tx) && !rolled_back.contains(&row_tx);
                if keep {
                    kept += 1;
                } else {
                    removed += 1;
                }
                keep
            }
        });
        let _ = original;
    }

    // 恢复 catalog: 从恢复出的表名建 schema (使用宽松 schema, 因为 redo 不含列定义)
    // 注意: 真正的 schema 应从单独的 catalog 文件恢复; 这里 demo 数据会跳过 recovery
    for table_name in tables.keys() {
        if catalog.get_schema(table_name).is_none() {
            // 无法从 redo 恢复 schema, 跳过 (demo 场景下 recovery 与 setup_demo_data 互斥)
            println!("[RECOVERY] 警告: 表 '{}' 的 schema 未知 (catalog 未持久化)", table_name);
        }
    }

    println!(
        "[RECOVERY] 完成: Roll-Forward 重放 {} 条, Roll-Back 移除 {} 条未提交元组, 保留 {} 条, 当前 SCN={}",
        records.len(),
        removed,
        kept,
        tx_mgr.scn.get()
    );
    Ok(())
}
