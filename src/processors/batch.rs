use crate::storage::{column_chunk::ColumnChunk, schema::ColumnDef};


// A columnar batch: one slice of rows flowing through the execution pipeline.
// Invariant: schema.len() == columns.len() and all columns have the same row
// count.  In the FullScan path, each on-disk part becomes one Batch.
pub struct Batch {
    pub schema: Vec<ColumnDef>,
    pub columns: Vec<ColumnChunk>,
}
