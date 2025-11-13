#[path = "../empty_decoder.rs"]
mod empty_decoder;

use {
    async_trait::async_trait,
    carbon_core::{
        error::CarbonResult,
        metrics::MetricsCollection,
        processor::Processor,
        transaction::TransactionProcessorInputType,
    },
    carbon_log_metrics::LogMetrics,
    carbon_yellowstone_grpc_datasource::YellowstoneGrpcGeyserClient,
    chrono,
    dotenv,
    empty_decoder::EmptyDecoderCollection,
    solana_account_decoder_client_types::token::UiTokenAmount,
    solana_pubkey::Pubkey,
    solana_transaction_status::TransactionStatusMeta,
    std::{
        collections::HashMap,
        env,
        sync::Arc,
    },
    tokio::sync::RwLock,
    yellowstone_grpc_proto::geyser::{CommitmentLevel, SubscribeRequestFilterTransactions},
};

/// Configuration loaded from environment variables
struct Config {
    geyser_url: String,
    x_token: Option<String>,
    program_filters: Vec<String>,
}

impl Config {
    fn from_env() -> Self {
        let geyser_url = env::var("GEYSER_URL")
            .expect("GEYSER_URL must be set in .env file");
        
        let x_token = env::var("X_TOKEN").ok();
        
        // Parse PROGRAM_FILTERS (comma-separated list of program IDs)
        let program_filters = env::var("PROGRAM_FILTERS")
            .expect("PROGRAM_FILTERS must be set in .env file (comma-separated program IDs)")
            .split(',')
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        
        Self {
            geyser_url,
            x_token,
            program_filters,
        }
    }
    
    /// Get verified program IDs for reference
    pub fn verified_program_ids() -> Vec<&'static str> {
        vec![
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", // PumpSwap
            "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // LetsBonk Launchpad
            "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG",  // Moonshot
        ]
    }
}

/// Represents a balance change (delta) for a single account
#[derive(Debug, Clone)]
struct BalanceDelta {
    account_index: usize,
    mint: String,
    raw_change: i128,
    ui_change: f64,
    decimals: u8,
    is_sol: bool,
}

impl BalanceDelta {
    fn is_inflow(&self) -> bool {
        self.raw_change > 0
    }

    fn is_outflow(&self) -> bool {
        self.raw_change < 0
    }

    fn abs_ui_change(&self) -> f64 {
        self.ui_change.abs()
    }
}

/// Build complete account keys list (static + loaded addresses from ALTs)
fn build_full_account_keys(
    metadata: &carbon_core::transaction::TransactionMetadata,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let message = &metadata.message;
    let mut all_keys = message.static_account_keys().to_vec();
    
    // Add loaded addresses from Address Lookup Tables (v0 transactions)
    let loaded = &meta.loaded_addresses;
    all_keys.extend(loaded.writable.iter().cloned());
    all_keys.extend(loaded.readonly.iter().cloned());
    
    all_keys
}

/// Extract SOL balance changes from transaction metadata
fn extract_sol_changes(
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

/// Extract token balance changes from transaction metadata
fn extract_token_changes(
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

    // Match pre and post balances by account_index
    for pre in pre_token_balances {
        let post = post_token_balances
            .iter()
            .find(|p| p.account_index == pre.account_index);

        let (pre_raw, pre_ui, decimals) = extract_token_amount(&pre.ui_token_amount);
        let (post_raw, post_ui, _) = match post {
            Some(p) => extract_token_amount(&p.ui_token_amount),
            None => (0, 0.0, decimals),
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

    // Check for new token accounts
    for post in post_token_balances {
        let exists_in_pre = pre_token_balances
            .iter()
            .any(|pre| pre.account_index == post.account_index);

        if !exists_in_pre {
            let (post_raw, post_ui, decimals) = extract_token_amount(&post.ui_token_amount);

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

/// Helper to extract raw amount, UI amount, and decimals from token amount
fn extract_token_amount(ui_amount: &UiTokenAmount) -> (u64, f64, u8) {
    let raw = ui_amount.amount.parse::<u64>().unwrap_or(0);
    let ui = ui_amount.ui_amount.unwrap_or(0.0);
    let decimals = ui_amount.decimals;
    (raw, ui, decimals)
}

/// Find the primary token mint (largest balance change, excluding wrapped SOL)
fn find_primary_token_mint(token_deltas: &[BalanceDelta]) -> Option<String> {
    token_deltas
        .iter()
        .filter(|d| !d.mint.starts_with("So11111"))
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.mint.clone())
}

/// Find user account (largest absolute SOL change, regardless of direction)
/// This works for both BUY (negative change) and SELL (positive change) transactions
fn find_user_account(sol_deltas: &[BalanceDelta]) -> Option<usize> {
    sol_deltas
        .iter()
        .max_by_key(|d| d.raw_change.abs())
        .map(|d| d.account_index)
}

/// Extract trade information from balance deltas
/// Returns: (mint, sol_amount, token_amount, direction)
fn extract_trade_info(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
) -> Option<(String, f64, f64, &'static str)> {
    // Find user account
    let user_idx = find_user_account(sol_deltas)?;

    // Find user's SOL change
    let user_sol_delta = sol_deltas.iter().find(|d| d.account_index == user_idx)?;

    let sol_amount = user_sol_delta.abs_ui_change();
    let direction = if user_sol_delta.is_outflow() {
        "BUY"
    } else if user_sol_delta.is_inflow() {
        "SELL"
    } else {
        "UNKNOWN"
    };

    // Find primary token mint
    let token_mint = find_primary_token_mint(token_deltas)?;

    // Find user's token change for this mint
    let user_token_delta = token_deltas
        .iter()
        .filter(|d| d.mint == token_mint)
        .max_by_key(|d| d.raw_change.abs())?;

    let token_amount = user_token_delta.abs_ui_change();

    Some((token_mint, sol_amount, token_amount, direction))
}

/// Extract discriminator from instruction data (first 8 bytes)
fn extract_discriminator(data: &[u8]) -> Option<[u8; 8]> {
    if data.len() < 8 {
        return None;
    }
    Some([
        data[0], data[1], data[2], data[3],
        data[4], data[5], data[6], data[7],
    ])
}

/// Format discriminator as hex string
fn format_discriminator(disc: &[u8; 8]) -> String {
    hex::encode(disc)
}

/// Determine action (BUY/SELL/UNKNOWN) based on discriminator
/// NOTE: This function is kept for reference but not actively used since we use metadata-based detection
#[allow(dead_code)]
fn determine_action(
    discriminator: &[u8; 8],
    buy_disc: Option<&[u8; 8]>,
    sell_disc: Option<&[u8; 8]>,
) -> &'static str {
    if let Some(buy) = buy_disc {
        if discriminator == buy {
            return "BUY";
        }
    }
    if let Some(sell) = sell_disc {
        if discriminator == sell {
            return "SELL";
        }
    }
    "UNKNOWN"
}

/// Processor that extracts discriminators and identifies BUY/SELL instructions
struct DiscriminatorProcessor {
    program_filters: Vec<String>,
}

#[async_trait]
impl Processor for DiscriminatorProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;
    
    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let meta = &metadata.meta;
        let message = &metadata.message;
        
        // Build account keys list
        let account_keys = build_full_account_keys(&*metadata, meta);
        
        // Extract balance changes from transaction metadata (Carbon's abstraction layer)
        let sol_deltas = extract_sol_changes(meta, &account_keys);
        let token_deltas = extract_token_changes(meta, &account_keys);
        
        // Extract trade information from metadata
        let trade_info = extract_trade_info(&sol_deltas, &token_deltas);
        
        // Get timestamp - block_time is Unix timestamp in seconds (i64)
        let timestamp = metadata.block_time
            .map(|t| {
                // Try to convert Unix timestamp to DateTime
                chrono::DateTime::<chrono::Utc>::from_timestamp(t, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| format!("timestamp:{}", t))
            })
            .unwrap_or_else(|| {
                // Fallback: use current time if block_time not available
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string()
            });
        
        // Track if we found any matching instructions
        let mut found_matching_instruction = false;
        
        // Iterate through instructions
        for instruction in message.instructions().iter() {
            let program_id_index = instruction.program_id_index as usize;
            
            // Bounds check
            if program_id_index >= account_keys.len() {
                continue;
            }
            
            let program_id = account_keys[program_id_index].to_string();
            
            // Filter by program IDs (check if this program is in our filter list)
            if !self.program_filters.contains(&program_id) {
                continue;
            }
            
            found_matching_instruction = true;
            
            // Extract discriminator from instruction data
            let discriminator = match extract_discriminator(&instruction.data) {
                Some(disc) => disc,
                None => {
                    log::debug!(
                        "Skipping instruction in transaction {}: insufficient data ({} bytes)",
                        metadata.signature,
                        instruction.data.len()
                    );
                    continue;
                }
            };
            
            // Format discriminator as hex
            let disc_hex = format_discriminator(&discriminator);
            
            // Use trade info from metadata (metadata-based detection is more accurate)
            let (mint, sol_amount, token_amount, action) = match &trade_info {
                Some((m, sol, tok, dir)) => {
                    // Prefer metadata-based detection (more accurate)
                    (m.clone(), *sol, *tok, *dir)
                }
                None => {
                    // Fallback: extract from deltas if metadata extraction failed
                    let mint_str = find_primary_token_mint(&token_deltas)
                        .unwrap_or_else(|| "unknown".to_string());
                    let sol_amt = sol_deltas
                        .iter()
                        .max_by_key(|d| d.raw_change.abs())
                        .map(|d| d.abs_ui_change())
                        .unwrap_or(0.0);
                    let tok_amt = token_deltas
                        .iter()
                        .max_by_key(|d| d.raw_change.abs())
                        .map(|d| d.abs_ui_change())
                        .unwrap_or(0.0);
                    (mint_str, sol_amt, tok_amt, "UNKNOWN")
                }
            };
            
            // Print output with full mint address and trade information
            println!(
                "[{}] sig={} program={} discriminator={} action={} mint={} sol={:.6} token={:.6}",
                timestamp,
                metadata.signature,
                program_id,
                disc_hex,
                action,
                mint,
                sol_amount,
                token_amount
            );
        }
        
        // If no matching instructions but we have trade info, still print it
        // This can happen when the transaction involves the program via CPI or indirectly
        if !found_matching_instruction {
            if let Some((mint, sol_amount, token_amount, action)) = trade_info {
                // Show all program IDs we're monitoring (for context)
                let programs_str = self.program_filters.join(",");
                println!(
                    "[{}] sig={} programs={} (no matching instruction) action={} mint={} sol={:.6} token={:.6}",
                    timestamp,
                    metadata.signature,
                    programs_str,
                    action,
                    mint,
                    sol_amount,
                    token_amount
                );
            }
        }
        
        Ok(())
    }
}

#[tokio::main]
pub async fn main() -> CarbonResult<()> {
    dotenv::dotenv().ok();
    
    let config = Config::from_env();
    
    // Initialize logger
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.filter_module("carbon_log_metrics", log::LevelFilter::Warn);
    builder.target(env_logger::Target::Stdout).init();
    
    // NOTE: Workaround for rustls issue
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Can't set crypto provider to aws_lc_rs");
    
    log::info!("🚀 Starting gRPC Discriminator Verification Script");
    log::info!("📊 Configuration:");
    log::info!("   GEYSER_URL: {}", config.geyser_url);
    log::info!("   PROGRAM_FILTERS: {} program(s)", config.program_filters.len());
    for (idx, program_id) in config.program_filters.iter().enumerate() {
        log::info!("     {}. {}", idx + 1, program_id);
    }
    log::info!("   Detection: Metadata-based (BUY/SELL from SOL flow direction)");
    
    // Setup transaction filters - create separate filter for each program ID (OR logic)
    // account_required uses AND logic (all accounts must be present), so we need separate filters
    // Yellowstone treats multiple filters as OR (any filter can match)
    let mut transaction_filters: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
    
    for (idx, program_id) in config.program_filters.iter().enumerate() {
        let filter = SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![],
            account_exclude: vec![],
            account_required: vec![program_id.clone()],
            signature: None,
        };
        transaction_filters.insert(format!("program_filter_{}", idx), filter);
    }
    
    log::info!("   Filter logic: OR (transactions matching ANY program will be included)");
    
    log::info!("🔌 Connecting to Yellowstone gRPC: {}", config.geyser_url);
    let yellowstone_grpc = YellowstoneGrpcGeyserClient::new(
        config.geyser_url.clone(),
        config.x_token.clone(),
        Some(CommitmentLevel::Confirmed),
        HashMap::default(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
    );
    
    // Create processor
    let processor = DiscriminatorProcessor {
        program_filters: config.program_filters.clone(),
    };
    
    log::info!("✅ Pipeline configured, starting data stream...");
    log::info!("📡 Monitoring {} program(s) for trades", config.program_filters.len());
    log::info!("Press Ctrl+C to stop");
    
    // Run pipeline
    carbon_core::pipeline::Pipeline::builder()
        .datasource(yellowstone_grpc)
        .metrics(Arc::new(LogMetrics::new()))
        .metrics_flush_interval(3)
        .transaction::<EmptyDecoderCollection, ()>(processor, None)
        .shutdown_strategy(carbon_core::pipeline::ShutdownStrategy::Immediate)
        .build()?
        .run()
        .await?;
    
    Ok(())
}

