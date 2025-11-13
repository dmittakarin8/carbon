use {
    crate::state::Trade,
    std::{
        collections::HashMap,
        time::{SystemTime, UNIX_EPOCH},
    },
};

/// Rolling time-window volume aggregator
/// 
/// Uses strict time-cutoff (not EMA) for rolling windows.
/// Trades outside the time window are excluded from calculations.
pub struct VolumeAggregator {
    /// Trades organized by token mint
    trades_by_mint: HashMap<String, Vec<Trade>>,
    /// Time windows in seconds
    windows: Vec<u64>,
}

impl VolumeAggregator {
    pub fn new() -> Self {
        Self {
            trades_by_mint: HashMap::new(),
            windows: vec![60, 300, 900], // 1m, 5m, 15m
        }
    }

    /// Add a trade to the aggregator
    pub fn add_trade(&mut self, trade: Trade) {
        self.trades_by_mint
            .entry(trade.mint.clone())
            .or_insert_with(Vec::new)
            .push(trade);
        
        // Cleanup old trades periodically (keep only trades within max window)
        self.cleanup_old_trades();
    }

    /// Get net volume for a token (buy volume - sell volume)
    #[allow(dead_code)]
    pub fn get_net_volume(&self, mint: &str) -> f64 {
        self.get_buy_volume(mint) - self.get_sell_volume(mint)
    }

    /// Get buy volume for a token
    #[allow(dead_code)]
    pub fn get_buy_volume(&self, mint: &str) -> f64 {
        self.get_volume_in_window(mint, None, |t| matches!(t.direction, crate::trade_extractor::TradeKind::Buy))
    }

    /// Get sell volume for a token
    #[allow(dead_code)]
    pub fn get_sell_volume(&self, mint: &str) -> f64 {
        self.get_volume_in_window(mint, None, |t| matches!(t.direction, crate::trade_extractor::TradeKind::Sell))
    }

    /// Get volume for a specific time window (strict cutoff)
    /// 
    /// window_seconds: None = all time, Some(n) = last n seconds
    #[allow(dead_code)]
    pub fn get_volume_in_window<F>(&self, mint: &str, window_seconds: Option<u64>, filter: F) -> f64
    where
        F: Fn(&Trade) -> bool,
    {
        let trades = match self.trades_by_mint.get(mint) {
            Some(trades) => trades,
            None => return 0.0,
        };

        let cutoff_time = if let Some(window) = window_seconds {
            current_timestamp() - window as i64
        } else {
            0 // All time
        };

        trades
            .iter()
            .filter(|trade| trade.timestamp >= cutoff_time)
            .filter(|trade| filter(trade))
            .map(|trade| trade.sol_amount)
            .sum()
    }

    /// Get volume for 1-minute window
    #[allow(dead_code)]
    pub fn get_volume_1m(&self, mint: &str) -> f64 {
        self.get_volume_in_window(mint, Some(60), |_| true)
    }

    /// Get volume for 5-minute window
    #[allow(dead_code)]
    pub fn get_volume_5m(&self, mint: &str) -> f64 {
        self.get_volume_in_window(mint, Some(300), |_| true)
    }

    /// Get volume for 15-minute window
    #[allow(dead_code)]
    pub fn get_volume_15m(&self, mint: &str) -> f64 {
        self.get_volume_in_window(mint, Some(900), |_| true)
    }

    /// Cleanup trades older than the maximum window
    fn cleanup_old_trades(&mut self) {
        let max_window = self.windows.iter().max().copied().unwrap_or(900);
        let cutoff_time = current_timestamp() - (max_window * 2) as i64; // Keep 2x max window for safety

        for trades in self.trades_by_mint.values_mut() {
            trades.retain(|trade| trade.timestamp >= cutoff_time);
        }
    }
}

impl Default for VolumeAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to get current Unix timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trade_extractor::TradeKind;

    fn create_test_trade(mint: &str, direction: TradeKind, sol_amount: f64, timestamp: i64) -> Trade {
        Trade {
            signature: solana_signature::Signature::default(),
            timestamp,
            mint: mint.to_string(),
            direction,
            sol_amount,
            token_amount: 0.0,
            token_decimals: 9,
        }
    }

    #[test]
    fn test_net_volume() {
        let mut agg = VolumeAggregator::new();
        let now = current_timestamp();
        
        agg.add_trade(create_test_trade("mint1", TradeKind::Buy, 1.0, now));
        agg.add_trade(create_test_trade("mint1", TradeKind::Sell, 0.5, now));
        
        assert_eq!(agg.get_net_volume("mint1"), 0.5);
    }

    #[test]
    fn test_time_window() {
        let mut agg = VolumeAggregator::new();
        let now = current_timestamp();
        
        // Add trade 2 minutes ago (outside 1m window)
        agg.add_trade(create_test_trade("mint1", TradeKind::Buy, 1.0, now - 120));
        // Add trade now (inside 1m window)
        agg.add_trade(create_test_trade("mint1", TradeKind::Buy, 2.0, now));
        
        // Should only include recent trade
        assert_eq!(agg.get_volume_1m("mint1"), 2.0);
    }
}

