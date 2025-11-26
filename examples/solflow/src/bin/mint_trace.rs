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
//! cargo run --bin mint_trace -- --mint <MINT_ADDRESS> --log-file mint_trace.log
//! ```
//!
//! ## Environment Variables
//!
//! - `GEYSER_URL` - gRPC endpoint (required)
//! - `X_TOKEN` - Authentication token (optional, but required for authenticated endpoints)
//! - `COMMITMENT_LEVEL` - Transaction commitment (default: Confirmed)
//! - `RUST_LOG` - Log level (default: info)
//!
//! ## Authentication
//!
//! Authentication is standardized: X_TOKEN must come from the project's .env file
//! and is loaded through RuntimeConfig, matching pipeline_runtime.
//!
//! The tool uses dotenv to load .env at startup, then RuntimeConfig reads X_TOKEN
//! from the environment. This ensures identical behavior to pipeline_runtime.
//!
//! **Important:** X_TOKEN must be set in the .env file, NOT exported in your shell.
//! If you see "401 Unauthorized" errors, check that .env contains X_TOKEN.
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
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use yellowstone_grpc_proto::geyser::CommitmentLevel;

#[path = "../empty_decoder.rs"]
mod empty_decoder;
use empty_decoder::EmptyDecoderCollection;

use solflow::streamer_core::{
    balance_extractor::{build_full_account_keys, extract_sol_changes, extract_token_changes},
    config::RuntimeConfig,
    grpc_client::create_single_account_client,
};

/// Logger helper for writing to console and/or file
///
/// Supports two modes:
/// - Console-only: All output goes to stdout
/// - File mode: All output goes to both console and file (with BufWriter for performance)
#[derive(Clone)]
struct Logger {
    file_writer: Option<Arc<Mutex<BufWriter<std::fs::File>>>>,
}

impl Logger {
    /// Create a console-only logger
    fn console_only() -> Self {
        Self { file_writer: None }
    }

    /// Create a logger that writes to both console and file
    fn with_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        
        let writer = BufWriter::new(file);
        
        Ok(Self {
            file_writer: Some(Arc::new(Mutex::new(writer))),
        })
    }

    /// Log a single line to console and file (if enabled)
    fn log_line(&self, line: &str) {
        // Always print to console
        println!("{}", line);

        // If file writer is enabled, also write to file
        if let Some(ref writer_arc) = self.file_writer {
            if let Ok(mut writer) = writer_arc.lock() {
                let _ = writeln!(writer, "{}", line);
            }
        }
    }

    /// Log multiple lines (used for complex blocks)
    fn log_block(&self, lines: Vec<String>) {
        for line in lines {
            self.log_line(&line);
        }
    }

    /// Flush the file buffer after each transaction block
    fn flush(&self) {
        if let Some(ref writer_arc) = self.file_writer {
            if let Ok(mut writer) = writer_arc.lock() {
                let _ = writer.flush();
            }
        }
    }
}

/// Command-line configuration for mint tracing
///
/// Note: gRPC and auth configuration is intentionally mirrored from pipeline_runtime
/// for consistency. We use RuntimeConfig to ensure identical connection behavior.
#[derive(Clone)]
struct MintTraceConfig {
    target_mint: String,
    log_file_path: Option<String>,
    runtime_config: RuntimeConfig,
}

impl MintTraceConfig {
    fn from_env_and_args() -> Result<Self, Box<dyn std::error::Error>> {
        let args: Vec<String> = std::env::args().collect();

        // Parse --mint argument
        let target_mint = args
            .windows(2)
            .find(|w| w[0] == "--mint")
            .map(|w| w[1].clone())
            .ok_or("Missing --mint argument. Usage: mint_trace --mint <MINT_ADDRESS> [--log-file <PATH>]")?;

        // Validate mint address is valid base58
        let _ = Pubkey::try_from(target_mint.as_str())
            .map_err(|_| format!("Invalid mint address: {}", target_mint))?;

        // Parse optional --log-file argument
        let log_file_path = args
            .windows(2)
            .find(|w| w[0] == "--log-file")
            .map(|w| w[1].clone());

        // Use RuntimeConfig to read env vars (same as pipeline_runtime)
        // This ensures consistent behavior for GEYSER_URL, X_TOKEN, COMMITMENT_LEVEL, etc.
        let runtime_config = RuntimeConfig::from_env()?;

        Ok(Self {
            target_mint,
            log_file_path,
            runtime_config,
        })
    }
}

/// Transaction processor that filters and logs all transactions involving the target mint
#[derive(Clone)]
struct MintTraceProcessor {
    target_mint: String,
    match_count: Arc<AtomicU64>,
    total_count: Arc<AtomicU64>,
    logger: Logger,
}

impl MintTraceProcessor {
    fn new(target_mint: String, logger: Logger) -> Self {
        Self {
            target_mint,
            match_count: Arc::new(AtomicU64::new(0)),
            total_count: Arc::new(AtomicU64::new(0)),
            logger,
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

    /// Print comprehensive transaction details using the logger
    fn print_transaction_details(
        &self,
        metadata: &Arc<carbon_core::transaction::TransactionMetadata>,
        account_keys: &[Pubkey],
        mints: &[String],
    ) {
        let match_num = self.match_count.load(Ordering::Relaxed);

        self.logger.log_line("");
        self.logger.log_line("╔═══════════════════════════════════════════════════════════════════════════════╗");
        self.logger.log_line(&format!("║ MINT MATCH #{:<67} ║", match_num));
        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");
        self.logger.log_line(&format!("║ Target Mint: {:<63} ║", self.target_mint));
        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Transaction metadata
        self.logger.log_line("║ 📊 TRANSACTION METADATA                                                       ║");
        self.logger.log_line(&format!("║ Slot:        {:>63} ║", metadata.slot));
        self.logger.log_line(&format!("║ Signature:   {:<63} ║", metadata.signature));
        self.logger.log_line(&format!("║ Fee Payer:   {:<63} ║", metadata.fee_payer));
        if let Some(block_time) = metadata.block_time {
            self.logger.log_line(&format!("║ Block Time:  {:>63} ║", block_time));
        }
        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // All mints involved in this transaction
        self.logger.log_line(&format!("║ 🪙 TOKEN MINTS ({:>2})                                                         ║", mints.len()));
        for (idx, mint) in mints.iter().enumerate() {
            let marker = if mint == &self.target_mint {
                "→ TARGET"
            } else {
                ""
            };
            self.logger.log_line(&format!("║   {}. {:<58} {} ║", idx + 1, mint, marker));
        }
        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Instruction tree
        self.logger.log_line("║ 📋 INSTRUCTION TREE                                                           ║");
        let message = &metadata.message;
        let instructions = message.instructions();
        self.logger.log_line(&format!("║   Total Instructions: {:<55} ║", instructions.len()));
        
        for (idx, instruction) in instructions.iter().enumerate() {
            let program_id_index = instruction.program_id_index as usize;
            let program_id = account_keys
                .get(program_id_index)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());

            self.logger.log_line(&format!("║   [{}] Outer Instruction                                                      ║", idx));
            self.logger.log_line(&format!("║       Program:  {:<59} ║", program_id));
            self.logger.log_line(&format!("║       Data Len: {:>3} bytes                                                    ║", instruction.data.len()));
            self.logger.log_line(&format!("║       Accounts: {:>3}                                                          ║", instruction.accounts.len()));

            // Print discriminator if instruction data >= 8 bytes
            if instruction.data.len() >= 8 {
                let discriminator = hex::encode(&instruction.data[0..8]);
                self.logger.log_line(&format!("║       Discriminator: 0x{:<51} ║", discriminator));
            }
        }

        // Inner instructions
        if let Some(inner_groups) = &metadata.meta.inner_instructions {
            self.logger.log_line("║                                                                               ║");
            self.logger.log_line(&format!("║   Inner Instructions: {:<59} ║", inner_groups.len()));
            
            for inner_group in inner_groups {
                let outer_idx = inner_group.index as usize;
                self.logger.log_line(&format!("║   [{}] Inner Group (from outer instruction {})                               ║", outer_idx, outer_idx));
                
                for (inner_idx, inner) in inner_group.instructions.iter().enumerate() {
                    let program_id_index = inner.instruction.program_id_index as usize;
                    let program_id = account_keys
                        .get(program_id_index)
                        .map(|pk| pk.to_string())
                        .unwrap_or_else(|| "UNKNOWN".to_string());

                    self.logger.log_line(&format!("║       [{}.{}] Program:  {:<51} ║", outer_idx, inner_idx, program_id));
                    self.logger.log_line(&format!("║             Data Len: {:>3} bytes                                          ║", inner.instruction.data.len()));
                }
            }
        }
        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Balance changes
        let sol_deltas = extract_sol_changes(&metadata.meta, account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, account_keys);

        self.logger.log_line("║ 💰 BALANCE CHANGES                                                            ║");
        self.logger.log_line(&format!("║   SOL Changes: {:<63} ║", sol_deltas.len()));
        for delta in &sol_deltas {
            let direction = if delta.is_inflow() { "+" } else { "-" };
            let account = account_keys
                .get(delta.account_index)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            self.logger.log_line(&format!("║     {} {:<8.6} SOL | {:<52} ║", direction, delta.abs_ui_change(), account));
        }

        self.logger.log_line("║                                                                               ║");
        self.logger.log_line(&format!("║   Token Changes: {:<60} ║", token_deltas.len()));
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
            
            self.logger.log_line(&format!("║     {} {:<12.2} tokens (decimals: {})                                   ║", 
                direction, delta.abs_ui_change(), delta.decimals));
            self.logger.log_line(&format!("║       Mint:    {:<58} {} ║", delta.mint, marker));
            self.logger.log_line(&format!("║       Account: {:<59} ║", account));
        }

        self.logger.log_line("╠═══════════════════════════════════════════════════════════════════════════════╣");

        // Transaction status
        let fee = metadata.meta.fee;
        let success = metadata.meta.status.is_ok();
        let status = if success { "✅ SUCCESS" } else { "❌ FAILED" };

        self.logger.log_line("║ 📈 TRANSACTION STATUS                                                         ║");
        self.logger.log_line(&format!("║   Status: {:<71} ║", status));
        self.logger.log_line(&format!("║   Fee:    {:<71} lamports ║", fee));

        if let Err(ref err) = metadata.meta.status {
            self.logger.log_line(&format!("║   Error:  {:<71} ║", err));
        }

        self.logger.log_line("╚═══════════════════════════════════════════════════════════════════════════════╝");
        self.logger.log_line("");
        
        // Flush the file buffer after each transaction block
        self.logger.flush();
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
        log::info!("🔌 Connecting to gRPC endpoint: {}", config.runtime_config.geyser_url);
        
        let client = match create_single_account_client(
            &config.runtime_config.geyser_url,
            config.runtime_config.x_token.clone(),
            &config.target_mint,
            config.runtime_config.commitment_level,
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
                let error_msg = format!("{}", e);
                
                // Check if this looks like an auth error
                if error_msg.contains("401") || error_msg.contains("Unauthorized") || error_msg.contains("invalid compression flag") {
                    log::error!("❌ gRPC authentication failed (attempt {}/{})", retry_count, max_retries);
                    log::error!("   Error details: {}", e);
                    log::error!("   💡 This usually means:");
                    log::error!("      - X_TOKEN is missing or invalid in .env file");
                    log::error!("      - GEYSER_URL requires authentication");
                    log::error!("   Check that your .env file contains:");
                    log::error!("      X_TOKEN=\"your-valid-token\"");
                } else {
                    log::error!("❌ Connection failed (attempt {}/{}): {}", retry_count, max_retries, e);
                }
                
                if retry_count >= max_retries {
                    return Err(format!("Failed to connect after {} attempts. Check X_TOKEN in .env file and GEYSER_URL", max_retries).into());
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
                let error_msg = format!("{}", e);
                
                // Check for auth-related errors in pipeline execution
                if error_msg.contains("401") || error_msg.contains("Unauthorized") || error_msg.contains("invalid compression flag") {
                    log::error!("❌ gRPC stream failed with authentication error (attempt {}/{})", retry_count, max_retries);
                    log::error!("   Error: {}", e);
                    log::error!("   💡 Check X_TOKEN in .env file");
                } else {
                    log::error!("❌ Pipeline error (attempt {}/{}): {}", retry_count, max_retries, e);
                }
                
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
    // CRITICAL: Load environment variables from .env file FIRST
    // This must happen before RuntimeConfig reads X_TOKEN
    // Both mint_trace and pipeline_runtime use this same pattern
    match dotenv() {
        Ok(path) => {
            eprintln!("✅ Loaded .env file from: {}", path.display());
        }
        Err(_) => {
            eprintln!("⚠️  No .env file found (this is usually an error)");
            eprintln!("   X_TOKEN must be provided via .env file for authentication");
        }
    }

    // Initialize rustls crypto provider (required for TLS connections)
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Parse configuration (RuntimeConfig will now read X_TOKEN from dotenv-loaded env)
    let config = MintTraceConfig::from_env_and_args()?;

    // Initialize logger
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .target(env_logger::Target::Stderr)
    .init();

    // Create logger based on configuration
    let logger = if let Some(ref log_file) = config.log_file_path {
        match Logger::with_file(log_file) {
            Ok(l) => {
                println!("📝 Logging to file: {}", log_file);
                l
            }
            Err(e) => {
                eprintln!("❌ Failed to open log file '{}': {}", log_file, e);
                return Err(e);
            }
        }
    } else {
        Logger::console_only()
    };

    println!("\n╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                          MINT TRACE - Transaction Monitor                     ║");
    println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
    println!("║ Target Mint:  {:<67} ║", config.target_mint);
    println!("║ Geyser URL:   {:<67} ║", config.runtime_config.geyser_url);
    println!("║ Commitment:   {:<67} ║", format!("{:?}", config.runtime_config.commitment_level));
    
    // Auth status (without leaking token value)
    let auth_status = if config.runtime_config.x_token.is_some() {
        "✅ Configured"
    } else {
        "⚠️  Not set (may fail on authenticated endpoints)"
    };
    println!("║ Auth Token:   {:<67} ║", auth_status);
    
    if let Some(ref log_file) = config.log_file_path {
        println!("║ Log File:     {:<67} ║", log_file);
    }
    println!("╠═══════════════════════════════════════════════════════════════════════════════╣");
    println!("║ This tool monitors ALL transactions involving the target mint address.       ║");
    println!("║ Press CTRL+C to stop.                                                         ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝");
    println!();

    log::info!("🎯 Target mint: {}", config.target_mint);
    log::info!("🔗 Geyser URL: {}", config.runtime_config.geyser_url);
    log::info!("📊 Commitment: {:?}", config.runtime_config.commitment_level);
    
    // Log auth status without exposing token
    // X_TOKEN must come from .env file (loaded by dotenv above)
    if config.runtime_config.x_token.is_some() {
        log::info!("🔐 X_TOKEN detected via .env file (authentication enabled)");
    } else {
        log::error!("❌ X_TOKEN missing in .env file (authentication will fail)");
        log::error!("   Add X_TOKEN to your .env file:");
        log::error!("   GEYSER_URL=\"https://your-endpoint.com\"");
        log::error!("   X_TOKEN=\"your-token-here\"");
        log::error!("");
        log::error!("   Do NOT export X_TOKEN in your shell - it must be in .env");
        
        return Err("Authentication error: X_TOKEN must be set in the project's .env file (not shell environment)".into());
    }
    
    if let Some(ref log_file) = config.log_file_path {
        log::info!("📝 Log file: {}", log_file);
    }

    // Create processor with logger
    let processor = MintTraceProcessor::new(config.target_mint.clone(), logger);

    // Run with automatic reconnection
    run_with_reconnect(&config, processor).await?;

    Ok(())
}
