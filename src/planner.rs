use crate::catalog::{Catalog, Schema, Column, DataType};
use crate::storage::{Value, Tuple};
use anyhow::{Result, anyhow};

use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::{
    BinaryOperator as SqlBinaryOperator, DataType as SqlDataType, Expr as SqlExpr,
    FunctionArg, FunctionArgExpr, JoinConstraint, JoinOperator, Query,
    SelectItem, SetExpr, Statement, TableFactor, Value as SqlValue,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    ColRef(String),
    Const(Value),
    BinaryOp {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FuncCall {
        name: String,
        arg: Box<Expr>,
    },
    Star,
}

impl Expr {
    pub fn eval(&self, tuple: &Tuple, schema: &Schema) -> Result<Value> {
        match self {
            Expr::Star => Err(anyhow!("Cannot evaluate Star expression directly")),
            Expr::Const(val) => Ok(val.clone()),
            Expr::ColRef(name) => {
                if let Some(idx) = schema.find_col_idx(name) {
                    Ok(tuple.values[idx].clone())
                } else {
                    Err(anyhow!(
                        "Column not found in schema: {} (available columns: {:?})",
                        name,
                        schema.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
                    ))
                }
            }
            Expr::BinaryOp { op, left, right } => {
                let l_val = left.eval(tuple, schema)?;
                let r_val = right.eval(tuple, schema)?;
                eval_binary_op(op, l_val, r_val)
            }
            Expr::FuncCall { name, arg } => {
                Err(anyhow!(
                    "Aggregate function {}({}) cannot be evaluated directly outside Aggregation operator",
                    name,
                    get_expr_name(arg)
                ))
            }
        }
    }
}

pub fn eval_binary_op(op: &str, left: Value, right: Value) -> Result<Value> {
    match op {
        "=" | "==" => Ok(Value::Bool(left == right)),
        "!=" | "<>" => Ok(Value::Bool(left != right)),
        ">" => Ok(Value::Bool(left > right)),
        ">=" => Ok(Value::Bool(left >= right)),
        "<" => Ok(Value::Bool(left < right)),
        "<=" => Ok(Value::Bool(left <= right)),
        "+" => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
            _ => Err(anyhow!("Invalid types for +")),
        },
        "-" => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - b as f64)),
            _ => Err(anyhow!("Invalid types for -")),
        },
        "*" => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * b as f64)),
            _ => Err(anyhow!("Invalid types for *")),
        },
        "/" => match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    Err(anyhow!("Division by zero"))
                } else {
                    Ok(Value::Int(a / b))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / b as f64)),
            _ => Err(anyhow!("Invalid types for /")),
        },
        "and" => Ok(Value::Bool(left.is_truthy() && right.is_truthy())),
        "or" => Ok(Value::Bool(left.is_truthy() || right.is_truthy())),
        _ => Err(anyhow!("Unsupported operator: {}", op)),
    }
}

pub fn get_expr_type(expr: &Expr, schema: &Schema) -> Result<DataType> {
    match expr {
        Expr::Const(val) => match val {
            Value::Int(_) => Ok(DataType::Int),
            Value::Float(_) => Ok(DataType::Float),
            Value::Varchar(_) => Ok(DataType::Varchar),
            Value::Bool(_) => Ok(DataType::Bool),
            Value::Null => Ok(DataType::Varchar),
        },
        Expr::ColRef(name) => {
            if let Some(idx) = schema.find_col_idx(name) {
                Ok(schema.columns[idx].data_type.clone())
            } else {
                Err(anyhow!("Column not found in schema: {}", name))
            }
        }
        Expr::BinaryOp { op, left, right } => {
            let lt = get_expr_type(left, schema)?;
            let rt = get_expr_type(right, schema)?;
            match op.as_str() {
                "=" | "==" | "!=" | "<>" | ">" | ">=" | "<" | "<=" | "and" | "or" => Ok(DataType::Bool),
                "+" | "-" | "*" | "/" => {
                    match (lt, rt) {
                        (DataType::Float, _) | (_, DataType::Float) => Ok(DataType::Float),
                        (DataType::Int, DataType::Int) => Ok(DataType::Int),
                        _ => Err(anyhow!("Arithmetic operations only supported on numeric types")),
                    }
                }
                _ => Err(anyhow!("Unsupported operator: {}", op)),
            }
        }
        Expr::FuncCall { name, arg } => {
            match name.as_str() {
                "count" => Ok(DataType::Int),
                "sum" | "avg" => {
                    let arg_t = get_expr_type(arg, schema)?;
                    match arg_t {
                        DataType::Int => if name == "avg" { Ok(DataType::Float) } else { Ok(DataType::Int) },
                        DataType::Float => Ok(DataType::Float),
                        _ => Err(anyhow!("Cannot aggregate non-numeric type: {:?}", arg_t)),
                    }
                }
                "min" | "max" => get_expr_type(arg, schema),
                _ => Err(anyhow!("Unsupported aggregate function: {}", name)),
            }
        }
        Expr::Star => Err(anyhow!("Star expression has no direct data type")),
    }
}

pub fn get_expr_name(expr: &Expr) -> String {
    match expr {
        Expr::Const(val) => format!("{}", val),
        Expr::ColRef(name) => name.clone(),
        Expr::BinaryOp { op, left, right } => format!("({} {} {})", get_expr_name(left), op, get_expr_name(right)),
        Expr::FuncCall { name, arg } => format!("{}({})", name, get_expr_name(arg)),
        Expr::Star => "*".to_string(),
    }
}

#[derive(Debug, Clone)]
pub enum LogicalPlan {
    DummyScan,
    Scan {
        table_name: String,
        alias: Option<String>,
    },
    Filter {
        child: Box<LogicalPlan>,
        predicate: Expr,
    },
    Project {
        child: Box<LogicalPlan>,
        exprs: Vec<(Expr, String)>,
    },
    Limit {
        child: Box<LogicalPlan>,
        limit: Option<usize>,
        offset: Option<usize>,
    },
    Sort {
        child: Box<LogicalPlan>,
        order_by: Vec<(Expr, bool)>,
    },
    Join {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        condition: Option<Expr>,
    },
    Agg {
        child: Box<LogicalPlan>,
        group_by: Vec<Expr>,
        agg_funcs: Vec<(String, Expr, String)>, // (func_name, arg_expr, generated_name)
    },
}

impl LogicalPlan {
    pub fn schema(&self, catalog: &Catalog) -> Result<Schema> {
        match self {
            LogicalPlan::DummyScan => Ok(Schema::new(vec![])),
            LogicalPlan::Scan { table_name, alias } => {
                let s = catalog.get_schema(table_name)
                    .ok_or_else(|| anyhow!("Table not found in catalog: {}", table_name))?;
                let qualifier = alias.as_ref().unwrap_or(table_name);
                let qualified_cols = s.columns.iter().map(|col| {
                    Column {
                        name: if col.name.contains('.') {
                            col.name.clone()
                        } else {
                            format!("{}.{}", qualifier, col.name)
                        },
                        data_type: col.data_type.clone(),
                    }
                }).collect();
                Ok(Schema::new(qualified_cols))
            }
            LogicalPlan::Filter { child, .. } => child.schema(catalog),
            LogicalPlan::Limit { child, .. } => child.schema(catalog),
            LogicalPlan::Sort { child, .. } => child.schema(catalog),
            LogicalPlan::Project { exprs, child } => {
                let child_schema = child.schema(catalog)?;
                let mut columns = Vec::new();
                for (expr, alias) in exprs {
                    let dtype = get_expr_type(expr, &child_schema)?;
                    columns.push(Column {
                        name: alias.clone(),
                        data_type: dtype,
                    });
                }
                Ok(Schema::new(columns))
            }
            LogicalPlan::Join { left, right, .. } => {
                let left_s = left.schema(catalog)?;
                let right_s = right.schema(catalog)?;
                let mut columns = left_s.columns;
                columns.extend(right_s.columns);
                Ok(Schema::new(columns))
            }
            LogicalPlan::Agg { group_by, agg_funcs, child } => {
                let child_schema = child.schema(catalog)?;
                let mut columns = Vec::new();
                for expr in group_by {
                    let name = get_expr_name(expr);
                    let dtype = get_expr_type(expr, &child_schema)?;
                    columns.push(Column { name, data_type: dtype });
                }
                for (func_name, arg_expr, alias) in agg_funcs {
                    let dtype = match func_name.as_str() {
                        "count" => DataType::Int,
                        "sum" | "avg" => {
                            let child_type = get_expr_type(arg_expr, &child_schema)?;
                            match child_type {
                                DataType::Int => if func_name == "avg" { DataType::Float } else { DataType::Int },
                                DataType::Float => DataType::Float,
                                _ => return Err(anyhow!("Cannot aggregate non-numeric type")),
                            }
                        }
                        "min" | "max" => get_expr_type(arg_expr, &child_schema)?,
                        _ => return Err(anyhow!("Unsupported aggregate function: {}", func_name)),
                    };
                    columns.push(Column {
                        name: alias.clone(),
                        data_type: dtype,
                    });
                }
                Ok(Schema::new(columns))
            }
        }
    }
}

#[derive(Debug)]
pub enum SQLStatement {
    CreateTable {
        table_name: String,
        schema: Schema,
    },
    Insert {
        table_name: String,
        rows: Vec<Vec<Expr>>,
    },
    Query(LogicalPlan),
}

pub fn bind_statement(sql: &str, catalog: &Catalog) -> Result<SQLStatement> {
    let dialect = PostgreSqlDialect {};
    let mut ast = Parser::parse_sql(&dialect, sql)?;
    if ast.is_empty() {
        return Err(anyhow!("Empty SQL query"));
    }
    let statement = ast.remove(0);
    
    match statement {
        Statement::CreateTable(create_table) => {
            let table_name = create_table.name.to_string();
            let mut cols = Vec::new();
            for col in create_table.columns {
                let col_name = col.name.value.clone();
                let dtype = match &col.data_type {
                    SqlDataType::Int(..) | SqlDataType::Integer(..) => DataType::Int,
                    SqlDataType::Float(..) | SqlDataType::Double | SqlDataType::DoublePrecision | SqlDataType::Real => DataType::Float,
                    SqlDataType::Varchar(..) | SqlDataType::Text | SqlDataType::String(..) => DataType::Varchar,
                    SqlDataType::Boolean | SqlDataType::Bool => DataType::Bool,
                    other => return Err(anyhow!("Unsupported column datatype: {:?}", other)),
                };
                cols.push(Column { name: col_name, data_type: dtype });
            }
            Ok(SQLStatement::CreateTable {
                table_name,
                schema: Schema::new(cols),
            })
        }
        Statement::Insert(insert_stmt) => {
            let table_name_str = insert_stmt.table_name.to_string();
            let query = insert_stmt.source.as_ref().ok_or_else(|| anyhow!("Missing source in INSERT"))?;
            
            let mut rows = Vec::new();
            if let SetExpr::Values(values) = &*query.body {
                for row in &values.rows {
                    let mut expr_row = Vec::new();
                    for expr in row {
                        expr_row.push(map_sql_expr(expr)?);
                    }
                    rows.push(expr_row);
                }
            } else {
                return Err(anyhow!("Only INSERT INTO ... VALUES is supported"));
            }
            
            Ok(SQLStatement::Insert {
                table_name: table_name_str,
                rows,
            })
        }
        Statement::Query(query) => {
            let logical_plan = bind_query(*query, catalog)?;
            Ok(SQLStatement::Query(logical_plan))
        }
        other => Err(anyhow!("Unsupported SQL statement: {:?}", other)),
    }
}

fn map_sql_expr(sql_expr: &SqlExpr) -> Result<Expr> {
    match sql_expr {
        SqlExpr::Identifier(ident) => Ok(Expr::ColRef(ident.value.clone())),
        SqlExpr::CompoundIdentifier(idents) => {
            let name = idents
                .iter()
                .map(|id| id.value.as_str())
                .collect::<Vec<_>>()
                .join(".");
            Ok(Expr::ColRef(name))
        }
        SqlExpr::Value(val) => {
            let v = match val {
                SqlValue::Number(num_str, _) => {
                    if num_str.contains('.') {
                        let f: f64 = num_str.parse()?;
                        Value::Float(f)
                    } else {
                        let i: i64 = num_str.parse()?;
                        Value::Int(i)
                    }
                }
                SqlValue::SingleQuotedString(s) | SqlValue::DoubleQuotedString(s) => Value::Varchar(s.clone()),
                SqlValue::Boolean(b) => Value::Bool(*b),
                SqlValue::Null => Value::Null,
                _ => return Err(anyhow!("Unsupported SQL literal: {:?}", val)),
            };
            Ok(Expr::Const(v))
        }
        SqlExpr::BinaryOp { left, op, right } => {
            let op_str = match op {
                SqlBinaryOperator::Plus => "+",
                SqlBinaryOperator::Minus => "-",
                SqlBinaryOperator::Multiply => "*",
                SqlBinaryOperator::Divide => "/",
                SqlBinaryOperator::Eq => "=",
                SqlBinaryOperator::NotEq => "!=",
                SqlBinaryOperator::Gt => ">",
                SqlBinaryOperator::GtEq => ">=",
                SqlBinaryOperator::Lt => "<",
                SqlBinaryOperator::LtEq => "<=",
                SqlBinaryOperator::And => "and",
                SqlBinaryOperator::Or => "or",
                _ => return Err(anyhow!("Unsupported operator: {:?}", op)),
            };
            Ok(Expr::BinaryOp {
                op: op_str.to_string(),
                left: Box::new(map_sql_expr(left)?),
                right: Box::new(map_sql_expr(right)?),
            })
        }
        SqlExpr::Function(func) => {
            let func_name = func.name.to_string().to_lowercase();
            if !matches!(func_name.as_str(), "count" | "sum" | "avg" | "min" | "max") {
                return Err(anyhow!("Unsupported function: {}", func_name));
            }
            let arg = match &func.args {
                sqlparser::ast::FunctionArguments::None => Expr::Star,
                sqlparser::ast::FunctionArguments::Subquery(_) => {
                    return Err(anyhow!("Subqueries in function arguments are not supported"));
                }
                sqlparser::ast::FunctionArguments::List(arg_list) => {
                    if arg_list.args.is_empty() {
                        Expr::Star
                    } else {
                        match &arg_list.args[0] {
                            FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => Expr::Star,
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(inner)) => map_sql_expr(inner)?,
                            _ => return Err(anyhow!("Unsupported function argument type")),
                        }
                    }
                }
            };

            Ok(Expr::FuncCall {
                name: func_name,
                arg: Box::new(arg),
            })
        }
        _ => Err(anyhow!("Unsupported SQL expression: {:?}", sql_expr)),
    }
}

fn bind_query(query: Query, catalog: &Catalog) -> Result<LogicalPlan> {
    let select = match *query.body {
        SetExpr::Select(s) => s,
        other => return Err(anyhow!("Unsupported query body: {:?}", other)),
    };
    
    // 1. Bind FROM
    let mut plan = if select.from.is_empty() {
        LogicalPlan::DummyScan
    } else {
        let mut curr_plan = bind_table_factor(&select.from[0].relation)?;
        for join in &select.from[0].joins {
            let right_plan = bind_table_factor(&join.relation)?;
            let cond = match &join.join_operator {
                JoinOperator::Inner(JoinConstraint::On(expr)) => Some(map_sql_expr(expr)?),
                JoinOperator::CrossJoin => None,
                other => return Err(anyhow!("Unsupported join operator: {:?}", other)),
            };
            curr_plan = LogicalPlan::Join {
                left: Box::new(curr_plan),
                right: Box::new(right_plan),
                condition: cond,
            };
        }
        curr_plan
    };

    let before_proj_schema = plan.schema(catalog)?;

    // 2. Bind WHERE (selection)
    if let Some(selection) = select.selection {
        let predicate = map_sql_expr(&selection)?;
        plan = LogicalPlan::Filter {
            child: Box::new(plan),
            predicate,
        };
    }

    // 3. Bind target projections
    let mut target_exprs = Vec::new();
    for select_item in select.projection {
        match select_item {
            SelectItem::UnnamedExpr(expr) => {
                let mapped = map_sql_expr(&expr)?;
                let name = get_expr_name(&mapped);
                target_exprs.push((mapped, name));
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                let mapped = map_sql_expr(&expr)?;
                target_exprs.push((mapped, alias.value.clone()));
            }
            SelectItem::Wildcard(..) => {
                for col in &before_proj_schema.columns {
                    target_exprs.push((Expr::ColRef(col.name.clone()), col.name.clone()));
                }
            }
            other => return Err(anyhow!("Unsupported select item: {:?}", other)),
        }
    }

    // 4. Bind GROUP BY
    let mut group_by = Vec::new();
    match select.group_by {
        sqlparser::ast::GroupByExpr::Expressions(exprs, _) => {
            for expr in exprs {
                group_by.push(map_sql_expr(&expr)?);
            }
        }
        sqlparser::ast::GroupByExpr::All(_) => {
            return Err(anyhow!("GROUP BY ALL is not supported"));
        }
    }

    let is_agg_query = !group_by.is_empty() || target_exprs.iter().any(|(expr, _)| has_agg_funcs(expr));

    if is_agg_query {
        let mut agg_funcs = Vec::new();
        let mut rewritten_proj = Vec::new();
        for (expr, alias) in target_exprs {
            let rewritten = extract_aggregates(&expr, &mut agg_funcs);
            rewritten_proj.push((rewritten, alias));
        }

        plan = LogicalPlan::Agg {
            child: Box::new(plan),
            group_by,
            agg_funcs,
        };

        plan = LogicalPlan::Project {
            child: Box::new(plan),
            exprs: rewritten_proj,
        };
    } else {
        plan = LogicalPlan::Project {
            child: Box::new(plan),
            exprs: target_exprs,
        };
    }

    // 5. Bind ORDER BY
    if let Some(order_by_struct) = &query.order_by {
        if !order_by_struct.exprs.is_empty() {
            let mut order_by = Vec::new();
            for order_by_expr in &order_by_struct.exprs {
                let expr = map_sql_expr(&order_by_expr.expr)?;
                let asc = order_by_expr.asc.unwrap_or(true);
                order_by.push((expr, asc));
            }
            plan = LogicalPlan::Sort {
                child: Box::new(plan),
                order_by,
            };
        }
    }

    // 6. Bind LIMIT / OFFSET
    let mut limit = None;
    if let Some(limit_expr) = query.limit {
        let mapped = map_sql_expr(&limit_expr)?;
        if let Expr::Const(Value::Int(i)) = mapped {
            limit = Some(i as usize);
        }
    }
    
    let mut offset = None;
    if let Some(offset_expr) = query.offset {
        let mapped = map_sql_expr(&offset_expr.value)?;
        if let Expr::Const(Value::Int(i)) = mapped {
            offset = Some(i as usize);
        }
    }

    if limit.is_some() || offset.is_some() {
        plan = LogicalPlan::Limit {
            child: Box::new(plan),
            limit,
            offset,
        };
    }

    Ok(plan)
}

fn bind_table_factor(tf: &TableFactor) -> Result<LogicalPlan> {
    match tf {
        TableFactor::Table { name, alias, .. } => {
            let alias_str = alias.as_ref().map(|a| a.name.value.clone());
            Ok(LogicalPlan::Scan {
                table_name: name.to_string(),
                alias: alias_str,
            })
        }
        other => Err(anyhow!("Unsupported FROM table factor: {:?}", other)),
    }
}

fn has_agg_funcs(expr: &Expr) -> bool {
    match expr {
        Expr::FuncCall { name, .. } => {
            matches!(name.as_str(), "count" | "sum" | "avg" | "min" | "max")
        }
        Expr::BinaryOp { left, right, .. } => has_agg_funcs(left) || has_agg_funcs(right),
        _ => false,
    }
}

fn extract_aggregates(
    expr: &Expr,
    agg_funcs: &mut Vec<(String, Expr, String)>,
) -> Expr {
    match expr {
        Expr::FuncCall { name, arg } if matches!(name.as_str(), "count" | "sum" | "avg" | "min" | "max") => {
            let gen_name = format!("{}({})", name, get_expr_name(arg));
            let exists = agg_funcs.iter().any(|(_, _, name)| name == &gen_name);
            if !exists {
                agg_funcs.push((name.clone(), *arg.clone(), gen_name.clone()));
            }
            Expr::ColRef(gen_name)
        }
        Expr::BinaryOp { op, left, right } => {
            let new_left = extract_aggregates(left, agg_funcs);
            let new_right = extract_aggregates(right, agg_funcs);
            Expr::BinaryOp {
                op: op.clone(),
                left: Box::new(new_left),
                right: Box::new(new_right),
            }
        }
        other => other.clone(),
    }
}
