//! Mint Trace - Comprehensive Transaction Monitoring Tool
//!
//! This binary provides detailed inspection of every transaction involving a specific mint address.
//! It uses Carbon's TransactionMetadata abstraction to ensure complete coverage of all instructions
//! (both outer and inner/CPI) without missing any buys, sells, or other operations.
//!
//! ## Purpose
//!
//! - Track ALL transactions involving a specific token mint
//! - Print fully decoded transaction logs including:
//!   * Slot and signature
//!   * All program IDs involved
//!   * Complete instruction tree (outer + inner)
//!   * All account keys (including ALT-loaded addresses)
//!   * All token mints extracted from balance changes
//!   * Balance deltas (SOL and token changes)
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin mint_trace -- --mint <MINT_ADDRESS>
//! ```
//!
//! ## Environment Variables
//!
//! - `GEYSER_URL` - gRPC endpoint (default: http://127.0.0.1:10000)
//! - `GEYSER_TOKEN` - Authentication token (optional)
//! - `COMMITMENT_LEVEL` - Transaction commitment (default: Confirmed)
//! - `RUST_LOG` - Log level (default: info)
//!
//! ## Technical Approach
//!
//! This tool does NOT use program-specific filtering at the gRPC level.
//! Instead, it:
//! 1. Subscribes to ALL transactions (account-based filtering for the mint)
//! 2. Inspects every transaction's token balance changes
//! 3. Matches against the target mint address
//! 4. Prints comprehensive logs for matches
//!
//! This ensures zero missed transactions, as the filtering happens after
//! Carbon's complete metadata extraction.

use carbon_core::{
    error::CarbonResult,
    metrics::MetricsCollection,
    pipeline::{Pipeline, ShutdownStrategy},
    processor::Processor,
    transaction::TransactionProcessorInputType,
};
use carbon_log_metrics::LogMetrics;
use dotenv::dotenv;
use solana_pubkey::Pubkey;
use solana_transaction_status::TransactionStatusMeta;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use yellowstone_grpc_proto::geyser::CommitmentLevel;

#[path = "../empty_decoder.rs"]
mod empty_decoder;
use empty_decoder::EmptyDecoderCollection;

use solflow::streamer_core::{
    balance_extractor::{build_full_account_keys, extract_sol_changes, extract_token_changes},
    grpc_client::create_single_account_client,
};

/// Command-line configuration for mint tracing
#[derive(Clone)]
struct MintTraceConfig {
    target_mint: String,
    geyser_url: String,
    geyser_token: Option<String>,
    commitment_level: CommitmentLevel,
}

impl MintTraceConfig {
    fn from_env_and_args() -> Result<Self, Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();

        // Parse --mint argument
        let target_mint = args
            .windows(2)
            .find(|w| w[0] == "--mint")
            .map(|w| w[1].clone())
            .ok_or("Missing --mint argument. Usage: mint_trace --mint <MINT_ADDRESS>")?;

        // Validate mint address is valid base58
        let _ = Pubkey::try_from(target_mint.as_str())
            .map_err(|_| format!("Invalid mint address: {}", target_mint))?;

        let geyser_url = std::env::var("GEYSER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:10000".to_string());

        let geyser_token = std::env::var("GEYSER_TOKEN").ok();

        let commitment_level = match std::env::var("COMMITMENT_LEVEL")
            .unwrap_or_else(|_| "confirmed".to_string())
            .to_lowercase()
            .as_str()
        {
            "processed" => CommitmentLevel::Processed,
            "confirmed" => CommitmentLevel::Confirmed,
            "finalized" => CommitmentLevel::Finalized,
            _ => CommitmentLevel::Confirmed,
        };

        Ok(Self {
            target_mint,
            geyser_url,
            geyser_token,
            commitment_level,
        })
    }
}

/// Transaction processor that filters and logs all transactions involving the target mint
#[derive(Clone)]
struct MintTraceProcessor {
    target_mint: String,
    match_count: Arc<AtomicU64>,
    total_count: Arc<AtomicU64>,
}

impl MintTraceProcessor {
    fn new(target_mint: String) -> Self {
        Self {
            target_mint,
            match_count: Arc::new(AtomicU64::new(0)),
            total_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Extract all unique mints from token balance changes
    fn extract_mints_from_transaction(&self, meta: &TransactionStatusMeta) -> Vec<String> {
        let mut mints = Vec::new();

        // Extract mints from pre_token_balances
        if let Some(pre_balances) = &meta.pre_token_balances {
            for balance in pre_balances {
                if !mints.contains(&balance.mint) {
                    mints.push(balance.mint.clone());
                }
            }
        }

        // Extract mints from post_token_balances
        if let Some(post_balances) = &meta.post_token_balances {
            for balance in post_balances {
                if !mints.contains(&balance.mint) {
                    mints.push(balance.mint.clone());
                }
            }
        }

        mints
    }

    /// Print comprehensive transaction details
    fn print_transaction_details(
        &self,
        metadata: &Arc<carbon_core::transaction::TransactionMetadata>,
        account_keys: &[Pubkey],
        mints: &[String],
    ) {
        let match_num = self.match_count.load(Ordering::Relaxed);

        println!("\n╔═══════════════════════════════════════════════════════════════════════════════╗");
        println!("║ MINT MATCH #{:<67} ║", match_num);
        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
        println!("║ Target Mint: {:<63} ║", self.target_mint);
        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Transaction metadata
        println!("║ 📊 TRANSACTION METADATA                                                       ║");
        println!("║ Slot:        {:>63} ║", metadata.slot);
        println!("║ Signature:   {:<63} ║", metadata.signature);
        println!("║ Fee Payer:   {:<63} ║", metadata.fee_payer);
        if let Some(block_time) = metadata.block_time {
            println!("║ Block Time:  {:>63} ║", block_time);
        }
        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // All mints involved in this transaction
        println!("║ 🪙 TOKEN MINTS ({:>2})                                                         ║", mints.len());
        for (idx, mint) in mints.iter().enumerate() {
            let marker = if mint == &self.target_mint {
                "→ TARGET"
            } else {
                ""
            };
            println!("║   {}. {:<58} {} ║", idx + 1, mint, marker);
        }
        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Instruction tree
        println!("║ 📋 INSTRUCTION TREE                                                           ║");
        let message = &metadata.message;
        let instructions = message.instructions();
        println!("║   Total Instructions: {:<55} ║", instructions.len());
        
        for (idx, instruction) in instructions.iter().enumerate() {
            let program_id_index = instruction.program_id_index as usize;
            let program_id = account_keys
                .get(program_id_index)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());

            println!("║   [{}] Outer Instruction                                                      ║", idx);
            println!("║       Program:  {:<59} ║", program_id);
            println!("║       Data Len: {:>3} bytes                                                    ║", instruction.data.len());
            println!("║       Accounts: {:>3}                                                          ║", instruction.accounts.len());

            // Print discriminator if instruction data >= 8 bytes
            if instruction.data.len() >= 8 {
                let discriminator = hex::encode(&instruction.data[0..8]);
                println!("║       Discriminator: 0x{:<51} ║", discriminator);
            }
        }

        // Inner instructions
        if let Some(inner_groups) = &metadata.meta.inner_instructions {
            println!("║                                                                               ║");
            println!("║   Inner Instructions: {:<59} ║", inner_groups.len());
            
            for inner_group in inner_groups {
                let outer_idx = inner_group.index as usize;
                println!("║   [{}] Inner Group (from outer instruction {})                               ║", outer_idx, outer_idx);
                
                for (inner_idx, inner) in inner_group.instructions.iter().enumerate() {
                    let program_id_index = inner.instruction.program_id_index as usize;
                    let program_id = account_keys
                        .get(program_id_index)
                        .map(|pk| pk.to_string())
                        .unwrap_or_else(|| "UNKNOWN".to_string());

                    println!("║       [{}.{}] Program:  {:<51} ║", outer_idx, inner_idx, program_id);
                    println!("║             Data Len: {:>3} bytes                                          ║", inner.instruction.data.len());
                }
            }
        }
        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Balance changes
        let sol_deltas = extract_sol_changes(&metadata.meta, account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, account_keys);

        println!("║ 💰 BALANCE CHANGES                                                            ║");
        println!("║   SOL Changes: {:<63} ║", sol_deltas.len());
        for delta in &sol_deltas {
            let direction = if delta.is_inflow() { "+" } else { "-" };
            let account = account_keys
                .get(delta.account_index)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            println!("║     {} {:<8.6} SOL | {:<52} ║", direction, delta.abs_ui_change(), account);
        }

        println!("║                                                                               ║");
        println!("║   Token Changes: {:<60} ║", token_deltas.len());
        for delta in &token_deltas {
            let direction = if delta.is_inflow() { "+" } else { "-" };
            let marker = if delta.mint == self.target_mint {
                "← TARGET"
            } else {
                ""
            };
            let account = account_keys
                .get(delta.account_index)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            
            println!("║     {} {:<12.2} tokens (decimals: {})                                   ║", 
                direction, delta.abs_ui_change(), delta.decimals);
            println!("║       Mint:    {:<58} {} ║", delta.mint, marker);
            println!("║       Account: {:<59} ║", account);
        }

        println!("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Transaction status
        let fee = metadata.meta.fee;
        let success = metadata.meta.status.is_ok();
        let status = if success { "✅ SUCCESS" } else { "❌ FAILED" };

        println!("║ 📈 TRANSACTION STATUS                                                         ║");
        println!("║   Status: {:<71} ║", status);
        println!("║   Fee:    {:<71} lamports ║", fee);

        if let Err(ref err) = metadata.meta.status {
            println!("║   Error:  {:<71} ║", err);
        }

        println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
        println!();
    }
}

#[async_trait::async_trait]
impl Processor for MintTraceProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;

    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        // Increment total transaction count
        let total = self.total_count.fetch_add(1, Ordering::Relaxed) + 1;

        // Log progress every 10,000 transactions
        if total % 10_000 == 0 {
            log::info!("📊 Processed {} transactions, {} matches", total, self.match_count.load(Ordering::Relaxed));
        }

        // Extract all mints from this transaction
        let mints = self.extract_mints_from_transaction(&metadata.meta);

        // Check if target mint is involved
        if !mints.iter().any(|m| m == &self.target_mint) {
            return Ok(());
        }

        // MATCH FOUND - Increment counter
        let match_count = self.match_count.fetch_add(1, Ordering::Relaxed) + 1;

        log::info!(
            "🎯 Match #{}: Signature {} (slot {})",
            match_count,
            metadata.signature,
            metadata.slot
        );

        // Build complete account keys (including ALT-loaded addresses)
        let account_keys = build_full_account_keys(&metadata, &metadata.meta);

        // Print comprehensive transaction details
        self.print_transaction_details(&metadata, &account_keys, &mints);

        Ok(())
    }
}

async fn run_with_reconnect(
    config: &MintTraceConfig,
    processor: MintTraceProcessor,
) -> Result<(), Box<dyn std::error::Error>> {
    let max_retries = 5;
    let mut retry_count = 0;

    loop {
        log::info!("🔌 Connecting to gRPC endpoint: {}", config.geyser_url);
        
        let client = match create_single_account_client(
            &config.geyser_url,
            config.geyser_token.clone(),
            &config.target_mint,
            config.commitment_level,
        )
        .await
        {
            Ok(c) => {
                log::info!("✅ Connected successfully");
                retry_count = 0; // Reset on successful connection
                c
            }
            Err(e) => {
                retry_count += 1;
                log::error!("❌ Connection failed (attempt {}/{}): {}", retry_count, max_retries, e);
                
                if retry_count >= max_retries {
                    return Err(format!("Failed to connect after {} attempts", max_retries).into());
                }
                
                let backoff = std::time::Duration::from_secs(2u64.pow(retry_count.min(5)));
                log::info!("⏳ Retrying in {:?}...", backoff);
                tokio::time::sleep(backoff).await;
                continue;
            }
        };

        log::info!("🚀 Starting mint trace pipeline");

        let proc = processor.clone();
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
            Pipeline::builder()
                .datasource(client)
                .metrics(Arc::new(LogMetrics::new()))
                .metrics_flush_interval(60)
                .transaction::<EmptyDecoderCollection, ()>(proc, None)
                .shutdown_strategy(ShutdownStrategy::Immediate)
                .build()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            Ok(())
        }
        .await;

        match result {
            Ok(_) => {
                log::info!("✅ Pipeline completed successfully");
                return Ok(());
            }
            Err(e) => {
                retry_count += 1;
                log::error!("❌ Pipeline error (attempt {}/{}): {}", retry_count, max_retries, e);
                
                if retry_count >= max_retries {
                    return Err(format!("Pipeline failed after {} attempts: {}", max_retries, e).into());
                }
                
                let backoff = std::time::Duration::from_secs(2u64.pow(retry_count.min(5)));
                log::info!("⏳ Reconnecting in {:?}...", backoff);
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv().ok();

    // Initialize rustls crypto provider (required for TLS connections)
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Parse configuration
    let config = MintTraceConfig::from_env_and_args()?;

    // Initialize logger
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .target(env_logger::Target::Stderr)
    .init();

    println!("\n╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                          MINT TRACE - Transaction Monitor                     ║");
    println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
    println!("║ Target Mint:  {:<67} ║", config.target_mint);
    println!("║ Geyser URL:   {:<67} ║", config.geyser_url);
    println!("║ Commitment:   {:<67} ║", format!("{:?}", config.commitment_level));
    println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
    println!("║ This tool monitors ALL transactions involving the target mint address.       ║");
    println!("║ Press CTRL+C to stop.                                                         ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();

    log::info!("🎯 Target mint: {}", config.target_mint);
    log::info!("🔗 Geyser URL: {}", config.geyser_url);
    log::info!("📊 Commitment: {:?}", config.commitment_level);

    // Create processor
    let processor = MintTraceProcessor::new(config.target_mint.clone());

    // Run with automatic reconnection
    run_with_reconnect(&config, processor).await?;

    Ok(())
}
