use solana_pubkey::Pubkey;
use solana_transaction_status::TransactionStatusMeta;

#[derive(Debug, Clone)]
pub struct BalanceDelta {
    pub account_index: usize,
    pub mint: String,
    pub raw_change: i128,
    pub ui_change: f64,
    pub decimals: u8,
    pub is_sol: bool,
}

impl BalanceDelta {
    pub fn is_inflow(&self) -> bool {
        self.raw_change > 0
    }

    pub fn is_outflow(&self) -> bool {
        self.raw_change < 0
    }

    pub fn abs_ui_change(&self) -> f64 {
        self.ui_change.abs()
    }
}

pub fn build_full_account_keys(
    metadata: &carbon_core::transaction::TransactionMetadata,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let message = &metadata.message;
    let mut all_keys = message.static_account_keys().to_vec();
    
    let loaded = &meta.loaded_addresses;
    all_keys.extend(loaded.writable.iter().cloned());
    all_keys.extend(loaded.readonly.iter().cloned());
    
    all_keys
}

pub fn extract_sol_changes(
    meta: &TransactionStatusMeta,
    _account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    let pre_balances = &meta.pre_balances;
    let post_balances = &meta.post_balances;
    const MIN_SOL_DELTA: f64 = 0.0001;

    let mut deltas = Vec::new();

    for (idx, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
        let raw_change = (*post as i128) - (*pre as i128);

        if raw_change == 0 {
            continue;
        }

        let ui_change = raw_change as f64 / 1_000_000_000.0;

        if ui_change.abs() < MIN_SOL_DELTA {
            continue;
        }

        deltas.push(BalanceDelta {
            account_index: idx,
            mint: "So11111111111111111111111111111111111111112".to_string(),
            raw_change,
            ui_change,
            decimals: 9,
            is_sol: true,
        });
    }

    deltas
}

pub fn extract_token_changes(
    meta: &TransactionStatusMeta,
    _account_keys: &[Pubkey],
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

    for pre in pre_token_balances {
        let post = post_token_balances
            .iter()
            .find(|p| p.account_index == pre.account_index);

        let pre_raw = pre.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
        let pre_ui = pre.ui_token_amount.ui_amount.unwrap_or(0.0);
        let decimals = pre.ui_token_amount.decimals;
        
        let (post_raw, post_ui) = match post {
            Some(p) => (
                p.ui_token_amount.amount.parse::<u64>().unwrap_or(0),
                p.ui_token_amount.ui_amount.unwrap_or(0.0),
            ),
            None => (0, 0.0),
        };

        let raw_change = (post_raw as i128) - (pre_raw as i128);
        let ui_change = post_ui - pre_ui;

        if raw_change == 0 {
            continue;
        }

        let account_index = pre.account_index as usize;

        deltas.push(BalanceDelta {
            account_index,
            mint: pre.mint.clone(),
            raw_change,
            ui_change,
            decimals,
            is_sol: false,
        });
    }

    for post in post_token_balances {
        let exists_in_pre = pre_token_balances
            .iter()
            .any(|pre| pre.account_index == post.account_index);

        if !exists_in_pre {
            let post_raw = post.ui_token_amount.amount.parse::<u64>().unwrap_or(0);
            let post_ui = post.ui_token_amount.ui_amount.unwrap_or(0.0);
            let decimals = post.ui_token_amount.decimals;

            if post_raw > 0 {
                let account_index = post.account_index as usize;

                deltas.push(BalanceDelta {
                    account_index,
                    mint: post.mint.clone(),
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
