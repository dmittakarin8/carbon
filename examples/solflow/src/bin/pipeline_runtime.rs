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
    scheduler::flush_scheduler_task,
    types::TradeEvent,
};
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
    info!("   └─ Metadata interval: {}ms", config.metadata_interval_ms);

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

    // Phase 4.2: Example of how to spawn streamers with pipeline integration
    // Uncomment and modify when ready to activate dual-channel streaming
    
    /* EXAMPLE: Spawn PumpSwap streamer with pipeline channel
    
    use solflow::streamer_core::{config::BackendType, StreamerConfig};
    
    let pumpswap_config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwap".to_string(),
        output_path: std::env::var("PUMPSWAP_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/pumpswap/events.jsonl".to_string()),
        backend: BackendType::Jsonl,
        pipeline_tx: Some(tx.clone()), // Enable dual-channel streaming
    };
    
    tokio::spawn(async move {
        info!("🚀 Starting PumpSwap streamer with pipeline integration");
        if let Err(e) = solflow::streamer_core::run(pumpswap_config).await {
            error!("❌ PumpSwap streamer failed: {}", e);
        }
    });
    
    // Repeat for other streamers (BonkSwap, Moonshot, JupiterDCA)...
    */
    
    info!("⚠️  Note: Streamer spawning currently disabled (see commented code above)");
    info!("   └─ To activate: Uncomment streamer spawn code in pipeline_runtime.rs");
    info!("   └─ This will enable dual-channel streaming to pipeline");

    // Spawn background tasks
    info!("🚀 Spawning background tasks...");

    // Task 1: Ingestion (processes trades from channel)
    let engine_ingestion = engine.clone();
    let db_writer_ingestion = db_writer.clone();
    let flush_interval = config.flush_interval_ms;
    tokio::spawn(async move {
        start_pipeline_ingestion(rx, engine_ingestion, db_writer_ingestion, flush_interval).await;
    });
    info!("   ├─ ✅ Ingestion task spawned");

    // Task 2: Flush scheduler (periodic aggregate writes)
    let engine_flush = engine.clone();
    let db_writer_flush = db_writer.clone();
    let flush_interval_scheduler = config.flush_interval_ms;
    tokio::spawn(async move {
        flush_scheduler_task(engine_flush, db_writer_flush, flush_interval_scheduler).await;
    });
    info!("   ├─ ✅ Flush scheduler spawned");

    // Task 3: Price scheduler (TODO: Phase 4.1)
    // tokio::spawn(async move {
    //     price_scheduler_task(engine.clone(), db_writer.clone(), config.price_interval_ms).await;
    // });
    info!("   ├─ ⏸️  Price scheduler (Phase 4.1)");

    // Task 4: Metadata scheduler (TODO: Phase 4.1)
    // tokio::spawn(async move {
    //     metadata_scheduler_task(engine.clone(), db_writer.clone(), config.metadata_interval_ms).await;
    // });
    info!("   └─ ⏸️  Metadata scheduler (Phase 4.1)");

    info!("✅ All background tasks running");
    info!("");
    info!("📊 Pipeline Status:");
    info!("   ├─ Ingestion: READY (waiting for trades)");
    info!("   ├─ Flush: ACTIVE (every {}ms)", config.flush_interval_ms);
    info!("   ├─ Price: DISABLED (Phase 4.1)");
    info!("   └─ Metadata: DISABLED (Phase 4.1)");
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
