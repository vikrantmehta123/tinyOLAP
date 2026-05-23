use arrow::array::RecordBatch;

#[derive(Debug)]
pub enum ExecutionError {
    Io(std::io::Error),
    InvalidData(String),
    Arrow(arrow::error::ArrowError)
}


pub trait ExecutionPlan {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>>;
}

impl From<std::io::Error> for ExecutionError {
    fn from(e: std::io::Error) -> Self { ExecutionError::Io(e) }
}

impl From<arrow::error::ArrowError> for ExecutionError {
    fn from(e: arrow::error::ArrowError) -> Self { ExecutionError::Arrow(e) }
}

impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionError::Io(e)           => write!(f, "I/O error: {}", e),
            ExecutionError::InvalidData(m)  => write!(f, "invalid data: {}", m),
            ExecutionError::Arrow(e)        => write!(f, "arrow error: {}", e),
        }
    }
}

impl std::error::Error for ExecutionError {}
