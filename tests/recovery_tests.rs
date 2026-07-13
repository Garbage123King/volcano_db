use std::collections::HashMap;
use std::io::Write;
use volcano_db::catalog::Catalog;
use volcano_db::recovery::recover;
use volcano_db::storage::{Tuple, Value};
use volcano_db::tx::{RedoRecord, TransactionManager, TxStatus};

/// Helper: write given RedoRecords to a redo.log file as JSON lines
fn write_redo_log(path: &str, records: &[RedoRecord]) {
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::File::create(path).unwrap();
    for rec in records {
        let line = serde_json::to_string(rec).unwrap();
        writeln!(file, "{}", line).unwrap();
    }
    file.flush().unwrap();
}

// ============ No redo.log file ============

#[test]
fn test_recover_no_redo_file() {
    let path = "./test_recover_none.log";
    let _ = std::fs::remove_file(path);
    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    // Should return Ok with no-op
    let result = recover(&mut cat, &mut tables, &mut tx_mgr, path);
    assert!(result.is_ok());
    assert!(tables.is_empty());
}

// ============ Empty redo.log file ============

#[test]
fn test_recover_empty_file() {
    let path = "./test_recover_empty.log";
    let _ = std::fs::remove_file(path);
    std::fs::write(path, "").unwrap();
    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    let result = recover(&mut cat, &mut tables, &mut tx_mgr, path);
    assert!(result.is_ok());
    assert!(tables.is_empty());
}

// ============ File with only whitespace/blank lines ============

#[test]
fn test_recover_only_blank_lines() {
    let path = "./test_recover_blank.log";
    let _ = std::fs::remove_file(path);
    std::fs::write(path, "\n\n   \n\n").unwrap();
    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    let result = recover(&mut cat, &mut tables, &mut tx_mgr, path);
    assert!(result.is_ok());
    assert!(tables.is_empty());
}

// ============ Malformed line in redo.log ============

#[test]
fn test_recover_malformed_line_skipped() {
    let path = "./test_recover_malformed.log";
    let _ = std::fs::remove_file(path);
    // Write a valid Begin + a malformed line + a valid Commit
    let valid_begin = serde_json::to_string(&RedoRecord::Begin { tx_id: 1, scn: 0 }).unwrap();
    let valid_commit = serde_json::to_string(&RedoRecord::Commit { tx_id: 1, scn: 1 }).unwrap();
    let content = format!("{}\nthis is not json\n{}\n", valid_begin, valid_commit);
    std::fs::write(path, content).unwrap();

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    let result = recover(&mut cat, &mut tables, &mut tx_mgr, path);
    // Should succeed (malformed line skipped with warning)
    assert!(result.is_ok());
    // The Begin and Commit should still be processed
    assert_eq!(tx_mgr.statuses.get(&1), Some(&TxStatus::Committed));
}

// ============ Recovery with committed transaction ============

#[test]
fn test_recover_committed_transaction() {
    let path = "./test_recover_committed.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1), Value::Varchar("Alice".to_string())],
            row_idx: 0,
        },
        RedoRecord::Commit { tx_id: 1, scn: 1 },
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // Should have 1 row in users table (committed)
    assert_eq!(tables.len(), 1);
    let users = tables.get("users").unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].values[0], Value::Int(1));
    assert_eq!(users[0].values[1], Value::Varchar("Alice".to_string()));
    // SCN should be advanced past max
    assert!(tx_mgr.scn.get() > 1);
}

// ============ Recovery with uncommitted transaction (no Commit record) ============

#[test]
fn test_recover_uncommitted_transaction_removed() {
    let path = "./test_recover_uncommitted.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1), Value::Varchar("Alice".to_string())],
            row_idx: 0,
        },
        // No Commit - transaction was uncommitted at crash
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // Roll-Back: uncommitted rows should be removed
    let users = tables.get("users").unwrap();
    assert_eq!(users.len(), 0);
}

// ============ Recovery with rolled-back transaction ============

#[test]
fn test_recover_rolled_back_transaction_removed() {
    let path = "./test_recover_rb.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1), Value::Varchar("Alice".to_string())],
            row_idx: 0,
        },
        RedoRecord::Rollback { tx_id: 1, scn: 0 },
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // Rolled-back rows should be removed
    let users = tables.get("users").unwrap();
    assert_eq!(users.len(), 0);
    assert_eq!(tx_mgr.statuses.get(&1), Some(&TxStatus::RolledBack));
}

// ============ Recovery with mixed committed + uncommitted ============

#[test]
fn test_recover_mixed_transactions() {
    let path = "./test_recover_mixed.log";
    let records = vec![
        // Tx 1: committed
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1), Value::Varchar("Alice".to_string())],
            row_idx: 0,
        },
        RedoRecord::Commit { tx_id: 1, scn: 1 },
        // Tx 2: uncommitted
        RedoRecord::Begin { tx_id: 2, scn: 1 },
        RedoRecord::Insert {
            tx_id: 2,
            scn: 1,
            table_name: "users".to_string(),
            values: vec![Value::Int(2), Value::Varchar("Bob".to_string())],
            row_idx: 1,
        },
        // No Commit for tx 2
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // Only Alice should remain (Bob was uncommitted)
    let users = tables.get("users").unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].values[1], Value::Varchar("Alice".to_string()));
}

// ============ Recovery with multiple tables ============

#[test]
fn test_recover_multiple_tables() {
    let path = "./test_recover_multi_table.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1)],
            row_idx: 0,
        },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "orders".to_string(),
            values: vec![Value::Int(100)],
            row_idx: 0,
        },
        RedoRecord::Commit { tx_id: 1, scn: 1 },
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    assert_eq!(tables.len(), 2);
    assert_eq!(tables.get("users").unwrap().len(), 1);
    assert_eq!(tables.get("orders").unwrap().len(), 1);
}

// ============ Recovery with system data (tx_id = None) ============

#[test]
fn test_recover_preserves_system_data() {
    // Manually insert system data (tx_id=None) into tables before recovery
    // Recovery should preserve these rows (they are not from any transaction)
    let path = "./test_recover_sys.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 0,
            table_name: "users".to_string(),
            values: vec![Value::Int(1)],
            row_idx: 0,
        },
        RedoRecord::Commit { tx_id: 1, scn: 1 },
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    // Pre-existing system data (tx_id=None) should be preserved
    tables.insert(
        "system_data".to_string(),
        vec![Tuple::new(vec![Value::Int(99)])],
    );
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // System data should still be there
    assert_eq!(tables.get("system_data").unwrap().len(), 1);
    // Recovered users table should also be there
    assert_eq!(tables.get("users").unwrap().len(), 1);
}

// ============ Recovery advances SCN correctly ============

#[test]
fn test_recover_advances_scn() {
    let path = "./test_recover_scn.log";
    let records = vec![
        RedoRecord::Begin { tx_id: 1, scn: 0 },
        RedoRecord::Insert {
            tx_id: 1,
            scn: 5,
            table_name: "users".to_string(),
            values: vec![Value::Int(1)],
            row_idx: 0,
        },
        RedoRecord::Commit { tx_id: 1, scn: 10 },
    ];
    write_redo_log(path, &records);

    let mut cat = Catalog::newaaa();
    let mut tables = HashMap::new();
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    recover(&mut cat, &mut tables, &mut tx_mgr, path).unwrap();

    // SCN should be advanced to max_scn + 1 = 11
    assert_eq!(tx_mgr.scn.get(), 11);
    // commit_scns should be populated
    assert_eq!(tx_mgr.commit_scns.get(&1), Some(&10));
}
