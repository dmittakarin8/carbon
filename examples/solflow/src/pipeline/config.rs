//! Pipeline configuration from environment variables
//!
//! Phase 4: Configuration management for pipeline runtime

use std::env;

/// Configuration for pipeline runtime
///
/// Loaded from environment variables with sensible defaults.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Path to SQLite database file
    pub db_path: String,
    
    /// Channel buffer size for trade ingestion (trades)
    pub channel_buffer: usize,
    
    /// Aggregate flush interval in milliseconds
    pub flush_interval_ms: u64,
    
    /// Price update interval in milliseconds
    pub price_interval_ms: u64,
    
    /// Metadata update interval in milliseconds
    pub metadata_interval_ms: u64,
    
    /// Master enable flag for pipeline
    pub enabled: bool,
}

impl PipelineConfig {
    /// Load configuration from environment variables
    ///
    /// Environment variables:
    /// - `SOLFLOW_DB_PATH` (default: /var/lib/solflow/solflow.db)
    /// - `STREAMER_CHANNEL_BUFFER` (default: 10000)
    /// - `AGGREGATE_FLUSH_INTERVAL_MS` (default: 5000)
    /// - `PRICE_UPDATE_INTERVAL_MS` (default: 10000)
    /// - `METADATA_UPDATE_INTERVAL_MS` (default: 60000)
    /// - `ENABLE_PIPELINE` (default: false)
    pub fn from_env() -> Self {
        Self {
            db_path: env::var("SOLFLOW_DB_PATH")
                .unwrap_or_else(|_| "/var/lib/solflow/solflow.db".to_string()),
            
            channel_buffer: env::var("STREAMER_CHANNEL_BUFFER")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000),
            
            flush_interval_ms: env::var("AGGREGATE_FLUSH_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5_000),
            
            price_interval_ms: env::var("PRICE_UPDATE_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000),
            
            metadata_interval_ms: env::var("METADATA_UPDATE_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60_000),
            
            enabled: env::var("ENABLE_PIPELINE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        // Test: Default configuration when no env vars set
        // Clear any existing env vars
        env::remove_var("SOLFLOW_DB_PATH");
        env::remove_var("STREAMER_CHANNEL_BUFFER");
        env::remove_var("ENABLE_PIPELINE");
        
        let config = PipelineConfig::from_env();
        
        assert_eq!(config.db_path, "/var/lib/solflow/solflow.db");
        assert_eq!(config.channel_buffer, 10_000);
        assert_eq!(config.flush_interval_ms, 5_000);
        assert_eq!(config.price_interval_ms, 10_000);
        assert_eq!(config.metadata_interval_ms, 60_000);
        assert_eq!(config.enabled, false);
    }
    
    #[test]
    fn test_custom_config() {
        // Test: Custom configuration from env vars
        env::set_var("SOLFLOW_DB_PATH", "/tmp/test.db");
        env::set_var("STREAMER_CHANNEL_BUFFER", "5000");
        env::set_var("AGGREGATE_FLUSH_INTERVAL_MS", "2000");
        env::set_var("ENABLE_PIPELINE", "true");
        
        let config = PipelineConfig::from_env();
        
        assert_eq!(config.db_path, "/tmp/test.db");
        assert_eq!(config.channel_buffer, 5_000);
        assert_eq!(config.flush_interval_ms, 2_000);
        assert_eq!(config.enabled, true);
        
        // Cleanup
        env::remove_var("SOLFLOW_DB_PATH");
        env::remove_var("STREAMER_CHANNEL_BUFFER");
        env::remove_var("AGGREGATE_FLUSH_INTERVAL_MS");
        env::remove_var("ENABLE_PIPELINE");
    }
}
