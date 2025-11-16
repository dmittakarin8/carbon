use std::env;
use yellowstone_grpc_proto::geyser::CommitmentLevel;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum BackendType {
    Jsonl,
    Sqlite,
}

#[derive(Clone)]
pub struct StreamerConfig {
    pub program_id: String,
    pub program_name: String,
    pub output_path: String,
    pub backend: BackendType,
    /// Optional pipeline channel for dual-channel streaming (Phase 4.2)
    /// When Some, trades are sent to both legacy writer AND pipeline engine
    pub pipeline_tx: Option<mpsc::Sender<crate::pipeline::types::TradeEvent>>,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub geyser_url: String,
    pub x_token: Option<String>,
    pub commitment_level: CommitmentLevel,
    pub rust_log: String,
    pub output_max_size_mb: u64,
    pub output_max_rotations: u32,
    pub enable_jsonl: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    MissingVariable(String),
    InvalidValue(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingVariable(var) => write!(f, "Missing environment variable: {}", var),
            ConfigError::InvalidValue(msg) => write!(f, "Invalid configuration value: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let geyser_url = env::var("GEYSER_URL")
            .map_err(|_| ConfigError::MissingVariable("GEYSER_URL".to_string()))?;

        if !geyser_url.starts_with("http://") && !geyser_url.starts_with("https://") {
            return Err(ConfigError::InvalidValue(
                "GEYSER_URL must start with http:// or https://".to_string(),
            ));
        }

        let x_token = env::var("X_TOKEN").ok();

        let commitment_str = env::var("COMMITMENT_LEVEL").unwrap_or_else(|_| "Confirmed".to_string());
        let commitment_level = match commitment_str.to_lowercase().as_str() {
            "finalized" => CommitmentLevel::Finalized,
            "confirmed" => CommitmentLevel::Confirmed,
            "processed" => CommitmentLevel::Processed,
            _ => {
                log::warn!(
                    "Invalid COMMITMENT_LEVEL '{}', defaulting to Confirmed",
                    commitment_str
                );
                CommitmentLevel::Confirmed
            }
        };

        let rust_log = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let output_max_size_mb = env::var("OUTPUT_MAX_SIZE_MB")
            .unwrap_or_else(|_| "100".to_string())
            .parse::<u64>()
            .unwrap_or(100);

        let output_max_rotations = env::var("OUTPUT_MAX_ROTATIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u32>()
            .unwrap_or(10);

        let enable_jsonl = env::var("ENABLE_JSONL")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            .parse::<bool>()
            .unwrap_or(false);

        Ok(Self {
            geyser_url,
            x_token,
            commitment_level,
            rust_log,
            output_max_size_mb,
            output_max_rotations,
            enable_jsonl,
        })
    }
}

impl StreamerConfig {
    pub fn parse_backend_from_args() -> BackendType {
        let args: Vec<String> = env::args().collect();
        
        if args.contains(&"--backend".to_string()) {
            if let Some(idx) = args.iter().position(|x| x == "--backend") {
                match args.get(idx + 1).map(|s| s.as_str()) {
                    Some("sqlite") => return BackendType::Sqlite,
                    Some("jsonl") => return BackendType::Jsonl,
                    _ => {}
                }
            }
        }
        
        BackendType::Jsonl // Default to JSONL
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.program_id.len() < 32 || self.program_id.len() > 44 {
            return Err(ConfigError::InvalidValue(
                format!("program_id must be 32-44 characters (base58 Pubkey), got {}", self.program_id.len()),
            ));
        }

        if self.program_name.is_empty() {
            return Err(ConfigError::InvalidValue(
                "program_name cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}
