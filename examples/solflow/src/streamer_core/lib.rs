use crate::instruction_scanner::InstructionScanner;
use crate::streamer_core::{
    balance_extractor::{build_full_account_keys, extract_sol_changes, extract_token_changes},
    blocklist_checker::BlocklistChecker,
    config::{BackendType, RuntimeConfig, StreamerConfig},
    grpc_client::{run_with_reconnect, create_multi_program_client},
    output_writer::{JsonlWriter, TradeEvent},
    sqlite_writer::SqliteWriter,
    trade_detector::extract_trade_info,
    writer_backend::WriterBackend,
};
use async_trait::async_trait;
use carbon_core::{
    error::CarbonResult,
    metrics::MetricsCollection,
    pipeline::{Pipeline, ShutdownStrategy},
    processor::Processor,
    transaction::TransactionProcessorInputType,
};
use carbon_log_metrics::LogMetrics;
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[path = "../empty_decoder.rs"]
mod empty_decoder;
use empty_decoder::EmptyDecoderCollection;

/// Convert streamer TradeEvent to pipeline TradeEvent format
///
/// Phase 4.2: Dual-channel streaming helper
fn convert_to_pipeline_event(
    event: &TradeEvent,
) -> crate::pipeline::types::TradeEvent {
    use crate::pipeline::types::TradeDirection;
    
    crate::pipeline::types::TradeEvent {
        timestamp: event.timestamp,
        mint: event.mint.clone(),
        direction: match event.action.as_str() {
            "BUY" => TradeDirection::Buy,
            "SELL" => TradeDirection::Sell,
            _ => TradeDirection::Unknown,
        },
        sol_amount: event.sol_amount,
        token_amount: event.token_amount,
        token_decimals: event.token_decimals,
        user_account: event.user_account.clone().unwrap_or_default(),
        source_program: event.program_name.clone(),
    }
}

#[derive(Clone)]
struct TradeProcessor {
    config: StreamerConfig,
    writer: Arc<Mutex<Box<dyn WriterBackend>>>,
    /// Optional pipeline channel for dual-channel streaming (Phase 4.2)
    pipeline_tx: Option<mpsc::Sender<crate::pipeline::types::TradeEvent>>,
    /// Counter for logging pipeline sends every 10k trades
    send_count: Arc<AtomicU64>,
    /// Flag to enable/disable JSONL writes (pipeline is always enabled)
    enable_jsonl: bool,
    /// Blocklist checker for GRPC-level filtering
    blocklist_checker: Option<BlocklistChecker>,
}

impl TradeProcessor {
    fn new(config: StreamerConfig, writer: Box<dyn WriterBackend>, enable_jsonl: bool, blocklist_checker: Option<BlocklistChecker>) -> Self {
        let pipeline_tx = config.pipeline_tx.clone();
        Self {
            config,
            writer: Arc::new(Mutex::new(writer)),
            pipeline_tx,
            send_count: Arc::new(AtomicU64::new(0)),
            enable_jsonl,
            blocklist_checker,
        }
    }
}

#[async_trait]
impl Processor for TradeProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;

    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let account_keys = build_full_account_keys(&metadata, &metadata.meta);
        let sol_deltas = extract_sol_changes(&metadata.meta, &account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, &account_keys);

        if let Some(trade_info) = extract_trade_info(&sol_deltas, &token_deltas, &account_keys) {
            // CRITICAL: Check blocklist BEFORE any processing
            // This is the earliest point in the pipeline - if blocked, discard immediately
            if let Some(ref checker) = self.blocklist_checker {
                match checker.is_blocked(&trade_info.mint) {
                    Ok(true) => {
                        // Token is blocked - discard trade event immediately
                        // No aggregation, no metrics, no DB writes, no WebSocket push
                        log::debug!("🚫 Blocked token detected, discarding: {}", trade_info.mint);
                        return Ok(());
                    }
                    Ok(false) => {
                        // Token is allowed - continue processing
                    }
                    Err(e) => {
                        // Blocklist check failed - log error but continue processing
                        // (fail-open to avoid blocking legitimate trades on DB issues)
                        log::warn!("⚠️  Blocklist check failed for {}: {}", trade_info.mint, e);
                    }
                }
            }

            let discriminator = extract_discriminator_hex(&metadata);

            let event = TradeEvent {
                timestamp: metadata.block_time.unwrap_or_else(|| Utc::now().timestamp()),
                signature: metadata.signature.to_string(),
                program_id: self.config.program_id.clone(),
                program_name: self.config.program_name.clone(),
                action: <&str>::from(trade_info.direction).to_string(),
                mint: trade_info.mint.clone(),
                sol_amount: trade_info.sol_amount,
                token_amount: trade_info.token_amount,
                token_decimals: trade_info.token_decimals,
                user_account: trade_info.user_account.map(|pk| pk.to_string()),
                discriminator,
            };

            // Phase 4.2 Primary Path: Send to pipeline channel (non-blocking)
            // This ALWAYS happens regardless of JSONL setting
            if let Some(tx) = &self.pipeline_tx {
                let pipeline_event = convert_to_pipeline_event(&event);
                
                // try_send is non-blocking - never impacts streamer performance
                if tx.try_send(pipeline_event).is_ok() {
                    // Log every 10,000 successful sends
                    let count = self.send_count.fetch_add(1, Ordering::Relaxed);
                    if count > 0 && count % 10_000 == 0 {
                        log::info!("📊 Pipeline ingestion active: {} trades sent", count);
                    }
                } else {
                    // Channel full or closed - log only once per 1000 failures
                    static FAILURE_COUNT: AtomicU64 = AtomicU64::new(0);
                    let failures = FAILURE_COUNT.fetch_add(1, Ordering::Relaxed);
                    if failures % 1000 == 0 {
                        log::warn!("⚠️  Pipeline channel full or closed (failures: {})", failures);
                    }
                }
            }

            // Optional: Write to JSONL (disabled by default, enabled via ENABLE_JSONL=true)
            if self.enable_jsonl {
                let mut writer = self.writer.lock().await;
                if let Err(e) = writer.write(&event).await {
                    log::error!("Failed to write JSONL event: {:?}", e);
                } else {
                    log::debug!(
                        "✅ JSONL: {} {} {:.6} SOL → {:.2} tokens ({})",
                        event.action,
                        event.signature,
                        event.sol_amount,
                        event.token_amount,
                        event.mint
                    );
                }
            }
        }

        Ok(())
    }
}

fn extract_discriminator_hex(metadata: &carbon_core::transaction::TransactionMetadata) -> String {
    let message = &metadata.message;
    
    for instruction in message.instructions() {
        if instruction.data.len() >= 8 {
            return hex::encode(&instruction.data[0..8]);
        }
    }
    
    "0000000000000000".to_string()
}

pub async fn run(streamer_config: StreamerConfig) -> Result<(), Box<dyn std::error::Error>> {
    streamer_config.validate()?;
    
    let runtime_config = RuntimeConfig::from_env()?;

    // Skip logger init if running inside pipeline_runtime (already initialized)
    if std::env::var("ENABLE_PIPELINE").unwrap_or_default() != "true" {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&runtime_config.rust_log))
            .target(env_logger::Target::Stderr)
            .try_init()
            .ok(); // Ignore error if already initialized
    }

    log::info!("🚀 Starting {} streamer", streamer_config.program_name);
    log::info!("   Program ID: {}", streamer_config.program_id);
    log::info!("   Output: {}", streamer_config.output_path);
    log::info!("   Geyser URL: {}", runtime_config.geyser_url);
    log::info!("   Commitment: {:?}", runtime_config.commitment_level);

    // Initialize blocklist checker (GRPC-level filtering)
    let blocklist_checker = match std::env::var("SOLFLOW_DB_PATH") {
        Ok(db_path) => {
            match BlocklistChecker::new(&db_path) {
                Ok(checker) => {
                    log::info!("✅ Blocklist checker initialized: {}", db_path);
                    log::info!("   └─ Blocked tokens will be discarded at GRPC ingestion layer");
                    Some(checker)
                }
                Err(e) => {
                    log::warn!("⚠️  Blocklist checker disabled: {}", e);
                    log::warn!("   └─ Set SOLFLOW_DB_PATH to enable GRPC-level token blocking");
                    None
                }
            }
        }
        Err(_) => {
            log::info!("ℹ️  Blocklist checker disabled (SOLFLOW_DB_PATH not set)");
            None
        }
    };

    // Log JSONL status
    if runtime_config.enable_jsonl {
        log::info!("📝 JSONL writes: ENABLED");
    } else {
        log::info!("📝 JSONL writes: DISABLED (set ENABLE_JSONL=true to enable)");
    }

    let writer: Box<dyn WriterBackend> = match streamer_config.backend {
        BackendType::Jsonl => {
            Box::new(JsonlWriter::new(
                &streamer_config.output_path,
                runtime_config.output_max_size_mb,
                runtime_config.output_max_rotations,
            )?)
        }
        BackendType::Sqlite => {
            Box::new(SqliteWriter::new(&streamer_config.output_path)?)
        }
    };
    
    log::info!("📊 Backend: {}", writer.backend_type());

    let processor = TradeProcessor::new(
        streamer_config.clone(), 
        writer, 
        runtime_config.enable_jsonl,
        blocklist_checker.clone()
    );

    run_with_reconnect(&runtime_config, &streamer_config.program_id, move |client| {
        let proc = processor.clone();
        async move {
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                Pipeline::builder()
                    .datasource(client)
                    .metrics(Arc::new(LogMetrics::new()))
                    .metrics_flush_interval(3)
                    .transaction::<EmptyDecoderCollection, ()>(proc, None)
                .shutdown_strategy(ShutdownStrategy::Immediate)
                .build()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
                .run()
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            Ok(())
            }.await;
            result
        }
    })
    .await?;

    Ok(())
}

/// Unified Trade Processor with integrated instruction scanner
///
/// This processor replaces the per-program TradeProcessor with a unified
/// version that scans transactions for any of the tracked programs before
/// processing balance deltas.
#[derive(Clone)]
struct UnifiedTradeProcessor {
    scanner: InstructionScanner,
    writer: Arc<Mutex<Box<dyn WriterBackend>>>,
    pipeline_tx: Option<mpsc::Sender<crate::pipeline::types::TradeEvent>>,
    send_count: Arc<AtomicU64>,
    enable_jsonl: bool,
    blocklist_checker: Option<BlocklistChecker>,
}

impl UnifiedTradeProcessor {
    fn new(
        scanner: InstructionScanner,
        writer: Box<dyn WriterBackend>,
        enable_jsonl: bool,
        blocklist_checker: Option<BlocklistChecker>,
        pipeline_tx: Option<mpsc::Sender<crate::pipeline::types::TradeEvent>>,
    ) -> Self {
        Self {
            scanner,
            writer: Arc::new(Mutex::new(writer)),
            pipeline_tx,
            send_count: Arc::new(AtomicU64::new(0)),
            enable_jsonl,
            blocklist_checker,
        }
    }
}

#[async_trait]
impl Processor for UnifiedTradeProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;

    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        // STEP 1: Scan for tracked programs (NEW - FILTERING LAYER)
        let program_match = match self.scanner.scan(&metadata) {
            Some(m) => m,
            None => {
                // No tracked program found - discard transaction immediately
                log::debug!("⏭️  No tracked program matched (signature: {})", metadata.signature);
                return Ok(());
            }
        };

        // VALIDATION PERIOD: Log all matches
        log::info!(
            "✅ Matched {} at {:?} (signature: {})",
            program_match.program_name,
            program_match.instruction_path,
            metadata.signature
        );

        // STEP 2: Extract balance deltas (UNCHANGED)
        let account_keys = build_full_account_keys(&metadata, &metadata.meta);
        let sol_deltas = extract_sol_changes(&metadata.meta, &account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, &account_keys);

        // STEP 3: Extract trade info (UNCHANGED)
        if let Some(trade_info) = extract_trade_info(&sol_deltas, &token_deltas, &account_keys) {
            // STEP 4: Blocklist check (UNCHANGED)
            if let Some(ref checker) = self.blocklist_checker {
                match checker.is_blocked(&trade_info.mint) {
                    Ok(true) => {
                        log::debug!("🚫 Blocked token: {}", trade_info.mint);
                        return Ok(());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log::warn!("⚠️  Blocklist check failed for {}: {}", trade_info.mint, e);
                    }
                }
            }

            let discriminator = extract_discriminator_hex(&metadata);

            // STEP 5: Create trade event (UPDATED WITH MATCHED PROGRAM)
            let event = TradeEvent {
                timestamp: metadata.block_time.unwrap_or_else(|| Utc::now().timestamp()),
                signature: metadata.signature.to_string(),
                program_id: program_match.program_id.to_string(),
                program_name: program_match.program_name.to_string(), // From scanner
                action: <&str>::from(trade_info.direction).to_string(),
                mint: trade_info.mint.clone(),
                sol_amount: trade_info.sol_amount,
                token_amount: trade_info.token_amount,
                token_decimals: trade_info.token_decimals,
                user_account: trade_info.user_account.map(|pk| pk.to_string()),
                discriminator,
            };

            // STEP 6: Write to pipeline + JSONL (UNCHANGED)
            if let Some(tx) = &self.pipeline_tx {
                let pipeline_event = convert_to_pipeline_event(&event);
                if tx.try_send(pipeline_event).is_ok() {
                    let count = self.send_count.fetch_add(1, Ordering::Relaxed);
                    if count > 0 && count % 10_000 == 0 {
                        log::info!("📊 Pipeline ingestion: {} trades sent", count);
                    }
                }
            }

            if self.enable_jsonl {
                let mut writer = self.writer.lock().await;
                if let Err(e) = writer.write(&event).await {
                    log::error!("Failed to write JSONL event: {:?}", e);
                } else {
                    log::debug!(
                        "✅ JSONL: {} {} {:.6} SOL → {:.2} tokens ({})",
                        event.action,
                        event.signature,
                        event.sol_amount,
                        event.token_amount,
                        event.mint
                    );
                }
            }
        }

        Ok(())
    }
}

/// Run the unified streamer with instruction scanner
///
/// This function replaces the per-program `run()` function for the unified streamer.
/// It uses multi-program gRPC filtering and scans both outer and inner instructions
/// for matches against the tracked program registry.
pub async fn run_unified(
    streamer_config: StreamerConfig,
    scanner: InstructionScanner,
) -> Result<(), Box<dyn std::error::Error>> {
    streamer_config.validate()?;

    let runtime_config = RuntimeConfig::from_env()?;

    // Initialize blocklist checker
    let blocklist_checker = match std::env::var("SOLFLOW_DB_PATH") {
        Ok(db_path) => {
            match BlocklistChecker::new(&db_path) {
                Ok(checker) => {
                    log::info!("✅ Blocklist checker initialized: {}", db_path);
                    log::info!("   └─ Blocked tokens will be discarded at scanner level");
                    Some(checker)
                }
                Err(e) => {
                    log::warn!("⚠️  Blocklist checker disabled: {}", e);
                    log::warn!("   └─ Set SOLFLOW_DB_PATH to enable token blocking");
                    None
                }
            }
        }
        Err(_) => {
            log::info!("ℹ️  Blocklist checker disabled (SOLFLOW_DB_PATH not set)");
            None
        }
    };

    // Log JSONL status
    if runtime_config.enable_jsonl {
        log::info!("📝 JSONL writes: ENABLED");
    } else {
        log::info!("📝 JSONL writes: DISABLED (set ENABLE_JSONL=true to enable)");
    }

    let writer: Box<dyn WriterBackend> = match streamer_config.backend {
        BackendType::Jsonl => {
            Box::new(JsonlWriter::new(
                &streamer_config.output_path,
                runtime_config.output_max_size_mb,
                runtime_config.output_max_rotations,
            )?)
        }
        BackendType::Sqlite => {
            Box::new(SqliteWriter::new(&streamer_config.output_path)?)
        }
    };

    log::info!("📊 Backend: {}", writer.backend_type());

    let pipeline_tx = streamer_config.pipeline_tx.clone();

    let processor = UnifiedTradeProcessor::new(
        scanner,
        writer,
        runtime_config.enable_jsonl,
        blocklist_checker,
        pipeline_tx,
    );

    // Create multi-program gRPC client and run with reconnect logic
    let mut backoff = crate::streamer_core::error_handler::ExponentialBackoff::new(5, 60, 10);

    loop {
        match create_multi_program_client(&runtime_config).await {
            Ok(client) => {
                log::info!("✅ Connected to gRPC server (multi-program filter)");
                backoff.reset();

                let proc = processor.clone();
                let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                    Pipeline::builder()
                        .datasource(client)
                        .metrics(Arc::new(LogMetrics::new()))
                        .metrics_flush_interval(3)
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

                if let Err(e) = result {
                    log::error!("❌ Pipeline error: {:?}", e);
                    backoff.sleep().await.map_err(|_| "Max retries exceeded")?;
                } else {
                    log::info!("✅ Pipeline completed gracefully");
                    return Ok(());
                }
            }
            Err(e) => {
                log::error!("❌ Connection failed: {:?}", e);
                backoff.sleep().await.map_err(|_| "Max retries exceeded")?;
            }
        }
    }
}
