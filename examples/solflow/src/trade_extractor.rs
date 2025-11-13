use {
    carbon_core::transaction::TransactionMetadata,
    solana_account_decoder_client_types::token::UiTokenAmount,
    solana_pubkey::Pubkey,
    solana_transaction_status::TransactionStatusMeta,
    std::sync::Arc,
};

/// Minimum SOL delta threshold to filter out negligible lamport noise
/// Trades with SOL changes smaller than this are ignored
pub const MIN_SOL_DELTA: f64 = 0.0001; // 0.0001 SOL

/// Trade direction enum for type-safe handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradeKind {
    Buy,
    Sell,
    Unknown,
}

/// Represents a balance change (delta) for a single account
#[derive(Debug, Clone)]
pub struct BalanceDelta {
    /// Account index in the transaction
    pub account_index: usize,
    /// Mint address (for tokens) or SOL
    pub mint: String,
    /// Owner/account address
    pub owner: Option<Pubkey>,
    /// Raw amount change (post - pre)
    pub raw_change: i128,
    /// Normalized UI amount change
    pub ui_change: f64,
    /// Decimals for this mint
    pub decimals: u8,
    /// Is this a SOL change (not token)?
    pub is_sol: bool,
}

impl BalanceDelta {
    /// Check if this is an inflow (positive change)
    pub fn is_inflow(&self) -> bool {
        self.raw_change > 0
    }

    /// Check if this is an outflow (negative change)
    pub fn is_outflow(&self) -> bool {
        self.raw_change < 0
    }

    /// Get absolute value of UI change
    pub fn abs_ui_change(&self) -> f64 {
        self.ui_change.abs()
    }
}

/// Build complete account keys list (static + loaded addresses from ALTs)
/// 
/// Solana v0 transactions can have accounts in Address Lookup Tables (ALTs).
/// These appear in TransactionStatusMeta.loaded_addresses (writable and readonly)
/// and are indexed after the static_account_keys.
pub fn build_full_account_keys(
    metadata: &Arc<TransactionMetadata>,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let message = &metadata.message;
    let mut all_keys = message.static_account_keys().to_vec();
    let static_count = all_keys.len();
    
    // Add loaded addresses from Address Lookup Tables (v0 transactions)
    let loaded = &meta.loaded_addresses;
    
    // Add writable loaded addresses
    all_keys.extend(loaded.writable.iter().cloned());
    
    // Add readonly loaded addresses
    all_keys.extend(loaded.readonly.iter().cloned());
    
    if !loaded.writable.is_empty() || !loaded.readonly.is_empty() {
        log::debug!(
            "Account keys: {} total ({} static + {} writable + {} readonly ALT)",
            all_keys.len(),
            static_count,
            loaded.writable.len(),
            loaded.readonly.len()
        );
    }
    
    all_keys
}

/// Extract all SOL balance changes from transaction metadata
/// 
/// Computes deltas by comparing postBalances to preBalances for each account.
/// Returns only non-zero changes above MIN_SOL_DELTA threshold.
pub fn extract_sol_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    let pre_balances = &meta.pre_balances;
    let post_balances = &meta.post_balances;

    let mut deltas = Vec::new();

    for (idx, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
        let raw_change = (*post as i128) - (*pre as i128);

        // Skip unchanged accounts
        if raw_change == 0 {
            continue;
        }

        let ui_change = raw_change as f64 / 1_000_000_000.0; // SOL has 9 decimals

        // Apply MIN_SOL_DELTA filter to ignore negligible noise
        if ui_change.abs() < MIN_SOL_DELTA {
            continue;
        }

        // Bounds checking for owner extraction
        let owner = if idx < account_keys.len() {
            account_keys.get(idx).copied()
        } else {
            log::warn!(
                "⚠️  SOL owner extraction failed: idx {} >= account_keys.len() {}",
                idx,
                account_keys.len()
            );
            None
        };

        deltas.push(BalanceDelta {
            account_index: idx,
            mint: "So11111111111111111111111111111111111111112".to_string(), // SOL mint
            owner,
            raw_change,
            ui_change,
            decimals: 9,
            is_sol: true,
        });
    }

    deltas
}

/// Extract all token balance changes from transaction metadata
/// 
/// Computes deltas by comparing postTokenBalances to preTokenBalances.
/// Uses the ui_token_amount field which already has proper decimal normalization.
pub fn extract_token_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    let pre_token_balances = match &meta.pre_token_balances {
        Some(balances) => balances,
        None => return Vec::new(),
    };

    let post_token_balances = match &meta.post_token_balances {
        Some(balances) => balances,
        None => return Vec::new(),
    };

    let mut deltas = Vec::new();

    // Match pre and post balances by account_index
    for pre in pre_token_balances {
        // Find corresponding post balance
        let post = post_token_balances
            .iter()
            .find(|p| p.account_index == pre.account_index);

        let (pre_raw, pre_ui, decimals) = extract_token_amount(&pre.ui_token_amount);

        let (post_raw, post_ui, _) = match post {
            Some(p) => extract_token_amount(&p.ui_token_amount),
            None => (0, 0.0, decimals), // Account closed
        };

        let raw_change = (post_raw as i128) - (pre_raw as i128);
        let ui_change = post_ui - pre_ui;

        // Skip unchanged accounts
        if raw_change == 0 {
            continue;
        }

        let account_index = pre.account_index as usize;
        
        // Bounds checking for token owner extraction
        let owner = if account_index < account_keys.len() {
            account_keys.get(account_index).copied()
        } else {
            log::warn!(
                "⚠️  Token owner extraction failed: idx {} >= account_keys.len() {}",
                account_index,
                account_keys.len()
            );
            None
        };

        deltas.push(BalanceDelta {
            account_index,
            mint: pre.mint.clone(),
            owner,
            raw_change,
            ui_change,
            decimals,
            is_sol: false,
        });
    }

    // Check for new token accounts (not in pre_token_balances)
    for post in post_token_balances {
        let exists_in_pre = pre_token_balances
            .iter()
            .any(|pre| pre.account_index == post.account_index);

        if !exists_in_pre {
            let (post_raw, post_ui, decimals) = extract_token_amount(&post.ui_token_amount);

            if post_raw > 0 {
                let account_index = post.account_index as usize;
                let owner = account_keys.get(account_index).copied();

                deltas.push(BalanceDelta {
                    account_index,
                    mint: post.mint.clone(),
                    owner,
                    raw_change: post_raw as i128,
                    ui_change: post_ui,
                    decimals,
                    is_sol: false,
                });
            }
        }
    }

    deltas
}

/// Helper to extract raw amount, UI amount, and decimals from token amount
fn extract_token_amount(ui_amount: &UiTokenAmount) -> (u64, f64, u8) {
    let raw = ui_amount.amount.parse::<u64>().unwrap_or(0);
    let ui = ui_amount.ui_amount.unwrap_or(0.0);
    let decimals = ui_amount.decimals;

    (raw, ui, decimals)
}

/// Find the primary user account (largest negative SOL change, typically index 0 or 1)
/// 
/// The user account is usually the one paying fees and/or trading.
pub fn find_user_account(sol_deltas: &[BalanceDelta]) -> Option<usize> {
    sol_deltas
        .iter()
        .filter(|d| d.is_outflow())
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.account_index)
}

/// Find the token mint involved in this transaction
/// 
/// Returns the mint address of the token with the largest balance change.
/// Filters out wrapped SOL (So11111...).
pub fn find_primary_token_mint(token_deltas: &[BalanceDelta]) -> Option<String> {
    token_deltas
        .iter()
        .filter(|d| !d.mint.starts_with("So11111")) // Skip wrapped SOL
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.mint.clone())
}

/// Determine trade direction from SOL delta
/// 
/// - Negative SOL change (outflow) = BUY (spending SOL to get tokens)
/// - Positive SOL change (inflow) = SELL (receiving SOL from selling tokens)
pub fn determine_trade_direction(sol_delta: &BalanceDelta) -> TradeKind {
    if sol_delta.is_outflow() {
        TradeKind::Buy
    } else if sol_delta.is_inflow() {
        TradeKind::Sell
    } else {
        TradeKind::Unknown
    }
}

/// Extract user's SOL and token volumes from balance deltas
/// 
/// This identifies the actual amounts the user spent/received, filtering out
/// pool changes, fees, and other accounts.
/// 
/// Returns: (sol_volume, token_volume, token_mint, decimals, direction)
pub fn extract_user_volumes(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
) -> Option<(f64, f64, String, u8, TradeKind)> {
    // Find user account
    let user_idx = find_user_account(sol_deltas)?;

    // Find user's SOL change
    let user_sol_delta = sol_deltas.iter().find(|d| d.account_index == user_idx)?;

    let sol_volume = user_sol_delta.abs_ui_change();
    let direction = determine_trade_direction(user_sol_delta);

    // Find primary token mint
    let token_mint = find_primary_token_mint(token_deltas)?;

    // Find user's token change for this mint
    let user_token_delta = token_deltas
        .iter()
        .filter(|d| d.mint == token_mint)
        .max_by_key(|d| d.raw_change.abs())?;

    let token_volume = user_token_delta.abs_ui_change();
    let decimals = user_token_delta.decimals;

    Some((sol_volume, token_volume, token_mint, decimals, direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_direction_buy() {
        let delta = BalanceDelta {
            account_index: 0,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            owner: None,
            raw_change: -100000000, // -0.1 SOL
            ui_change: -0.1,
            decimals: 9,
            is_sol: true,
        };
        assert_eq!(determine_trade_direction(&delta), TradeKind::Buy);
    }

    #[test]
    fn test_trade_direction_sell() {
        let delta = BalanceDelta {
            account_index: 0,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            owner: None,
            raw_change: 100000000, // +0.1 SOL
            ui_change: 0.1,
            decimals: 9,
            is_sol: true,
        };
        assert_eq!(determine_trade_direction(&delta), TradeKind::Sell);
    }
}

