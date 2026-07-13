use crate::catalog::Catalog;
use crate::handler::{handle_statement, setup_demo_data, DatabaseState};
use crate::protocol::{read_frame, write_frame};
use crate::recovery;
use crate::session::SessionState;
use crate::tx::TransactionManager;
use anyhow::Result;
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// 启动数据库 server: 先做 crash recovery (若 redo.log 存在), 再监听 TCP
pub fn run(addr: &str, trace: bool) -> Result<()> {
    let redo_path = "./redo.log";
    let has_redo = std::path::Path::new(redo_path).exists()
        && std::fs::metadata(redo_path)?.len() > 0;

    let mut db = DatabaseState {
        catalog: Catalog::newaaa(),
        tables: HashMap::new(),
        tx_mgr: TransactionManager::new(redo_path, trace)?,
    };

    if has_redo {
        println!("[BOOT] 检测到 redo.log, 执行 Instance Recovery...");
        recovery::recover(&mut db.catalog, &mut db.tables, &mut db.tx_mgr, redo_path)?;
        // recovery 不恢复 catalog schema, 需加载 demo 数据补 schema (不覆盖已恢复的数据)
        setup_demo_data(&mut db)?;
    } else {
        println!("[BOOT] 无 redo.log, 加载 demo 数据...");
        setup_demo_data(&mut db)?;
    }

    let db = Arc::new(Mutex::new(db));
    let listener = TcpListener::bind(addr)?;
    println!("[SERVER] 监听 {} (等待 client 连接)", addr);
    println!("[SERVER] 可用命令: /crash  /status");

    for stream in listener.incoming() {
        let mut stream = stream?;
        let db = Arc::clone(&db);
        thread::spawn(move || {
            let mut session = SessionState::new();
            loop {
                let sql = match read_frame(&mut stream) {
                    Ok(s) => s,
                    Err(_) => break,
                };
                // 锁粒度 = 单条语句: 执行完即释放, 事务期间不持锁
                let resp = {
                    let mut db = db.lock().unwrap();
                    handle_statement(&mut db, &mut session, &sql)
                };
                if write_frame(&mut stream, &resp).is_err() {
                    break;
                }
            }
        });
    }
    Ok(())
}
