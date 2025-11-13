use solflow::streamer_core::{run, StreamerConfig};
use solflow::streamer_core::config::BackendType;
use dotenv;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let backend = StreamerConfig::parse_backend_from_args();
    
    let output_path = match backend {
        BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
            .unwrap_or_else(|_| "data/solflow.db".to_string()),
        BackendType::Jsonl => std::env::var("PUMPSWAP_OUTPUT_PATH")
            .unwrap_or_else(|_| "streams/pumpswap/events.jsonl".to_string()),
    };

    let config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwap".to_string(),
        output_path,
        backend,
    };

    run(config).await
}
