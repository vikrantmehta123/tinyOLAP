//! Type definitions for tinyOLAP's type system
//! This is the single source of truth for the type system.
//! All other layers will import from here.

#[derive(Debug, Clone)]
pub enum DataType {
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Bool,
    Str,
}

#[derive(Debug, Clone)]
pub struct ColumnSchema {
    pub name: String, 
    pub data_type: DataType,
}

#[derive(Debug, Clone)]
pub struct TableSchema {
    pub name: String, 
    pub columns: Vec<ColumnSchema>,
}