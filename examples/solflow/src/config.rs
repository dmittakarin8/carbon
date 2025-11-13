use std::env;

/// Configuration loaded from environment variables
pub struct Config {
    pub geyser_url: String,
    pub x_token: Option<String>,
    pub program_filters: Vec<String>,
    pub rust_log: Option<String>,
}

impl Config {
    /// Load configuration from environment variables
    /// 
    /// By default, no program filtering is applied (processes all transactions).
    /// Set PROGRAM_FILTERS env variable (comma-separated) to filter by program IDs.
    pub fn from_env() -> Self {
        let geyser_url = env::var("GEYSER_URL")
            .expect("GEYSER_URL must be set in .env file");
        
        let x_token = env::var("X_TOKEN").ok();
        
        // Optional program filters (comma-separated list)
        // If not set, processes all transactions (unfiltered baseline)
        let program_filters = env::var("PROGRAM_FILTERS")
            .map(|s| {
                s.split(',')
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        
        let rust_log = env::var("RUST_LOG").ok();
        
        Self {
            geyser_url,
            x_token,
            program_filters,
            rust_log,
        }
    }
    
    /// Get verified program IDs for reference (not used by default)
    /// These are available for optional filtering via PROGRAM_FILTERS env var
    #[allow(dead_code)]
    pub fn verified_program_ids() -> Vec<&'static str> {
        vec![
            "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", // PumpSwap
            "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // LetsBonk Launchpad
            "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", // Meteora DLMM
        ]
    }
}

