//! Jupiter DCA Streamer
//!
//! Monitors Jupiter's Dollar-Cost Averaging program for DCA fill events.
//! Outputs trade events to JSONL in the same schema as PumpSwap/BonkSwap streamers.
//!
//! Program ID: DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M
//!
//! ## Usage
//!
//! ```bash
//! cargo run --release --bin jupiter_dca_streamer
//! ```
//!
//! ## Environment Variables
//!
//! - GEYSER_URL - Yellowstone gRPC endpoint (required)
//! - X_TOKEN - Authentication token (optional)
//! - JUPITER_DCA_OUTPUT_PATH - Output file path (optional, default: streams/jupiter_dca/events.jsonl)
//! - RUST_LOG - Logging level (optional, default: info)

use solflow::streamer_core::{config::StreamerConfig, run};
use solflow::streamer_core::config::BackendType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "data/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("JUPITER_DCA_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/jupiter_dca/events.jsonl".to_string()),
    };

    let config = StreamerConfig {
        program_id: "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string(),
        program_name: "JupiterDCA".to_string(),
        output_path,
        backend,
    };

    config.validate()?;

    run(config).await
}
