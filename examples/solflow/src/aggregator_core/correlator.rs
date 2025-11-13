//! Cross-stream correlation engine for detecting DCA accumulation patterns

use super::normalizer::Trade;
use std::collections::BTreeMap;

pub struct CorrelationEngine {
    correlation_window_secs: i64,
}

impl CorrelationEngine {
    pub fn new(correlation_window_secs: i64) -> Self {
        Self {
            correlation_window_secs,
        }
    }

    /// Compute percentage of PumpSwap BUY volume that occurs within ±N seconds of Jupiter DCA buys
    ///
    /// This metric indicates whether spot buying (PumpSwap) is correlated with DCA activity,
    /// suggesting coordinated accumulation.
    ///
    /// # Arguments
    /// * `pumpswap_buys` - PumpSwap BUY trades for a specific mint
    /// * `dca_buys` - Jupiter DCA BUY trades for the same mint
    ///
    /// # Returns
    /// Percentage (0.0-100.0) of PumpSwap buy volume that overlaps with DCA activity
    pub fn compute_dca_overlap(&self, pumpswap_buys: &[Trade], dca_buys: &[Trade]) -> f64 {
        if pumpswap_buys.is_empty() {
            return 0.0;
        }

        // Build index of DCA trades by timestamp for efficient range queries
        let dca_index: BTreeMap<i64, &Trade> =
            dca_buys.iter().map(|t| (t.timestamp, t)).collect();

        if dca_index.is_empty() {
            return 0.0;
        }

        let total_pumpswap_volume: f64 = pumpswap_buys.iter().map(|t| t.sol_amount).sum();

        let mut overlapping_volume = 0.0;

        for pumpswap_buy in pumpswap_buys {
            let range_start = pumpswap_buy.timestamp - self.correlation_window_secs;
            let range_end = pumpswap_buy.timestamp + self.correlation_window_secs;

            // Check if any DCA trade exists in the time window
            if dca_index.range(range_start..=range_end).next().is_some() {
                overlapping_volume += pumpswap_buy.sol_amount;
            }
        }

        (overlapping_volume / total_pumpswap_volume) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator_core::normalizer::TradeAction;

    fn create_test_trade(
        timestamp: i64,
        program_name: &str,
        action: TradeAction,
        sol_amount: f64,
    ) -> Trade {
        Trade {
            timestamp,
            signature: "test_sig".to_string(),
            program_name: program_name.to_string(),
            action,
            mint: "test_mint".to_string(),
            sol_amount,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: Some("user1".to_string()),
        }
    }

    #[test]
    fn test_perfect_overlap() {
        let engine = CorrelationEngine::new(60);

        let pumpswap_buys = vec![
            create_test_trade(1000, "PumpSwap", TradeAction::Buy, 10.0),
            create_test_trade(1030, "PumpSwap", TradeAction::Buy, 20.0),
        ];

        let dca_buys = vec![
            create_test_trade(1010, "JupiterDCA", TradeAction::Buy, 5.0),
            create_test_trade(1040, "JupiterDCA", TradeAction::Buy, 5.0),
        ];

        let overlap = engine.compute_dca_overlap(&pumpswap_buys, &dca_buys);
        assert_eq!(overlap, 100.0);
    }

    #[test]
    fn test_partial_overlap() {
        let engine = CorrelationEngine::new(60);

        let pumpswap_buys = vec![
            create_test_trade(1000, "PumpSwap", TradeAction::Buy, 10.0),
            create_test_trade(2000, "PumpSwap", TradeAction::Buy, 10.0), // No DCA nearby
        ];

        let dca_buys = vec![create_test_trade(1010, "JupiterDCA", TradeAction::Buy, 5.0)];

        let overlap = engine.compute_dca_overlap(&pumpswap_buys, &dca_buys);
        assert_eq!(overlap, 50.0);
    }

    #[test]
    fn test_no_overlap() {
        let engine = CorrelationEngine::new(60);

        let pumpswap_buys = vec![create_test_trade(1000, "PumpSwap", TradeAction::Buy, 10.0)];

        let dca_buys = vec![create_test_trade(2000, "JupiterDCA", TradeAction::Buy, 5.0)];

        let overlap = engine.compute_dca_overlap(&pumpswap_buys, &dca_buys);
        assert_eq!(overlap, 0.0);
    }

    #[test]
    fn test_empty_dca() {
        let engine = CorrelationEngine::new(60);

        let pumpswap_buys = vec![create_test_trade(1000, "PumpSwap", TradeAction::Buy, 10.0)];

        let dca_buys = vec![];

        let overlap = engine.compute_dca_overlap(&pumpswap_buys, &dca_buys);
        assert_eq!(overlap, 0.0);
    }
}
