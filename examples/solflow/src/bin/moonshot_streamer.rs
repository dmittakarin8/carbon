use solflow::streamer_core::{run, StreamerConfig};
use solflow::streamer_core::config::BackendType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "/var/lib/solflow/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("MOONSHOT_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/moonshot/events.jsonl".to_string()),
    };
    
    if backend == BackendType::Sqlite {
        log::info!("💾 SQLite backend using: {}", output_path);
    }

    let config = StreamerConfig {
        program_id: "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG".to_string(),
        program_name: "Moonshot".to_string(),
        output_path,
        backend,
        pipeline_tx: None, // Phase 4.2: Set by pipeline_runtime when enabled
    };

    run(config).await
}
