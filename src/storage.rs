use std::fmt;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Varchar(String),
    Bool(bool),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "NULL"),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Varchar(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tuple {
    pub values: Vec<Value>,
}

impl Tuple {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }
}

/*
* 如果使用#[derive(PartialEq, PartialOrd)]：
* 1、只有当两个枚举变体完全相同时，才会去比较它们包裹的值。
* 2、因为 Int 排在 Float 的前面，所以在派生出来的比较逻辑里，任何 Int 都绝对小于任何 Float。
*/

// 手动实现 两个值是否相等
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            // 核心：处理 Int 和 Float 的混合比较
            (&Value::Int(a), &Value::Float(b)) => (a as f64) == b,
            (&Value::Float(a), &Value::Int(b)) => a == (b as f64),
            (Value::Varchar(a), Value::Varchar(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            _ => false, // 类型不同且无法转换的，一律不相等
        }
    }
}

// 手动实现 两个值谁大谁小
impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Null, Value::Null) => Some(Ordering::Equal),
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            // 核心：将 Int 强转为 f64 再与 Float 比较大小
            (&Value::Int(a), &Value::Float(b)) => (a as f64).partial_cmp(&b),
            (&Value::Float(a), &Value::Int(b)) => a.partial_cmp(&(b as f64)),
            (Value::Varchar(a), Value::Varchar(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            // 混合类型（如 Int 和 Varchar）无法比较大小
            _ => None, 
        }
    }
}