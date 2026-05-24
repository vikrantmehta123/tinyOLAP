use std::fmt;
use arrow::array::RecordBatch;

#[derive(Debug)]
pub enum ExecutionError {
    Io(std::io::Error),
    InvalidData(String),
    Arrow(arrow::error::ArrowError),
}

pub trait ExecutionPlan: fmt::Display {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>>;
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result;
}

impl From<std::io::Error> for ExecutionError {
    fn from(e: std::io::Error) -> Self { ExecutionError::Io(e) }
}

impl From<arrow::error::ArrowError> for ExecutionError {
    fn from(e: arrow::error::ArrowError) -> Self { ExecutionError::Arrow(e) }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionError::Io(e)          => write!(f, "I/O error: {}", e),
            ExecutionError::InvalidData(m) => write!(f, "invalid data: {}", m),
            ExecutionError::Arrow(e)       => write!(f, "arrow error: {}", e),
        }
    }
}

impl std::error::Error for ExecutionError {}
