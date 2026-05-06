use super::batch::Batch;

#[derive(Debug)]
pub enum ExecutionError {
    Io(std::io::Error),
    InvalidData(String),
}

impl From<std::io::Error> for ExecutionError {
    fn from(e: std::io::Error) -> Self {
        ExecutionError::Io(e)
    }
}

// Pull-based execution model: the root drives the pipeline by calling
// next_batch() repeatedly.
//
// Return convention:
//   Some(Ok(batch))  — a batch is ready; call again for more
//   Some(Err(e))     — fatal error; the pipeline should be abandoned
//   None             — stream exhausted, no more data
pub trait Processor {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>>;
}
