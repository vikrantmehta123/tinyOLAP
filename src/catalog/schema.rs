//! Type definitions for tinyOLAP's type system
//! This is the single source of truth for the type system.
//! All other layers will import from here.
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Bool,
    Str,
}

impl DataType {
    /// Stable on-disk tag for this type. Used in zone maps and other
    /// type-erased on-disk structures. Values must not change once written.
    pub fn type_tag(&self) -> u8 {
        match self {
            DataType::I8   => 1,
            DataType::I16  => 2,
            DataType::I32  => 3,
            DataType::I64  => 4,
            DataType::U8   => 5,
            DataType::U16  => 6,
            DataType::U32  => 7,
            DataType::U64  => 8,
            DataType::F32  => 9,
            DataType::F64  => 10,
            DataType::Bool => 11,
            DataType::Str  => 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnSchema>,
    pub sort_key: Vec<usize>,
}

impl TableSchema {
    pub fn create(dir: &Path, def: &TableSchema) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let schema_path = dir.join("schema.json");
        let json = serde_json::to_string_pretty(def)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(schema_path, json)
    }

    pub fn open(dir: &Path) -> std::io::Result<TableSchema> {
        let schema_path = dir.join("schema.json");
        let json = fs::read_to_string(schema_path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn part_dir(table_dir: &Path, part_id: u32) -> PathBuf {
        table_dir.join(format!("part_{:05}", part_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_round_trip() {
        let dir = std::env::temp_dir().join("tinyolap_schema_test");
        let _ = std::fs::remove_dir_all(&dir);

        let def = TableSchema {
            name: "events".to_string(),
            columns: vec![
                ColumnSchema { name: "timestamp".to_string(), data_type: DataType::I64 },
                ColumnSchema { name: "user_id".to_string(),   data_type: DataType::U32 },
                ColumnSchema { name: "is_active".to_string(), data_type: DataType::Bool },
                ColumnSchema { name: "label".to_string(),     data_type: DataType::Str },
            ],
            sort_key: vec![0, 1],
        };

        TableSchema::create(&dir, &def).unwrap();
        let opened = TableSchema::open(&dir).unwrap();

        assert_eq!(opened.name, "events");
        assert_eq!(opened.columns.len(), 4);
        assert_eq!(opened.columns[0].name, "timestamp");
        assert!(matches!(opened.columns[0].data_type, DataType::I64));
        assert_eq!(opened.sort_key, vec![0, 1]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
