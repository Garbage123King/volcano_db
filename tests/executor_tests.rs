use std::collections::HashMap;
use volcano_db::catalog::{Catalog, Column, DataType, Schema};
use volcano_db::executor::{
    build_executor, AggState, DummyScanExecutor, ExecutionContext, Executor, SeqScanExecutor,
};
use volcano_db::planner::{Expr, LogicalPlan};
use volcano_db::storage::{Tuple, Value};
use volcano_db::tx::TransactionManager;

fn make_tx_mgr() -> TransactionManager {
    TransactionManager::new("./test_exec_redo.log", false).unwrap()
}

fn make_catalog_with_users() -> (Catalog, HashMap<String, Vec<Tuple>>) {
    let mut cat = Catalog::newaaa();
    cat.add_table(
        "users".to_string(),
        Schema::new(vec![
            Column { name: "id".to_string(), data_type: DataType::Int },
            Column { name: "name".to_string(), data_type: DataType::Varchar },
            Column { name: "age".to_string(), data_type: DataType::Int },
            Column { name: "score".to_string(), data_type: DataType::Float },
        ]),
    );
    let mut tables = HashMap::new();
    tables.insert(
        "users".to_string(),
        vec![
            Tuple::new(vec![
                Value::Int(1),
                Value::Varchar("Alice".to_string()),
                Value::Int(25),
                Value::Float(95.5),
            ]),
            Tuple::new(vec![
                Value::Int(2),
                Value::Varchar("Bob".to_string()),
                Value::Int(30),
                Value::Float(88.0),
            ]),
            Tuple::new(vec![
                Value::Int(3),
                Value::Varchar("Charlie".to_string()),
                Value::Int(22),
                Value::Float(92.0),
            ]),
        ],
    );
    (cat, tables)
}

fn exec_collect(
    plan: &LogicalPlan,
    catalog: &Catalog,
    tables: &HashMap<String, Vec<Tuple>>,
    tx_mgr: &TransactionManager,
) -> Vec<Tuple> {
    let mut exec = build_executor(plan, catalog).unwrap();
    let ctx = ExecutionContext {
        tables,
        tx_mgr,
        session_tx_id: None,
        cr_scn: tx_mgr.scn.get(),
    };
    exec.init(&ctx).unwrap();
    let mut results = Vec::new();
    while let Some(t) = exec.next(&ctx).unwrap() {
        results.push(t);
    }
    exec.close(&ctx).unwrap();
    results
}

// ============ AggState::Count ============

#[test]
fn test_agg_count() {
    let mut state = AggState::new("count");
    state.update(&Value::Int(1));
    state.update(&Value::Int(2));
    state.update(&Value::Null); // Null doesn't count
    state.update(&Value::Varchar("x".to_string()));
    assert_eq!(state.finalize(), Value::Int(3));
}

#[test]
fn test_agg_count_empty() {
    let state = AggState::new("count");
    assert_eq!(state.finalize(), Value::Int(0));
}

#[test]
fn test_agg_count_only_null() {
    let mut state = AggState::new("count");
    state.update(&Value::Null);
    state.update(&Value::Null);
    assert_eq!(state.finalize(), Value::Int(0));
}

// ============ AggState::Sum ============

#[test]
fn test_agg_sum_int() {
    let mut state = AggState::new("sum");
    state.update(&Value::Int(1));
    state.update(&Value::Int(2));
    state.update(&Value::Int(3));
    assert_eq!(state.finalize(), Value::Int(6));
}

#[test]
fn test_agg_sum_float() {
    let mut state = AggState::new("sum");
    state.update(&Value::Float(1.5));
    state.update(&Value::Float(2.5));
    assert_eq!(state.finalize(), Value::Float(4.0));
}

#[test]
fn test_agg_sum_int_float_mix() {
    let mut state = AggState::new("sum");
    state.update(&Value::Int(1)); // sum = Int(1)
    state.update(&Value::Float(2.5)); // sum = Int(1) + Float(2.5) = Float(3.5)
    assert_eq!(state.finalize(), Value::Float(3.5));

    let mut state = AggState::new("sum");
    state.update(&Value::Float(2.5)); // sum = Float(2.5)
    state.update(&Value::Int(1)); // sum = Float(2.5) + Int(1) = Float(3.5)
    assert_eq!(state.finalize(), Value::Float(3.5));
}

#[test]
fn test_agg_sum_with_null() {
    let mut state = AggState::new("sum");
    state.update(&Value::Null); // No effect
    state.update(&Value::Int(5));
    state.update(&Value::Null); // No effect
    assert_eq!(state.finalize(), Value::Int(5));
}

#[test]
fn test_agg_sum_all_null() {
    let mut state = AggState::new("sum");
    state.update(&Value::Null);
    state.update(&Value::Null);
    assert_eq!(state.finalize(), Value::Null);
}

#[test]
fn test_agg_sum_empty() {
    let state = AggState::new("sum");
    assert_eq!(state.finalize(), Value::Null);
}

// ============ AggState::Avg ============

#[test]
fn test_agg_avg_int() {
    let mut state = AggState::new("avg");
    state.update(&Value::Int(10));
    state.update(&Value::Int(20));
    state.update(&Value::Int(30));
    // avg = 60 / 3 = 20.0
    assert_eq!(state.finalize(), Value::Float(20.0));
}

#[test]
fn test_agg_avg_float() {
    let mut state = AggState::new("avg");
    state.update(&Value::Float(10.0));
    state.update(&Value::Float(20.0));
    // avg = 30.0 / 2 = 15.0
    assert_eq!(state.finalize(), Value::Float(15.0));
}

#[test]
fn test_agg_avg_int_float_mix() {
    let mut state = AggState::new("avg");
    state.update(&Value::Int(10)); // sum = Int(10), count = 1
    state.update(&Value::Float(20.0)); // sum = Float(30.0), count = 2
    assert_eq!(state.finalize(), Value::Float(15.0));
}

#[test]
fn test_agg_avg_with_null() {
    let mut state = AggState::new("avg");
    state.update(&Value::Null); // No effect
    state.update(&Value::Int(10));
    state.update(&Value::Null); // No effect
    // avg = 10 / 1 = 10.0
    assert_eq!(state.finalize(), Value::Float(10.0));
}

#[test]
fn test_agg_avg_empty() {
    let state = AggState::new("avg");
    // count == 0, returns Null
    assert_eq!(state.finalize(), Value::Null);
}

#[test]
fn test_agg_avg_all_null() {
    let mut state = AggState::new("avg");
    state.update(&Value::Null);
    state.update(&Value::Null);
    assert_eq!(state.finalize(), Value::Null);
}

// ============ AggState::Min ============

#[test]
fn test_agg_min_int() {
    let mut state = AggState::new("min");
    state.update(&Value::Int(5));
    state.update(&Value::Int(2));
    state.update(&Value::Int(8));
    assert_eq!(state.finalize(), Value::Int(2));
}

#[test]
fn test_agg_min_float() {
    let mut state = AggState::new("min");
    state.update(&Value::Float(5.5));
    state.update(&Value::Float(2.2));
    state.update(&Value::Float(8.8));
    assert_eq!(state.finalize(), Value::Float(2.2));
}

#[test]
fn test_agg_min_varchar() {
    let mut state = AggState::new("min");
    state.update(&Value::Varchar("banana".to_string()));
    state.update(&Value::Varchar("apple".to_string()));
    state.update(&Value::Varchar("cherry".to_string()));
    assert_eq!(state.finalize(), Value::Varchar("apple".to_string()));
}

#[test]
fn test_agg_min_with_null() {
    let mut state = AggState::new("min");
    state.update(&Value::Null); // Sets min to Null
    state.update(&Value::Int(5)); // 5 < Null? No (Null compares as None, min stays Null unless replaced)
    // Actually in update(): if min is Null, replace. If val < min... but val < Null is false
    // Looking at code: match min { Null => *min = val.clone(), _ => if val < min { *min = val.clone() } }
    // So first Null: min = Null (no, actually min starts as Null, so first non-null sets it)
    // Wait, min starts as Value::Null. First update: val is Null, val != Null is false, so no update
    // Second update: val is Int(5), min is still Null, so Null branch sets min = Int(5)
    assert_eq!(state.finalize(), Value::Int(5));
}

#[test]
fn test_agg_min_empty() {
    let state = AggState::new("min");
    assert_eq!(state.finalize(), Value::Null);
}

// ============ AggState::Max ============

#[test]
fn test_agg_max_int() {
    let mut state = AggState::new("max");
    state.update(&Value::Int(5));
    state.update(&Value::Int(2));
    state.update(&Value::Int(8));
    assert_eq!(state.finalize(), Value::Int(8));
}

#[test]
fn test_agg_max_float() {
    let mut state = AggState::new("max");
    state.update(&Value::Float(5.5));
    state.update(&Value::Float(2.2));
    state.update(&Value::Float(8.8));
    assert_eq!(state.finalize(), Value::Float(8.8));
}

#[test]
fn test_agg_max_varchar() {
    let mut state = AggState::new("max");
    state.update(&Value::Varchar("apple".to_string()));
    state.update(&Value::Varchar("banana".to_string()));
    state.update(&Value::Varchar("cherry".to_string()));
    assert_eq!(state.finalize(), Value::Varchar("cherry".to_string()));
}

#[test]
fn test_agg_max_with_null() {
    let mut state = AggState::new("max");
    state.update(&Value::Null); // No effect (val != Null is false)
    state.update(&Value::Int(5));
    state.update(&Value::Null); // No effect
    assert_eq!(state.finalize(), Value::Int(5));
}

#[test]
fn test_agg_max_empty() {
    let state = AggState::new("max");
    assert_eq!(state.finalize(), Value::Null);
}

// ============ AggState::new panic on unknown ============

#[test]
#[should_panic(expected = "Unknown aggregate function")]
fn test_agg_state_new_unknown_func() {
    let _ = AggState::new("stddev");
}

// ============ AggState::update with non-numeric for sum/avg ============

#[test]
fn test_agg_sum_with_non_numeric_no_effect() {
    let mut state = AggState::new("sum");
    state.update(&Value::Int(5));
    // Non-numeric values fall through to _ => {} in match
    state.update(&Value::Varchar("x".to_string()));
    state.update(&Value::Bool(true));
    assert_eq!(state.finalize(), Value::Int(5));
}

#[test]
fn test_agg_avg_with_non_numeric_no_effect() {
    let mut state = AggState::new("avg");
    state.update(&Value::Int(10));
    // Non-numeric values don't affect sum
    state.update(&Value::Varchar("x".to_string()));
    // But count still increments? Let me check the code:
    // In Avg::update: if val != Null { *count += 1; match sum { ... } }
    // So count increments for non-null non-numeric, but sum doesn't change
    // avg = 10 / 2 = 5.0
    assert_eq!(state.finalize(), Value::Float(5.0));
}

// ============ DummyScanExecutor ============

#[test]
fn test_dummy_scan_executor() {
    let mut exec = DummyScanExecutor::new();
    let tables = HashMap::new();
    let tx_mgr = make_tx_mgr();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: None,
        cr_scn: 0,
    };
    exec.init(&ctx).unwrap();
    // First next returns one empty tuple
    let t1 = exec.next(&ctx).unwrap();
    assert!(t1.is_some());
    assert_eq!(t1.unwrap().values.len(), 0);
    // Second next returns None
    let t2 = exec.next(&ctx).unwrap();
    assert!(t2.is_none());
    exec.close(&ctx).unwrap();
}

#[test]
fn test_dummy_scan_executor_reinit() {
    let mut exec = DummyScanExecutor::new();
    let tables = HashMap::new();
    let tx_mgr = make_tx_mgr();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: None,
        cr_scn: 0,
    };
    exec.init(&ctx).unwrap();
    let _ = exec.next(&ctx).unwrap();
    let _ = exec.next(&ctx).unwrap();
    // Re-init should reset
    exec.init(&ctx).unwrap();
    let t = exec.next(&ctx).unwrap();
    assert!(t.is_some());
}

// ============ SeqScanExecutor error on missing table ============

#[test]
fn test_seq_scan_missing_table() {
    let mut exec = SeqScanExecutor::new("nonexistent".to_string());
    let tables = HashMap::new();
    let tx_mgr = make_tx_mgr();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: None,
        cr_scn: 0,
    };
    exec.init(&ctx).unwrap();
    let result = exec.next(&ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Table not found in storage"));
}

// ============ LimitExecutor edge cases ============

#[test]
fn test_limit_offset_only_no_limit() {
    // LIMIT None, OFFSET 1 - should skip first row, return rest
    let (cat, tables) = make_catalog_with_users();
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Limit {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        limit: None,
        offset: Some(1),
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    assert_eq!(results.len(), 2); // 3 - 1 offset = 2
}

#[test]
fn test_limit_offset_exceeds_table_size() {
    let (cat, tables) = make_catalog_with_users();
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Limit {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        limit: None,
        offset: Some(100), // Way more than 3 rows
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    assert_eq!(results.len(), 0);
}

#[test]
fn test_limit_zero() {
    let (cat, tables) = make_catalog_with_users();
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Limit {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        limit: Some(0),
        offset: None,
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    assert_eq!(results.len(), 0);
}

#[test]
fn test_limit_larger_than_table() {
    let (cat, tables) = make_catalog_with_users();
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Limit {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        limit: Some(100),
        offset: None,
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    assert_eq!(results.len(), 3);
}

// ============ SortExecutor with NULL values ============

#[test]
fn test_sort_with_nulls() {
    let (cat, mut tables) = make_catalog_with_users();
    // Add rows with NULL values
    tables.insert(
        "users".to_string(),
        vec![
            Tuple::new(vec![Value::Int(1), Value::Varchar("Alice".to_string()), Value::Int(25), Value::Float(95.5)]),
            Tuple::new(vec![Value::Int(2), Value::Varchar("Bob".to_string()), Value::Null, Value::Float(88.0)]),
            Tuple::new(vec![Value::Int(3), Value::Varchar("Charlie".to_string()), Value::Int(22), Value::Float(92.0)]),
        ],
    );
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Sort {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        order_by: vec![(Expr::ColRef("users.age".to_string()), true)],
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    assert_eq!(results.len(), 3);
    // NULLs should be sorted as Equal (treated as equal to everything due to unwrap_or(Equal))
}

// ============ Cross join (no condition) ============

#[test]
fn test_cross_join() {
    let (mut cat, mut tables) = make_catalog_with_users();
    cat.add_table(
        "orders".to_string(),
        Schema::new(vec![
            Column { name: "id".to_string(), data_type: DataType::Int },
            Column { name: "user_id".to_string(), data_type: DataType::Int },
        ]),
    );
    tables.insert(
        "orders".to_string(),
        vec![
            Tuple::new(vec![Value::Int(101), Value::Int(1)]),
            Tuple::new(vec![Value::Int(102), Value::Int(2)]),
        ],
    );
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Join {
        left: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        right: Box::new(LogicalPlan::Scan {
            table_name: "orders".to_string(),
            alias: None,
        }),
        condition: None, // Cross join
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    // 3 users * 2 orders = 6 rows
    assert_eq!(results.len(), 6);
    // Each row should have 4 + 2 = 6 columns
    assert_eq!(results[0].values.len(), 6);
}

// ============ Empty relation aggregation ============

#[test]
fn test_global_agg_on_empty_table() {
    let (cat, mut tables) = make_catalog_with_users();
    tables.insert("users".to_string(), vec![]); // Empty table
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Agg {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        group_by: vec![],
        agg_funcs: vec![
            ("count".to_string(), Expr::Star, "count(*)".to_string()),
            ("sum".to_string(), Expr::ColRef("users.id".to_string()), "sum(users.id)".to_string()),
        ],
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    // Global aggregation on empty relation should produce one row with default values
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].values[0], Value::Int(0)); // COUNT(*) = 0
    assert_eq!(results[0].values[1], Value::Null); // SUM = NULL
}

// ============ Group by with NULL group key ============

#[test]
fn test_group_by_with_null_key() {
    let (cat, mut tables) = make_catalog_with_users();
    tables.insert(
        "users".to_string(),
        vec![
            Tuple::new(vec![Value::Int(1), Value::Varchar("A".to_string()), Value::Int(25), Value::Float(95.5)]),
            Tuple::new(vec![Value::Int(2), Value::Varchar("B".to_string()), Value::Null, Value::Float(88.0)]),
            Tuple::new(vec![Value::Int(3), Value::Varchar("C".to_string()), Value::Null, Value::Float(92.0)]),
        ],
    );
    let tx_mgr = make_tx_mgr();
    let plan = LogicalPlan::Agg {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        group_by: vec![Expr::ColRef("users.age".to_string())],
        agg_funcs: vec![("count".to_string(), Expr::Star, "count(*)".to_string())],
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    // Should have 2 groups: age=25 and age=NULL
    assert_eq!(results.len(), 2);
}

// ============ Visibility filtering in SeqScan ============

#[test]
fn test_seq_scan_visibility_filtering() {
    let (cat, mut tables) = make_catalog_with_users();
    // Add a row with tx_id that is NOT the session's and NOT committed
    tables.get_mut("users").unwrap().push(Tuple::new_with_meta(
        vec![Value::Int(99), Value::Varchar("Hidden".to_string()), Value::Int(1), Value::Float(1.0)],
        Some(999), // tx_id 999, not the session's tx
        100,
    ));
    let mut tx_mgr = make_tx_mgr();
    // Mark tx 999 as Active (not committed)
    // We need to insert into tx_mgr.statuses - but it's pub
    tx_mgr.statuses.insert(999, volcano_db::tx::TxStatus::Active);

    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    // Should only see 3 rows (the uncommitted row from tx 999 is hidden)
    assert_eq!(results.len(), 3);
}

#[test]
fn test_seq_scan_visibility_own_tx() {
    let (cat, mut tables) = make_catalog_with_users();
    // Add a row with tx_id = 5, which IS the session's tx
    tables.get_mut("users").unwrap().push(Tuple::new_with_meta(
        vec![Value::Int(99), Value::Varchar("Mine".to_string()), Value::Int(1), Value::Float(1.0)],
        Some(5),
        100,
    ));
    let mut tx_mgr = make_tx_mgr();
    tx_mgr.statuses.insert(5, volcano_db::tx::TxStatus::Active);

    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    // Session with tx_id = 5 should see its own row
    let mut exec = build_executor(&plan, &cat).unwrap();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: Some(5),
        cr_scn: tx_mgr.scn.get(),
    };
    exec.init(&ctx).unwrap();
    let mut count = 0;
    while let Some(_) = exec.next(&ctx).unwrap() {
        count += 1;
    }
    // Should see all 4 rows (3 normal + 1 own uncommitted)
    assert_eq!(count, 4);
}

#[test]
fn test_seq_scan_visibility_committed() {
    let (cat, mut tables) = make_catalog_with_users();
    // Add a committed row
    tables.get_mut("users").unwrap().push(Tuple::new_with_meta(
        vec![Value::Int(99), Value::Varchar("Committed".to_string()), Value::Int(1), Value::Float(1.0)],
        Some(7),
        100,
    ));
    let mut tx_mgr = make_tx_mgr();
    tx_mgr.statuses.insert(7, volcano_db::tx::TxStatus::Committed);
    tx_mgr.commit_scns.insert(7, 50); // committed at scn=50

    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    // Session with cr_scn=100 should see committed row (commit_scn=50 <= 100)
    let mut exec = build_executor(&plan, &cat).unwrap();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: None,
        cr_scn: 100,
    };
    exec.init(&ctx).unwrap();
    let mut count = 0;
    while let Some(_) = exec.next(&ctx).unwrap() {
        count += 1;
    }
    assert_eq!(count, 4); // 3 normal + 1 committed
}

#[test]
fn test_seq_scan_visibility_committed_after_cr_scn() {
    let (cat, mut tables) = make_catalog_with_users();
    tables.get_mut("users").unwrap().push(Tuple::new_with_meta(
        vec![Value::Int(99), Value::Varchar("Future".to_string()), Value::Int(1), Value::Float(1.0)],
        Some(7),
        100,
    ));
    let mut tx_mgr = make_tx_mgr();
    tx_mgr.statuses.insert(7, volcano_db::tx::TxStatus::Committed);
    tx_mgr.commit_scns.insert(7, 200); // committed at scn=200

    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    // Session with cr_scn=100 should NOT see row committed at scn=200
    let mut exec = build_executor(&plan, &cat).unwrap();
    let ctx = ExecutionContext {
        tables: &tables,
        tx_mgr: &tx_mgr,
        session_tx_id: None,
        cr_scn: 100,
    };
    exec.init(&ctx).unwrap();
    let mut count = 0;
    while let Some(_) = exec.next(&ctx).unwrap() {
        count += 1;
    }
    assert_eq!(count, 3); // Only the 3 original rows, future commit hidden
}

#[test]
fn test_seq_scan_visibility_rolled_back() {
    let (cat, mut tables) = make_catalog_with_users();
    tables.get_mut("users").unwrap().push(Tuple::new_with_meta(
        vec![Value::Int(99), Value::Varchar("RolledBack".to_string()), Value::Int(1), Value::Float(1.0)],
        Some(8),
        100,
    ));
    let mut tx_mgr = make_tx_mgr();
    tx_mgr.statuses.insert(8, volcano_db::tx::TxStatus::RolledBack);

    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    let results = exec_collect(&plan, &cat, &tables, &tx_mgr);
    // Rolled-back row should be invisible
    assert_eq!(results.len(), 3);
}
