use crate::streamer_core::{
    balance_extractor::{build_full_account_keys, extract_sol_changes, extract_token_changes},
    config::{BackendType, RuntimeConfig, StreamerConfig},
    grpc_client::run_with_reconnect,
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
use std::sync::Arc;
use tokio::sync::Mutex;

#[path = "../empty_decoder.rs"]
mod empty_decoder;
use empty_decoder::EmptyDecoderCollection;

#[derive(Clone)]
struct TradeProcessor {
    config: StreamerConfig,
    writer: Arc<Mutex<Box<dyn WriterBackend>>>,
}

impl TradeProcessor {
    fn new(config: StreamerConfig, writer: Box<dyn WriterBackend>) -> Self {
        Self {
            config,
            writer: Arc::new(Mutex::new(writer)),
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

            let mut writer = self.writer.lock().await;
            if let Err(e) = writer.write(&event).await {
                log::error!("Failed to write event: {:?}", e);
            } else {
                log::debug!(
                    "✅ {} {} {:.6} SOL → {:.2} tokens ({})",
                    event.action,
                    event.signature,
                    event.sol_amount,
                    event.token_amount,
                    event.mint
                );
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

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&runtime_config.rust_log))
        .target(env_logger::Target::Stderr)
        .init();

    log::info!("🚀 Starting {} streamer", streamer_config.program_name);
    log::info!("   Program ID: {}", streamer_config.program_id);
    log::info!("   Output: {}", streamer_config.output_path);
    log::info!("   Geyser URL: {}", runtime_config.geyser_url);
    log::info!("   Commitment: {:?}", runtime_config.commitment_level);

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

    let processor = TradeProcessor::new(streamer_config.clone(), writer);

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
