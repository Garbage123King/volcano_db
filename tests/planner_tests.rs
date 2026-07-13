use volcano_db::catalog::{Catalog, Column, DataType, Schema};
use volcano_db::planner::{
    bind_statement, eval_binary_op, get_expr_name, get_expr_type, Expr, LogicalPlan, SQLStatement,
};
use volcano_db::storage::{Tuple, Value};

// Helper: build a catalog with a test table
fn make_catalog() -> Catalog {
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
    cat
}

// ============ eval_binary_op coverage ============

#[test]
fn test_eval_eq_neq() {
    assert_eq!(
        eval_binary_op("=", Value::Int(5), Value::Int(5)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op("=", Value::Int(5), Value::Int(6)).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        eval_binary_op("!=", Value::Int(5), Value::Int(6)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op("<>", Value::Int(5), Value::Int(5)).unwrap(),
        Value::Bool(false)
    );
    // Cross-type equality
    assert_eq!(
        eval_binary_op("=", Value::Int(5), Value::Float(5.0)).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn test_eval_comparison() {
    assert_eq!(
        eval_binary_op(">", Value::Int(5), Value::Int(3)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op(">=", Value::Int(3), Value::Int(3)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op("<", Value::Int(3), Value::Int(5)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op("<=", Value::Int(3), Value::Int(3)).unwrap(),
        Value::Bool(true)
    );
    // Cross-type
    assert_eq!(
        eval_binary_op(">", Value::Int(6), Value::Float(5.5)).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn test_eval_arithmetic_int() {
    assert_eq!(
        eval_binary_op("+", Value::Int(3), Value::Int(4)).unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        eval_binary_op("-", Value::Int(10), Value::Int(3)).unwrap(),
        Value::Int(7)
    );
    assert_eq!(
        eval_binary_op("*", Value::Int(3), Value::Int(4)).unwrap(),
        Value::Int(12)
    );
    assert_eq!(
        eval_binary_op("/", Value::Int(12), Value::Int(4)).unwrap(),
        Value::Int(3)
    );
}

#[test]
fn test_eval_arithmetic_float() {
    assert_eq!(
        eval_binary_op("+", Value::Float(1.5), Value::Float(2.5)).unwrap(),
        Value::Float(4.0)
    );
    assert_eq!(
        eval_binary_op("-", Value::Float(5.0), Value::Float(2.0)).unwrap(),
        Value::Float(3.0)
    );
    assert_eq!(
        eval_binary_op("*", Value::Float(2.0), Value::Float(3.0)).unwrap(),
        Value::Float(6.0)
    );
    assert_eq!(
        eval_binary_op("/", Value::Float(6.0), Value::Float(2.0)).unwrap(),
        Value::Float(3.0)
    );
}

#[test]
fn test_eval_arithmetic_int_float_mix() {
    // Int + Float -> Float
    assert_eq!(
        eval_binary_op("+", Value::Int(3), Value::Float(0.5)).unwrap(),
        Value::Float(3.5)
    );
    assert_eq!(
        eval_binary_op("+", Value::Float(0.5), Value::Int(3)).unwrap(),
        Value::Float(3.5)
    );
    assert_eq!(
        eval_binary_op("-", Value::Int(5), Value::Float(0.5)).unwrap(),
        Value::Float(4.5)
    );
    assert_eq!(
        eval_binary_op("-", Value::Float(5.0), Value::Int(2)).unwrap(),
        Value::Float(3.0)
    );
    assert_eq!(
        eval_binary_op("*", Value::Int(3), Value::Float(2.0)).unwrap(),
        Value::Float(6.0)
    );
    assert_eq!(
        eval_binary_op("*", Value::Float(2.0), Value::Int(3)).unwrap(),
        Value::Float(6.0)
    );
    assert_eq!(
        eval_binary_op("/", Value::Int(6), Value::Float(2.0)).unwrap(),
        Value::Float(3.0)
    );
    assert_eq!(
        eval_binary_op("/", Value::Float(6.0), Value::Int(2)).unwrap(),
        Value::Float(3.0)
    );
}

#[test]
fn test_eval_division_by_zero() {
    let result = eval_binary_op("/", Value::Int(10), Value::Int(0));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Division by zero"));
}

#[test]
fn test_eval_arithmetic_invalid_types() {
    assert!(eval_binary_op("+", Value::Varchar("a".to_string()), Value::Int(1)).is_err());
    assert!(eval_binary_op("-", Value::Bool(true), Value::Int(1)).is_err());
    assert!(eval_binary_op("*", Value::Null, Value::Int(1)).is_err());
    assert!(eval_binary_op("/", Value::Varchar("x".to_string()), Value::Varchar("y".to_string())).is_err());
}

#[test]
fn test_eval_and_or() {
    assert_eq!(
        eval_binary_op("and", Value::Bool(true), Value::Bool(true)).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        eval_binary_op("and", Value::Bool(true), Value::Bool(false)).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        eval_binary_op("and", Value::Bool(false), Value::Bool(true)).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        eval_binary_op("or", Value::Bool(false), Value::Bool(false)).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        eval_binary_op("or", Value::Bool(false), Value::Bool(true)).unwrap(),
        Value::Bool(true)
    );
    // is_truthy: Int 0 is truthy!
    assert_eq!(
        eval_binary_op("and", Value::Int(0), Value::Int(1)).unwrap(),
        Value::Bool(true)
    );
    // Null is falsy
    assert_eq!(
        eval_binary_op("and", Value::Null, Value::Bool(true)).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        eval_binary_op("or", Value::Null, Value::Bool(true)).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn test_eval_unsupported_op() {
    let result = eval_binary_op("^", Value::Int(1), Value::Int(2));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported operator"));
}

// ============ Expr::eval coverage ============

#[test]
fn test_expr_const_eval() {
    let expr = Expr::Const(Value::Int(42));
    let schema = Schema::new(vec![]);
    let tuple = Tuple::new(vec![]);
    assert_eq!(expr.eval(&tuple, &schema).unwrap(), Value::Int(42));
}

#[test]
fn test_expr_colref_eval() {
    let expr = Expr::ColRef("name".to_string());
    let schema = Schema::new(vec![
        Column { name: "id".to_string(), data_type: DataType::Int },
        Column { name: "name".to_string(), data_type: DataType::Varchar },
    ]);
    let tuple = Tuple::new(vec![Value::Int(1), Value::Varchar("Alice".to_string())]);
    assert_eq!(expr.eval(&tuple, &schema).unwrap(), Value::Varchar("Alice".to_string()));
}

#[test]
fn test_expr_colref_not_found() {
    let expr = Expr::ColRef("missing".to_string());
    let schema = Schema::new(vec![
        Column { name: "id".to_string(), data_type: DataType::Int },
    ]);
    let tuple = Tuple::new(vec![Value::Int(1)]);
    let result = expr.eval(&tuple, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Column not found"));
}

#[test]
fn test_expr_star_eval_errors() {
    let expr = Expr::Star;
    let schema = Schema::new(vec![]);
    let tuple = Tuple::new(vec![]);
    let result = expr.eval(&tuple, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot evaluate Star"));
}

#[test]
fn test_expr_func_call_eval_errors() {
    let expr = Expr::FuncCall {
        name: "count".to_string(),
        arg: Box::new(Expr::Star),
    };
    let schema = Schema::new(vec![]);
    let tuple = Tuple::new(vec![]);
    let result = expr.eval(&tuple, &schema);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot be evaluated directly outside Aggregation"));
}

#[test]
fn test_expr_binary_op_eval() {
    let expr = Expr::BinaryOp {
        op: "+".to_string(),
        left: Box::new(Expr::Const(Value::Int(3))),
        right: Box::new(Expr::Const(Value::Int(4))),
    };
    let schema = Schema::new(vec![]);
    let tuple = Tuple::new(vec![]);
    assert_eq!(expr.eval(&tuple, &schema).unwrap(), Value::Int(7));
}

// ============ get_expr_type coverage ============

#[test]
fn test_get_expr_type_const() {
    let schema = Schema::new(vec![]);
    assert_eq!(get_expr_type(&Expr::Const(Value::Int(1)), &schema).unwrap(), DataType::Int);
    assert_eq!(get_expr_type(&Expr::Const(Value::Float(1.0)), &schema).unwrap(), DataType::Float);
    assert_eq!(
        get_expr_type(&Expr::Const(Value::Varchar("a".to_string())), &schema).unwrap(),
        DataType::Varchar
    );
    assert_eq!(get_expr_type(&Expr::Const(Value::Bool(true)), &schema).unwrap(), DataType::Bool);
    // Null defaults to Varchar
    assert_eq!(get_expr_type(&Expr::Const(Value::Null), &schema).unwrap(), DataType::Varchar);
}

#[test]
fn test_get_expr_type_colref() {
    let schema = Schema::new(vec![
        Column { name: "id".to_string(), data_type: DataType::Int },
        Column { name: "name".to_string(), data_type: DataType::Varchar },
    ]);
    assert_eq!(get_expr_type(&Expr::ColRef("id".to_string()), &schema).unwrap(), DataType::Int);
    assert_eq!(
        get_expr_type(&Expr::ColRef("name".to_string()), &schema).unwrap(),
        DataType::Varchar
    );
}

#[test]
fn test_get_expr_type_colref_not_found() {
    let schema = Schema::new(vec![]);
    let result = get_expr_type(&Expr::ColRef("missing".to_string()), &schema);
    assert!(result.is_err());
}

#[test]
fn test_get_expr_type_comparison_ops() {
    let schema = Schema::new(vec![]);
    let expr = Expr::BinaryOp {
        op: ">".to_string(),
        left: Box::new(Expr::Const(Value::Int(1))),
        right: Box::new(Expr::Const(Value::Int(2))),
    };
    assert_eq!(get_expr_type(&expr, &schema).unwrap(), DataType::Bool);

    let expr_and = Expr::BinaryOp {
        op: "and".to_string(),
        left: Box::new(Expr::Const(Value::Bool(true))),
        right: Box::new(Expr::Const(Value::Bool(false))),
    };
    assert_eq!(get_expr_type(&expr_and, &schema).unwrap(), DataType::Bool);
}

#[test]
fn test_get_expr_type_arithmetic() {
    let schema = Schema::new(vec![]);
    // Int + Int -> Int
    let expr = Expr::BinaryOp {
        op: "+".to_string(),
        left: Box::new(Expr::Const(Value::Int(1))),
        right: Box::new(Expr::Const(Value::Int(2))),
    };
    assert_eq!(get_expr_type(&expr, &schema).unwrap(), DataType::Int);

    // Int + Float -> Float
    let expr = Expr::BinaryOp {
        op: "+".to_string(),
        left: Box::new(Expr::Const(Value::Int(1))),
        right: Box::new(Expr::Const(Value::Float(2.0))),
    };
    assert_eq!(get_expr_type(&expr, &schema).unwrap(), DataType::Float);
}

#[test]
fn test_get_expr_type_arithmetic_invalid() {
    let schema = Schema::new(vec![]);
    // Varchar + Int -> error
    let expr = Expr::BinaryOp {
        op: "+".to_string(),
        left: Box::new(Expr::Const(Value::Varchar("a".to_string()))),
        right: Box::new(Expr::Const(Value::Int(1))),
    };
    let result = get_expr_type(&expr, &schema);
    assert!(result.is_err());
}

#[test]
fn test_get_expr_type_unsupported_op() {
    let schema = Schema::new(vec![]);
    let expr = Expr::BinaryOp {
        op: "^".to_string(),
        left: Box::new(Expr::Const(Value::Int(1))),
        right: Box::new(Expr::Const(Value::Int(2))),
    };
    let result = get_expr_type(&expr, &schema);
    assert!(result.is_err());
}

#[test]
fn test_get_expr_type_agg_funcs() {
    let schema = Schema::new(vec![
        Column { name: "id".to_string(), data_type: DataType::Int },
        Column { name: "score".to_string(), data_type: DataType::Float },
    ]);

    // COUNT(*) -> Int
    let count_expr = Expr::FuncCall {
        name: "count".to_string(),
        arg: Box::new(Expr::Star),
    };
    assert_eq!(get_expr_type(&count_expr, &schema).unwrap(), DataType::Int);

    // SUM(int) -> Int
    let sum_expr = Expr::FuncCall {
        name: "sum".to_string(),
        arg: Box::new(Expr::ColRef("id".to_string())),
    };
    assert_eq!(get_expr_type(&sum_expr, &schema).unwrap(), DataType::Int);

    // AVG(int) -> Float
    let avg_expr = Expr::FuncCall {
        name: "avg".to_string(),
        arg: Box::new(Expr::ColRef("id".to_string())),
    };
    assert_eq!(get_expr_type(&avg_expr, &schema).unwrap(), DataType::Float);

    // SUM(float) -> Float
    let sum_float = Expr::FuncCall {
        name: "sum".to_string(),
        arg: Box::new(Expr::ColRef("score".to_string())),
    };
    assert_eq!(get_expr_type(&sum_float, &schema).unwrap(), DataType::Float);

    // MIN(varchar) -> Varchar
    let min_expr = Expr::FuncCall {
        name: "min".to_string(),
        arg: Box::new(Expr::ColRef("id".to_string())),
    };
    assert_eq!(get_expr_type(&min_expr, &schema).unwrap(), DataType::Int);
}

#[test]
fn test_get_expr_type_agg_invalid_arg() {
    let schema = Schema::new(vec![
        Column { name: "name".to_string(), data_type: DataType::Varchar },
    ]);
    // SUM(varchar) -> error
    let sum_expr = Expr::FuncCall {
        name: "sum".to_string(),
        arg: Box::new(Expr::ColRef("name".to_string())),
    };
    let result = get_expr_type(&sum_expr, &schema);
    assert!(result.is_err());
}

#[test]
fn test_get_expr_type_unsupported_agg_func() {
    let schema = Schema::new(vec![]);
    let expr = Expr::FuncCall {
        name: "stddev".to_string(),
        arg: Box::new(Expr::Const(Value::Int(1))),
    };
    let result = get_expr_type(&expr, &schema);
    assert!(result.is_err());
}

#[test]
fn test_get_expr_type_star_errors() {
    let schema = Schema::new(vec![]);
    let result = get_expr_type(&Expr::Star, &schema);
    assert!(result.is_err());
}

// ============ get_expr_name coverage ============

#[test]
fn test_get_expr_name_all_variants() {
    assert_eq!(get_expr_name(&Expr::Const(Value::Int(42))), "42");
    assert_eq!(get_expr_name(&Expr::ColRef("name".to_string())), "name");
    assert_eq!(get_expr_name(&Expr::Star), "*");
    assert_eq!(
        get_expr_name(&Expr::FuncCall {
            name: "count".to_string(),
            arg: Box::new(Expr::Star),
        }),
        "count(*)"
    );
    assert_eq!(
        get_expr_name(&Expr::BinaryOp {
            op: "+".to_string(),
            left: Box::new(Expr::Const(Value::Int(1))),
            right: Box::new(Expr::Const(Value::Int(2))),
        }),
        "(1 + 2)"
    );
}

// ============ bind_statement error cases ============

#[test]
fn test_bind_empty_sql() {
    let cat = make_catalog();
    let result = bind_statement("", &cat);
    assert!(result.is_err());
}

#[test]
fn test_bind_unsupported_statement() {
    let cat = make_catalog();
    // DELETE is not supported
    let result = bind_statement("DELETE FROM users", &cat);
    assert!(result.is_err());
    // UPDATE is not supported
    let result = bind_statement("UPDATE users SET name = 'x'", &cat);
    assert!(result.is_err());
    // DROP is not supported
    let result = bind_statement("DROP TABLE users", &cat);
    assert!(result.is_err());
}

#[test]
fn test_bind_unsupported_data_type() {
    let cat = make_catalog();
    // DATE type not supported
    let result = bind_statement("CREATE TABLE t (d DATE)", &cat);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported column datatype"));
}

#[test]
fn test_bind_unsupported_query_body() {
    let cat = make_catalog();
    // UNION is a SetOperation, not Select
    let result = bind_statement(
        "SELECT id FROM users UNION SELECT id FROM users",
        &cat,
    );
    assert!(result.is_err());
}

#[test]
fn test_bind_unsupported_table_factor() {
    let cat = make_catalog();
    // Derived table (subquery in FROM) is not supported
    let result = bind_statement(
        "SELECT * FROM (SELECT id FROM users) AS sub",
        &cat,
    );
    assert!(result.is_err());
}

#[test]
fn test_bind_unsupported_join() {
    let mut cat = make_catalog();
    cat.add_table(
        "orders".to_string(),
        Schema::new(vec![
            Column { name: "id".to_string(), data_type: DataType::Int },
            Column { name: "user_id".to_string(), data_type: DataType::Int },
        ]),
    );
    // LEFT JOIN not supported
    let result = bind_statement(
        "SELECT * FROM users LEFT JOIN orders ON users.id = orders.user_id",
        &cat,
    );
    assert!(result.is_err());
}

#[test]
fn test_bind_unsupported_function() {
    let cat = make_catalog();
    let result = bind_statement("SELECT STDDEV(id) FROM users", &cat);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported function"));
}

#[test]
fn test_bind_unsupported_operator() {
    let cat = make_catalog();
    // LIKE is not in the supported BinaryOperator list
    let result = bind_statement("SELECT * FROM users WHERE name LIKE 'A%'", &cat);
    assert!(result.is_err());
}

#[test]
fn test_bind_table_not_found() {
    let cat = make_catalog();
    let result = bind_statement("SELECT * FROM nonexistent", &cat);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Table not found in catalog"));
}

#[test]
fn test_bind_create_table_various_types() {
    let mut cat = Catalog::newaaa();
    // Bool type
    let stmt = bind_statement("CREATE TABLE t1 (b BOOLEAN)", &cat).unwrap();
    if let SQLStatement::CreateTable { schema, .. } = stmt {
        cat.add_table("t1".to_string(), schema);
    }
    // Text and String as Varchar
    let stmt = bind_statement("CREATE TABLE t2 (s TEXT)", &cat).unwrap();
    if let SQLStatement::CreateTable { schema, .. } = stmt {
        cat.add_table("t2".to_string(), schema);
    }
    // Integer variant
    let stmt = bind_statement("CREATE TABLE t3 (i INTEGER)", &cat).unwrap();
    if let SQLStatement::CreateTable { schema, .. } = stmt {
        cat.add_table("t3".to_string(), schema);
    }
    // Double Precision as Float
    let stmt = bind_statement("CREATE TABLE t4 (d DOUBLE PRECISION)", &cat).unwrap();
    if let SQLStatement::CreateTable { schema, .. } = stmt {
        cat.add_table("t4".to_string(), schema);
    }
    // Real as Float
    let stmt = bind_statement("CREATE TABLE t5 (r REAL)", &cat).unwrap();
    if let SQLStatement::CreateTable { schema, .. } = stmt {
        cat.add_table("t5".to_string(), schema);
    }
}

#[test]
fn test_bind_insert_into_values() {
    let cat = make_catalog();
    let result = bind_statement(
        "INSERT INTO users VALUES (1, 'Alice', 25, 95.5)",
        &cat,
    );
    assert!(result.is_ok());
    if let Ok(SQLStatement::Insert { table_name, rows }) = result {
        assert_eq!(table_name, "users");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 4);
    } else {
        panic!("Expected Insert statement");
    }
}

#[test]
fn test_bind_select_with_bool_literal() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT TRUE, FALSE FROM users",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_select_with_null_literal() {
    let cat = make_catalog();
    let result = bind_statement("SELECT NULL FROM users", &cat);
    assert!(result.is_ok());
}

#[test]
fn test_bind_select_wildcard() {
    let cat = make_catalog();
    let result = bind_statement("SELECT * FROM users", &cat);
    assert!(result.is_ok());
}

#[test]
fn test_bind_select_expr_with_alias() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT id AS user_id, name AS user_name FROM users",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_select_with_qualified_column() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT users.id, users.name FROM users",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_cross_join() {
    let mut cat = make_catalog();
    cat.add_table(
        "orders".to_string(),
        Schema::new(vec![
            Column { name: "id".to_string(), data_type: DataType::Int },
        ]),
    );
    let result = bind_statement(
        "SELECT * FROM users CROSS JOIN orders",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_group_by() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT age, COUNT(*) FROM users GROUP BY age",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_limit_offset() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT * FROM users LIMIT 5 OFFSET 2",
        &cat,
    );
    assert!(result.is_ok());
}

#[test]
fn test_bind_limit_only() {
    let cat = make_catalog();
    let result = bind_statement("SELECT * FROM users LIMIT 3", &cat);
    assert!(result.is_ok());
}

#[test]
fn test_bind_offset_only() {
    let cat = make_catalog();
    let result = bind_statement("SELECT * FROM users OFFSET 2", &cat);
    assert!(result.is_ok());
}

#[test]
fn test_bind_order_by_asc_desc() {
    let cat = make_catalog();
    let result = bind_statement(
        "SELECT * FROM users ORDER BY id ASC, name DESC",
        &cat,
    );
    assert!(result.is_ok());
}

// ============ LogicalPlan::schema coverage ============

#[test]
fn test_logical_plan_dummy_scan_schema() {
    let cat = Catalog::newaaa();
    let schema = LogicalPlan::DummyScan.schema(&cat).unwrap();
    assert!(schema.columns.is_empty());
}

#[test]
fn test_logical_plan_scan_schema_qualified() {
    let cat = make_catalog();
    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: None,
    };
    let schema = plan.schema(&cat).unwrap();
    assert_eq!(schema.columns.len(), 4);
    // Columns should be qualified with table name
    assert_eq!(schema.columns[0].name, "users.id");
    assert_eq!(schema.columns[1].name, "users.name");
}

#[test]
fn test_logical_plan_scan_schema_with_alias() {
    let cat = make_catalog();
    let plan = LogicalPlan::Scan {
        table_name: "users".to_string(),
        alias: Some("u".to_string()),
    };
    let schema = plan.schema(&cat).unwrap();
    assert_eq!(schema.columns[0].name, "u.id");
    assert_eq!(schema.columns[1].name, "u.name");
}

#[test]
fn test_logical_plan_scan_schema_table_not_found() {
    let cat = Catalog::newaaa();
    let plan = LogicalPlan::Scan {
        table_name: "nonexistent".to_string(),
        alias: None,
    };
    let result = plan.schema(&cat);
    assert!(result.is_err());
}

#[test]
fn test_logical_plan_filter_schema_inherits_child() {
    let cat = make_catalog();
    let plan = LogicalPlan::Filter {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        predicate: Expr::Const(Value::Bool(true)),
    };
    let schema = plan.schema(&cat).unwrap();
    assert_eq!(schema.columns.len(), 4);
}

#[test]
fn test_logical_plan_join_schema_combines() {
    let mut cat = make_catalog();
    cat.add_table(
        "orders".to_string(),
        Schema::new(vec![
            Column { name: "id".to_string(), data_type: DataType::Int },
            Column { name: "user_id".to_string(), data_type: DataType::Int },
        ]),
    );
    let plan = LogicalPlan::Join {
        left: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        right: Box::new(LogicalPlan::Scan {
            table_name: "orders".to_string(),
            alias: None,
        }),
        condition: None,
    };
    let schema = plan.schema(&cat).unwrap();
    assert_eq!(schema.columns.len(), 6); // 4 from users + 2 from orders
}

#[test]
fn test_logical_plan_agg_schema() {
    let cat = make_catalog();
    let plan = LogicalPlan::Agg {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        group_by: vec![Expr::ColRef("users.age".to_string())],
        agg_funcs: vec![
            ("count".to_string(), Expr::Star, "count(*)".to_string()),
            ("sum".to_string(), Expr::ColRef("users.score".to_string()), "sum(users.score)".to_string()),
        ],
    };
    let schema = plan.schema(&cat).unwrap();
    assert_eq!(schema.columns.len(), 3); // 1 group + 2 aggs
    assert_eq!(schema.columns[0].name, "users.age");
    assert_eq!(schema.columns[1].name, "count(*)");
    assert_eq!(schema.columns[2].name, "sum(users.score)");
}

#[test]
fn test_logical_plan_agg_unsupported_func() {
    let cat = make_catalog();
    let plan = LogicalPlan::Agg {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        group_by: vec![],
        agg_funcs: vec![
            ("stddev".to_string(), Expr::ColRef("users.id".to_string()), "stddev(users.id)".to_string()),
        ],
    };
    let result = plan.schema(&cat);
    assert!(result.is_err());
}

#[test]
fn test_logical_plan_agg_non_numeric_sum() {
    let cat = make_catalog();
    let plan = LogicalPlan::Agg {
        child: Box::new(LogicalPlan::Scan {
            table_name: "users".to_string(),
            alias: None,
        }),
        group_by: vec![],
        agg_funcs: vec![
            ("sum".to_string(), Expr::ColRef("users.name".to_string()), "sum(users.name)".to_string()),
        ],
    };
    let result = plan.schema(&cat);
    assert!(result.is_err());
}
