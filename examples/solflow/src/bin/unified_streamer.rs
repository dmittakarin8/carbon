//! Unified SolFlow Streamer
//!
//! This binary replaces the 4 individual program streamers (PumpSwap, BonkSwap,
//! Moonshot, Jupiter DCA) with a single unified ingestion system that:
//!
//! - Subscribes to 5 programs via gRPC (including PumpFun)
//! - Scans both outer and inner (CPI) instructions
//! - Detects all tracked program interactions
//! - Provides complete coverage including nested program calls

use solflow::instruction_scanner::InstructionScanner;
use solflow::streamer_core::config::BackendType;
use solflow::streamer_core::{run_unified, RuntimeConfig, StreamerConfig};
use dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    // Initialize runtime configuration
    let runtime_config = RuntimeConfig::from_env()?;

    // Initialize logger if not already initialized by pipeline_runtime
    if std::env::var("ENABLE_PIPELINE").unwrap_or_default() != "true" {
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or(&runtime_config.rust_log),
        )
        .target(env_logger::Target::Stderr)
        .try_init()
        .ok();
    }

    log::info!("🚀 Starting Unified SolFlow Streamer");
    log::info!("   Tracked Programs: 5 (PumpFun, PumpSwap, BonkSwap, Moonshot, Jupiter DCA)");
    log::info!("   gRPC Filter: Multi-program subscription");
    log::info!("   Coverage: Outer + Inner (CPI) instructions");
    log::info!("   Geyser URL: {}", runtime_config.geyser_url);
    log::info!("   Commitment: {:?}", runtime_config.commitment_level);

    // Parse backend type from command line
    let backend = StreamerConfig::parse_backend_from_args();

    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "/var/lib/solflow/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("UNIFIED_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/unified/events.jsonl".to_string()),
    };

    if backend == BackendType::Sqlite {
        log::info!("💾 SQLite backend: {}", output_path);
    } else {
        log::info!("📝 JSONL backend: {}", output_path);
    }

    // Initialize the instruction scanner
    let scanner = InstructionScanner::new();

    // Create a config with placeholder program_id (validation requires valid base58)
    // The actual program filtering happens in the scanner
    let config = StreamerConfig {
        program_id: "11111111111111111111111111111111".to_string(), // Placeholder system program
        program_name: "Unified".to_string(),
        output_path,
        backend,
        pipeline_tx: None,
    };

    // Run the unified streamer with the scanner
    run_unified(config, scanner).await
}
