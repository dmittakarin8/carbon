//! [DEPRECATED] PumpSwap Trade Streamer
//!
//! This binary has been replaced by the unified streamer in `pipeline_runtime.rs`.
//! 
//! As of 2025-11-26, all 5 tracked programs (PumpFun, PumpSwap, BonkSwap, 
//! Moonshot, Jupiter DCA) are handled by a single unified streamer using 
//! InstructionScanner for multi-program detection.
//!
//! **Do not use this binary in production.** It will be removed in a future release.
//!
//! For details, see: `docs/20251126-unified-instruction-scanner-architecture.md`

use solflow::streamer_core::{run, StreamerConfig};
use solflow::streamer_core::config::BackendType;
use dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "/var/lib/solflow/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("PUMPSWAP_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/pumpswap/events.jsonl".to_string()),
    };
    
    if backend == BackendType::Sqlite {
        log::info!("💾 SQLite backend using: {}", output_path);
    }

    let config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwap".to_string(),
        output_path,
        backend,
        pipeline_tx: None, // Phase 4.2: Set by pipeline_runtime when enabled
    };

    run(config).await
}
