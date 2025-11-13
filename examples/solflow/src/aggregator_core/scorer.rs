//! Signal scoring for uptrend detection

use super::window::WindowMetrics;

pub struct SignalScorer;

impl SignalScorer {
    pub fn new() -> Self {
        Self
    }

    /// Compute uptrend score (0.0-1.0) based on multiple factors
    ///
    /// # Factors
    /// - Net flow (positive = buying pressure)
    /// - Buy/sell ratio (> 1.0 = more buying than selling)
    /// - Trade velocity (trades per minute)
    /// - Wallet diversity (unique buyers / total buys)
    ///
    /// # Returns
    /// Score between 0.0 (no uptrend) and 1.0 (strong uptrend)
    pub fn compute_uptrend_score(&self, metrics: &WindowMetrics) -> f64 {
        let total_volume = metrics.buy_volume_sol + metrics.sell_volume_sol;
        if total_volume == 0.0 {
            return 0.0;
        }

        // Component 1: Net flow normalized to [-1, 1] via sigmoid
        let net_flow_norm = sigmoid(metrics.net_flow_sol / 10.0);

        // Component 2: Buy ratio (0.0-1.0)
        let ratio_norm = metrics.buy_volume_sol / total_volume;

        // Component 3: Trade velocity (trades per minute)
        let window_minutes = metrics.window.duration_secs() as f64 / 60.0;
        let velocity = metrics.trades.len() as f64 / window_minutes;
        let velocity_norm = sigmoid(velocity);

        // Component 4: Wallet diversity (prevents wash trading)
        let wallet_diversity = if metrics.buy_count > 0 {
            (metrics.unique_buyers.len() as f64 / metrics.buy_count as f64).min(1.0)
        } else {
            0.0
        };

        // Weighted average
        let score =
            net_flow_norm * 0.3 + ratio_norm * 0.3 + velocity_norm * 0.2 + wallet_diversity * 0.2;

        score.clamp(0.0, 1.0)
    }
}

/// Sigmoid function for normalizing unbounded values to [0, 1]
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator_core::normalizer::{Trade, TradeAction};
    use crate::aggregator_core::window::{WindowMetrics, WindowSize};

    fn create_test_trade(action: TradeAction, sol_amount: f64, user: &str) -> Trade {
        Trade {
            timestamp: 1000,
            signature: "test_sig".to_string(),
            program_name: "Test".to_string(),
            action,
            mint: "test_mint".to_string(),
            sol_amount,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: Some(user.to_string()),
        }
    }

    #[test]
    fn test_strong_uptrend() {
        let mut metrics = WindowMetrics::new("test_mint".to_string(), WindowSize::Hour1);

        // High buy volume, multiple unique buyers
        metrics.add_trade(create_test_trade(TradeAction::Buy, 100.0, "user1"));
        metrics.add_trade(create_test_trade(TradeAction::Buy, 100.0, "user2"));
        metrics.add_trade(create_test_trade(TradeAction::Buy, 100.0, "user3"));
        metrics.add_trade(create_test_trade(TradeAction::Sell, 10.0, "user4"));

        let scorer = SignalScorer::new();
        let score = scorer.compute_uptrend_score(&metrics);

        assert!(score > 0.7, "Strong uptrend should score > 0.7, got {}", score);
    }

    #[test]
    fn test_weak_uptrend() {
        let mut metrics = WindowMetrics::new("test_mint".to_string(), WindowSize::Hour1);

        metrics.add_trade(create_test_trade(TradeAction::Buy, 10.0, "user1"));
        metrics.add_trade(create_test_trade(TradeAction::Sell, 8.0, "user2"));

        let scorer = SignalScorer::new();
        let score = scorer.compute_uptrend_score(&metrics);

        assert!(score < 0.7 && score > 0.3, "Weak uptrend should score 0.3-0.7, got {}", score);
    }

    #[test]
    fn test_downtrend() {
        let mut metrics = WindowMetrics::new("test_mint".to_string(), WindowSize::Hour1);

        metrics.add_trade(create_test_trade(TradeAction::Sell, 100.0, "user1"));
        metrics.add_trade(create_test_trade(TradeAction::Sell, 100.0, "user2"));
        metrics.add_trade(create_test_trade(TradeAction::Buy, 10.0, "user3"));

        let scorer = SignalScorer::new();
        let score = scorer.compute_uptrend_score(&metrics);

        assert!(score < 0.5, "Downtrend should score < 0.5, got {}", score);
    }
}
