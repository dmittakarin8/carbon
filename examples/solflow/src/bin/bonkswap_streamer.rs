use solflow::streamer_core::{run, StreamerConfig};
use solflow::streamer_core::config::BackendType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "/var/lib/solflow/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("BONKSWAP_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/bonkswap/events.jsonl".to_string()),
    };
    
    if backend == BackendType::Sqlite {
        log::info!("💾 SQLite backend using: {}", output_path);
    }

    let config = StreamerConfig {
        program_id: "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj".to_string(),
        program_name: "BonkSwap".to_string(),
        output_path,
        backend,
        pipeline_tx: None, // Phase 4.2: Set by pipeline_runtime when enabled
    };

    run(config).await
}
