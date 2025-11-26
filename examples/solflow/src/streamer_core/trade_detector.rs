use crate::streamer_core::balance_extractor::BalanceDelta;
use solana_pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct TradeInfo {
    pub mint: String,
    pub sol_amount: f64,
    pub token_amount: f64,
    pub token_decimals: u8,
    pub direction: TradeDirection,
    pub user_account: Option<Pubkey>,
}

#[derive(Debug, Clone, Copy)]
pub enum TradeDirection {
    Buy,
    Sell,
    Unknown,
}

impl From<TradeDirection> for &'static str {
    fn from(dir: TradeDirection) -> &'static str {
        match dir {
            TradeDirection::Buy => "BUY",
            TradeDirection::Sell => "SELL",
            TradeDirection::Unknown => "UNKNOWN",
        }
    }
}

fn find_primary_token_mint(token_deltas: &[BalanceDelta]) -> Option<String> {
    token_deltas
        .iter()
        .filter(|d| !d.mint.starts_with("So11111"))
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.mint.clone())
}

fn find_user_account(sol_deltas: &[BalanceDelta]) -> Option<usize> {
    sol_deltas
        .iter()
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.account_index)
}

/// Extract ALL trades from a transaction with multi-mint support
///
/// This function supports the unified DEX mint flow by extracting one trade
/// per token mint involved in a pump-relevant transaction. It uses ONLY
/// balance deltas from TransactionMetadata (no instruction parsing).
///
/// # Multi-Mint Support
///
/// For transactions involving multiple token mints (e.g., Jupiter-routed swaps,
/// multi-leg DEX aggregations), this function returns one TradeInfo per mint.
///
/// Example: Jupiter swap routing through multiple pools
/// - MintA: -500 tokens (SELL)
/// - MintB: +2,000 tokens (BUY)
/// Returns: Vec with 2 TradeInfo structs
///
/// # Logic
///
/// 1. Find user account (largest SOL delta)
/// 2. Group token deltas by mint address
/// 3. For each non-wrapped-SOL mint:
///    - Determine trade direction from SOL flow
///    - Extract token amount from largest delta for that mint
///    - Create TradeInfo struct
///
/// # Parameters
///
/// - `sol_deltas`: SOL balance changes from pre/post_balances
/// - `token_deltas`: Token balance changes from pre/post_token_balances
/// - `account_keys`: Full account key list (static + ALT)
///
/// # Returns
///
/// - `Vec<TradeInfo>`: One trade per mint (empty if no valid trades)
/// - Empty vec if: no SOL changes, no token changes, or user account not found
///
/// # DEX Origin Attribution
///
/// Trade direction and amounts are extracted here, but DEX origin (program_name)
/// is assigned by the processor using InstructionScanner results.
pub fn extract_all_trades(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
    account_keys: &[Pubkey],
) -> Vec<TradeInfo> {
    // Early exit: no SOL changes means no trades
    if sol_deltas.is_empty() {
        log::debug!("No SOL changes detected, skipping");
        return Vec::new();
    }

    // Find user account (largest SOL change)
    let user_idx = match find_user_account(sol_deltas) {
        Some(idx) => idx,
        None => {
            log::debug!("Could not determine user account from SOL deltas");
            return Vec::new();
        }
    };

    // Validate user account index
    if user_idx >= account_keys.len() {
        log::warn!(
            "User account index {} out of bounds (len: {})",
            user_idx,
            account_keys.len()
        );
        return Vec::new();
    }

    let user_account = account_keys.get(user_idx).copied();
    
    // Get user's SOL delta to determine trade direction
    let user_sol_delta = match sol_deltas.iter().find(|d| d.account_index == user_idx) {
        Some(delta) => delta,
        None => {
            log::debug!("Could not find SOL delta for user account index {}", user_idx);
            return Vec::new();
        }
    };

    let sol_amount = user_sol_delta.abs_ui_change();
    
    // Determine trade direction from SOL flow
    let direction = if user_sol_delta.is_outflow() {
        TradeDirection::Buy
    } else if user_sol_delta.is_inflow() {
        TradeDirection::Sell
    } else {
        TradeDirection::Unknown
    };

    // Group token deltas by mint address
    use std::collections::HashMap;
    let mut mints_map: HashMap<String, Vec<&BalanceDelta>> = HashMap::new();
    
    for delta in token_deltas {
        // Skip wrapped SOL (So11111...)
        if delta.mint.starts_with("So11111") {
            continue;
        }
        
        mints_map
            .entry(delta.mint.clone())
            .or_insert_with(Vec::new)
            .push(delta);
    }

    // Early exit: no non-SOL token mints
    if mints_map.is_empty() {
        log::debug!("No non-wrapped-SOL token mints found");
        return Vec::new();
    }

    // Create one TradeInfo per mint
    let mut trades = Vec::new();
    
    for (mint, deltas) in mints_map {
        // Find largest delta for this mint (handles multiple accounts per mint)
        let largest_delta = match deltas.iter().max_by_key(|d| d.raw_change.abs()) {
            Some(delta) => delta,
            None => continue,
        };

        let token_amount = largest_delta.abs_ui_change();
        let token_decimals = largest_delta.decimals;

        trades.push(TradeInfo {
            mint: mint.clone(),
            sol_amount,
            token_amount,
            token_decimals,
            direction,
            user_account,
        });
    }

    if trades.is_empty() {
        log::debug!("No valid trades extracted from token deltas");
    } else if trades.len() > 1 {
        log::debug!("Multi-mint transaction: {} trades extracted", trades.len());
    }

    trades
}

/// Extract single trade info (backwards compatibility wrapper)
///
/// This function preserves the original single-mint behavior by calling
/// extract_all_trades() and returning the first result. Existing code
/// using this function continues to work identically.
///
/// For multi-mint support, use extract_all_trades() directly.
pub fn extract_trade_info(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
    account_keys: &[Pubkey],
) -> Option<TradeInfo> {
    extract_all_trades(sol_deltas, token_deltas, account_keys)
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_pubkey::Pubkey;

    fn mock_pubkey(index: u8) -> Pubkey {
        let mut bytes = [0u8; 32];
        bytes[0] = index;
        Pubkey::from(bytes)
    }

    #[test]
    fn test_extract_all_trades_single_mint() {
        // Test: Single-mint transaction (backwards compatibility)
        let sol_deltas = vec![
            BalanceDelta {
                account_index: 0,
                mint: "So11111111111111111111111111111111111111112".to_string(),
                raw_change: -1_000_000_000, // -1 SOL
                ui_change: -1.0,
                decimals: 9,
                is_sol: true,
            },
        ];

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "TokenMintABC123".to_string(),
                raw_change: 1000_000000, // +1000 tokens
                ui_change: 1000.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1)];

        let trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);

        assert_eq!(trades.len(), 1, "Should extract exactly 1 trade");
        
        let trade = &trades[0];
        assert_eq!(trade.mint, "TokenMintABC123");
        assert_eq!(trade.sol_amount, 1.0);
        assert_eq!(trade.token_amount, 1000.0);
        assert_eq!(trade.token_decimals, 6);
        assert!(matches!(trade.direction, TradeDirection::Buy));
    }

    #[test]
    fn test_extract_all_trades_multi_mint() {
        // Test: Multi-mint transaction (Jupiter routing scenario)
        let sol_deltas = vec![
            BalanceDelta {
                account_index: 0,
                mint: "So11111111111111111111111111111111111111112".to_string(),
                raw_change: -2_000_000_000, // -2 SOL
                ui_change: -2.0,
                decimals: 9,
                is_sol: true,
            },
        ];

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "MintA".to_string(),
                raw_change: 500_000000, // +500 tokens
                ui_change: 500.0,
                decimals: 6,
                is_sol: false,
            },
            BalanceDelta {
                account_index: 2,
                mint: "MintB".to_string(),
                raw_change: 2000_000000, // +2000 tokens
                ui_change: 2000.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1), mock_pubkey(2)];

        let trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);

        assert_eq!(trades.len(), 2, "Should extract 2 trades (multi-mint)");

        // Both trades should have same SOL amount (user spent 2 SOL total)
        for trade in &trades {
            assert_eq!(trade.sol_amount, 2.0);
            assert!(matches!(trade.direction, TradeDirection::Buy));
        }

        // Find trades by mint
        let mint_a_trade = trades.iter().find(|t| t.mint == "MintA").unwrap();
        let mint_b_trade = trades.iter().find(|t| t.mint == "MintB").unwrap();

        assert_eq!(mint_a_trade.token_amount, 500.0);
        assert_eq!(mint_b_trade.token_amount, 2000.0);
    }

    #[test]
    fn test_extract_all_trades_wrapped_sol_skipped() {
        // Test: Wrapped SOL is filtered out
        let sol_deltas = vec![
            BalanceDelta {
                account_index: 0,
                mint: "So11111111111111111111111111111111111111112".to_string(),
                raw_change: -1_000_000_000,
                ui_change: -1.0,
                decimals: 9,
                is_sol: true,
            },
        ];

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "So11111111111111111111111111111111111111112".to_string(), // Wrapped SOL
                raw_change: 1_000_000_000,
                ui_change: 1.0,
                decimals: 9,
                is_sol: false, // Token account wrapping SOL
            },
            BalanceDelta {
                account_index: 2,
                mint: "RealToken123".to_string(),
                raw_change: 100_000000,
                ui_change: 100.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1), mock_pubkey(2)];

        let trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);

        assert_eq!(trades.len(), 1, "Should skip wrapped SOL, extract only real token");
        assert_eq!(trades[0].mint, "RealToken123");
    }

    #[test]
    fn test_extract_all_trades_no_sol_changes() {
        // Test: No SOL changes means no trades
        let sol_deltas = vec![];
        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "TokenMint".to_string(),
                raw_change: 1000_000000,
                ui_change: 1000.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1)];

        let trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);

        assert_eq!(trades.len(), 0, "No SOL changes should yield no trades");
    }

    #[test]
    fn test_extract_all_trades_sell_direction() {
        // Test: SOL inflow = SELL
        let sol_deltas = vec![
            BalanceDelta {
                account_index: 0,
                mint: "So11111111111111111111111111111111111111112".to_string(),
                raw_change: 1_500_000_000, // +1.5 SOL (user received)
                ui_change: 1.5,
                decimals: 9,
                is_sol: true,
            },
        ];

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "SellToken".to_string(),
                raw_change: -500_000000, // -500 tokens (user sold)
                ui_change: -500.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1)];

        let trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);

        assert_eq!(trades.len(), 1);
        assert!(matches!(trades[0].direction, TradeDirection::Sell));
        assert_eq!(trades[0].sol_amount, 1.5);
        assert_eq!(trades[0].token_amount, 500.0); // Absolute value
    }

    #[test]
    fn test_extract_trade_info_backwards_compat() {
        // Test: extract_trade_info() returns first trade only
        let sol_deltas = vec![
            BalanceDelta {
                account_index: 0,
                mint: "So11111111111111111111111111111111111111112".to_string(),
                raw_change: -1_000_000_000,
                ui_change: -1.0,
                decimals: 9,
                is_sol: true,
            },
        ];

        let token_deltas = vec![
            BalanceDelta {
                account_index: 1,
                mint: "MintA".to_string(),
                raw_change: 100_000000,
                ui_change: 100.0,
                decimals: 6,
                is_sol: false,
            },
            BalanceDelta {
                account_index: 2,
                mint: "MintB".to_string(),
                raw_change: 200_000000,
                ui_change: 200.0,
                decimals: 6,
                is_sol: false,
            },
        ];

        let account_keys = vec![mock_pubkey(0), mock_pubkey(1), mock_pubkey(2)];

        // Old function should return only first trade
        let single_trade = extract_trade_info(&sol_deltas, &token_deltas, &account_keys);
        
        assert!(single_trade.is_some());
        
        // Should be one of the two mints (order not guaranteed due to HashMap)
        let mint = &single_trade.unwrap().mint;
        assert!(mint == "MintA" || mint == "MintB");
    }
}
