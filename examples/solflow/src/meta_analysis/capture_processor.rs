use crate::meta_analysis::types::{
    BalanceDeltaRecord, CaptureMetadata, InnerInstructionRecord, TransactionCapture,
    TokenBalanceRecord, TokenAmountRecord,
};
use crate::streamer_core::balance_extractor::{
    build_full_account_keys, extract_sol_changes, extract_token_changes,
};
use async_trait::async_trait;
use solana_transaction_status::TransactionTokenBalance;
use carbon_core::{
    error::{CarbonResult, Error as CarbonError},
    metrics::MetricsCollection,
    processor::Processor,
    transaction::TransactionProcessorInputType,
};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

// Use the public empty_decoder from the library
use crate::empty_decoder::EmptyDecoderCollection;

/// Metadata capture processor for analysis (non-production)
#[derive(Clone)]
pub struct MetadataCaptureProcessor {
    program_name: String,
    output_path: PathBuf,
    transaction_count: Arc<AtomicUsize>,
    max_transactions: usize,
    capture_metadata: CaptureMetadata,
    start_time: Arc<tokio::sync::Mutex<chrono::DateTime<Utc>>>,
    inner_program_tracker: Arc<tokio::sync::Mutex<Vec<String>>>,
    inner_instruction_count: Arc<AtomicUsize>,
    transactions_with_inner: Arc<AtomicUsize>,
}

fn convert_token_balances(balances: &Option<Vec<TransactionTokenBalance>>) -> Option<Vec<TokenBalanceRecord>> {
    balances.as_ref().map(|bals| {
        bals.iter().map(|bal| TokenBalanceRecord {
            account_index: bal.account_index,
            mint: bal.mint.clone(),
            ui_token_amount: TokenAmountRecord {
                ui_amount: bal.ui_token_amount.ui_amount,
                decimals: bal.ui_token_amount.decimals,
                amount: bal.ui_token_amount.amount.clone(),
                ui_amount_string: bal.ui_token_amount.ui_amount_string.clone(),
            },
            owner: bal.owner.clone(),
            program_id: bal.program_id.clone(),
        }).collect()
    })
}

impl MetadataCaptureProcessor {
    pub fn new(
        program_name: String,
        output_path: PathBuf,
        max_transactions: usize,
        capture_metadata: CaptureMetadata,
    ) -> Self {
        Self {
            program_name,
            output_path,
            transaction_count: Arc::new(AtomicUsize::new(0)),
            max_transactions,
            capture_metadata,
            start_time: Arc::new(tokio::sync::Mutex::new(Utc::now())),
            inner_program_tracker: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            inner_instruction_count: Arc::new(AtomicUsize::new(0)),
            transactions_with_inner: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn get_session_metadata(&self) -> crate::meta_analysis::types::SessionMetadata {
        let inner_programs = self.inner_program_tracker.lock().await;
        let mut unique_programs: Vec<String> = inner_programs.iter().cloned().collect();
        unique_programs.sort();
        unique_programs.dedup();

        let start_time = *self.start_time.lock().await;
        let end_time = Utc::now();

        crate::meta_analysis::types::SessionMetadata {
            program_name: self.program_name.clone(),
            program_id: self.capture_metadata.program_id.clone(),
            capture_tool_version: self.capture_metadata.capture_tool_version.clone(),
            session_start_time: start_time.to_rfc3339(),
            session_end_time: end_time.to_rfc3339(),
            duration_seconds: (end_time - start_time).num_seconds(),
            transactions_captured: self.transaction_count.load(Ordering::SeqCst),
            transactions_target: self.max_transactions,
            capture_complete: self.transaction_count.load(Ordering::SeqCst) >= self.max_transactions,
            output_file: self
                .output_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            inner_instruction_stats: crate::meta_analysis::types::InnerInstructionStats {
                transactions_with_inner: self.transactions_with_inner.load(Ordering::SeqCst),
                total_inner_instructions: self.inner_instruction_count.load(Ordering::SeqCst),
                unique_inner_programs: unique_programs,
            },
        }
    }

    async fn write_jsonl(&self, capture: &TransactionCapture) -> Result<(), std::io::Error> {
        let json_line = serde_json::to_string(capture)?;
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.output_path)
            .await?;
        
        file.write_all(json_line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        
        Ok(())
    }

    fn extract_inner_instructions(
        &self,
        metadata: &carbon_core::transaction::TransactionMetadata,
        account_keys: &[solana_pubkey::Pubkey],
    ) -> Vec<InnerInstructionRecord> {
        metadata
            .meta
            .inner_instructions
            .as_ref()
            .map(|inner_groups| {
                inner_groups
                    .iter()
                    .flat_map(|inner_group| {
                        inner_group.instructions.iter().map(|inner| {
                            let program_id_index = inner.instruction.program_id_index;
                            let program_id = account_keys
                                .get(program_id_index as usize)
                                .map(|pk| pk.to_string())
                                .unwrap_or_else(|| "INVALID_INDEX".to_string());

                            let data_len = inner.instruction.data.len();
                            let data_hex_prefix = hex::encode(
                                &inner.instruction.data[..data_len.min(16)]
                            );

                            InnerInstructionRecord {
                                top_level_index: inner_group.index,
                                stack_height: inner.stack_height,
                                program_id_index,
                                program_id,
                                accounts: inner.instruction.accounts.clone(),
                                data_length: data_len,
                                data_hex_prefix,
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl Processor for MetadataCaptureProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;

    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        // Check transaction limit
        let count = self.transaction_count.fetch_add(1, Ordering::SeqCst);
        if count >= self.max_transactions {
            log::info!(
                "✅ Reached {} transactions for {}, stopping",
                self.max_transactions,
                self.program_name
            );
            return Err(CarbonError::Custom(
                "Transaction limit reached".to_string(),
            ));
        }

        // Build full account keys
        let account_keys = build_full_account_keys(&metadata, &metadata.meta);

        // Extract balance deltas
        let sol_deltas = extract_sol_changes(&metadata.meta, &account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, &account_keys);

        // Refinement #1: Extract inner instructions with program IDs
        let inner_instructions = self.extract_inner_instructions(&metadata, &account_keys);

        // Track inner instruction stats
        if !inner_instructions.is_empty() {
            self.transactions_with_inner.fetch_add(1, Ordering::SeqCst);
            self.inner_instruction_count
                .fetch_add(inner_instructions.len(), Ordering::SeqCst);

            let mut tracker = self.inner_program_tracker.lock().await;
            for inner in &inner_instructions {
                if !tracker.contains(&inner.program_id) {
                    tracker.push(inner.program_id.clone());
                }
            }
        }

        // Build capture record
        let capture = TransactionCapture {
            capture_metadata: self.capture_metadata.clone(),
            slot: metadata.slot,
            signature: metadata.signature.to_string(),
            block_time: metadata.block_time,
            fee_payer: metadata.fee_payer.to_string(),
            account_keys: account_keys.iter().map(|k| k.to_string()).collect(),
            static_key_count: metadata.message.static_account_keys().len(),
            pre_balances: metadata.meta.pre_balances.clone(),
            post_balances: metadata.meta.post_balances.clone(),
            pre_token_balances: convert_token_balances(&metadata.meta.pre_token_balances),
            post_token_balances: convert_token_balances(&metadata.meta.post_token_balances),
            sol_deltas: sol_deltas
                .iter()
                .map(BalanceDeltaRecord::from_balance_delta)
                .collect(),
            token_deltas: token_deltas
                .iter()
                .map(BalanceDeltaRecord::from_balance_delta)
                .collect(),
            inner_instructions,
            fee: metadata.meta.fee,
            rewards: metadata.meta.rewards.clone(),
            account_classifications: vec![],
        };

        // Write to JSONL
        self.write_jsonl(&capture).await.map_err(|e| {
            CarbonError::Custom(format!("Failed to write JSONL: {}", e))
        })?;

        log::info!(
            "[{}/{}] 📊 {} | Accounts: {}, SOL Δ: {}, Token Δ: {}, Inner: {}",
            count + 1,
            self.max_transactions,
            metadata.signature,
            account_keys.len(),
            sol_deltas.len(),
            token_deltas.len(),
            capture.inner_instructions.len()
        );

        Ok(())
    }
}
