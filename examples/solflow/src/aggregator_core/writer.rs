//! Unified writer interface for enriched metrics
//!
//! Routes writes to either JSONL or SQLite backend based on configuration.

use super::jsonl_writer::EnrichedMetricsWriter;
use super::sqlite_writer::SqliteAggregatorWriter;
use super::writer_backend::{AggregatorWriterBackend, AggregatorWriterError};
use crate::streamer_core::config::BackendType;
use std::path::PathBuf;

// Re-export EnrichedMetrics from jsonl_writer
pub use super::jsonl_writer::EnrichedMetrics;

/// Unified writer that routes to either JSONL or SQLite backend
pub enum AggregatorWriter {
    Jsonl(EnrichedMetricsWriter),
    Sqlite(SqliteAggregatorWriter),
}

impl AggregatorWriter {
    /// Create a new aggregator writer based on backend type
    pub fn new(backend: BackendType, base_path: PathBuf) -> Result<Self, AggregatorWriterError> {
        match backend {
            BackendType::Jsonl => {
                let writer = EnrichedMetricsWriter::new(base_path)?;
                Ok(AggregatorWriter::Jsonl(writer))
            }
            BackendType::Sqlite => {
                let writer = SqliteAggregatorWriter::new(base_path)?;
                Ok(AggregatorWriter::Sqlite(writer))
            }
        }
    }
    
    /// Write enriched metrics to the configured backend
    pub async fn write_metrics(&mut self, metrics: &EnrichedMetrics) -> Result<(), AggregatorWriterError> {
        match self {
            AggregatorWriter::Jsonl(w) => {
                w.write_metrics(metrics)?;
                Ok(())
            },
            AggregatorWriter::Sqlite(w) => w.write_metrics(metrics).await,
        }
    }
    
    /// Flush pending writes to storage
    pub async fn flush(&mut self) -> Result<(), AggregatorWriterError> {
        match self {
            AggregatorWriter::Jsonl(w) => {
                w.flush()?;
                Ok(())
            },
            AggregatorWriter::Sqlite(w) => w.flush().await,
        }
    }
    
    /// Get backend type for logging
    pub fn backend_type(&self) -> &'static str {
        match self {
            AggregatorWriter::Jsonl(_) => "JSONL",
            AggregatorWriter::Sqlite(_) => "SQLite",
        }
    }
}
