use volcano_db::storage::{Tuple, Value};

// ============ Value PartialEq ============

#[test]
fn test_value_eq_null_null() {
    assert_eq!(Value::Null, Value::Null);
}

#[test]
fn test_value_eq_int_int() {
    assert_eq!(Value::Int(42), Value::Int(42));
    assert_ne!(Value::Int(42), Value::Int(43));
}

#[test]
fn test_value_eq_float_float() {
    assert_eq!(Value::Float(1.5), Value::Float(1.5));
    assert_ne!(Value::Float(1.5), Value::Float(2.5));
}

#[test]
fn test_value_eq_int_float_cross_type() {
    // Int 5 == Float 5.0
    assert_eq!(Value::Int(5), Value::Float(5.0));
    assert_eq!(Value::Float(5.0), Value::Int(5));
    // Int 5 != Float 5.5
    assert_ne!(Value::Int(5), Value::Float(5.5));
    assert_ne!(Value::Float(5.5), Value::Int(5));
}

#[test]
fn test_value_eq_varchar_varchar() {
    assert_eq!(
        Value::Varchar("hello".to_string()),
        Value::Varchar("hello".to_string())
    );
    assert_ne!(
        Value::Varchar("hello".to_string()),
        Value::Varchar("world".to_string())
    );
}

#[test]
fn test_value_eq_bool_bool() {
    assert_eq!(Value::Bool(true), Value::Bool(true));
    assert_eq!(Value::Bool(false), Value::Bool(false));
    assert_ne!(Value::Bool(true), Value::Bool(false));
}

#[test]
fn test_value_eq_incompatible_types() {
    // Int vs Varchar - cannot convert, always false
    assert_ne!(Value::Int(5), Value::Varchar("5".to_string()));
    // Varchar vs Bool
    assert_ne!(
        Value::Varchar("true".to_string()),
        Value::Bool(true)
    );
    // Null vs Int
    assert_ne!(Value::Null, Value::Int(0));
    // Null vs Varchar
    assert_ne!(Value::Null, Value::Varchar("".to_string()));
    // Null vs Bool
    assert_ne!(Value::Null, Value::Bool(false));
    // Float vs Varchar
    assert_ne!(Value::Float(1.0), Value::Varchar("1".to_string()));
    // Bool vs Int
    assert_ne!(Value::Bool(true), Value::Int(1));
}

// ============ Value PartialOrd ============

#[test]
fn test_value_ord_null_null() {
    assert!(Value::Null >= Value::Null);
    assert!(Value::Null <= Value::Null);
}

#[test]
fn test_value_ord_int_int() {
    assert!(Value::Int(1) < Value::Int(2));
    assert!(Value::Int(2) > Value::Int(1));
    assert!(Value::Int(5) >= Value::Int(5));
    assert!(Value::Int(5) <= Value::Int(5));
}

#[test]
fn test_value_ord_float_float() {
    assert!(Value::Float(1.0) < Value::Float(2.0));
    assert!(Value::Float(2.0) > Value::Float(1.0));
    assert!(Value::Float(1.5) >= Value::Float(1.5));
}

#[test]
fn test_value_ord_int_float_cross_type() {
    // Int 5 < Float 5.5
    assert!(Value::Int(5) < Value::Float(5.5));
    // Int 5 == Float 5.0 (neither < nor >)
    assert!(!(Value::Int(5) < Value::Float(5.0)));
    assert!(!(Value::Int(5) > Value::Float(5.0)));
    // Float 5.5 > Int 5
    assert!(Value::Float(5.5) > Value::Int(5));
    // Float 5.0 == Int 5
    assert!(!(Value::Float(5.0) > Value::Int(5)));
    assert!(!(Value::Float(5.0) < Value::Int(5)));
}

#[test]
fn test_value_ord_varchar_varchar() {
    assert!(Value::Varchar("apple".to_string()) < Value::Varchar("banana".to_string()));
    assert!(Value::Varchar("banana".to_string()) > Value::Varchar("apple".to_string()));
    assert!(Value::Varchar("same".to_string()) <= Value::Varchar("same".to_string()));
}

#[test]
fn test_value_ord_bool_bool() {
    assert!(Value::Bool(false) < Value::Bool(true));
    assert!(Value::Bool(true) > Value::Bool(false));
    assert!(Value::Bool(false) <= Value::Bool(false));
}

#[test]
fn test_value_ord_incompatible_types_returns_none() {
    // Int vs Varchar - returns None, so all comparisons are false
    assert!(!(Value::Int(1) < Value::Varchar("1".to_string())));
    assert!(!(Value::Int(1) > Value::Varchar("1".to_string())));
    assert!(!(Value::Int(1) <= Value::Varchar("1".to_string())));
    assert!(!(Value::Int(1) >= Value::Varchar("1".to_string())));

    // Varchar vs Bool
    assert_eq!(
        Value::Varchar("a".to_string()).partial_cmp(&Value::Bool(true)),
        None
    );

    // Null vs Int
    assert_eq!(Value::Null.partial_cmp(&Value::Int(0)), None);

    // Bool vs Float
    assert_eq!(Value::Bool(true).partial_cmp(&Value::Float(1.0)), None);
}

// ============ Value::is_truthy ============

#[test]
fn test_value_is_truthy() {
    assert!(!Value::Null.is_truthy());

    assert!(Value::Int(1).is_truthy());
    assert!(Value::Int(0).is_truthy()); // non-zero AND zero both truthy for non-bool
    assert!(Value::Int(-1).is_truthy());

    assert!(Value::Float(0.0).is_truthy());
    assert!(Value::Float(1.5).is_truthy());

    assert!(Value::Varchar("".to_string()).is_truthy());
    assert!(Value::Varchar("hello".to_string()).is_truthy());

    assert!(Value::Bool(true).is_truthy());
    assert!(!Value::Bool(false).is_truthy());
}

// ============ Value Display ============

#[test]
fn test_value_display() {
    assert_eq!(format!("{}", Value::Null), "NULL");
    assert_eq!(format!("{}", Value::Int(42)), "42");
    assert_eq!(format!("{}", Value::Int(-7)), "-7");
    assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
    assert_eq!(format!("{}", Value::Varchar("hello".to_string())), "hello");
    assert_eq!(format!("{}", Value::Bool(true)), "true");
    assert_eq!(format!("{}", Value::Bool(false)), "false");
}

// ============ Tuple constructors ============

#[test]
fn test_tuple_new_defaults() {
    let t = Tuple::new(vec![Value::Int(1), Value::Varchar("a".to_string())]);
    assert_eq!(t.values.len(), 2);
    assert_eq!(t.values[0], Value::Int(1));
    assert_eq!(t.values[1], Value::Varchar("a".to_string()));
    // Default metadata
    assert_eq!(t.tx_id, None);
    assert_eq!(t.scn, 0);
}

#[test]
fn test_tuple_new_with_meta() {
    let t = Tuple::new_with_meta(
        vec![Value::Int(1)],
        Some(42),
        100,
    );
    assert_eq!(t.values.len(), 1);
    assert_eq!(t.values[0], Value::Int(1));
    assert_eq!(t.tx_id, Some(42));
    assert_eq!(t.scn, 100);
}

#[test]
fn test_tuple_new_with_meta_none_tx_id() {
    let t = Tuple::new_with_meta(vec![Value::Bool(true)], None, 5);
    assert_eq!(t.tx_id, None);
    assert_eq!(t.scn, 5);
}

// ============ Tuple PartialEq (only compares values, not metadata) ============

#[test]
fn test_tuple_eq_ignores_metadata() {
    let t1 = Tuple::new_with_meta(vec![Value::Int(1)], Some(1), 10);
    let t2 = Tuple::new_with_meta(vec![Value::Int(1)], Some(2), 20);
    let t3 = Tuple::new_with_meta(vec![Value::Int(1)], None, 0);
    // All three have identical values, so they should be equal despite metadata differences
    assert_eq!(t1, t2);
    assert_eq!(t1, t3);
    assert_eq!(t2, t3);
}

#[test]
fn test_tuple_neq_different_values() {
    let t1 = Tuple::new(vec![Value::Int(1)]);
    let t2 = Tuple::new(vec![Value::Int(2)]);
    assert_ne!(t1, t2);
}

// ============ Value serde roundtrip ============

#[test]
fn test_value_serde_roundtrip() {
    let values = vec![
        Value::Null,
        Value::Int(42),
        Value::Float(3.14),
        Value::Varchar("hello".to_string()),
        Value::Bool(true),
    ];
    for v in &values {
        let json = serde_json::to_string(v).unwrap();
        let decoded: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v, &decoded);
    }
}
