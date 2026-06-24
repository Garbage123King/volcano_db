use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Int,
    Float,
    Varchar,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub columns: Vec<Column>,
}

impl Schema {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    /// Finds a column's index by name. Supports both fully-qualified and simple names.
    pub fn find_col_idx(&self, name: &str) -> Option<usize> {
        if name.contains('.') {
            // Qualified name: must match exactly
            self.columns.iter().position(|col| col.name == name)
        } else {
            // Unqualified name: match the base name of the columns
            self.columns.iter().position(|col| {
                let col_base = col.name.split('.').last().unwrap_or(&col.name);
                col_base == name
            })
        }
    }
}

pub struct Catalog {
    pub tables: HashMap<String, Schema>,
}

/*
目录;一览表;系列;种类;产品样本;
*/

impl Catalog {
    pub fn newaaa() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn add_table(&mut self, name: String, schema: Schema) {
        self.tables.insert(name, schema);
    }

    pub fn get_schema(&self, name: &str) -> Option<&Schema> {
        self.tables.get(name)
    }
}
