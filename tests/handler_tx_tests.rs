use std::collections::HashMap;
use volcano_db::catalog::Catalog;
use volcano_db::handler::{handle_statement, setup_demo_data, DatabaseState};
use volcano_db::session::SessionState;
use volcano_db::tx::{TransactionManager, TxStatus};

fn make_db() -> DatabaseState {
    let _ = std::fs::remove_file("./test_handler_redo.log");
    let mut db = DatabaseState {
        catalog: Catalog::newaaa(),
        tables: HashMap::new(),
        tx_mgr: TransactionManager::new("./test_handler_redo.log", false).unwrap(),
    };
    setup_demo_data(&mut db).unwrap();
    db
}

fn row_count(resp: &str) -> usize {
    resp.lines()
        .last()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0)
}

// ============ /status command ============

#[test]
fn test_status_command_no_active_tx() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "/status");
    assert!(resp.contains("session="));
    assert!(resp.contains("tx_id=None"));
    assert!(resp.contains("cr_scn=0"));
    assert!(resp.contains("global_scn=0"));
    assert!(resp.contains("active_txs=[]"));
}

#[test]
fn test_status_command_with_active_tx() {
    let mut db = make_db();
    let mut session = SessionState::new();
    handle_statement(&mut db, &mut session, "BEGIN");
    let resp = handle_statement(&mut db, &mut session, "/status");
    assert!(resp.contains("tx_id=Some(1)"));
    // active_txs should list tx 1
    assert!(resp.contains("active_txs=[1]"));
}

#[test]
fn test_status_command_after_commit() {
    let mut db = make_db();
    let mut session = SessionState::new();
    handle_statement(&mut db, &mut session, "BEGIN");
    handle_statement(&mut db, &mut session, "COMMIT");
    let resp = handle_statement(&mut db, &mut session, "/status");
    assert!(resp.contains("tx_id=None"));
    // No active txs after commit
    assert!(resp.contains("active_txs=[]"));
    // SCN should have advanced
    assert!(resp.contains("global_scn=1"));
}

// ============ Transaction control error paths ============

#[test]
fn test_begin_with_active_tx_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    handle_statement(&mut db, &mut session, "BEGIN");
    let resp = handle_statement(&mut db, &mut session, "BEGIN");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("已有活跃事务"));
}

#[test]
fn test_commit_without_tx_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "COMMIT");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("没有活跃事务"));
}

#[test]
fn test_rollback_without_tx_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "ROLLBACK");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("没有活跃事务"));
}

#[test]
fn test_start_transaction_keyword() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "START TRANSACTION");
    assert!(resp.contains("BEGIN"));
    assert!(resp.contains("tx_id="));
}

// ============ CREATE TABLE errors ============

#[test]
fn test_create_duplicate_table_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "CREATE TABLE users (id INT)");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("already exists"));
}

#[test]
fn test_create_table_success() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "CREATE TABLE new_table (id INT, name VARCHAR)");
    assert!(resp.contains("created successfully"));
    assert!(db.catalog.get_schema("new_table").is_some());
    assert!(db.tables.contains_key("new_table"));
}

// ============ INSERT errors ============

#[test]
fn test_insert_into_nonexistent_table_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "INSERT INTO nonexistent VALUES (1)");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("not found"));
}

#[test]
fn test_insert_column_count_mismatch_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    // users has 4 columns, but we provide 2
    let resp = handle_statement(&mut db, &mut session, "INSERT INTO users VALUES (1, 'x')");
    assert!(resp.starts_with("Error:"));
    assert!(resp.contains("column count mismatch"));
}

#[test]
fn test_insert_success() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(
        &mut db,
        &mut session,
        "INSERT INTO users VALUES (6, 'Frank', 40, 85.0)",
    );
    assert!(resp.contains("Inserted 1 row(s)"));
}

#[test]
fn test_insert_multiple_rows() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(
        &mut db,
        &mut session,
        "INSERT INTO users VALUES (6, 'Frank', 40, 85.0), (7, 'Grace', 28, 90.0)",
    );
    assert!(resp.contains("Inserted 2 row(s)"));
}

// ============ SQL parse error ============

#[test]
fn test_sql_parse_error() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "NOT VALID SQL !!!");
    assert!(resp.starts_with("Error:"));
}

#[test]
fn test_unsupported_sql_statement() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "DROP TABLE users");
    assert!(resp.starts_with("Error:"));
}

// ============ SELECT with table not in catalog ============

#[test]
fn test_select_from_nonexistent_table_errors() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(&mut db, &mut session, "SELECT * FROM nonexistent");
    assert!(resp.starts_with("Error:"));
}

// ============ ROLLBACK removes rows correctly ============

#[test]
fn test_rollback_multiple_inserts() {
    let mut db = make_db();
    let mut session = SessionState::new();

    handle_statement(&mut db, &mut session, "BEGIN");
    handle_statement(&mut db, &mut session, "INSERT INTO users VALUES (6, 'Frank', 40, 85.0)");
    handle_statement(&mut db, &mut session, "INSERT INTO users VALUES (7, 'Grace', 28, 90.0)");
    handle_statement(&mut db, &mut session, "INSERT INTO users VALUES (8, 'Henry', 35, 77.0)");

    let resp = handle_statement(&mut db, &mut session, "SELECT name FROM users");
    assert_eq!(row_count(&resp), 8); // 5 + 3

    handle_statement(&mut db, &mut session, "ROLLBACK");

    let resp = handle_statement(&mut db, &mut session, "SELECT name FROM users");
    assert_eq!(row_count(&resp), 5);
}

// ============ ROLLBACK when undo row_idx out of bounds ============

#[test]
fn test_rollback_with_stale_row_idx() {
    let mut db = make_db();
    let mut session = SessionState::new();

    handle_statement(&mut db, &mut session, "BEGIN");
    handle_statement(&mut db, &mut session, "INSERT INTO users VALUES (6, 'Frank', 40, 85.0)");
    // Manually truncate the table to make row_idx stale
    db.tables.get_mut("users").unwrap().clear();
    // ROLLBACK should not panic even though row_idx is out of bounds
    let resp = handle_statement(&mut db, &mut session, "ROLLBACK");
    assert!(resp.contains("ROLLBACK"));
}

// ============ Empty result query ============

#[test]
fn test_select_empty_result() {
    let mut db = make_db();
    let mut session = SessionState::new();
    let resp = handle_statement(
        &mut db,
        &mut session,
        "SELECT * FROM users WHERE age > 1000",
    );
    assert!(resp.contains("0 row(s) in set"));
    // Should not include table header
    assert!(!resp.contains("+----"));
}

// ============ TransactionManager is_visible direct tests ============

#[test]
fn test_is_visible_no_tx_id() {
    let tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    // None tx_id = system data, always visible
    assert!(tx_mgr.is_visible(None, None, 0));
    assert!(tx_mgr.is_visible(None, Some(1), 100));
}

#[test]
fn test_is_visible_own_tx() {
    let tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    // Row from tx 5, session is tx 5
    assert!(tx_mgr.is_visible(Some(5), Some(5), 0));
}

#[test]
fn test_is_visible_committed_before_cr_scn() {
    let mut tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    tx_mgr.statuses.insert(5, TxStatus::Committed);
    tx_mgr.commit_scns.insert(5, 50);
    // Row committed at scn=50, query cr_scn=100
    assert!(tx_mgr.is_visible(Some(5), None, 100));
}

#[test]
fn test_is_visible_committed_after_cr_scn() {
    let mut tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    tx_mgr.statuses.insert(5, TxStatus::Committed);
    tx_mgr.commit_scns.insert(5, 200);
    // Row committed at scn=200, query cr_scn=100
    assert!(!tx_mgr.is_visible(Some(5), None, 100));
}

#[test]
fn test_is_visible_committed_no_commit_scn() {
    let mut tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    tx_mgr.statuses.insert(5, TxStatus::Committed);
    // No commit_scn entry - should default to u64::MAX, so invisible when cr_scn < MAX
    assert!(!tx_mgr.is_visible(Some(5), None, 100));
}

#[test]
fn test_is_visible_active_tx() {
    let mut tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    tx_mgr.statuses.insert(5, TxStatus::Active);
    // Active tx (not committed), not own session
    assert!(!tx_mgr.is_visible(Some(5), None, 100));
    assert!(!tx_mgr.is_visible(Some(5), Some(6), 100));
}

#[test]
fn test_is_visible_rolled_back_tx() {
    let mut tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    tx_mgr.statuses.insert(5, TxStatus::RolledBack);
    // Rolled-back tx
    assert!(!tx_mgr.is_visible(Some(5), None, 100));
}

#[test]
fn test_is_visible_unknown_tx() {
    let tx_mgr = TransactionManager::new("./test_tx_vis.log", false).unwrap();
    // tx_id not in statuses map
    assert!(!tx_mgr.is_visible(Some(999), None, 100));
}

// ============ TransactionManager begin/commit/rollback ============

#[test]
fn test_tx_begin_assigns_unique_ids() {
    let mut tx_mgr = TransactionManager::new("./test_tx_ids.log", false).unwrap();
    let id1 = tx_mgr.begin();
    let id2 = tx_mgr.begin();
    let id3 = tx_mgr.begin();
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
    assert_eq!(tx_mgr.statuses.len(), 3);
    assert_eq!(tx_mgr.undo_segments.len(), 3);
}

#[test]
fn test_tx_commit_advances_scn() {
    let mut tx_mgr = TransactionManager::new("./test_tx_commit.log", false).unwrap();
    let initial_scn = tx_mgr.scn.get();
    let tx_id = tx_mgr.begin();
    tx_mgr.commit(tx_id);
    assert_eq!(tx_mgr.scn.get(), initial_scn + 1);
    assert_eq!(tx_mgr.statuses.get(&tx_id), Some(&TxStatus::Committed));
    assert_eq!(tx_mgr.commit_scns.get(&tx_id), Some(&(initial_scn + 1)));
}

#[test]
fn test_tx_rollback_advances_status() {
    let mut tx_mgr = TransactionManager::new("./test_tx_rb.log", false).unwrap();
    let tx_id = tx_mgr.begin();
    tx_mgr.rollback(tx_id);
    assert_eq!(tx_mgr.statuses.get(&tx_id), Some(&TxStatus::RolledBack));
    // SCN does NOT advance on rollback
    assert_eq!(tx_mgr.scn.get(), 0);
}

#[test]
fn test_tx_record_insert() {
    let mut tx_mgr = TransactionManager::new("./test_tx_rec.log", false).unwrap();
    let tx_id = tx_mgr.begin();
    tx_mgr.record_insert(
        tx_id,
        "users",
        vec![volcano_db::storage::Value::Int(1)],
        0,
    );
    // Should have undo record
    assert_eq!(tx_mgr.undo_segments.get(&tx_id).unwrap().len(), 1);
    // Should have redo record in buffer
    assert_eq!(tx_mgr.redo_buffer.len(), 2); // Begin + Insert
}

#[test]
fn test_tx_lgwr_flush_on_commit() {
    let path = "./test_tx_lgwr.log";
    let _ = std::fs::remove_file(path);
    let mut tx_mgr = TransactionManager::new(path, false).unwrap();
    let tx_id = tx_mgr.begin();
    tx_mgr.record_insert(
        tx_id,
        "users",
        vec![volcano_db::storage::Value::Int(1)],
        0,
    );
    // Buffer has Begin + Insert
    assert_eq!(tx_mgr.redo_buffer.len(), 2);
    tx_mgr.commit(tx_id);
    // Buffer should be cleared after flush
    assert_eq!(tx_mgr.redo_buffer.len(), 0);
    // File should have content
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("begin"));
    assert!(content.contains("insert"));
    assert!(content.contains("commit"));
}

#[test]
fn test_tx_trace_mode() {
    // Just verify trace=true doesn't crash
    let mut tx_mgr = TransactionManager::new("./test_tx_trace.log", true).unwrap();
    let tx_id = tx_mgr.begin();
    tx_mgr.commit(tx_id);
    // No assertion needed, just verify no panic
}
