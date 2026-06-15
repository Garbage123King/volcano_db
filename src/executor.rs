use crate::catalog::{Catalog, Schema};
use crate::storage::{Value, Tuple};
use crate::planner::{Expr, LogicalPlan};
use anyhow::{Result, anyhow};
use std::collections::HashMap;

pub struct ExecutionContext<'a> {
    pub tables: &'a HashMap<String, Vec<Tuple>>,
}

pub trait Executor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()>;
    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>>;
    fn close(&mut self, ctx: &ExecutionContext) -> Result<()>;
}

// ==========================================
// DummyScanExecutor
// ==========================================
pub struct DummyScanExecutor {
    done: bool,
}

impl DummyScanExecutor {
    pub fn new() -> Self {
        Self { done: false }
    }
}

impl Executor for DummyScanExecutor {
    fn init(&mut self, _ctx: &ExecutionContext) -> Result<()> {
        self.done = false;
        Ok(())
    }

    fn next(&mut self, _ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        if !self.done {
            self.done = true;
            Ok(Some(Tuple::new(vec![])))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self, _ctx: &ExecutionContext) -> Result<()> {
        Ok(())
    }
}

// ==========================================
// SeqScanExecutor
// ==========================================
pub struct SeqScanExecutor {
    table_name: String,
    cursor: usize,
}

impl SeqScanExecutor {
    pub fn new(table_name: String) -> Self {
        Self {
            table_name,
            cursor: 0,
        }
    }
}

impl Executor for SeqScanExecutor {
    fn init(&mut self, _ctx: &ExecutionContext) -> Result<()> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        if let Some(tuples) = ctx.tables.get(&self.table_name) {
            if self.cursor < tuples.len() {
                let t = tuples[self.cursor].clone();
                self.cursor += 1;
                Ok(Some(t))
            } else {
                Ok(None)
            }
        } else {
            Err(anyhow!("Table not found in storage: {}", self.table_name))
        }
    }

    fn close(&mut self, _ctx: &ExecutionContext) -> Result<()> {
        Ok(())
    }
}

// ==========================================
// FilterExecutor
// ==========================================
pub struct FilterExecutor {
    child: Box<dyn Executor>,
    predicate: Expr,
    schema: Schema,
}

impl FilterExecutor {
    pub fn new(child: Box<dyn Executor>, predicate: Expr, schema: Schema) -> Self {
        Self {
            child,
            predicate,
            schema,
        }
    }
}

impl Executor for FilterExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.init(ctx)
    }

    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        loop {
            if let Some(tuple) = self.child.next(ctx)? {
                let val = self.predicate.eval(&tuple, &self.schema)?;
                if val.is_truthy() {
                    return Ok(Some(tuple));
                }
            } else {
                return Ok(None);
            }
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.close(ctx)
    }
}

// ==========================================
// ProjectExecutor
// ==========================================
pub struct ProjectExecutor {
    child: Box<dyn Executor>,
    exprs: Vec<Expr>,
    child_schema: Schema,
}

impl ProjectExecutor {
    pub fn new(child: Box<dyn Executor>, exprs: Vec<Expr>, child_schema: Schema) -> Self {
        Self {
            child,
            exprs,
            child_schema,
        }
    }
}

impl Executor for ProjectExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.init(ctx)
    }

    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        if let Some(tuple) = self.child.next(ctx)? {
            let mut projected_vals = Vec::new();
            for expr in &self.exprs {
                projected_vals.push(expr.eval(&tuple, &self.child_schema)?);
            }
            Ok(Some(Tuple::new(projected_vals)))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.close(ctx)
    }
}

// ==========================================
// LimitExecutor
// ==========================================
pub struct LimitExecutor {
    child: Box<dyn Executor>,
    limit: Option<usize>,
    offset: Option<usize>,
    cursor: usize,
}

impl LimitExecutor {
    pub fn new(child: Box<dyn Executor>, limit: Option<usize>, offset: Option<usize>) -> Self {
        Self {
            child,
            limit,
            offset,
            cursor: 0,
        }
    }
}

impl Executor for LimitExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.init(ctx)?;
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        let offset = self.offset.unwrap_or(0);
        
        // Skip offset tuples
        while self.cursor < offset {
            if self.child.next(ctx)?.is_some() {
                self.cursor += 1;
            } else {
                return Ok(None);
            }
        }

        // Limit check
        if let Some(limit_val) = self.limit {
            if self.cursor - offset >= limit_val {
                return Ok(None);
            }
        }

        if let Some(tuple) = self.child.next(ctx)? {
            self.cursor += 1;
            Ok(Some(tuple))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.close(ctx)
    }
}

// ==========================================
// SortExecutor
// ==========================================
pub struct SortExecutor {
    child: Box<dyn Executor>,
    order_by: Vec<(Expr, bool)>,
    schema: Schema,
    sorted_tuples: Vec<Tuple>,
    cursor: usize,
}

impl SortExecutor {
    pub fn new(child: Box<dyn Executor>, order_by: Vec<(Expr, bool)>, schema: Schema) -> Self {
        Self {
            child,
            order_by,
            schema,
            sorted_tuples: Vec::new(),
            cursor: 0,
        }
    }
}

impl Executor for SortExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.init(ctx)?;
        self.sorted_tuples.clear();
        self.cursor = 0;

        // Materialize all child tuples
        while let Some(tuple) = self.child.next(ctx)? {
            self.sorted_tuples.push(tuple);
        }

        // Sort in-memory
        let order_by = &self.order_by;
        let schema = &self.schema;
        self.sorted_tuples.sort_by(|a, b| {
            for (expr, asc) in order_by {
                let val_a = expr.eval(a, schema).unwrap_or(Value::Null);
                let val_b = expr.eval(b, schema).unwrap_or(Value::Null);
                let ord = val_a.partial_cmp(&val_b).unwrap_or(std::cmp::Ordering::Equal);
                if ord != std::cmp::Ordering::Equal {
                    return if *asc { ord } else { ord.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(())
    }

    fn next(&mut self, _ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        if self.cursor < self.sorted_tuples.len() {
            let t = self.sorted_tuples[self.cursor].clone();
            self.cursor += 1;
            Ok(Some(t))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.sorted_tuples.clear();
        self.child.close(ctx)
    }
}

// ==========================================
// NestedLoopJoinExecutor
// ==========================================
pub struct NestedLoopJoinExecutor {
    left: Box<dyn Executor>,
    right: Box<dyn Executor>,
    condition: Option<Expr>,
    combined_schema: Schema,
    
    // Execution state
    right_tuples: Vec<Tuple>,
    left_tuple: Option<Tuple>,
    right_cursor: usize,
}

impl NestedLoopJoinExecutor {
    pub fn new(
        left: Box<dyn Executor>,
        right: Box<dyn Executor>,
        condition: Option<Expr>,
        combined_schema: Schema,
    ) -> Self {
        Self {
            left,
            right,
            condition,
            combined_schema,
            right_tuples: Vec::new(),
            left_tuple: None,
            right_cursor: 0,
        }
    }
}

impl Executor for NestedLoopJoinExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.left.init(ctx)?;
        self.right.init(ctx)?;
        self.right_tuples.clear();
        self.right_cursor = 0;

        // Caching right relation
        while let Some(tuple) = self.right.next(ctx)? {
            self.right_tuples.push(tuple);
        }

        self.left_tuple = self.left.next(ctx)?;
        Ok(())
    }

    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        loop {
            let l_tuple = match &self.left_tuple {
                Some(t) => t,
                None => return Ok(None),
            };

            if self.right_cursor >= self.right_tuples.len() {
                // Advance left outer relation, reset inner cursor
                self.left_tuple = self.left.next(ctx)?;
                self.right_cursor = 0;
                continue;
            }

            let r_tuple = &self.right_tuples[self.right_cursor];
            self.right_cursor += 1;

            // Build combined tuple
            let mut combined_vals = l_tuple.values.clone();
            combined_vals.extend(r_tuple.values.clone());
            let combined_tuple = Tuple::new(combined_vals);

            // Verify join condition
            if let Some(cond) = &self.condition {
                let match_val = cond.eval(&combined_tuple, &self.combined_schema)?;
                if match_val.is_truthy() {
                    return Ok(Some(combined_tuple));
                }
            } else {
                // Cross Product
                return Ok(Some(combined_tuple));
            }
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.left_tuple = None;
        self.right_tuples.clear();
        self.left.close(ctx)?;
        self.right.close(ctx)?;
        Ok(())
    }
}

// ==========================================
// HashAggExecutor
// ==========================================
#[derive(Clone, Debug)]
pub enum AggState {
    Count { count: i64 },
    Sum { sum: Value },
    Avg { sum: Value, count: i64 },
    Min { min: Value },
    Max { max: Value },
}

impl AggState {
    pub fn new(func_name: &str) -> Self {
        match func_name {
            "count" => AggState::Count { count: 0 },
            "sum" => AggState::Sum { sum: Value::Null },
            "avg" => AggState::Avg { sum: Value::Null, count: 0 },
            "min" => AggState::Min { min: Value::Null },
            "max" => AggState::Max { max: Value::Null },
            _ => panic!("Unknown aggregate function"),
        }
    }

    pub fn update(&mut self, val: &Value) {
        match self {
            AggState::Count { count } => {
                if val != &Value::Null {
                    *count += 1;
                }
            }
            AggState::Sum { sum } => {
                if val != &Value::Null {
                    match sum {
                        Value::Null => *sum = val.clone(),
                        Value::Int(s) => {
                            if let Value::Int(v) = val {
                                *sum = Value::Int(*s + *v);
                            } else if let Value::Float(v) = val {
                                *sum = Value::Float(*s as f64 + *v);
                            }
                        }
                        Value::Float(s) => {
                            if let Value::Float(v) = val {
                                *sum = Value::Float(*s + *v);
                            } else if let Value::Int(v) = val {
                                *sum = Value::Float(*s + *v as f64);
                            }
                        }
                        _ => {}
                    }
                }
            }
            AggState::Avg { sum, count } => {
                if val != &Value::Null {
                    *count += 1;
                    match sum {
                        Value::Null => *sum = val.clone(),
                        Value::Int(s) => {
                            if let Value::Int(v) = val {
                                *sum = Value::Int(*s + *v);
                            } else if let Value::Float(v) = val {
                                *sum = Value::Float(*s as f64 + *v);
                            }
                        }
                        Value::Float(s) => {
                            if let Value::Float(v) = val {
                                *sum = Value::Float(*s + *v);
                            } else if let Value::Int(v) = val {
                                *sum = Value::Float(*s + *v as f64);
                            }
                        }
                        _ => {}
                    }
                }
            }
            AggState::Min { min } => {
                if val != &Value::Null {
                    match min {
                        Value::Null => *min = val.clone(),
                        _ => {
                            if val < min {
                                *min = val.clone();
                            }
                        }
                    }
                }
            }
            AggState::Max { max } => {
                if val != &Value::Null {
                    match max {
                        Value::Null => *max = val.clone(),
                        _ => {
                            if val > max {
                                *max = val.clone();
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn finalize(&self) -> Value {
        match self {
            AggState::Count { count } => Value::Int(*count),
            AggState::Sum { sum } => sum.clone(),
            AggState::Avg { sum, count } => {
                if *count == 0 {
                    Value::Null
                } else {
                    match sum {
                        Value::Int(s) => Value::Float(*s as f64 / *count as f64),
                        Value::Float(s) => Value::Float(*s / *count as f64),
                        _ => Value::Null,
                    }
                }
            }
            AggState::Min { min } => min.clone(),
            AggState::Max { max } => max.clone(),
        }
    }
}

pub struct HashAggExecutor {
    child: Box<dyn Executor>,
    group_by: Vec<Expr>,
    agg_funcs: Vec<(String, Expr, String)>,
    child_schema: Schema,
    
    // Hash groups
    groups: HashMap<String, Vec<AggState>>,
    group_keys: HashMap<String, Vec<Value>>,
    result_keys: Vec<String>,
    cursor: usize,
}

impl HashAggExecutor {
    pub fn new(
        child: Box<dyn Executor>,
        group_by: Vec<Expr>,
        agg_funcs: Vec<(String, Expr, String)>,
        child_schema: Schema,
    ) -> Self {
        Self {
            child,
            group_by,
            agg_funcs,
            child_schema,
            groups: HashMap::new(),
            group_keys: HashMap::new(),
            result_keys: Vec::new(),
            cursor: 0,
        }
    }
}

fn make_group_key(vals: &[Value]) -> String {
    vals.iter().map(|v| match v {
        Value::Null => "null".to_string(),
        Value::Int(i) => format!("i:{}", i),
        Value::Float(f) => format!("f:{}", f),
        Value::Varchar(s) => format!("s:{}", s),
        Value::Bool(b) => format!("b:{}", b),
    }).collect::<Vec<_>>().join(",")
}

impl Executor for HashAggExecutor {
    fn init(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.child.init(ctx)?;
        self.groups.clear();
        self.group_keys.clear();
        self.result_keys.clear();
        self.cursor = 0;

        let mut child_has_tuples = false;
        while let Some(tuple) = self.child.next(ctx)? {
            child_has_tuples = true;
            let group_vals: Vec<Value> = self.group_by.iter()
                .map(|expr| expr.eval(&tuple, &self.child_schema))
                .collect::<Result<Vec<_>>>()?;

            let key = make_group_key(&group_vals);
            let states = self.groups.entry(key.clone()).or_insert_with(|| {
                self.agg_funcs.iter().map(|(func, _, _)| AggState::new(func)).collect::<Vec<_>>()
            });

            self.group_keys.insert(key, group_vals);

            for (i, (_, arg_expr, _)) in self.agg_funcs.iter().enumerate() {
                let val = if arg_expr == &Expr::Star {
                    Value::Int(1)
                } else {
                    arg_expr.eval(&tuple, &self.child_schema)?
                };
                states[i].update(&val);
            }
        }

        // Handle global aggregation on empty relation
        if !child_has_tuples && self.group_by.is_empty() {
            let key = "".to_string();
            let states = self.agg_funcs.iter().map(|(func, _, _)| AggState::new(func)).collect::<Vec<_>>();
            self.groups.insert(key.clone(), states);
            self.group_keys.insert(key, vec![]);
        }

        self.result_keys = self.groups.keys().cloned().collect();
        Ok(())
    }

    fn next(&mut self, _ctx: &ExecutionContext) -> Result<Option<Tuple>> {
        if self.cursor < self.result_keys.len() {
            let key = &self.result_keys[self.cursor];
            self.cursor += 1;

            let group_vals = self.group_keys.get(key).unwrap();
            let states = self.groups.get(key).unwrap();

            let mut final_vals = group_vals.clone();
            for state in states {
                final_vals.push(state.finalize());
            }
            Ok(Some(Tuple::new(final_vals)))
        } else {
            Ok(None)
        }
    }

    fn close(&mut self, ctx: &ExecutionContext) -> Result<()> {
        self.groups.clear();
        self.group_keys.clear();
        self.result_keys.clear();
        self.child.close(ctx)
    }
}

// ==========================================
// Build physical executor from logical plan
// ==========================================
pub fn build_executor(plan: &LogicalPlan, catalog: &Catalog) -> Result<Box<dyn Executor>> {
    match plan {
        LogicalPlan::DummyScan => Ok(Box::new(DummyScanExecutor::new())),
        LogicalPlan::Scan { table_name } => Ok(Box::new(SeqScanExecutor::new(table_name.clone()))),
        LogicalPlan::Filter { child, predicate } => {
            let child_exec = build_executor(child, catalog)?;
            let child_schema = child.schema(catalog)?;
            Ok(Box::new(FilterExecutor::new(child_exec, predicate.clone(), child_schema)))
        }
        LogicalPlan::Project { child, exprs } => {
            let child_exec = build_executor(child, catalog)?;
            let child_schema = child.schema(catalog)?;
            let proj_exprs = exprs.iter().map(|(e, _)| e.clone()).collect();
            Ok(Box::new(ProjectExecutor::new(child_exec, proj_exprs, child_schema)))
        }
        LogicalPlan::Limit { child, limit, offset } => {
            let child_exec = build_executor(child, catalog)?;
            Ok(Box::new(LimitExecutor::new(child_exec, *limit, *offset)))
        }
        LogicalPlan::Sort { child, order_by } => {
            let child_exec = build_executor(child, catalog)?;
            let child_schema = child.schema(catalog)?;
            Ok(Box::new(SortExecutor::new(child_exec, order_by.clone(), child_schema)))
        }
        LogicalPlan::Join { left, right, condition } => {
            let left_exec = build_executor(left, catalog)?;
            let right_exec = build_executor(right, catalog)?;
            let combined_schema = plan.schema(catalog)?;
            Ok(Box::new(NestedLoopJoinExecutor::new(
                left_exec,
                right_exec,
                condition.clone(),
                combined_schema,
            )))
        }
        LogicalPlan::Agg { child, group_by, agg_funcs } => {
            let child_exec = build_executor(child, catalog)?;
            let child_schema = child.schema(catalog)?;
            Ok(Box::new(HashAggExecutor::new(
                child_exec,
                group_by.clone(),
                agg_funcs.clone(),
                child_schema,
            )))
        }
    }
}
