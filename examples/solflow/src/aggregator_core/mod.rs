//! Aggregator Core - Multi-Stream Correlation Engine
//!
//! This module provides the infrastructure for correlating multiple JSONL trade streams
//! (PumpSwap, BonkSwap, Jupiter DCA, etc.) to detect accumulation patterns and uptrend signals.
//!
//! # Architecture
//!
//! ```text
//! JSONL Streams → TailReader → Normalizer → TimeWindowAggregator
//!     ↓
//! CorrelationEngine (mint + ±60s timestamp join)
//!     ↓
//! SignalScorer (uptrend_score, dca_overlap_pct)
//!     ↓
//! SignalDetector (UPTREND, ACCUMULATION thresholds)
//!     ↓
//! EnrichedMetricsWriter → /streams/aggregates/{15m,1h,2h,4h}.jsonl
//! ```

pub mod correlator;
pub mod detector;
pub mod normalizer;
pub mod reader;
pub mod scorer;
pub mod window;
pub mod writer;

pub use correlator::CorrelationEngine;
pub use detector::SignalDetector;
pub use normalizer::{Trade, TradeAction};
pub use reader::TailReader;
pub use scorer::SignalScorer;
pub use window::{TimeWindowAggregator, WindowMetrics, WindowSize};
pub use writer::{EnrichedMetrics, EnrichedMetricsWriter};
