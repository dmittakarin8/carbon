//! Core type definitions for the aggregate-only architecture
//!
//! All structs match the SQL schema defined in `/sql/`:
//! - `TokenMetadata` → `token_metadata` table
//! - `AggregatedTokenState` → `token_aggregates` table
//! - Field names use exact SQL column names (snake_case)

/// Trade direction enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Buy,
    Sell,
    Unknown,
}

/// Token metadata matching the token_metadata table schema
///
/// Schema reference: `/sql/00_token_metadata.sql`
/// All field names are EXACT matches to SQL column names.
#[derive(Debug, Clone)]
pub struct TokenMetadata {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: u8,
    pub launch_platform: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Individual trade event from streamers
///
/// This represents a single trade extracted from on-chain data.
/// These events are held in-memory in rolling windows and never persisted as raw trades.
#[derive(Debug, Clone)]
pub struct TradeEvent {
    pub timestamp: i64,
    pub mint: String,
    pub direction: TradeDirection,
    pub sol_amount: f64,
    pub token_amount: f64,
    pub token_decimals: u8,
    pub user_account: String,
    pub source_program: String,
}

/// Aggregated token state matching the token_aggregates table schema
///
/// Schema reference: `/sql/02_token_aggregates.sql`
/// All field names are EXACT matches to SQL column names.
#[derive(Debug, Clone)]
pub struct AggregatedTokenState {
    // Primary key
    pub mint: String,

    // Source and timing
    pub source_program: String,
    pub last_trade_timestamp: Option<i64>,

    // Price and market data
    pub price_usd: Option<f64>,
    pub price_sol: Option<f64>,
    pub market_cap_usd: Option<f64>,

    // Net flow metrics (rolling windows)
    pub net_flow_60s_sol: Option<f64>,
    pub net_flow_300s_sol: Option<f64>,
    pub net_flow_900s_sol: Option<f64>,

    // Trade counts (60s window)
    pub buy_count_60s: Option<i32>,
    pub sell_count_60s: Option<i32>,

    // Trade counts (300s window)
    pub buy_count_300s: Option<i32>,
    pub sell_count_300s: Option<i32>,

    // Trade counts (900s window)
    pub buy_count_900s: Option<i32>,
    pub sell_count_900s: Option<i32>,

    // Advanced metrics (300s window)
    pub unique_wallets_300s: Option<i32>,
    pub bot_trades_300s: Option<i32>,
    pub bot_wallets_300s: Option<i32>,

    // Volume metrics (300s window)
    pub avg_trade_size_300s_sol: Option<f64>,
    pub volume_300s_sol: Option<f64>,

    // Timestamps
    pub updated_at: i64,
    pub created_at: i64,
}

impl AggregatedTokenState {
    /// Construct AggregatedTokenState from rolling metrics
    ///
    /// Phase 3-D: Aggregate Builder
    ///
    /// Converts in-memory rolling metrics into a SQL-schema-compliant aggregate state.
    /// This is the bridge between runtime computation and database persistence.
    ///
    /// Arguments:
    /// - `mint`: Token mint address (primary key)
    /// - `metrics`: Computed rolling metrics from TokenRollingState
    /// - `metadata`: Optional token metadata for enrichment (symbol, name, source_program)
    /// - `last_trade_ts`: Unix timestamp of most recent trade
    /// - `now`: Current Unix timestamp for updated_at
    ///
    /// Returns: Fully-populated AggregatedTokenState ready for database INSERT/UPDATE
    ///
    /// Note: Price fields (price_usd, price_sol, market_cap_usd) are set to None.
    /// These will be populated in Phase 4 by the price enrichment pipeline.
    pub fn from_metrics(
        mint: &str,
        metrics: &super::state::RollingMetrics,
        metadata: Option<&TokenMetadata>,
        last_trade_ts: i64,
        now: i64,
    ) -> Self {
        // Extract source_program from metadata or use default
        let source_program = metadata
            .and_then(|m| m.launch_platform.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Extract created_at from metadata or use current timestamp
        let created_at = metadata.map(|m| m.created_at).unwrap_or(now);

        // Compute derived metrics
        let avg_trade_size_300s_sol = Self::compute_avg_trade_size(metrics);
        let volume_300s_sol = Self::compute_volume_300s(metrics);

        Self {
            mint: mint.to_string(),
            source_program,
            last_trade_timestamp: Some(last_trade_ts),

            // Phase 4: Price enrichment (placeholder None values)
            price_usd: None,
            price_sol: None,
            market_cap_usd: None,

            // Net flow metrics (rolling windows)
            net_flow_60s_sol: Some(metrics.net_flow_60s_sol),
            net_flow_300s_sol: Some(metrics.net_flow_300s_sol),
            net_flow_900s_sol: Some(metrics.net_flow_900s_sol),

            // Trade counts (60s window)
            buy_count_60s: Some(metrics.buy_count_60s),
            sell_count_60s: Some(metrics.sell_count_60s),

            // Trade counts (300s window)
            buy_count_300s: Some(metrics.buy_count_300s),
            sell_count_300s: Some(metrics.sell_count_300s),

            // Trade counts (900s window)
            buy_count_900s: Some(metrics.buy_count_900s),
            sell_count_900s: Some(metrics.sell_count_900s),

            // Advanced metrics (300s window)
            unique_wallets_300s: Some(metrics.unique_wallets_300s),
            bot_trades_300s: Some(metrics.bot_trades_count_300s),
            bot_wallets_300s: Some(metrics.bot_wallets_count_300s),

            // Volume metrics (300s window)
            avg_trade_size_300s_sol,
            volume_300s_sol: Some(volume_300s_sol),

            // Timestamps
            updated_at: now,
            created_at,
        }
    }

    /// Compute average trade size from 300s window metrics
    ///
    /// Returns None if no trades in window (division by zero protection)
    fn compute_avg_trade_size(metrics: &super::state::RollingMetrics) -> Option<f64> {
        let total_trades = metrics.buy_count_300s + metrics.sell_count_300s;
        
        if total_trades > 0 {
            let volume = metrics.net_flow_300s_sol.abs();
            Some(volume / total_trades as f64)
        } else {
            None
        }
    }

    /// Compute total volume in 300s window
    ///
    /// Volume is the absolute value of net flow (ignores direction)
    fn compute_volume_300s(metrics: &super::state::RollingMetrics) -> f64 {
        metrics.net_flow_300s_sol.abs()
    }
}

// TODO: Phase 4 - Price enrichment pipeline
// - Integrate live price fetching (populate price_sol, price_usd)
// - Compute market_cap_usd = price_usd × token_supply
// - Add token supply fetching from on-chain data
// - Add price data source tracking (VibeStation vs BirdEye)

// TODO: Phase 4 - Metadata enrichment pipeline
// - Fetch token_metadata from SQLite database
// - Query VibeStation/BirdEye APIs for missing metadata
// - Cache metadata with TTL for performance
// - Pass enriched metadata to from_metrics() constructor

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::state::RollingMetrics;

    /// Helper to create test RollingMetrics
    fn make_test_metrics() -> RollingMetrics {
        RollingMetrics {
            net_flow_60s_sol: 10.5,
            net_flow_300s_sol: 45.2,
            net_flow_900s_sol: 120.8,
            buy_count_60s: 5,
            sell_count_60s: 2,
            buy_count_300s: 20,
            sell_count_300s: 8,
            buy_count_900s: 50,
            sell_count_900s: 25,
            unique_wallets_300s: 12,
            bot_wallets_count_300s: 2,
            bot_trades_count_300s: 6,
        }
    }

    /// Helper to create test TokenMetadata
    fn make_test_metadata(mint: &str, launch_platform: &str, created_at: i64) -> TokenMetadata {
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
    fn test_from_metrics_happy_path() {
        // Scenario: Full metadata provided, positive metrics
        let mint = "test_mint_123";
        let metrics = make_test_metrics();
        let metadata = make_test_metadata(mint, "pumpswap", 1000);
        let last_trade_ts = 2000;
        let now = 2100;

        let state = AggregatedTokenState::from_metrics(
            mint,
            &metrics,
            Some(&metadata),
            last_trade_ts,
            now,
        );

        // Verify basic fields
        assert_eq!(state.mint, mint);
        assert_eq!(state.source_program, "pumpswap");
        assert_eq!(state.last_trade_timestamp, Some(2000));

        // Verify timestamps
        assert_eq!(state.created_at, 1000); // From metadata
        assert_eq!(state.updated_at, 2100); // From now parameter

        // Verify net flow metrics
        assert_eq!(state.net_flow_60s_sol, Some(10.5));
        assert_eq!(state.net_flow_300s_sol, Some(45.2));
        assert_eq!(state.net_flow_900s_sol, Some(120.8));

        // Verify trade counts (60s)
        assert_eq!(state.buy_count_60s, Some(5));
        assert_eq!(state.sell_count_60s, Some(2));

        // Verify trade counts (300s)
        assert_eq!(state.buy_count_300s, Some(20));
        assert_eq!(state.sell_count_300s, Some(8));

        // Verify trade counts (900s)
        assert_eq!(state.buy_count_900s, Some(50));
        assert_eq!(state.sell_count_900s, Some(25));

        // Verify advanced metrics (300s)
        assert_eq!(state.unique_wallets_300s, Some(12));
        assert_eq!(state.bot_trades_300s, Some(6));
        assert_eq!(state.bot_wallets_300s, Some(2));

        // Verify computed volume metrics (300s)
        // volume = abs(net_flow_300s_sol) = abs(45.2) = 45.2
        assert_eq!(state.volume_300s_sol, Some(45.2));

        // avg_trade_size = volume / total_trades = 45.2 / (20 + 8) = 45.2 / 28 ≈ 1.614
        assert!(state.avg_trade_size_300s_sol.is_some());
        let avg = state.avg_trade_size_300s_sol.unwrap();
        assert!((avg - 1.614).abs() < 0.01); // Tolerance for float precision
    }

    #[test]
    fn test_from_metrics_missing_metadata() {
        // Scenario: No metadata provided, verify safe defaults
        let mint = "no_metadata_mint";
        let metrics = make_test_metrics();
        let last_trade_ts = 2000;
        let now = 2100;

        let state = AggregatedTokenState::from_metrics(mint, &metrics, None, last_trade_ts, now);

        // Verify default source_program when metadata is None
        assert_eq!(state.source_program, "unknown");

        // Verify created_at defaults to 'now' when no metadata
        assert_eq!(state.created_at, 2100);

        // Verify updated_at still uses 'now'
        assert_eq!(state.updated_at, 2100);

        // Verify other fields still populated correctly
        assert_eq!(state.mint, mint);
        assert_eq!(state.last_trade_timestamp, Some(2000));
        assert_eq!(state.net_flow_300s_sol, Some(45.2));
        assert_eq!(state.buy_count_300s, Some(20));
    }

    #[test]
    fn test_placeholder_price_fields_are_none() {
        // Scenario: Verify price fields are explicitly None (Phase 4 placeholder)
        let mint = "price_check_mint";
        let metrics = make_test_metrics();
        let metadata = make_test_metadata(mint, "bonkswap", 1000);
        let last_trade_ts = 2000;
        let now = 2100;

        let state = AggregatedTokenState::from_metrics(
            mint,
            &metrics,
            Some(&metadata),
            last_trade_ts,
            now,
        );

        // Critical: Price fields MUST be None (Phase 4 will populate these)
        assert_eq!(state.price_usd, None);
        assert_eq!(state.price_sol, None);
        assert_eq!(state.market_cap_usd, None);
    }

    #[test]
    fn test_timestamp_assignment() {
        // Scenario: Verify timestamp logic for created_at and updated_at
        let mint = "timestamp_mint";
        let metrics = make_test_metrics();

        // Case 1: With metadata (created_at from metadata)
        let metadata = make_test_metadata(mint, "moonshot", 1500);
        let state1 = AggregatedTokenState::from_metrics(mint, &metrics, Some(&metadata), 2000, 2500);

        assert_eq!(state1.created_at, 1500); // From metadata
        assert_eq!(state1.updated_at, 2500); // From now parameter

        // Case 2: Without metadata (created_at defaults to now)
        let state2 = AggregatedTokenState::from_metrics(mint, &metrics, None, 2000, 2500);

        assert_eq!(state2.created_at, 2500); // Defaults to now
        assert_eq!(state2.updated_at, 2500); // From now parameter

        // Case 3: Verify different timestamps work correctly
        let metadata3 = make_test_metadata(mint, "jupiter", 100);
        let state3 = AggregatedTokenState::from_metrics(mint, &metrics, Some(&metadata3), 5000, 10000);

        assert_eq!(state3.created_at, 100);   // From metadata (very old)
        assert_eq!(state3.updated_at, 10000); // Recent update
        assert!(state3.updated_at > state3.created_at); // Sanity check
    }

    #[test]
    fn test_compute_avg_trade_size_zero_trades() {
        // Edge case: No trades in 300s window
        let metrics = RollingMetrics {
            net_flow_60s_sol: 0.0,
            net_flow_300s_sol: 0.0,
            net_flow_900s_sol: 0.0,
            buy_count_60s: 0,
            sell_count_60s: 0,
            buy_count_300s: 0,
            sell_count_300s: 0,
            buy_count_900s: 0,
            sell_count_900s: 0,
            unique_wallets_300s: 0,
            bot_wallets_count_300s: 0,
            bot_trades_count_300s: 0,
        };

        let mint = "zero_trades_mint";
        let state = AggregatedTokenState::from_metrics(mint, &metrics, None, 1000, 2000);

        // avg_trade_size should be None (avoid division by zero)
        assert_eq!(state.avg_trade_size_300s_sol, None);

        // volume should be 0.0 (abs(0.0) = 0.0)
        assert_eq!(state.volume_300s_sol, Some(0.0));
    }

    #[test]
    fn test_compute_volume_negative_net_flow() {
        // Scenario: Negative net flow (more sells than buys)
        let metrics = RollingMetrics {
            net_flow_60s_sol: -5.0,
            net_flow_300s_sol: -30.0, // Negative (net selling)
            net_flow_900s_sol: -50.0,
            buy_count_60s: 2,
            sell_count_60s: 5,
            buy_count_300s: 10,
            sell_count_300s: 20,
            buy_count_900s: 25,
            sell_count_900s: 50,
            unique_wallets_300s: 8,
            bot_wallets_count_300s: 1,
            bot_trades_count_300s: 3,
        };

        let mint = "negative_flow_mint";
        let state = AggregatedTokenState::from_metrics(mint, &metrics, None, 1000, 2000);

        // net_flow should preserve sign (negative)
        assert_eq!(state.net_flow_300s_sol, Some(-30.0));

        // volume should be absolute value (positive)
        assert_eq!(state.volume_300s_sol, Some(30.0));

        // avg_trade_size should use absolute volume
        // avg = 30.0 / (10 + 20) = 30.0 / 30 = 1.0
        assert_eq!(state.avg_trade_size_300s_sol, Some(1.0));
    }

    #[test]
    fn test_metadata_launch_platform_variants() {
        // Test different launch_platform values
        let mint = "platform_test_mint";
        let metrics = make_test_metrics();

        // Case 1: launch_platform is Some("pumpswap")
        let metadata1 = make_test_metadata(mint, "pumpswap", 1000);
        let state1 = AggregatedTokenState::from_metrics(mint, &metrics, Some(&metadata1), 2000, 3000);
        assert_eq!(state1.source_program, "pumpswap");

        // Case 2: launch_platform is Some("bonkswap")
        let metadata2 = make_test_metadata(mint, "bonkswap", 1000);
        let state2 = AggregatedTokenState::from_metrics(mint, &metrics, Some(&metadata2), 2000, 3000);
        assert_eq!(state2.source_program, "bonkswap");

        // Case 3: launch_platform is None
        let mut metadata3 = make_test_metadata(mint, "", 1000);
        metadata3.launch_platform = None;
        let state3 = AggregatedTokenState::from_metrics(mint, &metrics, Some(&metadata3), 2000, 3000);
        assert_eq!(state3.source_program, "unknown");

        // Case 4: No metadata at all
        let state4 = AggregatedTokenState::from_metrics(mint, &metrics, None, 2000, 3000);
        assert_eq!(state4.source_program, "unknown");
    }
}
