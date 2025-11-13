//! Rolling time window aggregation for trade metrics

use super::normalizer::{Trade, TradeAction};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowSize {
    Min15,
    Hour1,
    Hour2,
    Hour4,
}

impl WindowSize {
    pub fn as_str(&self) -> &'static str {
        match self {
            WindowSize::Min15 => "15m",
            WindowSize::Hour1 => "1h",
            WindowSize::Hour2 => "2h",
            WindowSize::Hour4 => "4h",
        }
    }

    pub fn duration_secs(&self) -> i64 {
        match self {
            WindowSize::Min15 => 15 * 60,
            WindowSize::Hour1 => 60 * 60,
            WindowSize::Hour2 => 2 * 60 * 60,
            WindowSize::Hour4 => 4 * 60 * 60,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "15m" => Some(WindowSize::Min15),
            "1h" => Some(WindowSize::Hour1),
            "2h" => Some(WindowSize::Hour2),
            "4h" => Some(WindowSize::Hour4),
            _ => None,
        }
    }

    pub fn all() -> [WindowSize; 4] {
        [
            WindowSize::Min15,
            WindowSize::Hour1,
            WindowSize::Hour2,
            WindowSize::Hour4,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct WindowMetrics {
    pub mint: String,
    pub window: WindowSize,
    pub net_flow_sol: f64,
    pub buy_volume_sol: f64,
    pub sell_volume_sol: f64,
    pub buy_count: usize,
    pub sell_count: usize,
    pub unique_buyers: HashSet<String>,
    pub unique_sellers: HashSet<String>,
    pub trades: Vec<Trade>,
}

impl WindowMetrics {
    pub fn new(mint: String, window: WindowSize) -> Self {
        Self {
            mint,
            window,
            net_flow_sol: 0.0,
            buy_volume_sol: 0.0,
            sell_volume_sol: 0.0,
            buy_count: 0,
            sell_count: 0,
            unique_buyers: HashSet::new(),
            unique_sellers: HashSet::new(),
            trades: Vec::new(),
        }
    }

    pub fn add_trade(&mut self, trade: Trade) {
        match trade.action {
            TradeAction::Buy => {
                self.buy_volume_sol += trade.sol_amount;
                self.buy_count += 1;
                self.net_flow_sol += trade.sol_amount;
                if let Some(ref user) = trade.user_account {
                    self.unique_buyers.insert(user.clone());
                }
            }
            TradeAction::Sell => {
                self.sell_volume_sol += trade.sol_amount;
                self.sell_count += 1;
                self.net_flow_sol -= trade.sol_amount;
                if let Some(ref user) = trade.user_account {
                    self.unique_sellers.insert(user.clone());
                }
            }
        }

        self.trades.push(trade);
    }

    pub fn evict_old_trades(&mut self, cutoff_timestamp: i64) {
        self.trades.retain(|t| t.timestamp > cutoff_timestamp);
        self.recalculate();
    }

    fn recalculate(&mut self) {
        self.net_flow_sol = 0.0;
        self.buy_volume_sol = 0.0;
        self.sell_volume_sol = 0.0;
        self.buy_count = 0;
        self.sell_count = 0;
        self.unique_buyers.clear();
        self.unique_sellers.clear();

        for trade in &self.trades {
            match trade.action {
                TradeAction::Buy => {
                    self.buy_volume_sol += trade.sol_amount;
                    self.buy_count += 1;
                    self.net_flow_sol += trade.sol_amount;
                    if let Some(ref user) = trade.user_account {
                        self.unique_buyers.insert(user.clone());
                    }
                }
                TradeAction::Sell => {
                    self.sell_volume_sol += trade.sol_amount;
                    self.sell_count += 1;
                    self.net_flow_sol -= trade.sol_amount;
                    if let Some(ref user) = trade.user_account {
                        self.unique_sellers.insert(user.clone());
                    }
                }
            }
        }
    }
}

pub struct TimeWindowAggregator {
    windows: HashMap<(String, WindowSize), WindowMetrics>,
}

impl TimeWindowAggregator {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }

    pub fn add_trade(&mut self, trade: Trade) {
        for window in WindowSize::all() {
            let key = (trade.mint.clone(), window);
            self.windows
                .entry(key)
                .or_insert_with(|| WindowMetrics::new(trade.mint.clone(), window))
                .add_trade(trade.clone());
        }
    }

    pub fn evict_old_trades(&mut self, current_timestamp: i64) {
        for ((_, window), metrics) in self.windows.iter_mut() {
            let cutoff = current_timestamp - window.duration_secs();
            metrics.evict_old_trades(cutoff);
        }

        self.windows.retain(|_, metrics| !metrics.trades.is_empty());
    }

    pub fn get_all_metrics(&self) -> Vec<(&String, &WindowSize, &WindowMetrics)> {
        self.windows
            .iter()
            .map(|((mint, window), metrics)| (mint, window, metrics))
            .collect()
    }

    pub fn get_metrics(&self, mint: &str, window: WindowSize) -> Option<&WindowMetrics> {
        self.windows.get(&(mint.to_string(), window))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_trade(timestamp: i64, action: TradeAction, sol_amount: f64) -> Trade {
        Trade {
            timestamp,
            signature: "test_sig".to_string(),
            program_name: "Test".to_string(),
            action,
            mint: "test_mint".to_string(),
            sol_amount,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: Some("user1".to_string()),
        }
    }

    #[test]
    fn test_window_metrics_add_trade() {
        let mut metrics = WindowMetrics::new("test_mint".to_string(), WindowSize::Hour1);

        metrics.add_trade(create_test_trade(1000, TradeAction::Buy, 10.0));
        metrics.add_trade(create_test_trade(1100, TradeAction::Sell, 5.0));

        assert_eq!(metrics.buy_count, 1);
        assert_eq!(metrics.sell_count, 1);
        assert_eq!(metrics.buy_volume_sol, 10.0);
        assert_eq!(metrics.sell_volume_sol, 5.0);
        assert_eq!(metrics.net_flow_sol, 5.0);
        assert_eq!(metrics.trades.len(), 2);
    }

    #[test]
    fn test_window_eviction() {
        let mut metrics = WindowMetrics::new("test_mint".to_string(), WindowSize::Hour1);

        metrics.add_trade(create_test_trade(1000, TradeAction::Buy, 10.0));
        metrics.add_trade(create_test_trade(4000, TradeAction::Buy, 20.0));

        metrics.evict_old_trades(3000);

        assert_eq!(metrics.trades.len(), 1);
        assert_eq!(metrics.buy_volume_sol, 20.0);
    }

    #[test]
    fn test_aggregator_multiple_windows() {
        let mut agg = TimeWindowAggregator::new();

        let trade = create_test_trade(1000, TradeAction::Buy, 10.0);
        agg.add_trade(trade);

        assert_eq!(agg.windows.len(), 4);
        assert!(agg.get_metrics("test_mint", WindowSize::Min15).is_some());
        assert!(agg.get_metrics("test_mint", WindowSize::Hour1).is_some());
    }
}
