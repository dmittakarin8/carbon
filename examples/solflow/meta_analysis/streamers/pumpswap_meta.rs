use async_trait::async_trait;
use carbon_core::{
    error::CarbonResult,
    metrics::MetricsCollection,
    pipeline::{Pipeline, ShutdownStrategy},
    processor::Processor,
    transaction::TransactionProcessorInputType,
};
use carbon_log_metrics::LogMetrics;
use carbon_yellowstone_grpc_datasource::YellowstoneGrpcGeyserClient;
use chrono::Utc;
use dotenv;
use solflow::meta_analysis::{CaptureMetadata, MetadataCaptureProcessor};
use std::{collections::HashMap, env, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use yellowstone_grpc_proto::geyser::{CommitmentLevel, SubscribeRequestFilterTransactions};

// Use EmptyDecoderCollection from the library
type EmptyDecoderCollection = solflow::empty_decoder::EmptyDecoderCollection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    // Initialize rustls crypto provider (required for gRPC)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Refinement #3: Configurable sample size
    let sample_size = env::var("META_ANALYSIS_SAMPLE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let program_id = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string();
    let program_name = "PumpSwap".to_string();

    // Refinement #2: Embed program context
    let capture_metadata = CaptureMetadata {
        program_id: program_id.clone(),
        program_name: program_name.clone(),
        capture_tool_version: env!("CARGO_PKG_VERSION").to_string(),
        captured_at: Utc::now().timestamp(),
    };

    // Output path
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");
    let output_dir = PathBuf::from("data/meta_analysis/pumpswap/raw");
    tokio::fs::create_dir_all(&output_dir).await?;
    let output_path = output_dir.join(format!("{}_session.jsonl", timestamp));

    log::info!("🔬 PumpSwap Meta-Analysis Capture");
    log::info!("   Program ID: {}", program_id);
    log::info!("   Sample Size: {}", sample_size);
    log::info!("   Output: {}", output_path.display());

    // Create processor
    let processor = MetadataCaptureProcessor::new(
        program_name.clone(),
        output_path.clone(),
        sample_size,
        capture_metadata,
    );

    // Keep reference for session metadata
    let processor_for_meta = processor.clone();

    // Connect to Yellowstone
    let geyser_url = env::var("GEYSER_URL").expect("GEYSER_URL must be set");
    let x_token = env::var("X_TOKEN").ok();

    let mut transaction_filters: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
    transaction_filters.insert(
        "pumpswap_filter".to_string(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec![],
            account_exclude: vec![],
            account_required: vec![program_id],
            signature: None,
        },
    );

    let client = YellowstoneGrpcGeyserClient::new(
        geyser_url,
        x_token,
        Some(CommitmentLevel::Confirmed),
        HashMap::default(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(std::collections::HashSet::new())),
        Default::default(),
    );

    log::info!("📡 Starting capture...");

    // Run pipeline (will stop when transaction limit reached)
    let result = Pipeline::builder()
        .datasource(client)
        .metrics(Arc::new(LogMetrics::new()))
        .metrics_flush_interval(3)
        .transaction::<EmptyDecoderCollection, ()>(processor, None)
        .shutdown_strategy(ShutdownStrategy::Immediate)
        .build()?
        .run()
        .await;

    // Refinement #4: Write session metadata
    let session_meta = processor_for_meta.get_session_metadata().await;
    let meta_path = output_dir.join(format!("{}_session_meta.json", timestamp));
    let meta_json = serde_json::to_string_pretty(&session_meta)?;
    tokio::fs::write(meta_path, meta_json).await?;

    log::info!("✅ Capture complete: {} transactions", session_meta.transactions_captured);
    log::info!("   Inner instructions: {}", session_meta.inner_instruction_stats.total_inner_instructions);
    log::info!("   Unique programs: {}", session_meta.inner_instruction_stats.unique_inner_programs.len());

    result.map_err(|e| e.into())
}
