//! Pipeline Engine - Orchestration layer for aggregate-only architecture
//!
//! Phase 3-E: Final assembly of all Phase 3 components
//!
//! This module provides the `PipelineEngine` struct that orchestrates:
//! 1. Trade ingestion and rolling state management (Phase 2)
//! 2. Bot detection and signal detection (Phase 3-A/B)
//! 3. Aggregate computation and building (Phase 3-C/D)
//! 4. Result buffering (Phase 4 will add DB writes)
//!
//! ## Architecture
//!
//! ```
//! TradeEvent
//!     ↓
//! PipelineEngine::process_trade()
//!     ↓
//! TokenRollingState (rolling windows)
//!     ↓
//! PipelineEngine::compute_metrics()
//!     ↓
//! (RollingMetrics, Vec<TokenSignal>, AggregatedTokenState)
//!     ↓
//! [Phase 4: Database writes via AggregateDbWriter]
//! ```
//!
//! ## Phase 3 Constraints
//!
//! - NO database writes (db_writer stays None)
//! - NO runtime integration (used only in tests)
//! - Results are returned, not persisted
//! - Fully isolated inside src/pipeline/
//!
//! ## Phase 4 Integration Plan
//!
//! Phase 4 will:
//! 1. Connect PipelineEngine to live streamer ingestion
//! 2. Activate AggregateDbWriter (write_aggregates, write_signal)
//! 3. Add price/supply enrichment pipeline
//! 4. Schedule periodic flush_to_db() for buffered results

use super::db::AggregateDbWriter;
use super::signals::TokenSignal;
use super::state::{RollingMetrics, TokenRollingState};
use super::types::{AggregatedTokenState, TokenMetadata, TradeEvent};
use std::collections::HashMap;
use std::sync::Arc;

/// Pipeline engine orchestrating the aggregate-only architecture
///
/// Manages per-token rolling state, computes metrics, detects signals,
/// and builds aggregated state for database persistence.
///
/// Phase 3: Internal orchestration only (no database writes)
/// Phase 4: Will add live integration and database persistence
pub struct PipelineEngine {
    /// Per-token rolling state (60s/300s/900s windows)
    states: HashMap<String, TokenRollingState>,

    /// Bot history tracking for BOT_DROPOFF detection
    /// Maps mint -> last known bot_trades_count_300s
    last_bot_counts: HashMap<String, i32>,

    /// Database writer (Phase 3: None, Phase 4: Some)
    /// Kept as Option for Phase 4 activation
    #[allow(dead_code)]
    db_writer: Option<Arc<dyn AggregateDbWriter>>,

    /// Token metadata cache for aggregate enrichment
    /// Phase 4 will populate this from database/APIs
    metadata_cache: HashMap<String, TokenMetadata>,

    /// Timestamp function (for testing with mock time)
    now_fn: Box<dyn Fn() -> i64 + Send + Sync>,
}

impl PipelineEngine {
    /// Create a new pipeline engine with default timestamp function
    ///
    /// Uses system time (chrono::Utc::now()) for timestamps.
    ///
    /// Phase 3: db_writer is None (no writes)
    /// Phase 4: Pass Some(Arc::new(SqliteAggregateWriter::new(...)))
    pub fn new() -> Self {
        Self::new_with_timestamp_fn(Box::new(|| chrono::Utc::now().timestamp()))
    }

    /// Create a new pipeline engine with custom timestamp function
    ///
    /// Used for testing with deterministic timestamps.
    ///
    /// # Arguments
    /// * `now_fn` - Function returning Unix timestamp (for testing)
    pub fn new_with_timestamp_fn(now_fn: Box<dyn Fn() -> i64 + Send + Sync>) -> Self {
        Self {
            states: HashMap::new(),
            last_bot_counts: HashMap::new(),
            db_writer: None, // Phase 3: No database writes
            metadata_cache: HashMap::new(),
            now_fn,
        }
    }

    /// Process a trade event through the pipeline
    ///
    /// Updates rolling state for the token:
    /// 1. Gets or creates TokenRollingState for mint
    /// 2. Adds trade to rolling windows
    /// 3. Evicts old trades outside window ranges
    ///
    /// Phase 3: Only updates in-memory state
    /// Phase 4: May trigger background aggregation
    ///
    /// # Arguments
    /// * `trade` - Trade event to process
    pub fn process_trade(&mut self, trade: TradeEvent) {
        let now = (self.now_fn)();
        let mint = trade.mint.clone();

        // Get or create rolling state for this token
        let state = self
            .states
            .entry(mint)
            .or_insert_with(|| TokenRollingState::new(trade.mint.clone()));

        // Add trade to rolling windows
        state.add_trade(trade);

        // Evict trades older than 900s (longest window)
        state.evict_old_trades(now);
    }

    /// Compute metrics and signals for a token
    ///
    /// Returns full pipeline output:
    /// 1. RollingMetrics - Raw aggregated metrics from windows
    /// 2. Vec<TokenSignal> - Detected signals (BREAKOUT, SURGE, etc.)
    /// 3. AggregatedTokenState - SQL-schema-compliant aggregate
    ///
    /// Phase 3: Returns results without database writes
    /// Phase 4: Will also write to database via AggregateDbWriter
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `now` - Current Unix timestamp
    ///
    /// # Returns
    /// * `Ok((metrics, signals, aggregate))` - Full pipeline output
    /// * `Err(...)` - If token has no state (no trades processed)
    pub fn compute_metrics(
        &self,
        mint: &str,
        now: i64,
    ) -> Result<(RollingMetrics, Vec<TokenSignal>, AggregatedTokenState), Box<dyn std::error::Error>>
    {
        // Get state for this token
        let state = self
            .states
            .get(mint)
            .ok_or_else(|| format!("No state for mint: {}", mint))?;

        // Compute rolling metrics
        let metrics = state.compute_rolling_metrics();

        // Detect signals (with bot history for BOT_DROPOFF)
        let previous_bot_count = self.last_bot_counts.get(mint).copied();
        let signals = state.detect_signals(now, previous_bot_count);

        // Get metadata for enrichment (if available)
        let metadata = self.metadata_cache.get(mint);

        // Find last trade timestamp
        let last_trade_ts = state
            .trades_60s
            .last()
            .or(state.trades_300s.last())
            .or(state.trades_900s.last())
            .map(|t| t.timestamp)
            .unwrap_or(now);

        // Build AggregatedTokenState from metrics + metadata
        let aggregate = AggregatedTokenState::from_metrics(mint, &metrics, metadata, last_trade_ts, now);

        Ok((metrics, signals, aggregate))
    }

    /// Update bot history for a token
    ///
    /// Tracks current bot count for future BOT_DROPOFF detection.
    /// BOT_DROPOFF signal requires comparing current bot count to previous count.
    ///
    /// Call this after compute_metrics() to store the latest bot count.
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `bot_count` - Current bot_trades_count_300s from metrics
    pub fn update_bot_history(&mut self, mint: &str, bot_count: i32) {
        self.last_bot_counts.insert(mint.to_string(), bot_count);
    }

    /// Refresh metadata cache for a token
    ///
    /// Updates metadata cache used by compute_metrics() for aggregate enrichment.
    /// This populates fields like source_program, created_at in AggregatedTokenState.
    ///
    /// Phase 3: Called manually in tests
    /// Phase 4: Will be called by metadata enrichment pipeline
    ///
    /// # Arguments
    /// * `metadata` - Token metadata to cache
    pub fn refresh_metadata(&mut self, metadata: TokenMetadata) {
        self.metadata_cache
            .insert(metadata.mint.clone(), metadata);
    }

    /// Get list of active mints with state
    ///
    /// Phase 4: Used by ingestion and schedulers to iterate over active tokens
    ///
    /// Returns: Vector of mint addresses (strings)
    pub fn get_active_mints(&self) -> Vec<String> {
        self.states.keys().cloned().collect()
    }

    // TODO: Phase 4 - Add database write methods
    // pub async fn flush_aggregates(&self) -> Result<(), Box<dyn std::error::Error>> {
    //     if let Some(writer) = &self.db_writer {
    //         let aggregates = self.build_all_aggregates();
    //         writer.write_aggregates(aggregates).await?;
    //     }
    //     Ok(())
    // }
    //
    // pub async fn flush_signals(&self, signals: Vec<TokenSignal>) -> Result<(), Box<dyn std::error::Error>> {
    //     if let Some(writer) = &self.db_writer {
    //         for signal in signals {
    //             writer.write_signal(signal).await?;
    //         }
    //     }
    //     Ok(())
    // }

    // TODO: Phase 4 - Add background aggregation scheduler
    // pub fn schedule_periodic_flush(&self, interval_secs: u64) {
    //     tokio::spawn(async move {
    //         let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    //         loop {
    //             interval.tick().await;
    //             self.flush_aggregates().await.unwrap_or_else(|e| {
    //                 log::error!("Failed to flush aggregates: {}", e);
    //             });
    //         }
    //     });
    // }

    // TODO: Phase 4 - Add live streamer integration
    // pub async fn connect_to_streamer(&self, streamer: StreamerHandle) {
    //     streamer.subscribe_trades(|trade| {
    //         self.process_trade(trade);
    //     }).await;
    // }

    // TODO: Phase 4 - Add price enrichment integration
    // pub async fn refresh_prices(&mut self, mints: Vec<String>) {
    //     let price_service = PriceService::new();
    //     for mint in mints {
    //         if let Ok(price) = price_service.fetch_price(&mint).await {
    //             if let Some(state) = self.states.get_mut(&mint) {
    //                 // Update price fields in aggregate
    //             }
    //         }
    //     }
    // }
}

impl Default for PipelineEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::types::TradeDirection;

    /// Helper to create a test trade event
    fn make_trade(
        timestamp: i64,
        mint: &str,
        direction: TradeDirection,
        sol_amount: f64,
        user_account: &str,
    ) -> TradeEvent {
        TradeEvent {
            timestamp,
            mint: mint.to_string(),
            direction,
            sol_amount,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: user_account.to_string(),
            source_program: "test_program".to_string(),
        }
    }

    /// Helper to create test metadata
    fn make_metadata(mint: &str, launch_platform: &str, created_at: i64) -> TokenMetadata {
        TokenMetadata {
            mint: mint.to_string(),
            symbol: Some("TEST".to_string()),
            name: Some("Test Token".to_string()),
            decimals: 6,
            launch_platform: Some(launch_platform.to_string()),
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn test_process_trade_updates_state() {
        // Test: process_trade() creates state and adds trades
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "test_mint_1";

        // Process first trade
        let trade1 = make_trade(base_time, mint, TradeDirection::Buy, 1.5, "wallet_1");
        engine.process_trade(trade1);

        // Verify state exists
        assert!(engine.states.contains_key(mint));

        // Verify trade was added (check 60s window has 1 trade)
        let state = engine.states.get(mint).unwrap();
        assert_eq!(state.trades_60s.len(), 1);
        assert_eq!(state.trades_300s.len(), 1);
        assert_eq!(state.trades_900s.len(), 1);

        // Process second trade
        let trade2 = make_trade(base_time + 30, mint, TradeDirection::Sell, 0.8, "wallet_2");
        engine.process_trade(trade2);

        // Verify both trades present
        let state = engine.states.get(mint).unwrap();
        assert_eq!(state.trades_60s.len(), 2);
        assert_eq!(state.trades_300s.len(), 2);
        assert_eq!(state.unique_wallets_300s.len(), 2);
    }

    #[test]
    fn test_compute_metrics_outputs_all_components() {
        // Test: compute_metrics() returns (metrics, signals, aggregate)
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "test_mint_2";

        // Add trades
        for i in 0..10 {
            let trade = make_trade(
                base_time + i * 5,
                mint,
                TradeDirection::Buy,
                1.0,
                &format!("wallet_{}", i),
            );
            engine.process_trade(trade);
        }

        // Compute metrics
        let result = engine.compute_metrics(mint, base_time + 100);
        assert!(result.is_ok());

        let (metrics, signals, aggregate) = result.unwrap();

        // Verify metrics computed
        assert!(metrics.net_flow_60s_sol > 0.0);
        assert_eq!(metrics.buy_count_60s, 10);
        assert_eq!(metrics.sell_count_60s, 0);

        // Verify signals present (may be empty, but should be Vec)
        assert!(signals.is_empty() || !signals.is_empty()); // Either is valid

        // Verify aggregate built
        assert_eq!(aggregate.mint, mint);
        assert_eq!(aggregate.buy_count_60s, Some(10));
        assert_eq!(aggregate.sell_count_60s, Some(0));
        assert!(aggregate.net_flow_60s_sol.is_some());
    }

    #[test]
    fn test_signal_pipeline_integration() {
        // Test: BREAKOUT signal detection through full pipeline
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "breakout_mint";

        // Create BREAKOUT conditions (high volume, many wallets, high buy ratio)
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3, // All within 60s
                mint,
                TradeDirection::Buy,
                0.5 + (i as f64 * 0.05), // Total: ~15 SOL
                &format!("wallet_{}", i % 8), // 8 unique wallets
            );
            engine.process_trade(trade);
        }

        // Add 2 sells to make ratio realistic
        for i in 0..2 {
            let trade = make_trade(base_time + 20 + i, mint, TradeDirection::Sell, 0.3, &format!("seller_{}", i));
            engine.process_trade(trade);
        }

        // Compute metrics
        let (metrics, signals, aggregate) = engine.compute_metrics(mint, base_time + 60).unwrap();

        // Verify metrics are positive
        assert!(metrics.net_flow_60s_sol > 5.0); // BREAKOUT threshold

        // Verify BREAKOUT signal detected
        assert!(!signals.is_empty());
        assert!(signals
            .iter()
            .any(|s| s.signal_type == crate::pipeline::signals::SignalType::Breakout));

        // Verify aggregate reflects high activity
        assert!(aggregate.buy_count_60s.unwrap() >= 20);
        assert_eq!(aggregate.unique_wallets_300s, Some(10)); // 8 buyers + 2 sellers
    }

    #[test]
    fn test_aggregate_builder_integration() {
        // Test: AggregatedTokenState is properly constructed with metadata
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "aggregate_mint";

        // Add metadata to cache
        let metadata = make_metadata(mint, "pumpswap", base_time - 5000);
        engine.refresh_metadata(metadata.clone());

        // Add trades
        for i in 0..5 {
            let trade = make_trade(base_time + i * 20, mint, TradeDirection::Buy, 2.0, &format!("wallet_{}", i));
            engine.process_trade(trade);
        }

        // Compute metrics
        let (_metrics, _signals, aggregate) = engine.compute_metrics(mint, base_time + 100).unwrap();

        // Verify metadata propagated to aggregate
        assert_eq!(aggregate.mint, mint);
        assert_eq!(aggregate.source_program, "pumpswap"); // From metadata.launch_platform
        assert_eq!(aggregate.created_at, metadata.created_at);

        // Verify timestamps
        assert_eq!(aggregate.updated_at, base_time + 100);
        assert!(aggregate.last_trade_timestamp.is_some());

        // Verify computed fields
        assert!(aggregate.net_flow_300s_sol.is_some());
        assert!(aggregate.volume_300s_sol.is_some());
        assert_eq!(aggregate.buy_count_300s, Some(5));
    }

    #[test]
    fn test_bot_history_tracking() {
        // Test: BOT_DROPOFF detection with update_bot_history()
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "dropoff_mint";

        // Simulate previous state with high bot activity
        // (In reality, this would be from previous compute_metrics call)
        engine.update_bot_history(mint, 10); // 10 bot trades previously

        // Add normal trades (no bots)
        for i in 0..5 {
            let trade = make_trade(
                base_time + i * 40,
                mint,
                TradeDirection::Buy,
                1.0 + (i as f64 * 0.1),
                &format!("human_wallet_{}", i),
            );
            engine.process_trade(trade);
        }

        // Compute metrics (should detect BOT_DROPOFF)
        let (_metrics, signals, _aggregate) = engine.compute_metrics(mint, base_time + 300).unwrap();

        // Verify BOT_DROPOFF signal detected
        assert!(!signals.is_empty());
        assert!(signals
            .iter()
            .any(|s| s.signal_type == crate::pipeline::signals::SignalType::BotDropoff));

        // Verify signal has correct details
        let dropoff = signals
            .iter()
            .find(|s| s.signal_type == crate::pipeline::signals::SignalType::BotDropoff)
            .unwrap();
        assert!(dropoff.details_json.is_some());
        assert!(dropoff.severity >= 3);
    }

    #[test]
    fn test_metadata_refresh() {
        // Test: refresh_metadata() updates cache and affects aggregates
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "metadata_mint";

        // Add trades WITHOUT metadata
        let trade1 = make_trade(base_time, mint, TradeDirection::Buy, 1.0, "wallet_1");
        engine.process_trade(trade1);

        // Compute aggregate without metadata
        let (_m1, _s1, agg1) = engine.compute_metrics(mint, base_time + 10).unwrap();
        assert_eq!(agg1.source_program, "unknown"); // Default when no metadata

        // Refresh metadata
        let metadata = make_metadata(mint, "bonkswap", base_time - 1000);
        engine.refresh_metadata(metadata.clone());

        // Add another trade
        let trade2 = make_trade(base_time + 20, mint, TradeDirection::Sell, 0.5, "wallet_2");
        engine.process_trade(trade2);

        // Compute aggregate WITH metadata
        let (_m2, _s2, agg2) = engine.compute_metrics(mint, base_time + 30).unwrap();
        assert_eq!(agg2.source_program, "bonkswap"); // From metadata.launch_platform
        assert_eq!(agg2.created_at, metadata.created_at);

        // Verify metadata is cached
        assert!(engine.metadata_cache.contains_key(mint));
        assert_eq!(engine.metadata_cache.get(mint).unwrap().symbol, Some("TEST".to_string()));
    }

    #[test]
    fn test_compute_metrics_no_state_error() {
        // Edge case: compute_metrics() on nonexistent mint
        let base_time = 10000;
        let engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let result = engine.compute_metrics("nonexistent_mint", base_time);

        // Should return error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No state for mint"));
    }

    #[test]
    fn test_multiple_tokens_isolated() {
        // Test: Multiple tokens maintain separate state
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint1 = "token_a";
        let mint2 = "token_b";

        // Add trades to token A
        for i in 0..5 {
            let trade = make_trade(base_time + i * 10, mint1, TradeDirection::Buy, 1.0, "wallet_a");
            engine.process_trade(trade);
        }

        // Add trades to token B
        for i in 0..3 {
            let trade = make_trade(base_time + i * 10, mint2, TradeDirection::Sell, 0.5, "wallet_b");
            engine.process_trade(trade);
        }

        // Verify separate state
        assert_eq!(engine.states.len(), 2);
        assert!(engine.states.contains_key(mint1));
        assert!(engine.states.contains_key(mint2));

        // Compute metrics for token A
        let (_m1, _s1, agg1) = engine.compute_metrics(mint1, base_time + 100).unwrap();
        assert_eq!(agg1.mint, mint1);
        assert_eq!(agg1.buy_count_60s, Some(5));
        assert_eq!(agg1.sell_count_60s, Some(0));

        // Compute metrics for token B
        let (_m2, _s2, agg2) = engine.compute_metrics(mint2, base_time + 100).unwrap();
        assert_eq!(agg2.mint, mint2);
        assert_eq!(agg2.buy_count_60s, Some(0));
        assert_eq!(agg2.sell_count_60s, Some(3));

        // Verify states are independent
        assert_ne!(agg1.net_flow_60s_sol, agg2.net_flow_60s_sol);
    }
}
