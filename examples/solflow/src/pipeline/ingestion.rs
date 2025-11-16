//! Pipeline ingestion - async channel processor for trade events
//!
//! Phase 4: Live trade ingestion from streamers
//! Phase 4.3: Unified flush loop with single lock acquisition

use super::db::AggregateDbWriter;
use super::engine::PipelineEngine;
use super::types::TradeEvent;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

/// Start pipeline ingestion from trade event channel
///
/// This is the ONLY flush mechanism in the entire pipeline.
///
/// Main loop:
/// 1. Receives trades from streamers via mpsc channel
/// 2. Processes each trade through PipelineEngine
/// 3. Periodically flushes aggregates and signals to database (single lock acquisition)
///
/// Flush cycle optimization:
/// - Lock engine ONCE per flush (not once per mint)
/// - Compute all metrics while holding lock
/// - Release lock BEFORE database writes
/// - Log channel utilization for monitoring
///
/// Arguments:
/// - `rx`: Receiver end of trade event channel
/// - `engine`: Shared PipelineEngine instance (Arc<Mutex<>>)
/// - `db_writer`: Database writer for persisting aggregates and signals
/// - `flush_interval_ms`: How often to flush aggregates (milliseconds)
///
/// This function runs indefinitely until the channel is closed (streamer shutdown).
pub async fn start_pipeline_ingestion(
    mut rx: mpsc::Receiver<TradeEvent>,
    engine: Arc<Mutex<PipelineEngine>>,
    db_writer: Arc<dyn AggregateDbWriter + Send + Sync>,
    flush_interval_ms: u64,
) {
    log::info!("🚀 Starting pipeline ingestion (UNIFIED FLUSH LOOP)");
    log::info!("   ├─ Flush interval: {}ms", flush_interval_ms);
    log::info!("   └─ Waiting for trades...");

    let mut flush_timer = interval(Duration::from_millis(flush_interval_ms));
    let mut trade_count = 0u64;
    let mut last_log_time = std::time::Instant::now();
    let channel_capacity = 10000; // Match STREAMER_CHANNEL_BUFFER default

    loop {
        tokio::select! {
            // Receive trade from channel
            Some(trade) = rx.recv() => {
                // Process trade through engine (single lock acquisition)
                {
                    let mut engine_guard = engine.lock().unwrap();
                    engine_guard.process_trade(trade);
                }
                
                trade_count += 1;
                
                // Log throughput every 10 seconds
                if last_log_time.elapsed().as_secs() >= 10 {
                    let trades_per_sec = trade_count as f64 / last_log_time.elapsed().as_secs_f64();
                    log::info!("📊 Ingestion rate: {:.1} trades/sec (total: {})", trades_per_sec, trade_count);
                    last_log_time = std::time::Instant::now();
                    trade_count = 0;
                }
            }
            
            // Periodic flush timer - ONLY FLUSH MECHANISM
            _ = flush_timer.tick() => {
                let now = chrono::Utc::now().timestamp();
                let flush_start = std::time::Instant::now();
                
                // 1. Lock engine ONCE and compute all metrics
                let (aggregates, all_signals, active_mint_count) = {
                    let mut engine_guard = engine.lock().unwrap();
                    let active_mints = engine_guard.get_active_mints();
                    
                    if active_mints.is_empty() {
                        // No active tokens, skip flush
                        (Vec::new(), Vec::new(), 0)
                    } else {
                        let mut aggregates = Vec::new();
                        let mut all_signals = Vec::new();
                        
                        // Compute metrics for all mints while holding lock
                        for mint in &active_mints {
                            match engine_guard.compute_metrics(mint, now) {
                                Ok((metrics, signals, aggregate)) => {
                                    aggregates.push(aggregate);
                                    all_signals.extend(signals);
                                    
                                    // Update bot history for BOT_DROPOFF detection
                                    engine_guard.update_bot_history(mint, metrics.bot_trades_count_300s);
                                }
                                Err(e) => {
                                    log::warn!("⚠️  Failed to compute metrics for {}: {}", mint, e);
                                }
                            }
                        }
                        
                        (aggregates, all_signals, active_mints.len())
                    }
                }; // Lock released here
                
                // 2. Database writes (engine unlocked - no blocking)
                if !aggregates.is_empty() {
                    match db_writer.write_aggregates(aggregates.clone()).await {
                        Ok(_) => {
                            log::debug!("✅ Wrote {} aggregates to database", aggregates.len());
                        }
                        Err(e) => {
                            log::error!("❌ Failed to write aggregates: {}", e);
                        }
                    }
                }
                
                // Write signals to database
                let mut signals_written = 0;
                for signal in all_signals {
                    match db_writer.write_signal(signal.clone()).await {
                        Ok(_) => signals_written += 1,
                        Err(e) => {
                            // May fail due to blocklist - this is expected
                            log::debug!("⚠️  Signal not written (mint: {}, type: {:?}): {}", 
                                signal.mint, signal.signal_type, e);
                        }
                    }
                }
                
                if signals_written > 0 {
                    log::info!("🚨 Detected {} signals", signals_written);
                }
                
                // 3. Log channel health and flush performance
                let channel_usage = rx.len();
                let flush_duration = flush_start.elapsed();
                
                log::info!("📊 Flush complete: {} mints, {} signals | channel: {}/{} | {}ms", 
                    active_mint_count, 
                    signals_written,
                    channel_usage, 
                    channel_capacity,
                    flush_duration.as_millis());
                
                // Warn if channel is filling up (> 50% capacity)
                if channel_usage > channel_capacity / 2 {
                    log::warn!("⚠️  Channel usage high: {}/{} ({}%)", 
                        channel_usage, channel_capacity, 
                        (channel_usage * 100) / channel_capacity);
                }
            }
            
            // Channel closed (streamer shutdown)
            else => {
                log::warn!("⚠️  Trade channel closed, stopping ingestion");
                
                // Final flush before exit
                log::info!("🔄 Performing final flush...");
                let now = chrono::Utc::now().timestamp();
                
                let (aggregates, all_signals, _) = {
                    let mut engine_guard = engine.lock().unwrap();
                    let active_mints = engine_guard.get_active_mints();
                    
                    let mut aggregates = Vec::new();
                    let mut all_signals = Vec::new();
                    
                    for mint in &active_mints {
                        if let Ok((metrics, signals, aggregate)) = engine_guard.compute_metrics(mint, now) {
                            aggregates.push(aggregate);
                            all_signals.extend(signals);
                            engine_guard.update_bot_history(mint, metrics.bot_trades_count_300s);
                        }
                    }
                    
                    (aggregates, all_signals, active_mints.len())
                };
                
                if !aggregates.is_empty() {
                    if let Err(e) = db_writer.write_aggregates(aggregates).await {
                        log::error!("❌ Failed final aggregate flush: {}", e);
                    }
                }
                
                for signal in all_signals {
                    let _ = db_writer.write_signal(signal).await;
                }
                
                log::info!("✅ Final flush complete");
                break;
            }
        }
    }

    log::info!("✅ Pipeline ingestion stopped");
}

// Flush logic is now integrated directly into the tokio::select! loop above.
// This eliminates the need for a separate function and allows better control
// over lock acquisition timing.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::db::SqliteAggregateWriter;
    use crate::pipeline::types::TradeDirection;
    use tempfile::NamedTempFile;
    use rusqlite::Connection;
    
    /// Helper to create test trade event
    fn make_test_trade(timestamp: i64, mint: &str, sol_amount: f64) -> TradeEvent {
        TradeEvent {
            timestamp,
            mint: mint.to_string(),
            direction: TradeDirection::Buy,
            sol_amount,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: "test_wallet".to_string(),
            source_program: "pumpswap".to_string(),
        }
    }
    
    /// Helper to create test database
    fn create_test_db() -> (NamedTempFile, Arc<SqliteAggregateWriter>) {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();
        
        // Initialize schema
        let mut conn = Connection::open(db_path).unwrap();
        crate::pipeline::db::run_schema_migrations(&mut conn, "sql").unwrap();
        drop(conn);
        
        let writer = Arc::new(SqliteAggregateWriter::new(db_path).unwrap());
        (temp_file, writer)
    }
    
    #[tokio::test]
    async fn test_ingestion_processes_trades() {
        // Test: Trades flow through channel into PipelineEngine
        let (tx, rx) = mpsc::channel(100);
        let engine = Arc::new(Mutex::new(PipelineEngine::new()));
        let (_temp, db_writer) = create_test_db();
        
        // Spawn ingestion task
        let engine_clone = engine.clone();
        let ingestion_handle = tokio::spawn(async move {
            start_pipeline_ingestion(rx, engine_clone, db_writer, 1000).await;
        });
        
        // Send test trades
        let mint = "test_mint_123";
        for i in 0..10 {
            let trade = make_test_trade(1000 + i, mint, 1.0);
            tx.send(trade).await.unwrap();
        }
        
        // Give ingestion time to process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify trades were processed
        let engine_guard = engine.lock().unwrap();
        let active_mints = engine_guard.get_active_mints();
        assert!(active_mints.contains(&mint.to_string()));
        
        // Cleanup
        drop(tx); // Close channel
        let _ = tokio::time::timeout(Duration::from_secs(1), ingestion_handle).await;
    }
    
    #[tokio::test]
    async fn test_flush_writes_aggregates() {
        // Test: Periodic flush writes aggregates to database
        let engine = Arc::new(Mutex::new(PipelineEngine::new()));
        let (_temp, db_writer_concrete) = create_test_db();
        
        // Cast to trait object
        let db_writer: Arc<dyn AggregateDbWriter + Send + Sync> = db_writer_concrete;
        
        let mint = "flush_test_mint";
        let now = 1000;
        
        // Add trades to engine
        {
            let mut engine_guard = engine.lock().unwrap();
            for i in 0..5 {
                let trade = make_test_trade(now + i, mint, 2.0);
                engine_guard.process_trade(trade);
            }
        }
        
        // Manually trigger flush (inline logic - no separate function needed)
        let (aggregates, signals, _) = {
            let mut engine_guard = engine.lock().unwrap();
            let active_mints = engine_guard.get_active_mints();
            
            let mut aggregates = Vec::new();
            let mut signals = Vec::new();
            
            for mint in &active_mints {
                if let Ok((metrics, sigs, agg)) = engine_guard.compute_metrics(mint, now) {
                    aggregates.push(agg);
                    signals.extend(sigs);
                    engine_guard.update_bot_history(mint, metrics.bot_trades_count_300s);
                }
            }
            
            (aggregates, signals, active_mints.len())
        };
        
        // Write aggregates
        if !aggregates.is_empty() {
            db_writer.write_aggregates(aggregates).await.unwrap();
        }
        
        // Write signals
        for signal in signals {
            let _ = db_writer.write_signal(signal).await;
        }
        
        // Verify no errors (actual database verification would require exposing connection)
    }
}
