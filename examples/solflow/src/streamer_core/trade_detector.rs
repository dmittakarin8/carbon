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

pub fn extract_trade_info(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
    account_keys: &[Pubkey],
) -> Option<TradeInfo> {
    if sol_deltas.is_empty() {
        log::debug!("No SOL changes detected, skipping");
        return None;
    }

    let user_idx = find_user_account(sol_deltas)?;
    
    if user_idx >= account_keys.len() {
        log::warn!("User account index {} out of bounds (len: {})", user_idx, account_keys.len());
        return None;
    }

    let user_account = account_keys.get(user_idx).copied();
    let user_sol_delta = sol_deltas.iter().find(|d| d.account_index == user_idx)?;

    let sol_amount = user_sol_delta.abs_ui_change();
    let direction = if user_sol_delta.is_outflow() {
        TradeDirection::Buy
    } else if user_sol_delta.is_inflow() {
        TradeDirection::Sell
    } else {
        TradeDirection::Unknown
    };

    let token_mint = find_primary_token_mint(token_deltas)?;

    let user_token_delta = token_deltas
        .iter()
        .filter(|d| d.mint == token_mint)
        .max_by_key(|d| d.raw_change.abs())?;

    let token_amount = user_token_delta.abs_ui_change();
    let token_decimals = user_token_delta.decimals;

    Some(TradeInfo {
        mint: token_mint,
        sol_amount,
        token_amount,
        token_decimals,
        direction,
        user_account,
    })
}
