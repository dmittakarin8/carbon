use carbon_terminal::streamer_core::{run, StreamerConfig};
use carbon_terminal::streamer_core::config::BackendType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "data/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("BONKSWAP_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/bonkswap/events.jsonl".to_string()),
    };

    let config = StreamerConfig {
        program_id: "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj".to_string(),
        program_name: "BonkSwap".to_string(),
        output_path,
        backend,
    };

    run(config).await
}
