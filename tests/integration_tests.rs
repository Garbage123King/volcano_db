use std::collections::HashMap;
use volcano_db::catalog::{Catalog, Schema};
use volcano_db::storage::{Tuple, Value};
use volcano_db::planner::{bind_statement, SQLStatement};
use volcano_db::executor::{build_executor, ExecutionContext};

fn execute_query(
    sql: &str,
    catalog: &mut Catalog,
    tables: &mut HashMap<String, Vec<Tuple>>,
) -> Result<Vec<Tuple>, anyhow::Error> {
    let statement = bind_statement(sql, catalog)?;
    match statement {
        SQLStatement::CreateTable { table_name, schema } => {
            let name_lower = table_name.to_lowercase();
            catalog.add_table(name_lower.clone(), schema);
            tables.insert(name_lower, Vec::new());
            Ok(vec![])
        }
        SQLStatement::Insert { table_name, rows } => {
            let name_lower = table_name.to_lowercase();
            let schema = catalog.get_schema(&name_lower).ok_or_else(|| anyhow::anyhow!("Table not found"))?;
            let mut inserted = vec![];
            for row in rows {
                if row.len() != schema.columns.len() {
                    return Err(anyhow::anyhow!(
                        "Column count mismatch: expected {}, got {}",
                        schema.columns.len(),
                        row.len()
                    ));
                }
                let mut vals = vec![];
                for expr in row {
                    vals.push(expr.eval(&Tuple::new(vec![]), &Schema::new(vec![]))?);
                }
                let t = Tuple::new(vals);
                tables.get_mut(&name_lower).unwrap().push(t.clone());
                inserted.push(t);
            }
            Ok(inserted)
        }
        SQLStatement::Query(logical_plan) => {
            let mut physical_plan = build_executor(&logical_plan, catalog)?;
            let ctx = ExecutionContext { tables };
            physical_plan.init(&ctx)?;
            let mut results = vec![];
            while let Some(tuple) = physical_plan.next(&ctx)? {
                results.push(tuple);
            }
            physical_plan.close(&ctx)?;
            Ok(results)
        }
    }
}

fn setup_test_db() -> (Catalog, HashMap<String, Vec<Tuple>>) {
    let mut catalog = Catalog::newaaa();
    let mut tables = HashMap::new();

    // Create users table
    execute_query(
        "CREATE TABLE users (id INT, name VARCHAR, age INT, score FLOAT);",
        &mut catalog,
        &mut tables,
    ).unwrap();

    // Insert mock users
    execute_query(
        "INSERT INTO users VALUES 
         (1, 'Alice', 25, 95.5), 
         (2, 'Bob', 30, 88.0), 
         (3, 'Charlie', 22, 92.0),
         (4, 'David', 30, 75.5),
         (5, 'Eva', 25, 99.0);",
        &mut catalog,
        &mut tables,
    ).unwrap();

    // Create orders table
    execute_query(
        "CREATE TABLE orders (id INT, user_id INT, amount FLOAT);",
        &mut catalog,
        &mut tables,
    ).unwrap();

    // Insert mock orders
    execute_query(
        "INSERT INTO orders VALUES 
         (101, 1, 150.0), 
         (102, 2, 320.5), 
         (103, 1, 45.0),
         (104, 3, 20.0),
         (105, 9, 999.0);",
        &mut catalog,
        &mut tables,
    ).unwrap();

    (catalog, tables)
}

#[test]
fn test_simple_select_filter_project() {
    let (mut catalog, mut tables) = setup_test_db();
    
    // Query: SELECT name, age FROM users WHERE age > 25
    let result = execute_query(
        "SELECT name, age FROM users WHERE age > 25;",
        &mut catalog,
        &mut tables,
    ).unwrap();

    assert_eq!(result.len(), 2);
    // Bob (30) and David (30)
    assert_eq!(result[0].values, vec![Value::Varchar("Bob".to_string()), Value::Int(30)]);
    assert_eq!(result[1].values, vec![Value::Varchar("David".to_string()), Value::Int(30)]);
}

#[test]
fn test_global_aggregates() {
    let (mut catalog, mut tables) = setup_test_db();

    // Query: SELECT COUNT(*), SUM(age), AVG(score) FROM users
    let result = execute_query(
        "SELECT COUNT(*), SUM(age) FROM users;",
        &mut catalog,
        &mut tables,
    ).unwrap();

    assert_eq!(result.len(), 1);
    // COUNT = 5, SUM = 25+30+22+30+25 = 132
    assert_eq!(result[0].values, vec![Value::Int(5), Value::Int(132)]);
}

#[test]
fn test_group_by_order_by() {
    let (mut catalog, mut tables) = setup_test_db();

    // Query: SELECT age, COUNT(*) FROM users GROUP BY age ORDER BY age DESC
    let result = execute_query(
        "SELECT age, COUNT(*) FROM users GROUP BY age ORDER BY age DESC;",
        &mut catalog,
        &mut tables,
    ).unwrap();

    assert_eq!(result.len(), 3);
    // Age 30: 2
    assert_eq!(result[0].values, vec![Value::Int(30), Value::Int(2)]);
    // Age 25: 2
    assert_eq!(result[1].values, vec![Value::Int(25), Value::Int(2)]);
    // Age 22: 1
    assert_eq!(result[2].values, vec![Value::Int(22), Value::Int(1)]);
}

#[test]
fn test_joins() {
    let (mut catalog, mut tables) = setup_test_db();

    // Query: SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id ORDER BY orders.amount
    let result = execute_query(
        "SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id ORDER BY orders.amount;",
        &mut catalog,
        &mut tables,
    ).unwrap();

    // Matches: 
    // - Charlie (id 3) with order 104 (amount 20.0)
    // - Alice (id 1) with order 103 (amount 45.0)
    // - Alice (id 1) with order 101 (amount 150.0)
    // - Bob (id 2) with order 102 (amount 320.5)
    // Total 4 matches (Eva has no orders, order 105 has non-existent user 9)
    assert_eq!(result.len(), 4);
    assert_eq!(result[0].values, vec![Value::Varchar("Charlie".to_string()), Value::Float(20.0)]);
    assert_eq!(result[1].values, vec![Value::Varchar("Alice".to_string()), Value::Float(45.0)]);
    assert_eq!(result[2].values, vec![Value::Varchar("Alice".to_string()), Value::Float(150.0)]);
    assert_eq!(result[3].values, vec![Value::Varchar("Bob".to_string()), Value::Float(320.5)]);
}

#[test]
fn test_limit_offset() {
    let (mut catalog, mut tables) = setup_test_db();

    // Query: SELECT name FROM users ORDER BY id LIMIT 2 OFFSET 1
    let result = execute_query(
        "SELECT name FROM users ORDER BY id LIMIT 2 OFFSET 1;",
        &mut catalog,
        &mut tables,
    ).unwrap();

    // Total users sorted by id: Alice (1), Bob (2), Charlie (3), David (4), Eva (5)
    // Limit 2 Offset 1 yields: Bob (2), Charlie (3)
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].values, vec![Value::Varchar("Bob".to_string())]);
    assert_eq!(result[1].values, vec![Value::Varchar("Charlie".to_string())]);
}
