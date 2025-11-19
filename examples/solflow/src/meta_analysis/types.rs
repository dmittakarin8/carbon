use serde::{Deserialize, Serialize};
use solana_transaction_status::Reward;

/// Metadata about the capture session (Refinement #2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureMetadata {
    pub program_id: String,
    pub program_name: String,
    pub capture_tool_version: String,
    pub captured_at: i64,
}

/// Serializable token balance (mirrors TransactionTokenBalance structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalanceRecord {
    pub account_index: u8,
    pub mint: String,
    pub ui_token_amount: TokenAmountRecord,
    pub owner: String,
    pub program_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAmountRecord {
    pub ui_amount: Option<f64>,
    pub decimals: u8,
    pub amount: String,
    pub ui_amount_string: String,
}

/// Complete transaction capture with full metadata surface area
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCapture {
    // Refinement #2: Program context
    pub capture_metadata: CaptureMetadata,
    
    // Transaction identity
    pub slot: u64,
    pub signature: String,
    pub block_time: Option<i64>,
    pub fee_payer: String,
    
    // Account keys (static + ALT-loaded)
    pub account_keys: Vec<String>,
    pub static_key_count: usize,
    
    // Raw balance data from Solana
    pub pre_balances: Vec<u64>,
    pub post_balances: Vec<u64>,
    pub pre_token_balances: Option<Vec<TokenBalanceRecord>>,
    pub post_token_balances: Option<Vec<TokenBalanceRecord>>,
    
    // Processed deltas (our extraction logic)
    pub sol_deltas: Vec<BalanceDeltaRecord>,
    pub token_deltas: Vec<BalanceDeltaRecord>,
    
    // Refinement #1: Inner instructions with program IDs
    pub inner_instructions: Vec<InnerInstructionRecord>,
    
    // Fees and rewards
    pub fee: u64,
    pub rewards: Option<Vec<Reward>>,
    
    // Classification (filled by post-processing)
    pub account_classifications: Vec<AccountClassRecord>,
}

/// Refinement #1: Inner instruction with resolved program ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerInstructionRecord {
    pub top_level_index: u8,
    pub stack_height: Option<u32>,
    pub program_id_index: u8,
    pub program_id: String,
    pub accounts: Vec<u8>,
    pub data_length: usize,
    pub data_hex_prefix: String,
}

/// Balance delta record (from our extraction logic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceDeltaRecord {
    pub account_index: usize,
    pub mint: String,
    pub raw_change: i128,
    pub ui_change: f64,
    pub decimals: u8,
    pub is_sol: bool,
}

impl BalanceDeltaRecord {
    pub fn from_balance_delta(delta: &crate::streamer_core::balance_extractor::BalanceDelta) -> Self {
        Self {
            account_index: delta.account_index,
            mint: delta.mint.clone(),
            raw_change: delta.raw_change,
            ui_change: delta.ui_change,
            decimals: delta.decimals,
            is_sol: delta.is_sol,
        }
    }
}

/// Account classification record (post-processing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountClassRecord {
    pub account_index: usize,
    pub account_key: String,
    pub classification: String,
    pub confidence: f64,
}

/// Refinement #4: Session metadata file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub program_name: String,
    pub program_id: String,
    pub capture_tool_version: String,
    pub session_start_time: String,
    pub session_end_time: String,
    pub duration_seconds: i64,
    pub transactions_captured: usize,
    pub transactions_target: usize,
    pub capture_complete: bool,
    pub output_file: String,
    pub inner_instruction_stats: InnerInstructionStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerInstructionStats {
    pub transactions_with_inner: usize,
    pub total_inner_instructions: usize,
    pub unique_inner_programs: Vec<String>,
}
