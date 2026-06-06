use tinyolap::catalog::schema::{ColumnSchema, DataType, TableSchema};

/// One column per supported type. Shared by every test directory.
pub fn wide_schema() -> TableSchema {
    use DataType::*;
    TableSchema {
        name: "wide".to_string(),
        columns: vec![
            ColumnSchema { name: "ts".to_string(),    data_type: I64 },
            ColumnSchema { name: "i8_c".to_string(),  data_type: I8 },
            ColumnSchema { name: "i16_c".to_string(), data_type: I16 },
            ColumnSchema { name: "i32_c".to_string(), data_type: I32 },
            ColumnSchema { name: "u8_c".to_string(),  data_type: U8 },
            ColumnSchema { name: "u16_c".to_string(), data_type: U16 },
            ColumnSchema { name: "u32_c".to_string(), data_type: U32 },
            ColumnSchema { name: "u64_c".to_string(), data_type: U64 },
            ColumnSchema { name: "f32_c".to_string(), data_type: F32 },
            ColumnSchema { name: "f64_c".to_string(), data_type: F64 },
            ColumnSchema { name: "flag".to_string(),  data_type: Bool },
            ColumnSchema { name: "label".to_string(), data_type: Str },
        ],
        sort_key: vec![0],
    }
}
