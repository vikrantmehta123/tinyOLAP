//! FullScan reads every part in the table and returns one Batch per part.
//! Parts are read in parallel with rayon because each part is an independent
//! directory — no shared mutable state, so no coordination is needed.

use rayon::prelude::*;
use std::path::{Path, PathBuf};

use crate::storage::{
    column_chunk::ColumnChunk,
    column_reader::ColumnReader,
    part_discovery::discover_parts,
    schema::{ColumnDef, DataType, TableDef},
    string_column_reader::StringColumnReader,
};

use super::{
    batch::Batch,
    processor::{ExecutionError, Processor},
};

/// Read one part's columns into a Batch. Shared by FullScan and ZoneMapScan.
pub fn read_part(part_dir: &Path, columns: &[ColumnDef]) -> Result<Batch, ExecutionError> {
    let mut cols = Vec::with_capacity(columns.len());
    for col in columns {
        let chunk = match col.data_type {
            DataType::I8   => ColumnChunk::I8(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::I16  => ColumnChunk::I16(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::I32  => ColumnChunk::I32(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::I64  => ColumnChunk::I64(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::U8   => ColumnChunk::U8(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::U16  => ColumnChunk::U16(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::U32  => ColumnChunk::U32(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::U64  => ColumnChunk::U64(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::F32  => ColumnChunk::F32(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::F64  => ColumnChunk::F64(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::Bool => ColumnChunk::Bool(ColumnReader::open(part_dir, &col.name)?.read_all()?),
            DataType::Str  => ColumnChunk::Str(StringColumnReader::open(part_dir, &col.name)?.read_all()?),
        };
        cols.push(chunk);
    }
    Ok(Batch { schema: columns.to_vec(), columns: cols })
}


pub struct FullScan {
    batches: Vec<Batch>, // reversed — pop() yields parts in ascending order
}

impl FullScan {
    pub fn new(table_dir: PathBuf, columns: Vec<ColumnDef>) -> Result<Self, ExecutionError> {
        let part_ids = discover_parts(&table_dir)?;

        let results: Vec<Result<Batch, ExecutionError>> = part_ids
            .par_iter()
            .map(|&part_id| {
                let part_dir = TableDef::part_dir(&table_dir, part_id);
                read_part(&part_dir, &columns)
            })
            .collect();

        let mut batches: Vec<Batch> = results.into_iter().collect::<Result<_, _>>()?;
        batches.reverse();

        Ok(Self { batches })
    }
}

impl Processor for FullScan {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>> {
        self.batches.pop().map(Ok)
    }
}
