use crate::streamer_core::output_writer::TradeEvent;
use async_trait::async_trait;

#[derive(Debug)]
pub enum WriterError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
    Database(String),
}

impl From<std::io::Error> for WriterError {
    fn from(err: std::io::Error) -> Self {
        WriterError::Io(err)
    }
}

impl From<serde_json::Error> for WriterError {
    fn from(err: serde_json::Error) -> Self {
        WriterError::Serialization(err)
    }
}

impl From<rusqlite::Error> for WriterError {
    fn from(err: rusqlite::Error) -> Self {
        WriterError::Database(err.to_string())
    }
}

impl std::fmt::Display for WriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriterError::Io(e) => write!(f, "IO error: {}", e),
            WriterError::Serialization(e) => write!(f, "Serialization error: {}", e),
            WriterError::Database(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for WriterError {}

#[async_trait]
pub trait WriterBackend: Send {
    /// Write a single trade event
    async fn write(&mut self, event: &TradeEvent) -> Result<(), WriterError>;
    
    /// Flush pending writes to storage
    async fn flush(&mut self) -> Result<(), WriterError>;
    
    /// Get backend type for logging
    fn backend_type(&self) -> &'static str;
}
