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

    // Refinement #3: Moonshot-specific sample size
    let sample_size = env::var("META_ANALYSIS_MOONSHOT_SAMPLE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| env::var("META_ANALYSIS_SAMPLE_SIZE")
            .ok()
            .and_then(|s| s.parse().ok()))
        .unwrap_or(150);  // Default 150 for Moonshot

    let program_id = "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG".to_string();
    let program_name = "Moonshot".to_string();

    let capture_metadata = CaptureMetadata {
        program_id: program_id.clone(),
        program_name: program_name.clone(),
        capture_tool_version: env!("CARGO_PKG_VERSION").to_string(),
        captured_at: Utc::now().timestamp(),
    };

    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S");
    let output_dir = PathBuf::from("data/meta_analysis/moonshot/raw");
    tokio::fs::create_dir_all(&output_dir).await?;
    let output_path = output_dir.join(format!("{}_session.jsonl", timestamp));

    log::info!("🔬 Moonshot Meta-Analysis Capture");
    log::info!("   Program ID: {}", program_id);
    log::info!("   Sample Size: {} (increased for CPI analysis)", sample_size);
    log::info!("   Output: {}", output_path.display());

    let processor = MetadataCaptureProcessor::new(
        program_name.clone(),
        output_path.clone(),
        sample_size,
        capture_metadata,
    );

    let processor_for_meta = processor.clone();

    let geyser_url = env::var("GEYSER_URL").expect("GEYSER_URL must be set");
    let x_token = env::var("X_TOKEN").ok();

    let mut transaction_filters: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
    transaction_filters.insert(
        "moonshot_filter".to_string(),
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

    let result = Pipeline::builder()
        .datasource(client)
        .metrics(Arc::new(LogMetrics::new()))
        .metrics_flush_interval(3)
        .transaction::<EmptyDecoderCollection, ()>(processor, None)
        .shutdown_strategy(ShutdownStrategy::Immediate)
        .build()?
        .run()
        .await;

    let session_meta = processor_for_meta.get_session_metadata().await;
    let meta_path = output_dir.join(format!("{}_session_meta.json", timestamp));
    let meta_json = serde_json::to_string_pretty(&session_meta)?;
    tokio::fs::write(meta_path, meta_json).await?;

    log::info!("✅ Capture complete: {} transactions", session_meta.transactions_captured);
    log::info!("   Inner instructions: {}", session_meta.inner_instruction_stats.total_inner_instructions);
    log::info!("   Unique programs: {}", session_meta.inner_instruction_stats.unique_inner_programs.len());

    result.map_err(|e| e.into())
}
