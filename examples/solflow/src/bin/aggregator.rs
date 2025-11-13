//! Aggregator Binary - Multi-Stream Correlation Engine
//!
//! Correlates PumpSwap and Jupiter DCA trade streams to detect accumulation patterns.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --release --bin aggregator
//! ```
//!
//! ## Environment Variables
//!
//! - PUMPSWAP_STREAM_PATH - Path to PumpSwap JSONL stream (default: streams/pumpswap/events.jsonl)
//! - JUPITER_DCA_STREAM_PATH - Path to Jupiter DCA JSONL stream (default: streams/jupiter_dca/events.jsonl)
//! - AGGREGATES_OUTPUT_PATH - Output directory for enriched metrics (default: streams/aggregates)
//! - SOLFLOW_DB_PATH - SQLite database path (default: data/solflow.db) - used when --backend sqlite
//! - CORRELATION_WINDOW_SECS - Time window for DCA correlation in seconds (default: 60)
//! - UPTREND_THRESHOLD - Uptrend score threshold (default: 0.7)
//! - ACCUMULATION_THRESHOLD - DCA overlap percentage threshold (default: 25.0)
//! - EMISSION_INTERVAL_SECS - How often to emit metrics (default: 60)
//! - RUST_LOG - Logging level (optional, default: info)

use solflow::aggregator_core::{
    AggregatorWriter, CorrelationEngine, EnrichedMetrics, SignalDetector, SignalScorer,
    TailReader, TimeWindowAggregator, Trade, TradeAction,
};
use solflow::streamer_core::config::BackendType;
use chrono::Utc;
use std::env;
use std::path::PathBuf;
use tokio::time::{interval, Duration};

fn parse_backend_from_args() -> BackendType {
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
    BackendType::Jsonl
}

#[derive(Debug)]
struct AggregatorConfig {
    backend: BackendType,
    pumpswap_path: PathBuf,
    jupiter_dca_path: PathBuf,
    output_path: PathBuf,
    correlation_window_secs: i64,
    uptrend_threshold: f64,
    accumulation_threshold: f64,
    emission_interval_secs: u64,
}

impl AggregatorConfig {
    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let backend = parse_backend_from_args();
        
        let output_path = match backend {
            BackendType::Sqlite => std::env::var("SOLFLOW_DB_PATH")
                .unwrap_or_else(|_| "data/solflow.db".to_string()),
            BackendType::Jsonl => std::env::var("AGGREGATES_OUTPUT_PATH")
                .unwrap_or_else(|_| "streams/aggregates".to_string()),
        };
        
        Ok(Self {
            backend,
            pumpswap_path: std::env::var("PUMPSWAP_STREAM_PATH")
                .unwrap_or_else(|_| "streams/pumpswap/events.jsonl".to_string())
                .into(),
            jupiter_dca_path: std::env::var("JUPITER_DCA_STREAM_PATH")
                .unwrap_or_else(|_| "streams/jupiter_dca/events.jsonl".to_string())
                .into(),
            output_path: output_path.into(),
            correlation_window_secs: std::env::var("CORRELATION_WINDOW_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            uptrend_threshold: std::env::var("UPTREND_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.7),
            accumulation_threshold: std::env::var("ACCUMULATION_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(25.0),
            emission_interval_secs: std::env::var("EMISSION_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    dotenv::dotenv().ok();

    let config = AggregatorConfig::from_env()?;

    log::info!("🚀 Starting Aggregator Enrichment System");
    log::info!("   PumpSwap stream: {}", config.pumpswap_path.display());
    log::info!("   Jupiter DCA stream: {}", config.jupiter_dca_path.display());
    log::info!("   Output: {}", config.output_path.display());
    log::info!("   Correlation window: {}s", config.correlation_window_secs);
    log::info!("   Uptrend threshold: {}", config.uptrend_threshold);
    log::info!(
        "   Accumulation threshold: {}%",
        config.accumulation_threshold
    );
    log::info!("   Emission interval: {}s", config.emission_interval_secs);

    // Initialize components
    let mut pumpswap_reader = TailReader::new(config.pumpswap_path.clone());
    let mut jupiter_dca_reader = TailReader::new(config.jupiter_dca_path.clone());
    let mut aggregator = TimeWindowAggregator::new();
    let correlator = CorrelationEngine::new(config.correlation_window_secs);
    let scorer = SignalScorer::new();
    let detector = SignalDetector::new(config.uptrend_threshold, config.accumulation_threshold);
    let mut writer = AggregatorWriter::new(config.backend, config.output_path.clone())?;
    
    log::info!("📊 Backend: {}", writer.backend_type());

    // Start readers
    log::info!("📖 Starting stream readers...");
    pumpswap_reader.start().await?;
    jupiter_dca_reader.start().await?;

    // Create emission ticker
    let mut emission_ticker = interval(Duration::from_secs(config.emission_interval_secs));
    emission_ticker.tick().await; // Skip first immediate tick

    log::info!("✅ Aggregator running - processing trades...");

    loop {
        tokio::select! {
            // Read from PumpSwap stream
            line_result = pumpswap_reader.read_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        if let Ok(trade) = Trade::from_jsonl(&line) {
                            aggregator.add_trade(trade);
                        } else {
                            log::warn!("Failed to parse PumpSwap trade: {}", line);
                        }
                    }
                    Ok(None) => {
                        // Should not happen with read_line implementation
                    }
                    Err(e) => {
                        log::error!("PumpSwap stream error: {}", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }

            // Read from Jupiter DCA stream
            line_result = jupiter_dca_reader.read_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        if let Ok(trade) = Trade::from_jsonl(&line) {
                            aggregator.add_trade(trade);
                        } else {
                            log::warn!("Failed to parse Jupiter DCA trade: {}", line);
                        }
                    }
                    Ok(None) => {
                        // Should not happen
                    }
                    Err(e) => {
                        log::error!("Jupiter DCA stream error: {}", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }

            // Emit enriched metrics
            _ = emission_ticker.tick() => {
                let current_timestamp = Utc::now().timestamp();
                log::info!("⏱️  Computing enriched metrics...");

                // Evict old trades first
                aggregator.evict_old_trades(current_timestamp);

                let mut metrics_count = 0;
                let mut signals_count = 0;

                for (mint, window, metrics) in aggregator.get_all_metrics() {
                    // Filter trades by program
                    let pumpswap_buys: Vec<Trade> = metrics
                        .trades
                        .iter()
                        .filter(|t| t.program_name == "PumpSwap" && t.action == TradeAction::Buy)
                        .cloned()
                        .collect();

                    let dca_buys: Vec<Trade> = metrics
                        .trades
                        .iter()
                        .filter(|t| t.program_name == "JupiterDCA" && t.action == TradeAction::Buy)
                        .cloned()
                        .collect();

                    // Compute correlation
                    let dca_overlap_pct = correlator.compute_dca_overlap(&pumpswap_buys, &dca_buys);

                    // Compute scores
                    let uptrend_score = scorer.compute_uptrend_score(metrics);

                    // Detect signals
                    let signal = detector.detect_signals(
                        uptrend_score,
                        dca_overlap_pct,
                        metrics.net_flow_sol,
                    );

                    if signal.is_some() {
                        signals_count += 1;
                    }

                    let total_volume = metrics.buy_volume_sol + metrics.sell_volume_sol;
                    let buy_sell_ratio = if total_volume > 0.0 {
                        metrics.buy_volume_sol / total_volume
                    } else {
                        0.0
                    };

                    let enriched = EnrichedMetrics {
                        mint: mint.clone(),
                        window: window.as_str().to_string(),
                        net_flow_sol: metrics.net_flow_sol,
                        buy_sell_ratio,
                        dca_overlap_pct,
                        uptrend_score,
                        signal: signal.clone(),
                        timestamp: current_timestamp,
                    };

                    if let Err(e) = writer.write_metrics(&enriched).await {
                        log::error!("Failed to write enriched metrics: {}", e);
                    }

                    metrics_count += 1;

                    // Log interesting signals
                    if let Some(sig) = signal {
                        log::info!(
                            "🎯 {} signal: {} (window: {}, uptrend: {:.2}, dca_overlap: {:.1}%)",
                            sig,
                            mint,
                            window.as_str(),
                            uptrend_score,
                            dca_overlap_pct
                        );
                    }
                }

                log::info!(
                    "✅ Emitted {} metrics ({} signals)",
                    metrics_count,
                    signals_count
                );
            }
        }
    }
}
