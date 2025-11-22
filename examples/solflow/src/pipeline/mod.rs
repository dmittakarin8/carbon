//! # Aggregate-Only Aggregator (Phase 1 Scaffolding)
//!
//! This module will implement an in-memory rolling-window aggregator that:
//! - Processes trade events from streamers (no raw trade storage)
//! - Maintains 60s/300s/900s rolling windows per token
//! - Computes aggregate metrics (net flow, counts, unique wallets)
//! - Detects signals (BREAKOUT, FOCUSED, SURGE, BOT_DROPOFF)
//! - Writes to SQLite: token_aggregates, token_signals
//!
//! ## Architecture: Aggregate-Only System
//!
//! **Key Principle:** Raw trades are NEVER persisted to disk.
//!
//! Instead:
//! 1. Trade events arrive from streamers (in-memory only)
//! 2. Rolling windows maintain recent trades (60s, 300s, 900s)
//! 3. Aggregates are computed periodically from windows
//! 4. Only aggregated metrics are written to SQLite
//! 5. Old trades are evicted from memory after window expires
//!
//! Benefits:
//! - Minimal disk I/O (no raw trade writes)
//! - Constant memory footprint (bounded windows)
//! - Fast queries (pre-aggregated data)
//! - Historical analysis via signal events (not raw trades)
//!
//! ## Phase 1 Status (CURRENT)
//!
//! - ✅ Type definitions and trait signatures
//! - ❌ NO logic implementation (all TODO markers)
//! - ❌ NOT integrated into runtime (unused code)
//!
//! **This is scaffolding only.** No operational code exists in Phase 1.
//!
//! ## Phase 2 (Next Steps)
//!
//! Phase 2 will implement:
//! - Rolling window logic (add_trade, evict_old_trades)
//! - Aggregate computation (net flow, counts, averages)
//! - Signal detection (threshold-based rules)
//! - SQLite writer (AggregateDbWriter implementation)
//! - Integration with existing aggregator binary
//!
//! ## Phase 3 (Future)
//!
//! - Wire into runtime (replace JSONL-based aggregator)
//! - Add real-time metrics emission
//! - Performance optimization (batch writes, caching)
//! - Advanced signal detection (ML-based anomalies)
//!
//! ## Schema Reference
//!
//! All types match SQL schema in `/sql/`:
//! - `token_aggregates` → `AggregatedTokenState`
//! - `token_signals` → `TokenSignal`
//! - `mint_blocklist` → `BlocklistProvider` trait
//!
//! See `/sql/readme.md` for agent rules and schema documentation.
//!
//! ## Module Organization
//!
//! - `types` - Core data structures (TradeEvent, AggregatedTokenState)
//! - `state` - Per-token rolling state container
//! - `windows` - Rolling window trait definitions
//! - `db` - Database writer trait
//! - `signals` - Signal type definitions
//! - `blocklist` - Blocklist checking trait

pub mod types;
pub mod state;
pub mod windows;
pub mod db;
pub mod signals;
pub mod blocklist;
pub mod engine;
pub mod config;
pub mod ingestion;
pub mod dexscreener;
pub mod persistence_scorer;
// Note: scheduler module removed in Phase 4.3 - unified flush loop now handles all periodic tasks

// Re-export commonly used types
pub use types::{TradeEvent, TradeDirection, AggregatedTokenState};
pub use signals::{SignalType, TokenSignal};
pub use state::TokenRollingState;
pub use windows::{RollingWindow, WindowManager};
pub use db::AggregateDbWriter;
pub use blocklist::BlocklistProvider;
pub use engine::PipelineEngine;
pub use config::PipelineConfig;
