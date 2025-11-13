//! SQLite writer for enriched metrics
//!
//! Maps EnrichedMetrics to TradeEvent schema for unified database storage.

use async_trait::async_trait;
use serde_json::json;
use crate::streamer_core::{
    output_writer::TradeEvent, 
    sqlite_writer::SqliteWriter,
    writer_backend::WriterBackend,
};
use super::writer::EnrichedMetrics;
use super::writer_backend::{AggregatorWriterBackend, AggregatorWriterError};

/// SQLite backend for enriched metrics
pub struct SqliteAggregatorWriter {
    sqlite_writer: SqliteWriter,
    monotonic_counter: u64,
}

impl SqliteAggregatorWriter {
    pub fn new(db_path: impl AsRef<std::path::Path>) -> Result<Self, AggregatorWriterError> {
        let writer = SqliteWriter::new(db_path)
            .map_err(|e| AggregatorWriterError::Database(e.to_string()))?;
        
        log::info!("✅ SQLite aggregator writer initialized");
        
        Ok(Self {
            sqlite_writer: writer,
            monotonic_counter: 0,
        })
    }
}

#[async_trait]
impl AggregatorWriterBackend for SqliteAggregatorWriter {
    async fn write_metrics(&mut self, metrics: &EnrichedMetrics) -> Result<(), AggregatorWriterError> {
        // Build discriminator JSON with all enrichment data
        let discriminator_json = json!({
            "uptrend_score": metrics.uptrend_score,
            "dca_overlap_pct": metrics.dca_overlap_pct,
            "buy_sell_ratio": metrics.buy_sell_ratio,
        });
        
        // Map EnrichedMetrics to TradeEvent schema
        let event = TradeEvent {
            timestamp: metrics.timestamp,
            signature: format!(
                "agg_{}_{}_{}_{}",
                metrics.mint,
                metrics.window,
                metrics.timestamp,
                self.monotonic_counter
            ),
            program_id: "AGGREGATOR_SYSTEM".to_string(),
            program_name: "Aggregator".to_string(),
            action: metrics.signal.clone().unwrap_or_else(|| "NEUTRAL".to_string()),
            mint: metrics.mint.clone(),
            sol_amount: metrics.net_flow_sol,
            token_amount: 0.0,  // Not applicable for aggregated metrics
            token_decimals: 0,   // Not applicable for aggregated metrics
            user_account: None,
            discriminator: discriminator_json.to_string(),
        };
        
        self.sqlite_writer.write(&event).await
            .map_err(|e| AggregatorWriterError::Database(e.to_string()))?;
        
        self.monotonic_counter += 1;
        
        log::debug!(
            "✅ Aggregator metrics written: {} (window: {}, signal: {:?})",
            metrics.mint,
            metrics.window,
            metrics.signal
        );
        
        Ok(())
    }
    
    async fn flush(&mut self) -> Result<(), AggregatorWriterError> {
        self.sqlite_writer.flush().await
            .map_err(|e| AggregatorWriterError::Database(e.to_string()))
    }
    
    fn backend_type(&self) -> &'static str {
        "SQLite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use rusqlite::{Connection, params};

    fn create_test_metrics(mint: &str) -> EnrichedMetrics {
        EnrichedMetrics {
            mint: mint.to_string(),
            window: "1h".to_string(),
            net_flow_sol: 123.45,
            buy_sell_ratio: 0.68,
            dca_overlap_pct: 27.3,
            uptrend_score: 0.82,
            signal: Some("ACCUMULATION".to_string()),
            timestamp: 1700000000,
        }
    }

    #[tokio::test]
    async fn test_sqlite_aggregator_write() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteAggregatorWriter::new(&db_path).unwrap();
        
        let metrics = create_test_metrics("test_mint");
        
        writer.write_metrics(&metrics).await.unwrap();
        writer.flush().await.unwrap();
        
        // Verify insert
        let conn = Connection::open(&db_path).unwrap();
        let (program_name, token_amount, token_decimals, discriminator): (String, f64, u8, String) = conn.query_row(
            "SELECT program_name, token_amount, token_decimals, discriminator FROM trades WHERE mint = ?1",
            params![metrics.mint],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap();
        
        assert_eq!(program_name, "Aggregator");
        assert_eq!(token_amount, 0.0);
        assert_eq!(token_decimals, 0);
        assert!(discriminator.contains("uptrend_score"));
        assert!(discriminator.contains("dca_overlap_pct"));
        assert!(discriminator.contains("buy_sell_ratio"));
    }

    #[tokio::test]
    async fn test_monotonic_counter_uniqueness() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteAggregatorWriter::new(&db_path).unwrap();
        
        // Write same metrics 3 times
        let metrics = create_test_metrics("same_mint");
        
        writer.write_metrics(&metrics).await.unwrap();
        writer.write_metrics(&metrics).await.unwrap();
        writer.write_metrics(&metrics).await.unwrap();
        writer.flush().await.unwrap();
        
        // Verify all 3 inserts succeeded (unique signatures)
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE mint = ?1",
            params![metrics.mint],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 3);
        
        // Verify signatures are unique
        let unique_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT signature) FROM trades WHERE mint = ?1",
            params![metrics.mint],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(unique_count, 3);
    }

    #[tokio::test]
    async fn test_discriminator_json_format() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteAggregatorWriter::new(&db_path).unwrap();
        
        let metrics = create_test_metrics("json_test");
        
        writer.write_metrics(&metrics).await.unwrap();
        writer.flush().await.unwrap();
        
        let conn = Connection::open(&db_path).unwrap();
        let discriminator: String = conn.query_row(
            "SELECT discriminator FROM trades WHERE mint = ?1",
            params![metrics.mint],
            |row| row.get(0),
        ).unwrap();
        
        // Parse as JSON to verify structure
        let parsed: serde_json::Value = serde_json::from_str(&discriminator).unwrap();
        
        assert_eq!(parsed["uptrend_score"], 0.82);
        assert_eq!(parsed["dca_overlap_pct"], 27.3);
        assert_eq!(parsed["buy_sell_ratio"], 0.68);
    }
}
