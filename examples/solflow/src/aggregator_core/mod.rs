//! Aggregator Core - Multi-Stream Correlation Engine
//!
//! This module provides the infrastructure for correlating multiple trade streams
//! (PumpSwap, BonkSwap, Jupiter DCA, etc.) to detect accumulation patterns and uptrend signals.
//!
//! # Architecture
//!
//! ```text
//! SQLite Database → SqliteTradeReader → TimeWindowAggregator
//!     ↓
//! CorrelationEngine (mint + ±60s timestamp join)
//!     ↓
//! SignalScorer (uptrend_score, dca_overlap_pct)
//!     ↓
//! SignalDetector (UPTREND, ACCUMULATION thresholds)
//!     ↓
//! AggregatorWriter → JSONL or SQLite backend
//! ```

pub mod correlator;
pub mod detector;
pub mod normalizer;
pub mod sqlite_reader;
pub mod scorer;
pub mod window;
pub mod writer_backend;
pub mod jsonl_writer;
pub mod sqlite_writer;
pub mod writer;

pub use correlator::CorrelationEngine;
pub use detector::SignalDetector;
pub use normalizer::{Trade, TradeAction};
pub use sqlite_reader::SqliteTradeReader;
pub use scorer::SignalScorer;
pub use window::{TimeWindowAggregator, WindowMetrics, WindowSize};
pub use writer_backend::{AggregatorWriterBackend, AggregatorWriterError};
pub use jsonl_writer::EnrichedMetricsWriter;
pub use sqlite_writer::SqliteAggregatorWriter;
pub use writer::{AggregatorWriter, EnrichedMetrics};
