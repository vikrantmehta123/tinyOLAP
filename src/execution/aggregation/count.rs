use std::{fmt, sync::Arc};

use arrow::{array::{ArrayRef, RecordBatch, UInt64Array}, datatypes::{DataType, Field, Schema}};

use crate::{execution::executor::{ExecutionError, ExecutionPlan}};



pub struct CountExec {
    count: u64, 
    column_name: String,
    emitted: bool, 
    child: Box<dyn ExecutionPlan>, 
    output_schema: Arc<Schema>,
}

impl CountExec {
    pub fn new(column_name: String, child: Box<dyn ExecutionPlan>) -> Self {
        let output_field = Field::new(
            format!("count({})", column_name), 
            DataType::UInt64,
            false
        );

        let output_schema = Arc::new(Schema::new(vec![output_field]));

        
        Self {
            count: 0, 
            column_name, 
            emitted: false, 
            child, 
            output_schema
        }
    }
}

impl fmt::Display for CountExec{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl ExecutionPlan for CountExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        if self.emitted {
            return None;
        }  

        loop {
            let batch = match self.child.next_batch() {
                None             => break,
                Some(Ok(b))      => b,
                Some(Err(e))     => return Some(Err(e)),
            };  

            self.count += batch.num_rows() as u64;
        } 

        // Build the one-row output batch.

        let arr: ArrayRef = Arc::new(UInt64Array::from(vec![self.count]));
        let batch = match RecordBatch::try_new(self.output_schema.clone(), vec![arr]) {
            Ok(b)  => b,
            Err(e) => return Some(Err(e.into())),
        };

        self.emitted = true;
        Some(Ok(batch))


    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Count({})", indent, self.column_name)?;
        self.child.fmt_indented(f, depth + 1)
    }
}