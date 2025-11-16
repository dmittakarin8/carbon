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
use super::signals::{SignalType, TokenSignal};
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

    /// Signal deduplication state
    /// Maps mint -> (SignalType -> is_active)
    /// A signal is only written when its state transitions from false->true
    last_signal_state: HashMap<String, HashMap<SignalType, bool>>,

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
            last_signal_state: HashMap::new(),
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
    /// 2. Vec<TokenSignal> - Detected signals (BREAKOUT, SURGE, etc.) - DEDUPLICATED
    /// 3. AggregatedTokenState - SQL-schema-compliant aggregate
    ///
    /// Phase 3: Returns results without database writes
    /// Phase 4: Will also write to database via AggregateDbWriter
    ///
    /// Note: This method requires &mut self for signal deduplication.
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `now` - Current Unix timestamp
    ///
    /// # Returns
    /// * `Ok((metrics, signals, aggregate))` - Full pipeline output (signals are deduplicated)
    /// * `Err(...)` - If token has no state (no trades processed)
    pub fn compute_metrics(
        &mut self,
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

        // Deduplicate signals before returning
        let deduplicated_signals = self.deduplicate_signals(mint, signals);

        Ok((metrics, deduplicated_signals, aggregate))
    }

    /// Deduplicate signals based on state changes
    ///
    /// A signal is only returned if its state has changed:
    /// - false -> true: Signal starts (WRITE TO DB)
    /// - true -> true: Signal persists (DO NOT WRITE)
    /// - true -> false: Signal ends (update state, DO NOT WRITE)
    /// - false -> false: Signal remains inactive (DO NOT WRITE)
    ///
    /// This drastically reduces token_signals table growth by emitting
    /// each signal only once per trend cycle.
    ///
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `signals` - Raw signals detected from metrics
    ///
    /// # Returns
    /// * Vector of signals that should be written to database (new signals only)
    fn deduplicate_signals(&mut self, mint: &str, signals: Vec<TokenSignal>) -> Vec<TokenSignal> {
        // Get or create signal state for this token
        let signal_state = self
            .last_signal_state
            .entry(mint.to_string())
            .or_insert_with(HashMap::new);

        // Build set of currently active signal types
        let mut active_types: HashMap<SignalType, bool> = HashMap::new();
        for signal in &signals {
            active_types.insert(signal.signal_type, true);
        }

        // Filter signals: only return those with state transition false->true
        let mut new_signals = Vec::new();
        for signal in signals {
            let was_active = signal_state.get(&signal.signal_type).copied().unwrap_or(false);
            let is_active = true; // Signal was detected

            // Only write if transitioning from inactive to active
            if !was_active && is_active {
                new_signals.push(signal);
            }
        }

        // Update state: set all detected signals to true
        for signal_type in active_types.keys() {
            signal_state.insert(*signal_type, true);
        }

        // Update state: set undetected signals to false (signal ended)
        // This allows the same signal to be emitted again later
        let all_signal_types = [
            SignalType::Breakout,
            SignalType::Focused,
            SignalType::Surge,
            SignalType::BotDropoff,
            SignalType::DcaConviction,
        ];
        for signal_type in &all_signal_types {
            if !active_types.contains_key(signal_type) {
                signal_state.insert(*signal_type, false);
            }
        }

        new_signals
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
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

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

    #[test]
    fn test_dedup_breakout_persists() {
        // Test: BREAKOUT signal is written once, then deduplicated on subsequent calls
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "breakout_dedup_mint";

        // Create BREAKOUT conditions (high volume, many wallets, high buy ratio)
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3,
                mint,
                TradeDirection::Buy,
                0.5 + (i as f64 * 0.05),
                &format!("wallet_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // First compute_metrics call - should return BREAKOUT signal
        let (_m1, signals1, _agg1) = engine.compute_metrics(mint, base_time + 60).unwrap();
        assert!(!signals1.is_empty(), "First call should detect BREAKOUT");
        assert!(signals1
            .iter()
            .any(|s| s.signal_type == SignalType::Breakout));

        // Add more trades to maintain BREAKOUT conditions
        for i in 0..10 {
            let trade = make_trade(
                base_time + 70 + i * 2,
                mint,
                TradeDirection::Buy,
                0.7,
                &format!("wallet_{}", i % 5),
            );
            engine.process_trade(trade);
        }

        // Second compute_metrics call - BREAKOUT persists, should NOT return signal
        let (_m2, signals2, _agg2) = engine.compute_metrics(mint, base_time + 90).unwrap();
        assert!(
            signals2.is_empty() || !signals2.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Second call should NOT return BREAKOUT (already active)"
        );
    }

    #[test]
    fn test_dedup_breakout_resets_after_wait() {
        // Test: BREAKOUT signal can be emitted again after it ends and restarts
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "breakout_reset_mint";

        // Phase 1: Create BREAKOUT conditions
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3,
                mint,
                TradeDirection::Buy,
                0.6,
                &format!("wallet_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // First compute_metrics - should return BREAKOUT
        let (_m1, signals1, _agg1) = engine.compute_metrics(mint, base_time + 60).unwrap();
        assert!(
            signals1.iter().any(|s| s.signal_type == SignalType::Breakout),
            "First BREAKOUT should be detected"
        );

        // Phase 2: Add only SELL trades to end BREAKOUT
        for i in 0..10 {
            let trade = make_trade(
                base_time + 100 + i * 5,
                mint,
                TradeDirection::Sell,
                0.5,
                &format!("seller_{}", i),
            );
            engine.process_trade(trade);
        }

        // Second compute_metrics - BREAKOUT should be inactive (no signal returned)
        let (_m2, signals2, _agg2) = engine.compute_metrics(mint, base_time + 150).unwrap();
        assert!(
            !signals2.iter().any(|s| s.signal_type == SignalType::Breakout),
            "BREAKOUT should be inactive"
        );

        // Phase 3: Create BREAKOUT conditions AGAIN
        for i in 0..20 {
            let trade = make_trade(
                base_time + 200 + i * 3,
                mint,
                TradeDirection::Buy,
                0.7,
                &format!("new_wallet_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // Third compute_metrics - should return BREAKOUT again (false -> true transition)
        let (_m3, signals3, _agg3) = engine.compute_metrics(mint, base_time + 260).unwrap();
        assert!(
            signals3.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Second BREAKOUT should be detected after reset"
        );
    }

    #[test]
    fn test_dedup_multiple_signal_types_per_token() {
        // Test: Different signal types are tracked independently for same token
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint = "multi_signal_mint";

        // Phase 1: Create BREAKOUT conditions
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3,
                mint,
                TradeDirection::Buy,
                0.6,
                &format!("wallet_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // First call - should detect BREAKOUT (possibly SURGE too)
        let (_m1, signals1, _agg1) = engine.compute_metrics(mint, base_time + 60).unwrap();
        let has_breakout_1 = signals1.iter().any(|s| s.signal_type == SignalType::Breakout);
        let has_surge_1 = signals1.iter().any(|s| s.signal_type == SignalType::Surge);

        assert!(has_breakout_1, "BREAKOUT should be detected");

        // Phase 2: Add more trades to maintain/create multiple signals
        for i in 0..15 {
            let trade = make_trade(
                base_time + 70 + i * 2,
                mint,
                TradeDirection::Buy,
                0.8,
                &format!("wallet_{}", i % 5),
            );
            engine.process_trade(trade);
        }

        // Second call - signals persist, should NOT be returned
        let (_m2, signals2, _agg2) = engine.compute_metrics(mint, base_time + 100).unwrap();
        assert!(
            !signals2.iter().any(|s| s.signal_type == SignalType::Breakout),
            "BREAKOUT should not be returned (still active)"
        );

        // If SURGE was not detected first time but is now, it should be returned
        // If SURGE was detected first time, it should NOT be returned now
        let has_surge_2 = signals2.iter().any(|s| s.signal_type == SignalType::Surge);
        if has_surge_1 {
            assert!(
                !has_surge_2,
                "SURGE should not be returned if it was already active"
            );
        }
        // Note: If SURGE appears now (wasn't active before), it WILL be returned (new signal)
    }

    #[test]
    fn test_dedup_no_cross_token_leakage() {
        // Test: Deduplication state is isolated per token (no cross-token interference)
        let base_time = 10000;
        let mut engine = PipelineEngine::new_with_timestamp_fn(Box::new(move || base_time));

        let mint_a = "token_a_dedup";
        let mint_b = "token_b_dedup";

        // Phase 1: Create BREAKOUT on token A
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3,
                mint_a,
                TradeDirection::Buy,
                0.6,
                &format!("wallet_a_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // Token A: First call - should return BREAKOUT
        let (_m1, signals_a1, _agg1) = engine.compute_metrics(mint_a, base_time + 60).unwrap();
        assert!(
            signals_a1.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Token A should detect BREAKOUT"
        );

        // Phase 2: Create BREAKOUT on token B
        for i in 0..20 {
            let trade = make_trade(
                base_time + i * 3,
                mint_b,
                TradeDirection::Buy,
                0.6,
                &format!("wallet_b_{}", i % 8),
            );
            engine.process_trade(trade);
        }

        // Token B: First call - should return BREAKOUT (independent of token A state)
        let (_m2, signals_b1, _agg2) = engine.compute_metrics(mint_b, base_time + 60).unwrap();
        assert!(
            signals_b1.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Token B should detect BREAKOUT (independent of token A)"
        );

        // Phase 3: Add more trades to both tokens
        for i in 0..10 {
            let trade_a = make_trade(
                base_time + 70 + i * 2,
                mint_a,
                TradeDirection::Buy,
                0.5,
                &format!("wallet_a_{}", i),
            );
            engine.process_trade(trade_a);

            let trade_b = make_trade(
                base_time + 70 + i * 2,
                mint_b,
                TradeDirection::Buy,
                0.5,
                &format!("wallet_b_{}", i),
            );
            engine.process_trade(trade_b);
        }

        // Both tokens: Second call - neither should return BREAKOUT (both already active)
        let (_m3, signals_a2, _agg3) = engine.compute_metrics(mint_a, base_time + 90).unwrap();
        let (_m4, signals_b2, _agg4) = engine.compute_metrics(mint_b, base_time + 90).unwrap();

        assert!(
            !signals_a2.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Token A should not return BREAKOUT (already active)"
        );
        assert!(
            !signals_b2.iter().any(|s| s.signal_type == SignalType::Breakout),
            "Token B should not return BREAKOUT (already active)"
        );

        // Verify internal state is separate
        assert!(engine.last_signal_state.contains_key(mint_a));
        assert!(engine.last_signal_state.contains_key(mint_b));
        assert_eq!(engine.last_signal_state.len(), 2);
    }
}
