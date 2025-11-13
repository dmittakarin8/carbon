//! SolFlow Signals Binary - Phase 3 Analytics Engine
//!
//! Computes windowed analytics from SQLite trades table and emits signals
//! based on the REAL DEMAND BREAKOUT scoring model.
//!
//! Runs every 10 seconds to:
//! 1. Query PumpSwap accumulation (SOL buy flow)
//! 2. Query Jupiter DCA events + volume
//! 3. Query Aggregator buy flow
//! 4. Query wallet diversity (unique buyers)
//! 5. Compute score per token
//! 6. Emit signals to signals table (with 30-min deduplication)
//! 7. Trim old trades (>24 hours)

use rusqlite::{params, Connection, Result as SqliteResult};
use solflow::sqlite_pragma::apply_optimized_pragmas;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

/// Database connection path
const DB_PATH: &str = "/var/lib/solflow/solflow.db";

/// Score threshold for signal emission
const SCORE_THRESHOLD: f64 = 10.0;

/// Deduplication window (30 minutes)
const DEDUPE_WINDOW_SECS: i64 = 1800;

/// Poll interval (10 seconds)
const POLL_INTERVAL_SECS: u64 = 10;

/// Trade retention window (24 hours)
const TRADE_RETENTION_SECS: i64 = 86400;

/// DCA statistics for a token
#[derive(Debug, Default, Clone)]
struct DcaStats {
    events: i64,
    volume: f64,
}

/// Aggregated token metrics
#[derive(Debug)]
struct TokenMetrics {
    mint: String,
    pumpswap_flow: f64,
    dca_stats: DcaStats,
    aggregator_flow: f64,
    wallet_diversity: i64,
}

impl TokenMetrics {
    fn compute_score(&self) -> f64 {
        // REAL DEMAND BREAKOUT scoring model
        (self.pumpswap_flow * 0.6)
            + (self.dca_stats.volume * 2.0)
            + (self.dca_stats.events as f64 * 1.0)
            + (self.aggregator_flow * 0.4)
            + (self.wallet_diversity as f64 * 0.2)
    }
}

/// Load PumpSwap buy flow (1 hour window)
fn load_pumpswap_flow(conn: &Connection) -> SqliteResult<HashMap<String, f64>> {
    let mut stmt = conn.prepare(
        "SELECT mint, SUM(sol_amount) AS flow
         FROM trades
         WHERE program_name = 'PumpSwap'
           AND action = 'BUY'
           AND timestamp >= strftime('%s', 'now') - 3600
         GROUP BY mint",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let mut result = HashMap::new();
    for row in rows {
        let (mint, flow) = row?;
        result.insert(mint, flow);
    }

    Ok(result)
}

/// Load Jupiter DCA events and volume (1 hour window)
fn load_dca_data(conn: &Connection) -> SqliteResult<HashMap<String, DcaStats>> {
    let mut stmt = conn.prepare(
        "SELECT mint, COUNT(*) AS events, SUM(sol_amount) AS volume
         FROM trades
         WHERE program_name = 'JupiterDCA'
           AND timestamp >= strftime('%s', 'now') - 3600
         GROUP BY mint",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            DcaStats {
                events: row.get::<_, i64>(1)?,
                volume: row.get::<_, f64>(2)?,
            },
        ))
    })?;

    let mut result = HashMap::new();
    for row in rows {
        let (mint, stats) = row?;
        result.insert(mint, stats);
    }

    Ok(result)
}

/// Load Aggregator buy flow (1 hour window)
fn load_aggregator_flow(conn: &Connection) -> SqliteResult<HashMap<String, f64>> {
    let mut stmt = conn.prepare(
        "SELECT mint, SUM(sol_amount) AS flow
         FROM trades
         WHERE program_name = 'Aggregator'
           AND action = 'BUY'
           AND timestamp >= strftime('%s', 'now') - 3600
         GROUP BY mint",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let mut result = HashMap::new();
    for row in rows {
        let (mint, flow) = row?;
        result.insert(mint, flow);
    }

    Ok(result)
}

/// Load wallet diversity (unique buyers in 1 hour window)
fn load_wallet_diversity(conn: &Connection) -> SqliteResult<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT mint, COUNT(DISTINCT user_account) AS diversity
         FROM trades
         WHERE action = 'BUY'
           AND timestamp >= strftime('%s', 'now') - 3600
           AND user_account IS NOT NULL
         GROUP BY mint",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut result = HashMap::new();
    for row in rows {
        let (mint, diversity) = row?;
        result.insert(mint, diversity);
    }

    Ok(result)
}

/// Check if a signal should be emitted (deduplication check)
fn should_emit_signal(conn: &Connection, mint: &str) -> SqliteResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) 
         FROM signals
         WHERE mint = ?1
           AND timestamp >= strftime('%s', 'now') - ?2",
        params![mint, DEDUPE_WINDOW_SECS],
        |row| row.get(0),
    )?;

    Ok(count == 0)
}

/// Insert signal into signals table
fn insert_signal(conn: &Connection, metrics: &TokenMetrics, score: f64) -> SqliteResult<()> {
    let reason = format!(
        "DEMAND_BREAKOUT: pumpswap={:.2} dca_events={} dca_vol={:.2} agg={:.2} wallets={}",
        metrics.pumpswap_flow,
        metrics.dca_stats.events,
        metrics.dca_stats.volume,
        metrics.aggregator_flow,
        metrics.wallet_diversity
    );

    conn.execute(
        "INSERT INTO signals 
         (mint, score, pumpswap_flow, dca_events, aggregator_flow, wallet_diversity, timestamp, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'), ?7)",
        params![
            metrics.mint,
            score,
            metrics.pumpswap_flow,
            metrics.dca_stats.events,
            metrics.aggregator_flow,
            metrics.wallet_diversity as f64,
            reason,
        ],
    )?;

    println!(
        "🚨 NEW SIGNAL: {} score={:.2} pumpswap={:.2} dca={} agg={:.2} wallets={}",
        metrics.mint,
        score,
        metrics.pumpswap_flow,
        metrics.dca_stats.events,
        metrics.aggregator_flow,
        metrics.wallet_diversity
    );

    Ok(())
}

/// Trim old trades (older than 24 hours)
fn trim_old_trades(conn: &Connection) -> SqliteResult<usize> {
    let deleted = conn.execute(
        "DELETE FROM trades WHERE timestamp < strftime('%s', 'now') - ?1",
        params![TRADE_RETENTION_SECS],
    )?;

    if deleted > 0 {
        log::info!("🧹 Trimmed {} old trades (>24h)", deleted);
    }

    Ok(deleted)
}

/// Merge all data sources into unified token metrics
fn merge_metrics(
    pumpswap: HashMap<String, f64>,
    dca: HashMap<String, DcaStats>,
    aggregator: HashMap<String, f64>,
    wallets: HashMap<String, i64>,
) -> Vec<TokenMetrics> {
    let mut all_mints = std::collections::HashSet::new();
    all_mints.extend(pumpswap.keys().cloned());
    all_mints.extend(dca.keys().cloned());
    all_mints.extend(aggregator.keys().cloned());
    all_mints.extend(wallets.keys().cloned());

    all_mints
        .into_iter()
        .map(|mint| TokenMetrics {
            mint: mint.clone(),
            pumpswap_flow: pumpswap.get(&mint).copied().unwrap_or(0.0),
            dca_stats: dca.get(&mint).cloned().unwrap_or_default(),
            aggregator_flow: aggregator.get(&mint).copied().unwrap_or(0.0),
            wallet_diversity: wallets.get(&mint).copied().unwrap_or(0),
        })
        .collect()
}

/// Main analytics loop
async fn run_analytics_loop(conn: &Connection) -> SqliteResult<()> {
    log::info!("📊 Loading analytics data...");

    // Load all data sources
    let pumpswap_flow = load_pumpswap_flow(conn)?;
    let dca_data = load_dca_data(conn)?;
    let aggregator_flow = load_aggregator_flow(conn)?;
    let wallet_diversity = load_wallet_diversity(conn)?;

    log::debug!(
        "📈 Data loaded: pumpswap={} dca={} agg={} wallets={}",
        pumpswap_flow.len(),
        dca_data.len(),
        aggregator_flow.len(),
        wallet_diversity.len()
    );

    // Merge into unified metrics
    let metrics = merge_metrics(pumpswap_flow, dca_data, aggregator_flow, wallet_diversity);

    log::debug!("🔍 Analyzing {} unique tokens", metrics.len());

    // Process each token
    let mut signals_emitted = 0;
    for token in metrics {
        let score = token.compute_score();

        if score >= SCORE_THRESHOLD {
            if should_emit_signal(conn, &token.mint)? {
                insert_signal(conn, &token, score)?;
                signals_emitted += 1;
            } else {
                log::debug!(
                    "⏭️  Skipped signal for {} (recent signal exists, score={:.2})",
                    token.mint,
                    score
                );
            }
        }
    }

    if signals_emitted > 0 {
        log::info!("✅ Emitted {} new signals", signals_emitted);
    }

    // Trim old trades
    trim_old_trades(conn)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("🚀 SolFlow Signals Engine starting...");
    log::info!("📂 Database: {}", DB_PATH);
    log::info!("⏱️  Poll interval: {}s", POLL_INTERVAL_SECS);
    log::info!("🎯 Score threshold: {}", SCORE_THRESHOLD);
    log::info!("🔒 Dedupe window: {}min", DEDUPE_WINDOW_SECS / 60);

    // Open database with optimized PRAGMAs
    let conn = Connection::open(DB_PATH)?;
    apply_optimized_pragmas(&conn)?;

    log::info!("✅ Database connection established (WAL mode)");

    // Verify tables exist
    let trades_count: i64 = conn.query_row("SELECT COUNT(*) FROM trades", [], |row| row.get(0))?;

    let signals_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM signals", [], |row| row.get(0))?;

    log::info!(
        "📊 Database state: {} trades, {} signals",
        trades_count,
        signals_count
    );

    // Main loop
    log::info!("🔄 Starting analytics loop (Ctrl+C to stop)...");

    loop {
        match run_analytics_loop(&conn).await {
            Ok(_) => log::debug!("✅ Analytics cycle completed"),
            Err(e) => log::error!("❌ Analytics error: {}", e),
        }

        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}
