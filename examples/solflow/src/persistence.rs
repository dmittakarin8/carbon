use {
    crate::state::Trade,
    serde::{Deserialize, Serialize},
    std::{
        fs,
        path::Path,
        time::Duration,
    },
    tokio::time::interval,
};

/// Persistence configuration
pub struct PersistenceConfig {
    pub file_path: String,
    pub autosave_interval: Duration,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            file_path: "trades.json".to_string(),
            autosave_interval: Duration::from_secs(60), // 60 seconds
        }
    }
}

/// Snapshot of state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub trades: Vec<Trade>,
    pub timestamp: i64,
}

/// Save state snapshot to JSON file
pub fn save_snapshot(trades: &[Trade], file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = StateSnapshot {
        trades: trades.to_vec(),
        timestamp: crate::state::current_timestamp(),
    };
    
    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(file_path, json)?;
    
    log::debug!("Saved {} trades to {}", trades.len(), file_path);
    Ok(())
}

/// Load state snapshot from JSON file
pub fn load_snapshot(file_path: &str) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
    if !Path::new(file_path).exists() {
        log::info!("No existing snapshot file found: {}", file_path);
        return Ok(Vec::new());
    }
    
    let json = fs::read_to_string(file_path)?;
    let snapshot: StateSnapshot = serde_json::from_str(&json)?;
    
    log::info!("Loaded {} trades from {}", snapshot.trades.len(), file_path);
    Ok(snapshot.trades)
}

/// Background task that periodically saves state snapshot
pub async fn persistence_task(
    state: std::sync::Arc<tokio::sync::RwLock<crate::state::State>>,
    config: PersistenceConfig,
) {
    let mut interval_timer = interval(config.autosave_interval);
    
    loop {
        interval_timer.tick().await;
        
        let trades = {
            let state = state.read().await;
            state.get_recent_trades().to_vec()
        };
        
        if let Err(e) = save_snapshot(&trades, &config.file_path) {
            log::warn!("Failed to save snapshot: {}", e);
        }
    }
}

