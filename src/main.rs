use std::collections::HashMap;
use std::io::{self, Write};
use anyhow::{anyhow, Result, Context};
use comfy_table::Table;

use volcano_db::catalog::{Catalog, Schema};
use volcano_db::storage::Tuple;
use volcano_db::planner::{bind_statement, SQLStatement};
use volcano_db::executor::{build_executor, ExecutionContext};


fn print_banner() {
    println!("\x1b[36m");
    println!("  _    __      __                      ____  ____  ");
    println!(" | |  / /___  / /________ _____  ____ / __ \\/ __ ) ");
    println!(" | | / / __ \\/ / ___/ __ `/ __ \\/ __ \\/ / / / __  | ");
    println!(" | |/ / /_/ / / /__/ /_/ / / / / /_/ / /_/ / /_/ /  ");
    println!(" |___/\\____/_/\\___/\\__,_/_/ /_/\\____/_____/_____/   ");
    println!("                                                    ");
    println!("   Volcano Physical Execution Database Engine");
    println!("   Built with Rust & sqlparser (PostgreSQL Dialect)");
    println!("\x1b[0m");
    println!("Type SQL statements followed by a semicolon ';' and press Enter.");
    println!("Type 'exit' or 'quit' to exit.\n");
}

fn main() -> Result<()> {
    print_banner();

    let mut catalog = Catalog::newaaa();
    let mut tables: HashMap<String, Vec<Tuple>> = HashMap::new();

    // Add some default tables and mock data for immediate testing
    setup_demo_data(&mut catalog, &mut tables)?;

    let mut input = String::new();
    loop {
        if input.trim().is_empty() {
            print!("\x1b[32mvolcano_db> \x1b[0m");
        } else {
            print!("\x1b[33m        -> \x1b[0m");
        }
        io::stdout().flush()?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }

        input.push_str(&line);
        if input.trim().ends_with(';') {
            let sql = input.trim().trim_end_matches(';').trim();
            if !sql.is_empty() {
                if let Err(e) = execute_sql(sql, &mut catalog, &mut tables) {
                    println!("\x1b[31mError: {}\x1b[0m", e);
                }
            }
            input.clear();
        }
    }

    println!("\nGoodbye!");
    Ok(())
}

fn execute_sql(sql: &str, catalog: &mut Catalog, tables: &mut HashMap<String, Vec<Tuple>>) -> Result<()> {
    let statement = bind_statement(sql, catalog)?;
    match statement {
        SQLStatement::CreateTable { table_name, schema } => {
            let name_lower = table_name.to_lowercase();
            if catalog.get_schema(&name_lower).is_some() {
                return Err(anyhow!("Table '{}' already exists", table_name));
            }
            catalog.add_table(name_lower.clone(), schema);
            tables.insert(name_lower.clone(), Vec::new());
            println!("Table '{}' created successfully.", table_name);
        }
        SQLStatement::Insert { table_name, rows } => {
            let name_lower = table_name.to_lowercase();
            let schema = catalog.get_schema(&name_lower)
                .ok_or_else(|| anyhow!("Table '{}' not found", table_name))?;
            
            let mut inserted_count = 0;
            let target_table = tables.get_mut(&name_lower)
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
                    let val = expr.eval(&dummy_tuple, &dummy_schema)
                        .context(format!("Failed to evaluate insert value at position {}", i))?;
                    vals.push(val);
                }
                
                target_table.push(Tuple::new(vals));
                inserted_count += 1;
            }
            println!("Inserted {} row(s).", inserted_count);
        }
        SQLStatement::Query(logical_plan) => {
            let mut physical_plan = build_executor(&logical_plan, catalog)?;
            let ctx = ExecutionContext { tables };
            
            physical_plan.init(&ctx)?;
            
            let query_schema = logical_plan.schema(catalog)?;
            let mut table = Table::new();
            
            // Set header names
            let headers: Vec<String> = query_schema.columns.iter().map(|c| c.name.clone()).collect();
            table.set_header(headers);
            
            let mut row_count = 0;
            while let Some(tuple) = physical_plan.next(&ctx)? {
                let row_vals: Vec<String> = tuple.values.iter().map(|v| format!("{}", v)).collect();
                table.add_row(row_vals);
                row_count += 1;
            }
            
            physical_plan.close(&ctx)?;
            
            if row_count > 0 {
                println!("{}", table);
            }
            println!("{} row(s) in set.", row_count);
        }
    }
    Ok(())
}

fn setup_demo_data(catalog: &mut Catalog, tables: &mut HashMap<String, Vec<Tuple>>) -> Result<()> {
    // 1. Create a users table
    // Users: id INT, name VARCHAR, age INT, score FLOAT
    execute_sql(
        "CREATE TABLE users (id INT, name VARCHAR, age INT, score FLOAT);",
        catalog,
        tables,
    )?;

    // Insert mock users
    execute_sql(
        "INSERT INTO users VALUES 
         (1, 'Alice', 25, 95.5), 
         (2, 'Bob', 30, 88.0), 
         (3, 'Charlie', 22, 92.0),
         (4, 'David', 30, 75.5),
         (5, 'Eva', 25, 99.0);",
        catalog,
        tables,
    )?;

    // 2. Create a categories table
    // Categories: id INT, category_name VARCHAR
    execute_sql(
        "CREATE TABLE categories (id INT, category_name VARCHAR);",
        catalog,
        tables,
    )?;

    execute_sql(
        "INSERT INTO categories VALUES
         (10, 'Electronics'),
         (20, 'Books'),
         (30, 'Clothing');",
        catalog,
        tables,
    )?;

    // 3. Create a products table
    // Products: id INT, product_name VARCHAR, category_id INT
    execute_sql(
        "CREATE TABLE products (id INT, product_name VARCHAR, category_id INT);",
        catalog,
        tables,
    )?;

    execute_sql(
        "INSERT INTO products VALUES 
         (501, 'Laptop', 10), 
         (502, 'Phone', 10), 
         (503, 'Rust Programming Book', 20),
         (504, 'T-Shirt', 30);",
        catalog,
        tables,
    )?;

    // 4. Create an orders table
    // Orders: id INT, user_id INT, amount FLOAT
    execute_sql(
        "CREATE TABLE orders (id INT, user_id INT, product_id INT, amount FLOAT);",
        catalog,
        tables,
    )?;

    // Insert mock orders
    execute_sql(
        "INSERT INTO orders VALUES 
         (101, 1, 501, 1500.0), -- Alice bought Laptop (Electronics)
         (102, 2, 503, 88.0),   -- Bob bought Rust Book (Books)
         (103, 1, 504, 45.0),   -- Alice bought T-Shirt (Clothing)
         (104, 3, 502, 999.0),  -- Charlie bought Phone (Electronics)
         (105, 9, 501, 999.0);", // user_id 9 is non-existent to test join filtration
        catalog,
        tables,
    )?;

    println!("\x1b[32mDemo data loaded successfully.\x1b[0m");
    println!("Tables:");
    println!("  - users (id, name, age, score)");
    println!("  - categories (id, category_name)");
    println!("  - products (id, product_name, category_id)");
    println!("  - orders (id, user_id, product_id, amount)\n");
    Ok(())
}
