#[cfg(test)]
mod tests;

pub mod aggregator_core;
mod aggregator;
mod config;
pub mod empty_decoder;
mod persistence;
mod state;
pub mod sqlite_pragma;
mod trade_extractor;
mod ui;

pub mod streamer_core;
pub mod pipeline;
pub mod meta_analysis;

use {
    async_trait::async_trait,
    carbon_core::{
        error::CarbonResult,
        metrics::MetricsCollection,
        processor::Processor,
        transaction::TransactionProcessorInputType,
    },
    empty_decoder::EmptyDecoderCollection,
    carbon_log_metrics::LogMetrics,
    carbon_yellowstone_grpc_datasource::YellowstoneGrpcGeyserClient,
    config::Config,
    state::{State, StateMessage, current_timestamp},
    std::{
        collections::HashMap,
        sync::Arc,
    },
    tokio::sync::{mpsc, RwLock},
    trade_extractor::{build_full_account_keys, extract_sol_changes, extract_token_changes, extract_user_volumes},
    yellowstone_grpc_proto::geyser::{CommitmentLevel, SubscribeRequestFilterTransactions},
};

#[tokio::main]
pub async fn main() -> CarbonResult<()> {
    dotenv::dotenv().ok();
    
    let config = Config::from_env();
    
    // Initialize logger if RUST_LOG is set
    // Write logs to stderr (will be suppressed when UI enters alternate screen)
    // Suppress carbon_log_metrics spam by default (set RUST_LOG=carbon_log_metrics=warn to see them)
    let mut builder = if config.rust_log.is_some() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    } else {
        env_logger::Builder::from_default_env()
    };
    
    // Filter out carbon_log_metrics unless explicitly enabled
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    if !log_level.contains("carbon_log_metrics") {
        builder.filter_module("carbon_log_metrics", log::LevelFilter::Warn);
    }
    
    builder.target(env_logger::Target::Stderr).init();
    
    // NOTE: Workaround for rustls issue
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Can't set crypto provider to aws_lc_rs");
    
    // Log startup information (before UI starts to avoid overlay)
    log::info!("🚀 Starting SolFlow...");
    log::info!("📊 Configuration:");
    log::info!("   GEYSER_URL: {}", config.geyser_url);
    let filters_str = if config.program_filters.is_empty() {
        "None (processing all transactions)".to_string()
    } else {
        format!("{:?}", config.program_filters)
    };
    log::info!("   Program Filters: {}", filters_str);
    
    // Setup transaction filter (match pattern from jupiter-swap-alerts)
    let mut transaction_filters: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
    
    if !config.program_filters.is_empty() {
        let transaction_filter = SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![],
            account_exclude: vec![],
            account_required: config.program_filters.clone(),
            signature: None,
        };
        
        transaction_filters.insert("transaction_filter".to_string(), transaction_filter);
    }
    
    // Create bounded channel for state messages (backpressure handling)
    let (tx, rx) = mpsc::channel::<StateMessage>(1000);
    
    // Create shared state
    let state = Arc::new(RwLock::new(State::new(1000))); // Keep last 1000 trades
    
    // Load previous state from persistence (if exists)
    if let Ok(previous_trades) = persistence::load_snapshot("trades.json") {
        let trade_count = previous_trades.len();
        let mut state_write = state.write().await;
        for trade in previous_trades {
            state_write.add_trade(trade);
        }
        log::info!("Loaded {} trades from persistence", trade_count);
    }
    
    // Spawn background aggregator task
    let state_clone = state.clone();
    tokio::spawn(async move {
        state::state_aggregator_task(rx, state_clone).await;
    });
    
    // Spawn persistence task (autosave every 60s)
    let state_for_persistence = state.clone();
    tokio::spawn(async move {
        persistence::persistence_task(state_for_persistence, persistence::PersistenceConfig::default()).await;
    });
    
    log::info!("🔌 Connecting to Yellowstone gRPC: {}", config.geyser_url);
    let yellowstone_grpc = YellowstoneGrpcGeyserClient::new(
        config.geyser_url,
        config.x_token,
        Some(CommitmentLevel::Confirmed),
        HashMap::default(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
        Default::default(),
    );
    
    // Create processor with channel sender
    let processor = TradeProcessor { tx };
    log::info!("✅ Pipeline configured, starting data stream...");
    
    // Spawn UI task (needed for terminal interface)
    let state_for_ui = state.clone();
    let ui_handle = tokio::spawn(async move {
        if let Err(e) = ui::run_ui(state_for_ui).await {
            log::error!("UI error: {}", e);
        }
    });
    
    // Run pipeline directly (matching jupiter-swap-alerts pattern)
    // Use tokio::select to run both UI and pipeline concurrently
    tokio::select! {
        _ = ui_handle => {
            log::info!("UI exited");
        }
        result = async {
            log::info!("📡 Starting pipeline...");
            carbon_core::pipeline::Pipeline::builder()
                .datasource(yellowstone_grpc)
                .metrics(Arc::new(LogMetrics::new()))
                .metrics_flush_interval(3)
                .transaction::<EmptyDecoderCollection, ()>(processor, None)
                .shutdown_strategy(carbon_core::pipeline::ShutdownStrategy::Immediate)
                .build()?
                .run()
                .await
        } => {
            match result {
                Ok(_) => log::info!("✅ Pipeline completed successfully"),
                Err(e) => log::error!("❌ Pipeline error: {:?}", e),
            }
        }
    }
    
    Ok(())
}

/// Processor that extracts trades from transactions and sends them to state aggregator
pub struct TradeProcessor {
    tx: mpsc::Sender<StateMessage>,
}

#[async_trait]
impl Processor for TradeProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;
    
    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let meta = &metadata.meta;
        
        // Log first transaction received (for connection verification)
        static FIRST_TX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
        if FIRST_TX.swap(false, std::sync::atomic::Ordering::Relaxed) {
            log::info!("📥 First transaction received: {}", metadata.signature);
        }
        
        // Build full account keys list (handles v0 transactions with ALTs)
        let account_keys = build_full_account_keys(&metadata, meta);
        
        // Extract balance changes
        let sol_deltas = extract_sol_changes(meta, &account_keys);
        let token_deltas = extract_token_changes(meta, &account_keys);
        
        // Skip if no significant balance changes
        if sol_deltas.is_empty() && token_deltas.is_empty() {
            log::debug!("Skipping transaction {}: no balance changes", metadata.signature);
            return Ok(());
        }
        
        // Extract user volumes (filters out pool/fee accounts)
        if let Some((sol_volume, token_volume, token_mint, decimals, direction)) =
            extract_user_volumes(&sol_deltas, &token_deltas)
        {
            // Create trade struct
            let trade = state::Trade {
                signature: metadata.signature,
                timestamp: metadata.block_time.unwrap_or_else(current_timestamp),
                mint: token_mint.clone(),
                direction,
                sol_amount: sol_volume,
                token_amount: token_volume,
                token_decimals: decimals,
            };
            
            // Store values for logging before moving trade
            let direction_str = match trade.direction {
                crate::trade_extractor::TradeKind::Buy => "BUY",
                crate::trade_extractor::TradeKind::Sell => "SELL",
                crate::trade_extractor::TradeKind::Unknown => "UNK",
            };
            let mint_short = trade.mint[..8].to_string();
            let sol_amount = trade.sol_amount;
            let token_amount = trade.token_amount;
            let signature = trade.signature;
            
            // Diagnostic logging: signature, mint, SOL Δ, token Δ, direction
            log::info!(
                "Trade: sig={} mint={} sol_Δ={:.6} token_Δ={:.6} dir={}",
                signature,
                mint_short,
                sol_amount,
                token_amount,
                direction_str
            );
            
            // Send to state aggregator via channel
            if let Err(e) = self.tx.send(StateMessage::Trade(trade)).await {
                log::warn!("Failed to send trade to state aggregator: {}", e);
                // Count dropped transactions in metrics
                // Note: carbon-log-metrics will automatically count this
            } else {
                log::debug!("✅ Trade sent to aggregator: {} {} {:.6} SOL", 
                    mint_short, 
                    direction_str,
                    sol_amount
                );
            }
        } else {
            // No valid trade extracted (e.g., fee-only transaction)
            log::debug!("No valid trade extracted from transaction: {} (has {} SOL deltas, {} token deltas)", 
                metadata.signature, 
                sol_deltas.len(), 
                token_deltas.len()
            );
        }
        
        Ok(())
    }
}

