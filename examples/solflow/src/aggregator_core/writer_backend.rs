//! Writer backend trait for enriched metrics
//!
//! Defines the interface for writing aggregated enrichment data to different backends.

use async_trait::async_trait;
use super::writer::EnrichedMetrics;

#[derive(Debug)]
pub enum AggregatorWriterError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
    Database(String),
}

impl From<std::io::Error> for AggregatorWriterError {
    fn from(err: std::io::Error) -> Self {
        AggregatorWriterError::Io(err)
    }
}

impl From<serde_json::Error> for AggregatorWriterError {
    fn from(err: serde_json::Error) -> Self {
        AggregatorWriterError::Serialization(err)
    }
}

impl std::fmt::Display for AggregatorWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggregatorWriterError::Io(e) => write!(f, "IO error: {}", e),
            AggregatorWriterError::Serialization(e) => write!(f, "Serialization error: {}", e),
            AggregatorWriterError::Database(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for AggregatorWriterError {}

/// Backend trait for writing enriched metrics
#[async_trait]
pub trait AggregatorWriterBackend: Send {
    /// Write a single enriched metrics entry
    async fn write_metrics(&mut self, metrics: &EnrichedMetrics) -> Result<(), AggregatorWriterError>;
    
    /// Flush pending writes to storage
    async fn flush(&mut self) -> Result<(), AggregatorWriterError>;
    
    /// Get backend type for logging
    fn backend_type(&self) -> &'static str;
}
