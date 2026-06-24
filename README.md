# Volcano DB

An educational, SQL-compliant, in-memory relational database query execution engine built in Rust. It utilizes the **Volcano (iterator-based) physical execution model** and leverages `sqlparser-rs` with **PostgreSQL dialect** support for SQL parsing and binding.

```text
  _    __      __                      ____  ____  
 | |  / /___  / /________ _____  ____ / __ \/ __ ) 
 | | / / __ \/ / ___/ __ `/ __ \/ __ \/ / / / __  | 
 | |/ / /_/ / / /__/ /_/ / / / / /_/ / /_/ / /_/ /  
 |___/\____/_/\___/\__,_/_/ /_/\____/_____/_____/   
                                                    
```

---

## Features

- **Volcano Execution Interface**: Every physical operator implements the classic Volcano iterator model:
  - `init()`: Reset execution state, open files, or pre-fetch child buffers.
  - `next()`: Pull a single `Tuple` from the operator, returning `Ok(None)` when exhaustion is reached.
  - `close()`: Terminate and release child resources.
- **In-Memory Catalog & Storage**: Memory-backed schema catalogs and tables mapping table names to collections of Tuples.
- **Robust SQL Dialect Parsing**: Fully supports PostgreSQL syntax (via `sqlparser-rs`), translating query trees into logical and physical plans.
- **Complex Query Capabilities**:
  - Filter Predicate Evaluation (`WHERE`)
  - Arithmetic Expressions on Column Projections (`SELECT a + b * 2`)
  - Global & Grouped Aggregations (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX` with optional `GROUP BY`)
  - Inner Joins & Cross Products (`JOIN ... ON ...`)
  - In-memory Sorting (`ORDER BY ... ASC/DESC`)
  - Row Limit and Pagination (`LIMIT / OFFSET`)
- **Interactive multi-line REPL**: Input queries across multiple lines, run with table visualization outputs via `comfy-table`.

---

## Project Structure

- `src/lib.rs`: Exposes module definitions.
- `src/storage.rs`: Defs for `Value` representations, arithmetic operations, and row comparisons.
- `src/catalog.rs`: Table catalog tracking schema and qualified column resolution.
- `src/planner.rs`: Mappings from parsed SQL AST into logical plans.
- `src/executor.rs`: Physical Volcano operators implementations.
- `src/main.rs`: Entry point containing the multi-line input CLI REPL loop.
- `tests/integration_tests.rs`: Integration tests suite verifying query plans correctness.

---

## Getting Started

### Prerequisites

Ensure you have a recent version of the Rust compiler and cargo toolchain installed (`cargo 1.94.1` or higher).

### Compiling

Build the binaries with optimal compiler options:
```bash
cargo build --release
```

### Running the Interactive REPL

Run the executable to launch the interactive terminal shell:
```bash
cargo run
```

At startup, the CLI automatically loads four mock datasets:
- **`users`**: columns `id` (INT), `name` (VARCHAR), `age` (INT), `score` (FLOAT)
- **`categories`**: columns `id` (INT), `category_name` (VARCHAR)
- **`products`**: columns `id` (INT), `product_name` (VARCHAR), `category_id` (INT)
- **`orders`**: columns `id` (INT), `user_id` (INT), `product_id` (INT), `amount` (FLOAT)

### Testing SQL Statements in the REPL

Terminating commands with a semicolon `;` allows writing multi-line SQL queries:

```sql
-- Join, Filter, Project, Order By, and Aliases
SELECT users.name, orders.amount * 1.05 AS adjusted_amount
FROM users JOIN orders ON users.id = orders.user_id
WHERE orders.amount > 50
ORDER BY orders.amount DESC;

-- Group by with Aggregates
SELECT age, COUNT(*), SUM(score) 
FROM users 
GROUP BY age 
ORDER BY age;

-- Join, Gourp by
SELECT u.name, u.age, COUNT(o.id) as order_count, SUM(o.amount) as total_spent
FROM users u
JOIN orders o ON u.id = o.user_id
WHERE u.age >= 25 AND o.amount > 50
GROUP BY u.name, u.age
ORDER BY total_spent DESC, order_count ASC
LIMIT 3;

-- Three joins
SELECT
  u.name,
  o.id AS order_id,
  p.product_name,
  c.category_name,
  o.amount
FROM orders o
       JOIN users u ON o.user_id = u.id
       JOIN products p ON o.product_id = p.id
       JOIN categories c ON p.category_id = c.id;
```

---

## Verifying Correctness

Execute the integration test suite to verify operator accuracy:
```bash
cargo test
```

Tests include:
- `test_simple_select_filter_project`: Evaluates simple filters and SELECT lists.
- `test_global_aggregates`: Checks aggregate sum and count calculations on empty/non-empty sets.
- `test_group_by_order_by`: Validates grouped aggregations and descending ordering correctness.
- `test_joins`: Asserts Nest-Loop inner join correctness.
- `test_limit_offset`: Validates tuple pagination offsets.
