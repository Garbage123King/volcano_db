# Volcano DB

An educational, SQL-compliant, in-memory relational database query execution engine built in Rust. It utilizes the **Volcano (iterator-based) physical execution model** and leverages `sqlparser-rs` with **PostgreSQL dialect** support for SQL parsing and binding. Now with **Oracle-aligned transaction management**, **redo/undo logging**, and **crash recovery**.

```text
  _    __      __                      ____  ____  
 | |  / /___  / /________ _____  ____ / __ \/ __ ) 
 | | / / __ \/ / ___/ __ `/ __ \/ __ \/ / / / __  | 
 | |/ / /_/ / / /__/ /_/ / / / / /_/ / /_/ / /_/ /  
 |___/\____/_/\___/\__,_/_/ /_/\____/_____/_____/   
                                                    
```

---

## Features

### Query Engine

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

### Transaction Management (Oracle-Aligned)

- **System Change Number (SCN)**: Global monotonic logical clock advancing on every commit.
- **BEGIN / COMMIT / ROLLBACK**: Full transaction control with undo-based rollback.
- **Undo Segments**: In-memory undo records enabling transaction rollback (reverse-order replay).
- **Redo Log Buffer & LGWR**: Write-Ahead Logging — redo entries buffered in memory and flushed to `redo.log` on commit by the simulated LGWR (Log Writer).
- **Consistent Read (CR SCN)**: Statement-level read consistency — each `SELECT` captures the current SCN and only sees rows committed at or before that SCN.
- **Multi-Session Read Isolation**: Server-client architecture where concurrent sessions observe Read Committed isolation — uncommitted rows from one session are invisible to others.
- **Instance Recovery (Crash Recovery)**:
  - **Roll-Forward**: On startup, replays all redo log records to rebuild in-memory tables.
  - **Roll-Back**: Removes rows from transactions that never committed (no `Commit` record in redo log).
- **Crash Simulation**: `/crash` command terminates the server immediately without flushing the redo buffer, enabling recovery testing.

### Client-Server Architecture

- **TCP-based protocol**: Length-prefixed binary framing for client-server communication.
- **Concurrent sessions**: Each client connection runs in its own thread with a private session state.
- **Statement-level locking**: The global database state is locked only for the duration of a single SQL statement, allowing concurrent transaction execution across sessions.

---

## Architecture

```text
┌──────────────┐     TCP      ┌─────────────────────────────────────────┐
│  Client REPL │ ◄──────────► │            DB Server                    │
│  (client.rs) │              │                                         │
└──────────────┘              │  ┌─────────────┐  ┌──────────────────┐  │
                              │  │ Session Mgr │  │  DatabaseState   │  │
┌──────────────┐     TCP      │  │ (per-conn)  │  │  ├─ Catalog      │  │
│  Client REPL │ ◄──────────► │  │  session_id │  │  ├─ Tables       │  │
│  (client.rs) │              │  │  tx_id      │  │  └─ TxManager    │  │
└──────────────┘              │  │  cr_scn     │  │     ├─ SCN        │  │
                              │  └─────────────┘  │     ├─ UndoSegs   │  │
                              │                   │     ├─ RedoBuffer │  │
                              │                   │     └─ LGWR ──────┼──┼──► ./redo.log
                              │                   └──────────────────┘  │
                              └─────────────────────────────────────────┘
```

---

## Project Structure

### Core Engine

- `src/lib.rs`: Exposes module definitions.
- `src/storage.rs`: `Value` representations (with serde), `Tuple` with transaction metadata (`tx_id`, `scn`).
- `src/catalog.rs`: Table catalog tracking schema and qualified column resolution.
- `src/planner.rs`: Mappings from parsed SQL AST into logical plans.
- `src/executor.rs`: Physical Volcano operators with CR SCN visibility filtering.

### Transaction & Recovery

- `src/tx.rs`: `TransactionManager`, `SCN`, `UndoRecord`, `RedoRecord`, `LGWR` flush logic, visibility check.
- `src/recovery.rs`: Instance Recovery — Roll-Forward (replay redo) + Roll-Back (remove uncommitted).
- `src/session.rs`: Per-session state (`session_id`, `current_tx_id`, `cr_scn`).

### Networking

- `src/protocol.rs`: Length-prefixed TCP frame protocol (`write_frame` / `read_frame`).
- `src/server.rs`: TCP accept loop, per-connection thread spawning, startup recovery.
- `src/client.rs`: Interactive REPL client sending SQL over TCP.
- `src/handler.rs`: SQL statement dispatcher (transactions, CRUD, special commands).

### Entry Point

- `src/main.rs`: Mode dispatcher — `server` or `client` based on CLI argument.

### Tests & Examples

- `tests/integration_tests.rs`: Query engine integration tests (filter, agg, join, sort, limit).
- `tests/tx_tests.rs`: Transaction tests (rollback, commit, read isolation, crash recovery).
- `examples/multi_session_test.rs`: End-to-end multi-session read isolation demo.
- `examples/crash_test.rs`: End-to-end crash recovery demo.

---

## Getting Started

### Prerequisites

Ensure you have a recent version of the Rust compiler and cargo toolchain installed (`cargo 1.94.1` or higher).

### Compiling

Build the binaries with optimal compiler options:
```bash
cargo build --release
```

### Running the Server

Start the database server (default address `127.0.0.1:3208`):
```bash
cargo run -- server
# or with a custom address:
cargo run -- server 127.0.0.1:15432
```

At startup, the server automatically:
1. Checks for `./redo.log` — if found and non-empty, performs **Instance Recovery** (Roll-Forward + Roll-Back).
2. Loads four mock datasets (if no recovery needed or to supplement recovered data):
   - **`users`**: `id` (INT), `name` (VARCHAR), `age` (INT), `score` (FLOAT)
   - **`categories`**: `id` (INT), `category_name` (VARCHAR)
   - **`products`**: `id` (INT), `product_name` (VARCHAR), `category_id` (INT)
   - **`orders`**: `id` (INT), `user_id` (INT), `product_id` (INT), `amount` (FLOAT)

### Running the Client

Connect a client to the running server:
```bash
cargo run -- client
# or with a custom address:
cargo run -- client 127.0.0.1:15432
```

---

## Usage

### SQL Statements

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

### Transaction Control

```sql
-- Begin a transaction
BEGIN;

-- Insert data (visible only to this session until commit)
INSERT INTO users VALUES (6, 'Frank', 40, 85.0);

-- Query sees the uncommitted row (same session)
SELECT name FROM users;

-- Commit makes it visible to all sessions
COMMIT;
```

```sql
-- Rollback removes the inserted row
BEGIN;
INSERT INTO users VALUES (7, 'Grace', 28, 90.0);
ROLLBACK;

-- Grace is gone
SELECT name FROM users;
```

### Multi-Session Read Isolation

Open two client terminals connected to the same server:

```text
# Terminal A                          # Terminal B
volcano_db> BEGIN;                    volcano_db> SELECT name FROM users;
volcano_db> INSERT INTO users         -- 5 rows (cannot see Frank yet)
  VALUES (6, 'Frank', 40, 85.0);
                                      volcano_db> SELECT name FROM users;
volcano_db> SELECT name FROM users;   -- 5 rows (Frank invisible)
-- 6 rows (sees own uncommitted)

volcano_db> COMMIT;
                                      volcano_db> SELECT name FROM users;
                                      -- 6 rows (Frank now visible)
```

### Special Commands

| Command   | Description                                                      |
|-----------|------------------------------------------------------------------|
| `/status` | Display current session, transaction ID, CR SCN, and global SCN. |
| `/crash`  | Simulate a crash — server exits immediately without flushing redo buffer. |

### Crash Recovery Demo

```bash
# Terminal A: Start server
cargo run -- server 127.0.0.1:15432

# Terminal B: Connect client, commit one row, leave another uncommitted, then crash
cargo run -- client 127.0.0.1:15432

volcano_db> BEGIN;
volcano_db> INSERT INTO users VALUES (7, 'Grace', 28, 90.0);
volcano_db> COMMIT;          -- Grace's redo is flushed to disk

volcano_db> BEGIN;
volcano_db> INSERT INTO users VALUES (8, 'Henry', 35, 77.0);
volcano_db> /crash           -- Server crashes; Henry's redo is lost (in buffer)

# Restart server — it will perform Instance Recovery
cargo run -- server 127.0.0.1:15432

# Grace is recovered (committed), Henry is gone (uncommitted)
```

---

## Verifying Correctness

Execute the full test suite:
```bash
cargo test
```

### Query Engine Tests (`tests/integration_tests.rs`)

- `test_simple_select_filter_project`: Simple filters and SELECT lists.
- `test_global_aggregates`: Aggregate sum and count calculations.
- `test_group_by_order_by`: Grouped aggregations and descending ordering.
- `test_joins`: Nested-loop inner join correctness.
- `test_limit_offset`: Tuple pagination offsets.
- `test_joins_with_aliases`: Join with table aliases in all ON-clause orderings.

### Transaction Tests (`tests/tx_tests.rs`)

- `test_rollback_removes_inserted_rows`: INSERT inside transaction + ROLLBACK removes the row.
- `test_commit_persists_rows`: INSERT inside transaction + COMMIT persists the row.
- `test_read_isolation_between_sessions`: Uncommitted rows invisible to other sessions; visible after commit.
- `test_crash_recovery_uncommitted`: Redo log replay correctly reconstructs committed data.

### End-to-End Examples

```bash
# Multi-session read isolation demo (requires running server)
cargo run --example multi_session_test

# Crash recovery demo (requires running server)
cargo run --example crash_test
```
