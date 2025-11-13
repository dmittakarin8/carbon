#[cfg(test)]
mod tests {
    use {
        crate::trade_extractor::{determine_trade_direction, TradeKind, BalanceDelta},
    };

    /// Test BUY/SELL detection from balance deltas
    #[test]
    fn test_trade_direction_detection() {
        // Test BUY (negative SOL change)
        let buy_delta = BalanceDelta {
            account_index: 0,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            owner: None,
            raw_change: -100_000_000, // -0.1 SOL
            ui_change: -0.1,
            decimals: 9,
            is_sol: true,
        };
        assert_eq!(determine_trade_direction(&buy_delta), TradeKind::Buy);

        // Test SELL (positive SOL change)
        let sell_delta = BalanceDelta {
            account_index: 0,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            owner: None,
            raw_change: 100_000_000, // +0.1 SOL
            ui_change: 0.1,
            decimals: 9,
            is_sol: true,
        };
        assert_eq!(determine_trade_direction(&sell_delta), TradeKind::Sell);

        // Test Unknown (zero change)
        let zero_delta = BalanceDelta {
            account_index: 0,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            owner: None,
            raw_change: 0,
            ui_change: 0.0,
            decimals: 9,
            is_sol: true,
        };
        assert_eq!(determine_trade_direction(&zero_delta), TradeKind::Unknown);
    }

    /// Test mint extraction (filtering wrapped SOL)
    #[test]
    fn test_mint_extraction() {
        use crate::trade_extractor::find_primary_token_mint;

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "So11111111111111111111111111111111111111112".to_string(), // Wrapped SOL
                owner: None,
                raw_change: 1000,
                ui_change: 0.001,
                decimals: 9,
                is_sol: false,
            },
            BalanceDelta {
                account_index: 2,
                mint: "TokenMint123".to_string(),
                owner: None,
                raw_change: 1_000_000,
                ui_change: 1.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let mint = find_primary_token_mint(&token_deltas);
        assert_eq!(mint, Some("TokenMint123".to_string()));
    }
}

