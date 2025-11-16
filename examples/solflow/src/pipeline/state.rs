//! In-memory rolling state management for tokens
//!
//! Phase 2: Rolling window logic and lifecycle methods implemented
//! Phase 3-A: Bot detection implemented
//! Phase 3-B: Signal detection implemented

use super::types::{TradeDirection, TradeEvent};
use super::signals::{SignalType, TokenSignal};
use std::collections::{HashMap, HashSet};

/// Per-token rolling state container
///
/// Maintains rolling buffers for three time windows:
/// - 60s (1 minute)
/// - 300s (5 minutes)
/// - 900s (15 minutes)
#[derive(Debug, Clone)]
pub struct TokenRollingState {
    /// Token mint address
    pub mint: String,

    /// Rolling buffer: trades in last 60 seconds
    pub trades_60s: Vec<TradeEvent>,

    /// Rolling buffer: trades in last 300 seconds (5 minutes)
    pub trades_300s: Vec<TradeEvent>,

    /// Rolling buffer: trades in last 900 seconds (15 minutes)
    pub trades_900s: Vec<TradeEvent>,

    /// Unique wallet addresses in 300s window
    pub unique_wallets_300s: HashSet<String>,

    /// Bot wallet addresses in 300s window
    pub bot_wallets_300s: HashSet<String>,

    /// Trades grouped by source program (for DCA correlation)
    /// Key: source_program (e.g., "PumpSwap", "BonkSwap", "Moonshot", "JupiterDCA")
    /// Value: Vector of trades from that program
    pub trades_by_program: HashMap<String, Vec<TradeEvent>>,
}

/// Internal metrics snapshot computed from rolling windows
///
/// This is NOT directly mapped to AggregatedTokenState.
/// It's an intermediate representation for Phase 2 only.
#[derive(Debug, Clone)]
pub struct RollingMetrics {
    // Net flow metrics
    pub net_flow_60s_sol: f64,
    pub net_flow_300s_sol: f64,
    pub net_flow_900s_sol: f64,

    // Trade counts (60s window)
    pub buy_count_60s: i32,
    pub sell_count_60s: i32,

    // Trade counts (300s window)
    pub buy_count_300s: i32,
    pub sell_count_300s: i32,

    // Trade counts (900s window)
    pub buy_count_900s: i32,
    pub sell_count_900s: i32,

    // Advanced metrics (300s window)
    pub unique_wallets_300s: i32,
    
    // Bot detection metrics (Phase 3-A)
    pub bot_wallets_count_300s: i32,
    pub bot_trades_count_300s: i32,
}

/// Bot detection heuristics applied to a trade window
///
/// Phase 3-A: Bot Detection Implementation
/// 
/// Detects wallets exhibiting bot-like behavior based on:
/// 1. High-frequency trading: > 10 trades in 300s window
/// 2. Rapid consecutive trades: Multiple trades within 1 second
/// 3. Alternating buy/sell patterns: Repeated flip-flopping
/// 4. Near-identical trade sizes: Repeated same SOL amounts
///
/// Returns: (Set of bot wallet addresses, total count of trades from bots)
///
/// TODO: Phase 3+ refinements
/// - Add MEV transaction pattern detection
/// - Integrate known bot wallet blocklist
/// - Tune thresholds based on production data
/// - Add probabilistic scoring (0.0-1.0) instead of binary classification
fn detect_bot_wallets(trades: &[TradeEvent]) -> (HashSet<String>, i32) {
    // Wallet-level statistics for bot detection
    #[derive(Debug, Default)]
    struct WalletStats {
        trade_count: usize,
        timestamps: Vec<i64>,
        directions: Vec<TradeDirection>,
        sol_amounts: Vec<f64>,
    }

    // Group trades by wallet
    let mut wallet_stats: HashMap<String, WalletStats> = HashMap::new();
    
    for trade in trades {
        let stats = wallet_stats
            .entry(trade.user_account.clone())
            .or_default();
        
        stats.trade_count += 1;
        stats.timestamps.push(trade.timestamp);
        stats.directions.push(trade.direction);
        stats.sol_amounts.push(trade.sol_amount);
    }

    let mut bot_wallets = HashSet::new();
    let mut bot_trades_count = 0;

    for (wallet, stats) in wallet_stats.iter() {
        let mut is_bot = false;

        // Heuristic 1: High-frequency trading (> 10 trades in 300s)
        // TODO: Tune threshold - may need adjustment for high-volume tokens
        if stats.trade_count > 10 {
            is_bot = true;
        }

        // Heuristic 2: Rapid consecutive trades (multiple trades within 1s)
        if !is_bot && stats.timestamps.len() >= 2 {
            let mut sorted_timestamps = stats.timestamps.clone();
            sorted_timestamps.sort_unstable();
            
            let mut rapid_trades = 0;
            for window in sorted_timestamps.windows(2) {
                if window[1] - window[0] <= 1 {
                    rapid_trades += 1;
                }
            }
            
            // TODO: Tune threshold - 3+ rapid trades is suspicious
            if rapid_trades >= 3 {
                is_bot = true;
            }
        }

        // Heuristic 3: Alternating buy/sell pattern (flip-flopping)
        if !is_bot && stats.directions.len() >= 4 {
            let mut alternations = 0;
            for window in stats.directions.windows(2) {
                if window[0] != window[1] 
                    && window[0] != TradeDirection::Unknown 
                    && window[1] != TradeDirection::Unknown {
                    alternations += 1;
                }
            }
            
            // TODO: Tune threshold - 70%+ alternation rate is suspicious
            let alternation_rate = alternations as f64 / (stats.directions.len() - 1) as f64;
            if alternation_rate > 0.7 {
                is_bot = true;
            }
        }

        // Heuristic 4: Near-identical trade sizes (repeated same amounts)
        if !is_bot && stats.sol_amounts.len() >= 3 {
            let mut identical_count = 0;
            let epsilon = 0.0001; // SOL precision tolerance
            
            for i in 0..stats.sol_amounts.len() {
                for j in (i + 1)..stats.sol_amounts.len() {
                    if (stats.sol_amounts[i] - stats.sol_amounts[j]).abs() < epsilon {
                        identical_count += 1;
                    }
                }
            }
            
            // TODO: Tune threshold - 50%+ identical pairs is suspicious
            let max_pairs = (stats.sol_amounts.len() * (stats.sol_amounts.len() - 1)) / 2;
            let identical_rate = identical_count as f64 / max_pairs as f64;
            if identical_rate > 0.5 {
                is_bot = true;
            }
        }

        if is_bot {
            bot_wallets.insert(wallet.clone());
            bot_trades_count += stats.trade_count as i32;
        }
    }

    (bot_wallets, bot_trades_count)
}

/// Signal detection configuration constants
///
/// Phase 3-B: Signal Detection Implementation
///
/// These thresholds control signal triggering sensitivity.
/// TODO: Tune based on production data and false positive rates.
mod signal_thresholds {
    // BREAKOUT thresholds
    pub const BREAKOUT_NET_FLOW_60S_MIN: f64 = 5.0; // Min 5 SOL net inflow in 60s
    pub const BREAKOUT_WALLET_GROWTH_MIN: i32 = 5; // Min 5 new unique wallets
    pub const BREAKOUT_BUY_RATIO_MIN: f64 = 0.75; // 75% buys vs total trades
    
    // FOCUSED thresholds
    pub const FOCUSED_WALLET_CONCENTRATION_MAX: f64 = 0.3; // Max 30% of volume from single wallet
    pub const FOCUSED_MIN_VOLUME: f64 = 3.0; // Min 3 SOL volume
    pub const FOCUSED_BOT_RATIO_MAX: f64 = 0.2; // Max 20% bot trades
    
    // SURGE thresholds
    pub const SURGE_VOLUME_RATIO_MIN: f64 = 3.0; // 60s volume ≥ 3x average 300s volume
    pub const SURGE_BUY_COUNT_60S_MIN: i32 = 10; // Min 10 buys in 60s
    pub const SURGE_NET_FLOW_60S_MIN: f64 = 8.0; // Min 8 SOL net inflow
    
    // BOT_DROPOFF thresholds
    pub const BOT_DROPOFF_DECLINE_RATIO_MIN: f64 = 0.5; // 50%+ bot trade decline
    pub const BOT_DROPOFF_MIN_PREVIOUS_BOTS: i32 = 5; // Need at least 5 bot trades before
    pub const BOT_DROPOFF_NEW_WALLET_MIN: i32 = 3; // Min 3 new wallets entering
}

/// Compute DCA-to-spot correlation for a token
///
/// Measures overlap between Jupiter DCA BUYs and spot BUYs (PumpSwap, BonkSwap, Moonshot)
/// within a 60-second time window.
///
/// Arguments:
/// - `spot_trades`: BUY trades from spot programs (PumpSwap, BonkSwap, Moonshot)
/// - `dca_trades`: BUY trades from Jupiter DCA
/// - `window_secs`: Time window for correlation (default: 60 seconds)
///
/// Returns: (overlap_ratio, matched_dca_count)
/// - overlap_ratio: Percentage of DCA trades with matching spot trades (0.0-1.0)
/// - matched_dca_count: Number of DCA trades that had overlapping spot activity
fn compute_dca_correlation(
    spot_trades: &[TradeEvent],
    dca_trades: &[TradeEvent],
    window_secs: i64,
) -> (f64, usize) {
    if dca_trades.is_empty() {
        return (0.0, 0);
    }

    let mut matched_dca_count = 0;

    // For each DCA trade, check if there's a spot trade within ±window_secs
    for dca_trade in dca_trades {
        let dca_ts = dca_trade.timestamp;
        let has_matching_spot = spot_trades.iter().any(|spot_trade| {
            let time_diff = (spot_trade.timestamp - dca_ts).abs();
            time_diff <= window_secs
        });

        if has_matching_spot {
            matched_dca_count += 1;
        }
    }

    let overlap_ratio = matched_dca_count as f64 / dca_trades.len() as f64;
    (overlap_ratio, matched_dca_count)
}

/// Detect trading signals from rolling metrics
///
/// Phase 3-B: Signal Detection Implementation
///
/// Analyzes rolling window metrics and trade patterns to detect:
/// - BREAKOUT: Sharp volume increase with wallet growth
/// - FOCUSED: Concentrated buying from few non-bot wallets
/// - SURGE: Explosive buy volume spike
/// - BOT_DROPOFF: Sudden bot activity decline opening market
/// - DCA_CONVICTION: Jupiter DCA BUYs overlap with spot BUYs
///
/// Returns: Vec of detected signals with scores and details
///
/// TODO: Phase 3+ refinements
/// - Add historical baseline comparison (requires state tracking)
/// - Implement multi-timeframe confirmation (60s + 300s alignment)
/// - Add price momentum indicators (requires price data)
/// - Machine learning scoring model
fn detect_signals(
    mint: &str,
    metrics: &RollingMetrics,
    current_timestamp: i64,
    previous_bot_count: Option<i32>, // For BOT_DROPOFF detection
    trades_by_program: &HashMap<String, Vec<TradeEvent>>, // For DCA_CONVICTION detection
) -> Vec<TokenSignal> {
    use signal_thresholds::*;
    
    let mut signals = Vec::new();

    // Calculate derived metrics for detection
    let total_trades_60s = metrics.buy_count_60s + metrics.sell_count_60s;
    let total_trades_300s = metrics.buy_count_300s + metrics.sell_count_300s;
    
    let buy_ratio_60s = if total_trades_60s > 0 {
        metrics.buy_count_60s as f64 / total_trades_60s as f64
    } else {
        0.0
    };
    
    let bot_ratio_300s = if total_trades_300s > 0 {
        metrics.bot_trades_count_300s as f64 / total_trades_300s as f64
    } else {
        0.0
    };
    
    // Average 300s volume per 60s window (for surge detection)
    let avg_volume_per_60s = metrics.net_flow_300s_sol.abs() / 5.0;
    
    // BREAKOUT Detection
    // Sharp positive net flow with wallet growth and high buy ratio
    if metrics.net_flow_60s_sol > BREAKOUT_NET_FLOW_60S_MIN
        && metrics.unique_wallets_300s >= BREAKOUT_WALLET_GROWTH_MIN
        && buy_ratio_60s > BREAKOUT_BUY_RATIO_MIN
    {
        // Compute breakout score (0.0-1.0)
        let flow_score = (metrics.net_flow_60s_sol / 20.0).min(1.0);
        let wallet_score = (metrics.unique_wallets_300s as f64 / 20.0).min(1.0);
        let ratio_score = buy_ratio_60s;
        let breakout_score = (flow_score + wallet_score + ratio_score) / 3.0;
        
        let details = format!(
            r#"{{"net_flow_60s":{:.2},"unique_wallets":{},"buy_ratio":{:.2}}}"#,
            metrics.net_flow_60s_sol, metrics.unique_wallets_300s, buy_ratio_60s
        );
        
        let severity = if breakout_score > 0.8 { 5 }
                       else if breakout_score > 0.6 { 4 }
                       else if breakout_score > 0.4 { 3 }
                       else { 2 };
        
        signals.push(
            TokenSignal::new(mint.to_string(), SignalType::Breakout, 60, current_timestamp)
                .with_severity(severity)
                .with_score(breakout_score)
                .with_details(details),
        );
    }
    
    // FOCUSED Detection
    // Concentrated buying from few wallets, low bot activity
    if metrics.net_flow_300s_sol > FOCUSED_MIN_VOLUME
        && bot_ratio_300s < FOCUSED_BOT_RATIO_MAX
        && metrics.unique_wallets_300s > 0
        && metrics.unique_wallets_300s <= 10
    {
        // Concentration metric: inverse of wallet count (fewer wallets = higher concentration)
        let concentration = 1.0 / metrics.unique_wallets_300s as f64;
        
        // Focused score based on volume and concentration
        let volume_score = (metrics.net_flow_300s_sol / 10.0).min(1.0);
        let concentration_score = concentration.min(1.0);
        let bot_absence_score = 1.0 - bot_ratio_300s;
        let focused_score = (volume_score + concentration_score + bot_absence_score) / 3.0;
        
        let details = format!(
            r#"{{"net_flow_300s":{:.2},"unique_wallets":{},"bot_ratio":{:.2}}}"#,
            metrics.net_flow_300s_sol, metrics.unique_wallets_300s, bot_ratio_300s
        );
        
        let severity = if metrics.unique_wallets_300s <= 3 { 4 } else { 3 };
        
        signals.push(
            TokenSignal::new(mint.to_string(), SignalType::Focused, 300, current_timestamp)
                .with_severity(severity)
                .with_score(focused_score)
                .with_details(details),
        );
    }
    
    // SURGE Detection
    // Explosive buy volume spike (60s volume >> average 300s volume)
    if metrics.net_flow_60s_sol > SURGE_NET_FLOW_60S_MIN
        && metrics.buy_count_60s >= SURGE_BUY_COUNT_60S_MIN
        && avg_volume_per_60s > 0.0
    {
        let volume_ratio = metrics.net_flow_60s_sol / avg_volume_per_60s;
        
        if volume_ratio >= SURGE_VOLUME_RATIO_MIN {
            // Surge score based on volume acceleration
            let ratio_score = (volume_ratio / 10.0).min(1.0);
            let velocity_score = (metrics.buy_count_60s as f64 / 30.0).min(1.0);
            let surge_score = (ratio_score + velocity_score) / 2.0;
            
            let details = format!(
                r#"{{"net_flow_60s":{:.2},"volume_ratio":{:.2},"buy_count":{}}}"#,
                metrics.net_flow_60s_sol, volume_ratio, metrics.buy_count_60s
            );
            
            let severity = if volume_ratio >= 5.0 { 5 }
                           else if volume_ratio >= 4.0 { 4 }
                           else { 3 };
            
            signals.push(
                TokenSignal::new(mint.to_string(), SignalType::Surge, 60, current_timestamp)
                    .with_severity(severity)
                    .with_score(surge_score)
                    .with_details(details),
            );
        }
    }
    
    // BOT_DROPOFF Detection
    // Sudden decline in bot activity with new wallet influx
    if let Some(prev_bot_count) = previous_bot_count {
        if prev_bot_count >= BOT_DROPOFF_MIN_PREVIOUS_BOTS
            && metrics.unique_wallets_300s >= BOT_DROPOFF_NEW_WALLET_MIN
        {
            let bot_decline = if prev_bot_count > 0 {
                (prev_bot_count - metrics.bot_trades_count_300s) as f64 / prev_bot_count as f64
            } else {
                0.0
            };
            
            if bot_decline >= BOT_DROPOFF_DECLINE_RATIO_MIN {
                // Bot dropoff score based on decline magnitude and new wallets
                let decline_score = bot_decline.min(1.0);
                let wallet_score = (metrics.unique_wallets_300s as f64 / 10.0).min(1.0);
                let dropoff_score = (decline_score + wallet_score) / 2.0;
                
                let details = format!(
                    r#"{{"bot_decline_pct":{:.0},"prev_bot_count":{},"new_wallets":{}}}"#,
                    bot_decline * 100.0, prev_bot_count, metrics.unique_wallets_300s
                );
                
                let severity = if bot_decline >= 0.8 { 4 } else { 3 };
                
                signals.push(
                    TokenSignal::new(mint.to_string(), SignalType::BotDropoff, 300, current_timestamp)
                        .with_severity(severity)
                        .with_score(dropoff_score)
                        .with_details(details),
                );
            }
        }
    }
    
    // DCA_CONVICTION Detection
    // Jupiter DCA BUYs overlap with spot BUYs (coordinated accumulation)
    // Collect spot BUY trades (PumpSwap, BonkSwap, Moonshot)
    let spot_programs = ["PumpSwap", "BonkSwap", "Moonshot"];
    let mut spot_buys = Vec::new();
    for program in &spot_programs {
        if let Some(trades) = trades_by_program.get(*program) {
            for trade in trades {
                if trade.direction == TradeDirection::Buy {
                    spot_buys.push(trade.clone());
                }
            }
        }
    }
    
    // Collect DCA BUY trades
    let mut dca_buys = Vec::new();
    if let Some(dca_trades) = trades_by_program.get("JupiterDCA") {
        for trade in dca_trades {
            if trade.direction == TradeDirection::Buy {
                dca_buys.push(trade.clone());
            }
        }
    }
    
    // Compute correlation if we have both spot and DCA activity
    if !spot_buys.is_empty() && !dca_buys.is_empty() {
        let (overlap_ratio, matched_count) = compute_dca_correlation(&spot_buys, &dca_buys, 60);
        
        // Threshold: 25%+ overlap = DCA_CONVICTION signal
        if overlap_ratio >= 0.25 {
            let details = format!(
                r#"{{"overlap_ratio":{:.2},"dca_buys":{},"spot_buys":{},"matched_dca":{}}}"#,
                overlap_ratio, dca_buys.len(), spot_buys.len(), matched_count
            );
            
            // Severity based on overlap strength
            let severity = if overlap_ratio >= 0.5 { 5 }
                           else if overlap_ratio >= 0.4 { 4 }
                           else if overlap_ratio >= 0.3 { 3 }
                           else { 2 };
            
            signals.push(
                TokenSignal::new(mint.to_string(), SignalType::DcaConviction, 60, current_timestamp)
                    .with_severity(severity)
                    .with_score(overlap_ratio)
                    .with_details(details),
            );
        }
    }
    
    signals
}

impl TokenRollingState {
    /// Create a new rolling state container for a token
    ///
    /// Phase 2: Proper initialization with capacity hints
    pub fn new(mint: String) -> Self {
        Self {
            mint,
            trades_60s: Vec::with_capacity(100),
            trades_300s: Vec::with_capacity(500),
            trades_900s: Vec::with_capacity(1500),
            unique_wallets_300s: HashSet::new(),
            bot_wallets_300s: HashSet::new(),
            trades_by_program: HashMap::new(),
        }
    }

    /// Add a trade to rolling windows
    ///
    /// Phase 2: Implemented
    /// - Pushes trade to all three window buffers
    /// - Updates unique_wallets_300s with trade wallet
    /// - Updates bot_wallets_300s with placeholder logic
    /// - Adds trade to program-specific bucket for DCA correlation
    pub fn add_trade(&mut self, trade: TradeEvent) {
        // Track wallet in 300s window
        self.unique_wallets_300s
            .insert(trade.user_account.clone());

        // TODO: Phase 3 - Implement actual bot detection logic
        // For now, use placeholder: no bot detection
        // Bot detection will be based on:
        // - High frequency trading patterns
        // - MEV transaction characteristics
        // - Known bot wallet addresses
        // Placeholder: never mark as bot in Phase 2
        let _is_bot = false;

        // Add to program-specific bucket for DCA correlation
        self.trades_by_program
            .entry(trade.source_program.clone())
            .or_insert_with(Vec::new)
            .push(trade.clone());

        // Add to all window buffers (most recent trades)
        self.trades_60s.push(trade.clone());
        self.trades_300s.push(trade.clone());
        self.trades_900s.push(trade);
    }

    /// Evict trades older than window cutoffs
    ///
    /// Phase 2: Implemented
    /// - Removes trades outside each window's time range
    /// - Recomputes unique_wallets_300s from remaining trades
    /// - Recomputes bot_wallets_300s from remaining trades
    /// - Evicts old trades from program-specific buckets
    pub fn evict_old_trades(&mut self, now: i64) {
        let cutoff_60s = now - 60;
        let cutoff_300s = now - 300;
        let cutoff_900s = now - 900;

        // Evict from 60s window
        self.trades_60s
            .retain(|trade| trade.timestamp >= cutoff_60s);

        // Evict from 300s window
        self.trades_300s
            .retain(|trade| trade.timestamp >= cutoff_300s);

        // Evict from 900s window
        self.trades_900s
            .retain(|trade| trade.timestamp >= cutoff_900s);

        // Evict from program-specific buckets (use 900s window as longest)
        for trades in self.trades_by_program.values_mut() {
            trades.retain(|trade| trade.timestamp >= cutoff_900s);
        }

        // Recompute unique wallets from remaining 300s trades
        self.unique_wallets_300s.clear();
        for trade in &self.trades_300s {
            self.unique_wallets_300s.insert(trade.user_account.clone());
        }

        // Recompute bot wallets from remaining 300s trades
        // TODO: Phase 3 - Implement actual bot detection logic
        // For now, placeholder: no bot detection
        self.bot_wallets_300s.clear();
    }

    /// Detect trading signals from current rolling state
    ///
    /// Phase 3-B: Signal Detection
    /// Analyzes rolling metrics to detect BREAKOUT, FOCUSED, SURGE, BOT_DROPOFF, DCA_CONVICTION signals
    ///
    /// Arguments:
    /// - `current_timestamp`: Current Unix timestamp for signal creation
    /// - `previous_bot_count`: Optional previous bot trade count for BOT_DROPOFF detection
    ///
    /// Returns: Vec of detected signals
    pub fn detect_signals(
        &self,
        current_timestamp: i64,
        previous_bot_count: Option<i32>,
    ) -> Vec<TokenSignal> {
        let metrics = self.compute_rolling_metrics();
        detect_signals(&self.mint, &metrics, current_timestamp, previous_bot_count, &self.trades_by_program)
    }

    /// Compute rolling metrics from current window state
    ///
    /// Phase 2: Implemented
    /// Phase 3-A: Bot detection integrated
    /// Returns internal metrics snapshot (not AggregatedTokenState)
    pub fn compute_rolling_metrics(&self) -> RollingMetrics {
        // Helper function to compute net flow and counts for a window
        fn compute_window_metrics(
            trades: &[TradeEvent],
        ) -> (f64, i32, i32) {
            let mut net_flow = 0.0;
            let mut buy_count = 0;
            let mut sell_count = 0;

            for trade in trades {
                match trade.direction {
                    TradeDirection::Buy => {
                        net_flow += trade.sol_amount;
                        buy_count += 1;
                    }
                    TradeDirection::Sell => {
                        net_flow -= trade.sol_amount;
                        sell_count += 1;
                    }
                    TradeDirection::Unknown => {
                        // Unknown direction: don't affect net flow
                        // but could be counted separately if needed
                    }
                }
            }

            (net_flow, buy_count, sell_count)
        }

        // Compute metrics for each window
        let (net_flow_60s, buy_count_60s, sell_count_60s) =
            compute_window_metrics(&self.trades_60s);
        let (net_flow_300s, buy_count_300s, sell_count_300s) =
            compute_window_metrics(&self.trades_300s);
        let (net_flow_900s, buy_count_900s, sell_count_900s) =
            compute_window_metrics(&self.trades_900s);

        // Phase 3-A: Detect bot wallets in 300s window
        let (bot_wallets, bot_trades_count) = detect_bot_wallets(&self.trades_300s);

        RollingMetrics {
            net_flow_60s_sol: net_flow_60s,
            net_flow_300s_sol: net_flow_300s,
            net_flow_900s_sol: net_flow_900s,
            buy_count_60s,
            sell_count_60s,
            buy_count_300s,
            sell_count_300s,
            buy_count_900s,
            sell_count_900s,
            unique_wallets_300s: self.unique_wallets_300s.len() as i32,
            bot_wallets_count_300s: bot_wallets.len() as i32,
            bot_trades_count_300s: bot_trades_count,
        }
    }
}

// TODO: Phase 3-C - Integration with database writer (AggregateDbWriter implementation)
// TODO: Phase 3-D - Price and market cap enrichment (price_sol, price_usd, market_cap_usd)
// TODO: Phase 4 - Runtime integration (wire into aggregator binary)

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_bot_detection_no_bots() {
        // Scenario: Normal trading activity, no bot-like behavior
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        
        // Add 5 trades from different wallets with normal spacing
        for i in 0..5 {
            let trade = make_trade(
                base_time + i * 30, // 30s apart
                "test_mint",
                if i % 2 == 0 { TradeDirection::Buy } else { TradeDirection::Sell },
                1.0 + (i as f64 * 0.1),
                &format!("wallet_{}", i),
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: No bots detected (low frequency, diverse wallets, varied amounts)
        assert_eq!(metrics.bot_wallets_count_300s, 0);
        assert_eq!(metrics.bot_trades_count_300s, 0);
        assert_eq!(metrics.unique_wallets_300s, 5);
    }

    #[test]
    fn test_bot_detection_high_frequency() {
        // Scenario: Single wallet making 15 trades in 300s window
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let bot_wallet = "high_freq_bot";
        
        // Add 15 trades from same wallet (exceeds HIGH_FREQ_THRESHOLD of 10)
        for i in 0..15 {
            let trade = make_trade(
                base_time + i * 20, // 20s apart
                "test_mint",
                TradeDirection::Buy,
                1.5,
                bot_wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: Bot detected due to high-frequency trading
        assert_eq!(metrics.bot_wallets_count_300s, 1);
        assert_eq!(metrics.bot_trades_count_300s, 15);
        assert_eq!(metrics.unique_wallets_300s, 1);
    }

    #[test]
    fn test_bot_detection_rapid_consecutive() {
        // Scenario: Wallet making 5 trades with multiple <1s gaps
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let bot_wallet = "rapid_bot";
        
        // Create 5 trades with 4 consecutive <1s gaps
        let timestamps = vec![base_time, base_time + 0, base_time + 1, base_time + 1, base_time + 2];
        
        for &ts in timestamps.iter() {
            let trade = make_trade(
                ts,
                "test_mint",
                TradeDirection::Buy,
                2.0,
                bot_wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: Bot detected due to rapid consecutive trades
        assert_eq!(metrics.bot_wallets_count_300s, 1);
        assert_eq!(metrics.bot_trades_count_300s, 5);
    }

    #[test]
    fn test_bot_detection_alternating_pattern() {
        // Scenario: Wallet alternating BUY/SELL repeatedly
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let bot_wallet = "flip_bot";
        
        // Create 8 trades alternating between BUY and SELL
        for i in 0..8 {
            let direction = if i % 2 == 0 {
                TradeDirection::Buy
            } else {
                TradeDirection::Sell
            };
            
            let trade = make_trade(
                base_time + i * 20,
                "test_mint",
                direction,
                1.0,
                bot_wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: Bot detected due to >70% alternation rate
        assert_eq!(metrics.bot_wallets_count_300s, 1);
        assert_eq!(metrics.bot_trades_count_300s, 8);
    }

    #[test]
    fn test_bot_detection_identical_sizes() {
        // Scenario: Wallet making 5 trades with identical SOL amounts
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let bot_wallet = "identical_bot";
        
        // Create 5 trades with exactly the same amount
        for i in 0..5 {
            let trade = make_trade(
                base_time + i * 30,
                "test_mint",
                TradeDirection::Buy,
                1.23456, // Exact same amount
                bot_wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: Bot detected due to >50% identical trade pairs
        assert_eq!(metrics.bot_wallets_count_300s, 1);
        assert_eq!(metrics.bot_trades_count_300s, 5);
    }

    #[test]
    fn test_bot_detection_mixed_activity() {
        // Scenario: Mix of normal wallets and bot wallets
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        
        // Add 3 normal trades from different wallets
        for i in 0..3 {
            let trade = make_trade(
                base_time + i * 40,
                "test_mint",
                TradeDirection::Buy,
                1.0 + (i as f64 * 0.5),
                &format!("normal_wallet_{}", i),
            );
            state.add_trade(trade);
        }
        
        // Add 12 high-frequency trades from a bot wallet
        for i in 0..12 {
            let trade = make_trade(
                base_time + i * 10,
                "test_mint",
                TradeDirection::Sell,
                0.5,
                "bot_wallet",
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: 1 bot detected, 4 total unique wallets
        assert_eq!(metrics.bot_wallets_count_300s, 1);
        assert_eq!(metrics.bot_trades_count_300s, 12);
        assert_eq!(metrics.unique_wallets_300s, 4);
    }

    #[test]
    fn test_bot_detection_edge_case_empty() {
        // Edge case: No trades in window
        let state = TokenRollingState::new("test_mint".to_string());
        
        let metrics = state.compute_rolling_metrics();

        assert_eq!(metrics.bot_wallets_count_300s, 0);
        assert_eq!(metrics.bot_trades_count_300s, 0);
        assert_eq!(metrics.unique_wallets_300s, 0);
    }

    #[test]
    fn test_bot_detection_threshold_boundary() {
        // Boundary test: Exactly 10 trades (just below high-freq threshold)
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let wallet = "boundary_wallet";
        
        // Add exactly 10 trades (threshold is > 10)
        // Vary amounts slightly to avoid identical-size detection
        for i in 0..10 {
            let trade = make_trade(
                base_time + i * 25,
                "test_mint",
                TradeDirection::Buy,
                1.0 + (i as f64 * 0.01), // Vary amounts: 1.00, 1.01, 1.02, ...
                wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Expect: NOT flagged as bot (needs > 10, not >= 10)
        assert_eq!(metrics.bot_wallets_count_300s, 0);
        assert_eq!(metrics.bot_trades_count_300s, 0);
    }

    #[test]
    fn test_bot_detection_with_unknown_direction() {
        // Test that Unknown direction trades don't break alternation detection
        let mut state = TokenRollingState::new("test_mint".to_string());
        
        let base_time = 1000;
        let wallet = "unknown_wallet";
        
        // Mix of Buy, Sell, and Unknown trades
        let directions = vec![
            TradeDirection::Buy,
            TradeDirection::Unknown,
            TradeDirection::Sell,
            TradeDirection::Unknown,
            TradeDirection::Buy,
        ];
        
        // Vary amounts to avoid identical-size detection
        for (i, direction) in directions.iter().enumerate() {
            let trade = make_trade(
                base_time + i as i64 * 20,
                "test_mint",
                *direction,
                1.0 + (i as f64 * 0.1), // Vary amounts: 1.0, 1.1, 1.2, 1.3, 1.4
                wallet,
            );
            state.add_trade(trade);
        }

        let metrics = state.compute_rolling_metrics();

        // Should not crash, Unknown trades are excluded from alternation check
        // This wallet should not be flagged (only 2 Buy/Sell, not enough for pattern)
        assert_eq!(metrics.bot_wallets_count_300s, 0);
    }

    // === Phase 3-B: Signal Detection Tests ===

    #[test]
    fn test_signal_detection_breakout() {
        // Scenario: Sharp volume spike with wallet growth → BREAKOUT signal
        let mut state = TokenRollingState::new("breakout_mint".to_string());
        
        let base_time = 10000;
        
        // Create BREAKOUT conditions:
        // - High net flow in 60s (> 5 SOL)
        // - Multiple unique wallets (≥ 5)
        // - High buy ratio (> 75%)
        
        // Add 20 BUY trades from 8 different wallets (60s window)
        for i in 0..20 {
            let trade = make_trade(
                base_time + i as i64 * 3, // 3s apart (all within 60s)
                "breakout_mint",
                TradeDirection::Buy,
                0.5 + (i as f64 * 0.05), // Vary amounts: 0.5-1.45 SOL
                &format!("wallet_{}", i % 8), // 8 unique wallets
            );
            state.add_trade(trade);
        }
        
        // Add 2 SELL trades to make it realistic
        for i in 0..2 {
            let trade = make_trade(
                base_time + 20 + i,
                "breakout_mint",
                TradeDirection::Sell,
                0.3,
                &format!("seller_{}", i),
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 60, None);

        // Expect: BREAKOUT detected
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|s| s.signal_type == SignalType::Breakout));
        
        let breakout = signals.iter().find(|s| s.signal_type == SignalType::Breakout).unwrap();
        assert_eq!(breakout.window_seconds, 60);
        assert!(breakout.score.is_some());
        assert!(breakout.score.unwrap() > 0.5); // Should have decent score
        assert!(breakout.details_json.is_some());
    }

    #[test]
    fn test_signal_detection_surge() {
        // Scenario: Explosive volume spike (60s >> average 300s) → SURGE signal
        let mut state = TokenRollingState::new("surge_mint".to_string());
        
        let base_time = 10000;
        
        // First, establish baseline 300s volume (low activity)
        for i in 0..5 {
            let trade = make_trade(
                base_time - 200 + i * 30, // Older trades (200-50s ago)
                "surge_mint",
                TradeDirection::Buy,
                0.5,
                &format!("baseline_wallet_{}", i),
            );
            state.add_trade(trade);
        }
        
        // Then, explosive 60s volume spike
        for i in 0..15 {
            let trade = make_trade(
                base_time + i as i64 * 4, // Recent trades (0-56s ago)
                "surge_mint",
                TradeDirection::Buy,
                1.0, // Total: 15 SOL in 60s vs ~2.5 SOL in 300s baseline
                &format!("surge_wallet_{}", i),
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 60, None);

        // Expect: SURGE detected (60s volume >> 300s average)
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|s| s.signal_type == SignalType::Surge));
        
        let surge = signals.iter().find(|s| s.signal_type == SignalType::Surge).unwrap();
        assert_eq!(surge.window_seconds, 60);
        assert!(surge.score.is_some());
        assert!(surge.severity >= 3); // High severity
        assert!(surge.details_json.is_some());
    }

    #[test]
    fn test_signal_detection_focused() {
        // Scenario: Concentrated buying from few wallets, no bots → FOCUSED signal
        let mut state = TokenRollingState::new("focused_mint".to_string());
        
        let base_time = 10000;
        
        // Create FOCUSED conditions:
        // - Moderate volume (> 3 SOL)
        // - Few unique wallets (≤ 10, preferably ≤ 3)
        // - Low bot activity (< 20%)
        
        // Add 12 trades from only 2 wallets (high concentration)
        for i in 0..12 {
            let trade = make_trade(
                base_time + i as i64 * 20,
                "focused_mint",
                TradeDirection::Buy,
                0.4 + (i as f64 * 0.02), // Total: ~5.5 SOL
                if i < 6 { "whale_1" } else { "whale_2" },
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 300, None);

        // Expect: FOCUSED detected (concentrated, no bots)
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|s| s.signal_type == SignalType::Focused));
        
        let focused = signals.iter().find(|s| s.signal_type == SignalType::Focused).unwrap();
        assert_eq!(focused.window_seconds, 300);
        assert!(focused.score.is_some());
        assert_eq!(focused.severity, 4); // High severity for only 2 wallets
        assert!(focused.details_json.is_some());
    }

    #[test]
    fn test_signal_detection_bot_dropoff() {
        // Scenario: Bot activity drops significantly → BOT_DROPOFF signal
        let mut state = TokenRollingState::new("dropoff_mint".to_string());
        
        let base_time = 10000;
        
        // Create BOT_DROPOFF conditions:
        // - Previous bot count was high (≥ 5)
        // - Current bot count is low
        // - New wallets entering (≥ 3)
        
        // Add 4 normal trades from different wallets
        for i in 0..4 {
            let trade = make_trade(
                base_time + i as i64 * 50,
                "dropoff_mint",
                TradeDirection::Buy,
                1.0 + (i as f64 * 0.1),
                &format!("human_wallet_{}", i),
            );
            state.add_trade(trade);
        }

        // Simulate previous state had 10 bot trades
        let previous_bot_count = Some(10);
        
        let signals = state.detect_signals(base_time + 300, previous_bot_count);

        // Expect: BOT_DROPOFF detected (bot count: 10 → 0, with 4 new wallets)
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|s| s.signal_type == SignalType::BotDropoff));
        
        let dropoff = signals.iter().find(|s| s.signal_type == SignalType::BotDropoff).unwrap();
        assert_eq!(dropoff.window_seconds, 300);
        assert!(dropoff.score.is_some());
        assert!(dropoff.severity >= 3);
        assert!(dropoff.details_json.is_some());
    }

    #[test]
    fn test_signal_detection_no_signals() {
        // Scenario: Normal trading activity without signal-worthy patterns
        let mut state = TokenRollingState::new("normal_mint".to_string());
        
        let base_time = 10000;
        
        // Add normal trades: moderate volume, balanced buy/sell, varied wallets
        for i in 0..8 {
            let direction = if i % 3 == 0 {
                TradeDirection::Sell
            } else {
                TradeDirection::Buy
            };
            
            let trade = make_trade(
                base_time + i as i64 * 30,
                "normal_mint",
                direction,
                0.5 + (i as f64 * 0.1),
                &format!("wallet_{}", i),
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 300, None);

        // Expect: No signals detected (normal activity)
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_signal_detection_multiple_signals() {
        // Scenario: Conditions trigger multiple signal types simultaneously
        let mut state = TokenRollingState::new("multi_signal_mint".to_string());
        
        let base_time = 10000;
        
        // Create conditions for both BREAKOUT and SURGE:
        // - High volume spike (SURGE)
        // - Wallet growth (BREAKOUT)
        // - High buy ratio (BREAKOUT)
        
        // Baseline trades
        for i in 0..3 {
            let trade = make_trade(
                base_time - 200 + i * 50,
                "multi_signal_mint",
                TradeDirection::Buy,
                0.3,
                &format!("old_wallet_{}", i),
            );
            state.add_trade(trade);
        }
        
        // Massive spike in 60s window
        for i in 0..25 {
            let trade = make_trade(
                base_time + i as i64 * 2,
                "multi_signal_mint",
                TradeDirection::Buy,
                0.8,
                &format!("new_wallet_{}", i % 12), // 12 unique wallets
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 60, None);

        // Expect: Multiple signals (at least BREAKOUT, possibly SURGE)
        assert!(signals.len() >= 1);
        assert!(signals.iter().any(|s| s.signal_type == SignalType::Breakout));
        
        // All signals should have proper metadata
        for signal in &signals {
            assert_eq!(signal.mint, "multi_signal_mint");
            assert!(signal.score.is_some());
            assert!(signal.score.unwrap() >= 0.0 && signal.score.unwrap() <= 1.0);
            assert!(signal.severity >= 1 && signal.severity <= 5);
        }
    }

    #[test]
    fn test_signal_detection_edge_case_empty_state() {
        // Edge case: No trades, no signals
        let state = TokenRollingState::new("empty_mint".to_string());
        
        let signals = state.detect_signals(10000, None);

        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_signal_detection_thresholds_boundary() {
        // Test exact threshold boundaries
        let mut state = TokenRollingState::new("threshold_mint".to_string());
        
        let base_time = 10000;
        
        // Create conditions just below BREAKOUT threshold
        // BREAKOUT needs: net_flow_60s > 5.0, wallets >= 5, buy_ratio > 0.75
        
        // Add exactly 5.0 SOL net flow (threshold is > 5.0)
        for i in 0..10 {
            let trade = make_trade(
                base_time + i as i64 * 5,
                "threshold_mint",
                TradeDirection::Buy,
                0.5, // Total: 5.0 SOL
                &format!("wallet_{}", i % 5), // Exactly 5 wallets
            );
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 60, None);

        // Expect: No BREAKOUT (5.0 is not > 5.0)
        assert!(!signals.iter().any(|s| s.signal_type == SignalType::Breakout));
    }

    // === DCA_CONVICTION Tests ===

    #[test]
    fn test_dca_conviction_aligned_trades() {
        // Scenario: DCA BUYs align with spot BUYs → DCA_CONVICTION signal
        let mut state = TokenRollingState::new("dca_conviction_mint".to_string());
        
        let base_time = 10000;
        
        // Add PumpSwap BUY trades
        for i in 0..10 {
            let trade = TradeEvent {
                timestamp: base_time + i * 10,
                mint: "dca_conviction_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 1.0,
                token_amount: 1000.0,
                token_decimals: 6,
                user_account: format!("spot_wallet_{}", i),
                source_program: "PumpSwap".to_string(),
            };
            state.add_trade(trade);
        }
        
        // Add Jupiter DCA BUY trades that overlap with spot trades (within ±60s)
        for i in 0..5 {
            let trade = TradeEvent {
                timestamp: base_time + i * 20 + 5, // Offset by 5s (within 60s window)
                mint: "dca_conviction_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 0.5,
                token_amount: 500.0,
                token_decimals: 6,
                user_account: format!("dca_wallet_{}", i),
                source_program: "JupiterDCA".to_string(),
            };
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 120, None);

        // Expect: DCA_CONVICTION detected (all 5 DCA trades overlap with spot trades)
        assert!(!signals.is_empty(), "Should detect DCA_CONVICTION signal");
        assert!(signals.iter().any(|s| s.signal_type == SignalType::DcaConviction));
        
        let dca_signal = signals.iter().find(|s| s.signal_type == SignalType::DcaConviction).unwrap();
        assert_eq!(dca_signal.window_seconds, 60);
        assert!(dca_signal.score.is_some());
        assert!(dca_signal.score.unwrap() >= 0.25); // Above threshold
        assert!(dca_signal.details_json.is_some());
        
        // Verify details contain expected fields
        let details = dca_signal.details_json.as_ref().unwrap();
        assert!(details.contains("overlap_ratio"));
        assert!(details.contains("dca_buys"));
        assert!(details.contains("spot_buys"));
    }

    #[test]
    fn test_dca_conviction_no_overlap() {
        // Scenario: DCA BUYs but no overlapping spot BUYs → no signal
        let mut state = TokenRollingState::new("no_overlap_mint".to_string());
        
        let base_time = 10000;
        
        // Add PumpSwap BUY trades (early window)
        for i in 0..5 {
            let trade = TradeEvent {
                timestamp: base_time + i * 10,
                mint: "no_overlap_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 1.0,
                token_amount: 1000.0,
                token_decimals: 6,
                user_account: format!("spot_wallet_{}", i),
                source_program: "PumpSwap".to_string(),
            };
            state.add_trade(trade);
        }
        
        // Add Jupiter DCA BUY trades much later (> 60s gap)
        for i in 0..5 {
            let trade = TradeEvent {
                timestamp: base_time + 200 + i * 10, // 200s+ later (outside ±60s window)
                mint: "no_overlap_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 0.5,
                token_amount: 500.0,
                token_decimals: 6,
                user_account: format!("dca_wallet_{}", i),
                source_program: "JupiterDCA".to_string(),
            };
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 300, None);

        // Expect: No DCA_CONVICTION (no overlap)
        assert!(!signals.iter().any(|s| s.signal_type == SignalType::DcaConviction));
    }

    #[test]
    fn test_dca_conviction_below_threshold() {
        // Scenario: Only 20% DCA overlap (below 25% threshold) → no signal
        let mut state = TokenRollingState::new("below_threshold_mint".to_string());
        
        let base_time = 10000;
        
        // Add spot BUY trades
        for i in 0..3 {
            let trade = TradeEvent {
                timestamp: base_time + i * 20,
                mint: "below_threshold_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 1.0,
                token_amount: 1000.0,
                token_decimals: 6,
                user_account: format!("spot_wallet_{}", i),
                source_program: "BonkSwap".to_string(),
            };
            state.add_trade(trade);
        }
        
        // Add 5 DCA trades, only 1 overlaps (20% overlap)
        for i in 0..5 {
            let timestamp = if i == 0 {
                base_time + 10 // First one overlaps
            } else {
                base_time + 500 + i * 10 // Rest are far away
            };
            
            let trade = TradeEvent {
                timestamp,
                mint: "below_threshold_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 0.5,
                token_amount: 500.0,
                token_decimals: 6,
                user_account: format!("dca_wallet_{}", i),
                source_program: "JupiterDCA".to_string(),
            };
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 600, None);

        // Expect: No DCA_CONVICTION (20% < 25% threshold)
        assert!(!signals.iter().any(|s| s.signal_type == SignalType::DcaConviction));
    }

    #[test]
    fn test_dca_conviction_multiple_spot_programs() {
        // Scenario: DCA overlaps with trades from multiple spot programs
        let mut state = TokenRollingState::new("multi_spot_mint".to_string());
        
        let base_time = 10000;
        
        // Add BUYs from all three spot programs
        let spot_programs = ["PumpSwap", "BonkSwap", "Moonshot"];
        for (idx, program) in spot_programs.iter().enumerate() {
            for i in 0..3 {
                let trade = TradeEvent {
                    timestamp: base_time + (idx * 30) as i64 + i * 10,
                    mint: "multi_spot_mint".to_string(),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: format!("{}_wallet_{}", program, i),
                    source_program: program.to_string(),
                };
                state.add_trade(trade);
            }
        }
        
        // Add DCA BUYs that overlap with all three programs
        for i in 0..4 {
            let trade = TradeEvent {
                timestamp: base_time + i * 25 + 5,
                mint: "multi_spot_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 0.5,
                token_amount: 500.0,
                token_decimals: 6,
                user_account: format!("dca_wallet_{}", i),
                source_program: "JupiterDCA".to_string(),
            };
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 120, None);

        // Expect: DCA_CONVICTION detected (overlap with multiple spot programs)
        assert!(signals.iter().any(|s| s.signal_type == SignalType::DcaConviction));
    }

    #[test]
    fn test_dca_conviction_only_buy_direction() {
        // Scenario: SELL trades should NOT be considered for DCA_CONVICTION
        let mut state = TokenRollingState::new("sell_test_mint".to_string());
        
        let base_time = 10000;
        
        // Add spot SELL trades (should be ignored)
        for i in 0..5 {
            let trade = TradeEvent {
                timestamp: base_time + i * 10,
                mint: "sell_test_mint".to_string(),
                direction: TradeDirection::Sell, // SELL direction
                sol_amount: 1.0,
                token_amount: 1000.0,
                token_decimals: 6,
                user_account: format!("spot_wallet_{}", i),
                source_program: "PumpSwap".to_string(),
            };
            state.add_trade(trade);
        }
        
        // Add DCA BUY trades that would overlap if SELLs counted
        for i in 0..3 {
            let trade = TradeEvent {
                timestamp: base_time + i * 10 + 5,
                mint: "sell_test_mint".to_string(),
                direction: TradeDirection::Buy,
                sol_amount: 0.5,
                token_amount: 500.0,
                token_decimals: 6,
                user_account: format!("dca_wallet_{}", i),
                source_program: "JupiterDCA".to_string(),
            };
            state.add_trade(trade);
        }

        let signals = state.detect_signals(base_time + 60, None);

        // Expect: No DCA_CONVICTION (spot SELLs don't count)
        assert!(!signals.iter().any(|s| s.signal_type == SignalType::DcaConviction));
    }

    #[test]
    fn test_dca_conviction_severity_levels() {
        // Test severity levels based on overlap ratio
        // Use exact counts to avoid rounding issues
        let test_cases = vec![
            (2, 10, 2),  // 2/10 = 0.20 → below threshold, no signal expected
            (3, 10, 3),  // 3/10 = 0.30 → severity 3
            (4, 10, 4),  // 4/10 = 0.40 → severity 4
            (5, 10, 5),  // 5/10 = 0.50 → severity 5
        ];
        
        for (overlapping_count, total_count, expected_severity) in test_cases {
            let overlap_ratio = overlapping_count as f64 / total_count as f64;
            let mut state = TokenRollingState::new(format!("severity_test_{:.2}", overlap_ratio));
            let base_time = 10000;
            
            // Add spot BUYs
            for i in 0..10 {
                let trade = TradeEvent {
                    timestamp: base_time + i * 5,
                    mint: format!("severity_test_{:.2}", overlap_ratio),
                    direction: TradeDirection::Buy,
                    sol_amount: 1.0,
                    token_amount: 1000.0,
                    token_decimals: 6,
                    user_account: format!("spot_{}", i),
                    source_program: "PumpSwap".to_string(),
                };
                state.add_trade(trade);
            }
            
            // Add DCA BUYs with exact overlap count
            for i in 0..total_count {
                let timestamp = if i < overlapping_count {
                    base_time + i as i64 * 5 + 2 // Overlapping
                } else {
                    base_time + 500 + i as i64 * 10 // Non-overlapping
                };
                
                let trade = TradeEvent {
                    timestamp,
                    mint: format!("severity_test_{:.2}", overlap_ratio),
                    direction: TradeDirection::Buy,
                    sol_amount: 0.5,
                    token_amount: 500.0,
                    token_decimals: 6,
                    user_account: format!("dca_{}", i),
                    source_program: "JupiterDCA".to_string(),
                };
                state.add_trade(trade);
            }
            
            let signals = state.detect_signals(base_time + 600, None);
            
            // 0.20 ratio is below 0.25 threshold, should NOT emit signal
            if overlap_ratio < 0.25 {
                assert!(!signals.iter().any(|s| s.signal_type == SignalType::DcaConviction),
                    "Overlap ratio {:.2} should NOT emit signal (below threshold)", overlap_ratio);
            } else {
                // Above threshold, should emit signal with correct severity
                if let Some(dca_signal) = signals.iter().find(|s| s.signal_type == SignalType::DcaConviction) {
                    assert_eq!(dca_signal.severity, expected_severity, 
                        "Overlap ratio {:.2} should have severity {}", overlap_ratio, expected_severity);
                } else {
                    panic!("Expected DCA_CONVICTION signal for ratio {:.2}", overlap_ratio);
                }
            }
        }
    }
}
