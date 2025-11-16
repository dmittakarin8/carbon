//! Pipeline Runtime - Phase 4 Activation Layer
//!
//! This binary orchestrates the new analytics pipeline:
//! - Initializes SQLite database with schema
//! - Creates PipelineEngine with database writer
//! - Spawns background tasks (ingestion, schedulers)
//! - Runs in parallel with legacy aggregator (zero impact)
//!
//! Usage:
//!   cargo run --release --bin pipeline_runtime
//!
//! Environment variables:
//!   SOLFLOW_DB_PATH - SQLite database path (default: /var/lib/solflow/solflow.db)
//!   ENABLE_PIPELINE - Master switch (default: false)
//!   AGGREGATE_FLUSH_INTERVAL_MS - Flush interval (default: 5000)
//!   STREAMER_CHANNEL_BUFFER - Channel size (default: 10000)

use dotenv::dotenv;
use log::{error, info};
use rusqlite::Connection;
use solflow::pipeline::{
    config::PipelineConfig,
    db::{run_schema_migrations, AggregateDbWriter, SqliteAggregateWriter},
    engine::PipelineEngine,
    ingestion::start_pipeline_ingestion,
    types::TradeEvent,
};
use solflow::streamer_core::{config::{BackendType, StreamerConfig}, run as run_streamer};
use std::env;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize environment and logging
    dotenv().ok();
    env_logger::init();

    info!("🚀 Pipeline Runtime - Phase 4 Activation Layer");
    info!("   ├─ Version: 0.1.0");
    info!("   └─ Mode: Parallel (legacy aggregator unaffected)");

    // Load configuration
    let config = PipelineConfig::from_env();

    if !config.enabled {
        info!("⚠️  Pipeline is DISABLED (set ENABLE_PIPELINE=true to activate)");
        info!("   └─ Exiting gracefully...");
        return Ok(());
    }

    info!("✅ Pipeline ENABLED");
    info!("   ├─ Database: {}", config.db_path);
    info!("   ├─ Channel buffer: {} trades", config.channel_buffer);
    info!("   ├─ Flush interval: {}ms", config.flush_interval_ms);
    info!("   ├─ Price interval: {}ms", config.price_interval_ms);
    info!("   ├─ Metadata interval: {}ms", config.metadata_interval_ms);
    info!("   └─ Integrated streamers: 4 (PumpSwap, BonkSwap, Moonshot, JupiterDCA)");

    // Initialize database
    info!("🔧 Initializing database...");
    let mut conn = Connection::open(&config.db_path)?;

    // Run schema migrations (idempotent)
    run_schema_migrations(&mut conn, "sql")?;
    drop(conn); // Close temporary connection

    // Create database writer
    let db_writer: Arc<dyn AggregateDbWriter + Send + Sync> =
        Arc::new(SqliteAggregateWriter::new(&config.db_path)?);
    info!("✅ Database initialized");

    // Create PipelineEngine
    let engine = Arc::new(Mutex::new(PipelineEngine::new()));
    info!("✅ PipelineEngine created");

    // Create trade event channel
    let (tx, rx) = mpsc::channel::<TradeEvent>(config.channel_buffer);
    info!("✅ Trade channel created (buffer: {})", config.channel_buffer);

    // Phase 4.2b: Spawn all streamers with pipeline integration
    info!("🚀 Spawning streamers with pipeline integration...");
    
    // Streamer 1: PumpSwap
    let tx_pump = tx.clone();
    tokio::spawn(async move {
        info!("   ├─ Starting PumpSwap streamer with pipeline connected");
        let streamer_config = StreamerConfig {
            program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
            program_name: "PumpSwap".to_string(),
            output_path: env::var("PUMPSWAP_OUTPUT_PATH")
                .unwrap_or_else(|_| "streams/pumpswap/events.jsonl".to_string()),
            backend: BackendType::Jsonl,
            pipeline_tx: Some(tx_pump),
        };
        if let Err(e) = run_streamer(streamer_config).await {
            error!("❌ PumpSwap streamer failed: {}", e);
        }
    });
    
    // Streamer 2: BonkSwap
    let tx_bonk = tx.clone();
    tokio::spawn(async move {
        info!("   ├─ Starting BonkSwap streamer with pipeline connected");
        let streamer_config = StreamerConfig {
            program_id: "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj".to_string(),
            program_name: "BonkSwap".to_string(),
            output_path: env::var("BONKSWAP_OUTPUT_PATH")
                .unwrap_or_else(|_| "streams/bonkswap/events.jsonl".to_string()),
            backend: BackendType::Jsonl,
            pipeline_tx: Some(tx_bonk),
        };
        if let Err(e) = run_streamer(streamer_config).await {
            error!("❌ BonkSwap streamer failed: {}", e);
        }
    });
    
    // Streamer 3: Moonshot
    let tx_moon = tx.clone();
    tokio::spawn(async move {
        info!("   ├─ Starting Moonshot streamer with pipeline connected");
        let streamer_config = StreamerConfig {
            program_id: "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG".to_string(),
            program_name: "Moonshot".to_string(),
            output_path: env::var("MOONSHOT_OUTPUT_PATH")
                .unwrap_or_else(|_| "streams/moonshot/events.jsonl".to_string()),
            backend: BackendType::Jsonl,
            pipeline_tx: Some(tx_moon),
        };
        if let Err(e) = run_streamer(streamer_config).await {
            error!("❌ Moonshot streamer failed: {}", e);
        }
    });
    
    // Streamer 4: Jupiter DCA
    let tx_jup = tx.clone();
    tokio::spawn(async move {
        info!("   └─ Starting JupiterDCA streamer with pipeline connected");
        let streamer_config = StreamerConfig {
            program_id: "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string(),
            program_name: "JupiterDCA".to_string(),
            output_path: env::var("JUPITER_DCA_OUTPUT_PATH")
                .unwrap_or_else(|_| "streams/jupiter_dca/events.jsonl".to_string()),
            backend: BackendType::Jsonl,
            pipeline_tx: Some(tx_jup),
        };
        if let Err(e) = run_streamer(streamer_config).await {
            error!("❌ JupiterDCA streamer failed: {}", e);
        }
    });
    
    info!("✅ All 4 streamers spawned and connected to pipeline");

    // Spawn background tasks
    info!("🚀 Spawning background tasks...");

    // Task 1: Ingestion (processes trades from channel + unified flush loop)
    let engine_ingestion = engine.clone();
    let db_writer_ingestion = db_writer.clone();
    let flush_interval = config.flush_interval_ms;
    tokio::spawn(async move {
        start_pipeline_ingestion(rx, engine_ingestion, db_writer_ingestion, flush_interval).await;
    });
    info!("   └─ ✅ Ingestion task spawned (includes unified flush loop)");

    info!("✅ All background tasks running");
    info!("");
    info!("📊 Pipeline Status:");
    info!("   ├─ Ingestion: READY (unified flush every {}ms)", config.flush_interval_ms);
    info!("   └─ Streamers: 4 active (PumpSwap, BonkSwap, Moonshot, JupiterDCA)");
    info!("");
    info!("🔄 Press CTRL+C to shutdown gracefully");

    // Wait for CTRL+C
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("");
            info!("⚠️  Received CTRL+C, shutting down...");
        }
        Err(err) => {
            error!("❌ Failed to listen for CTRL+C: {}", err);
        }
    }

    // Cleanup: Drop tx to close channel
    drop(tx);

    // Give tasks time to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    info!("✅ Pipeline runtime stopped");
    Ok(())
}
