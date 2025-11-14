//! Database writer trait for aggregate-only architecture
//!
//! Phase 3-C: SQLite implementation with rusqlite
//! Phase 4: Schema migration loader added

// TODO: Phase 4 - Add connection pooling for concurrent writes

use super::signals::TokenSignal;
use super::types::AggregatedTokenState;
use async_trait::async_trait;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Trait for writing aggregates and signals to SQLite
///
/// Tables written (see `/sql/` directory):
/// - `token_aggregates` - UPSERT on mint (rolling-window metrics)
/// - `token_signals` - INSERT (append-only signal events)
///
/// Important: Signal writes MUST check `mint_blocklist` first (see AGENTS.md)
#[async_trait]
pub trait AggregateDbWriter: Send + Sync {
    /// Write aggregate metrics to token_aggregates table
    ///
    /// SQL reference: `/sql/02_token_aggregates.sql`
    ///
    /// Operation: UPSERT (INSERT ... ON CONFLICT(mint) DO UPDATE)
    /// - If mint exists: update all fields
    /// - If mint doesn't exist: insert new row
    async fn write_aggregates(
        &self,
        aggregates: Vec<AggregatedTokenState>,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Write signal event to token_signals table
    ///
    /// SQL reference: `/sql/03_token_signals.sql`
    ///
    /// Operation: INSERT (append-only)
    ///
    /// CRITICAL: Must check mint_blocklist BEFORE writing:
    /// ```sql
    /// SELECT mint FROM mint_blocklist
    /// WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)
    /// ```
    /// If row exists, DO NOT write signal.
    async fn write_signal(
        &self,
        signal: TokenSignal,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// Run schema migrations from SQL files
///
/// Phase 4: Idempotent schema loader
///
/// Reads all .sql files from the specified directory and executes them.
/// All SQL files must use "IF NOT EXISTS" clauses for idempotency.
///
/// Arguments:
/// - `conn`: SQLite connection (mutable reference)
/// - `schema_dir`: Path to directory containing .sql files
///
/// Returns: Ok(()) if all migrations succeed, Err(...) on first failure
///
/// Example:
/// ```
/// let mut conn = Connection::open("solflow.db")?;
/// run_schema_migrations(&mut conn, "sql")?;
/// ```
pub fn run_schema_migrations(
    conn: &mut Connection,
    schema_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema_path = Path::new(schema_dir);
    
    if !schema_path.exists() {
        return Err(format!("Schema directory not found: {}", schema_dir).into());
    }

    // Enable WAL mode for better concurrency (Phase 4 requirement)
    // Note: PRAGMA journal_mode returns results, so we use execute instead of query
    conn.pragma_update(None, "journal_mode", "WAL")?;
    log::info!("📊 Enabled WAL mode for SQLite database");

    // Read all .sql files and sort alphabetically (ensures proper ordering: 00_, 01_, 02_, etc.)
    let mut sql_files: Vec<_> = fs::read_dir(schema_path)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().and_then(|s| s.to_str()) == Some("sql")
        })
        .collect();
    
    sql_files.sort_by_key(|entry| entry.file_name());

    log::info!("🔧 Running schema migrations from: {}", schema_dir);
    
    for entry in sql_files {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        
        log::info!("   ├─ Executing: {}", filename);
        
        let sql_content = fs::read_to_string(&path)?;
        
        // Execute the SQL file (expects IF NOT EXISTS clauses)
        conn.execute_batch(&sql_content)?;
        
        log::info!("   └─ ✅ Success: {}", filename);
    }

    log::info!("✅ All schema migrations completed successfully");
    
    Ok(())
}

/// SQLite implementation of AggregateDbWriter
///
/// Phase 3-C: Basic implementation without pooling or WAL mode
/// Phase 4: Will add connection pooling and WAL mode
pub struct SqliteAggregateWriter {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAggregateWriter {
    /// Create a new SQLite writer
    ///
    /// Arguments:
    /// - `db_path`: Path to SQLite database file (must already exist with schema)
    ///
    /// Note: Does NOT create database or schema. Caller must ensure database
    /// exists and has schema from `/sql/*.sql` files.
    ///
    /// TODO: Phase 4 - Enable WAL mode: PRAGMA journal_mode=WAL
    pub fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;
        
        // TODO: Phase 4 - Enable WAL mode for better concurrency:
        // conn.execute("PRAGMA journal_mode=WAL", [])?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Check if a mint is in the blocklist
    ///
    /// Returns: true if mint is blocked, false if allowed
    fn check_blocklist(
        conn: &Connection,
        mint: &str,
        now: i64,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut stmt = conn.prepare(
            "SELECT mint FROM mint_blocklist 
             WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)",
        )?;

        let blocked = stmt.exists([mint, &now.to_string()])?;
        Ok(blocked)
    }
}

#[async_trait]
impl AggregateDbWriter for SqliteAggregateWriter {
    /// Write aggregate metrics to token_aggregates table
    ///
    /// Performs UPSERT for each aggregate:
    /// - If mint exists: updates all fields (preserves created_at, updates updated_at)
    /// - If mint doesn't exist: inserts new row
    ///
    /// TODO: Phase 4 - Implement batch transaction for multiple aggregates
    async fn write_aggregates(
        &self,
        aggregates: Vec<AggregatedTokenState>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();

        // TODO: Phase 4 - Use single transaction for all aggregates
        // Currently: separate operation per aggregate (simpler, less performant)

        for agg in aggregates {
            conn.execute(
                r#"
                INSERT INTO token_aggregates (
                    mint, source_program, last_trade_timestamp,
                    net_flow_60s_sol, net_flow_300s_sol, net_flow_900s_sol,
                    buy_count_60s, sell_count_60s,
                    buy_count_300s, sell_count_300s,
                    buy_count_900s, sell_count_900s,
                    unique_wallets_300s, bot_trades_300s, bot_wallets_300s,
                    avg_trade_size_300s_sol, volume_300s_sol,
                    price_usd, price_sol, market_cap_usd,
                    updated_at, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(mint) DO UPDATE SET
                    source_program = excluded.source_program,
                    last_trade_timestamp = excluded.last_trade_timestamp,
                    net_flow_60s_sol = excluded.net_flow_60s_sol,
                    net_flow_300s_sol = excluded.net_flow_300s_sol,
                    net_flow_900s_sol = excluded.net_flow_900s_sol,
                    buy_count_60s = excluded.buy_count_60s,
                    sell_count_60s = excluded.sell_count_60s,
                    buy_count_300s = excluded.buy_count_300s,
                    sell_count_300s = excluded.sell_count_300s,
                    buy_count_900s = excluded.buy_count_900s,
                    sell_count_900s = excluded.sell_count_900s,
                    unique_wallets_300s = excluded.unique_wallets_300s,
                    bot_trades_300s = excluded.bot_trades_300s,
                    bot_wallets_300s = excluded.bot_wallets_300s,
                    avg_trade_size_300s_sol = excluded.avg_trade_size_300s_sol,
                    volume_300s_sol = excluded.volume_300s_sol,
                    price_usd = excluded.price_usd,
                    price_sol = excluded.price_sol,
                    market_cap_usd = excluded.market_cap_usd,
                    updated_at = excluded.updated_at
                "#,
                rusqlite::params![
                    agg.mint,
                    agg.source_program,
                    agg.last_trade_timestamp,
                    agg.net_flow_60s_sol,
                    agg.net_flow_300s_sol,
                    agg.net_flow_900s_sol,
                    agg.buy_count_60s,
                    agg.sell_count_60s,
                    agg.buy_count_300s,
                    agg.sell_count_300s,
                    agg.buy_count_900s,
                    agg.sell_count_900s,
                    agg.unique_wallets_300s,
                    agg.bot_trades_300s,
                    agg.bot_wallets_300s,
                    agg.avg_trade_size_300s_sol,
                    agg.volume_300s_sol,
                    agg.price_usd,
                    agg.price_sol,
                    agg.market_cap_usd,
                    agg.updated_at,
                    agg.created_at,
                ],
            )?;
        }

        Ok(())
    }

    /// Write signal event to token_signals table
    ///
    /// Checks mint_blocklist first, then inserts signal if allowed.
    ///
    /// TODO: Phase 4 - Add write scheduling/buffering to reduce I/O
    async fn write_signal(
        &self,
        signal: TokenSignal,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();

        // Validate JSON if present
        if let Some(ref json) = signal.details_json {
            validate_json(json)?;
        }

        // Check blocklist
        let blocked = Self::check_blocklist(&conn, &signal.mint, signal.created_at)?;
        if blocked {
            return Err(format!("Mint {} is blocked, signal not written", signal.mint).into());
        }

        // Insert signal
        conn.execute(
            r#"
            INSERT INTO token_signals (
                mint, signal_type, window_seconds, severity, score, details_json, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![
                signal.mint,
                signal.signal_type.as_str(),
                signal.window_seconds,
                signal.severity,
                signal.score,
                signal.details_json,
                signal.created_at,
            ],
        )?;

        Ok(())
    }
}

/// Validate JSON string
///
/// Ensures JSON is well-formed before storing in database.
/// Returns error if JSON is malformed.
fn validate_json(json: &str) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::from_str::<serde_json::Value>(json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::signals::SignalType;
    use crate::pipeline::types::AggregatedTokenState;
    use tempfile::NamedTempFile;

    /// Helper to create a test database with schema
    fn create_test_db() -> Result<(NamedTempFile, SqliteAggregateWriter), Box<dyn std::error::Error>>
    {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        // Create database and schema
        let conn = Connection::open(db_path)?;

        // Schema from /sql/01_mint_blocklist.sql
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS mint_blocklist (
                mint            TEXT PRIMARY KEY,
                reason          TEXT,
                blocked_by      TEXT,
                created_at      INTEGER NOT NULL,
                expires_at      INTEGER
            )
            "#,
            [],
        )?;

        // Schema from /sql/02_token_aggregates.sql
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS token_aggregates (
                mint                    TEXT PRIMARY KEY,
                source_program          TEXT NOT NULL,
                last_trade_timestamp    INTEGER,
                price_usd               REAL,
                price_sol               REAL,
                market_cap_usd          REAL,
                net_flow_60s_sol        REAL,
                net_flow_300s_sol       REAL,
                net_flow_900s_sol       REAL,
                buy_count_60s           INTEGER,
                sell_count_60s          INTEGER,
                buy_count_300s          INTEGER,
                sell_count_300s         INTEGER,
                buy_count_900s          INTEGER,
                sell_count_900s         INTEGER,
                unique_wallets_300s     INTEGER,
                bot_trades_300s         INTEGER,
                bot_wallets_300s        INTEGER,
                avg_trade_size_300s_sol REAL,
                volume_300s_sol         REAL,
                updated_at              INTEGER NOT NULL,
                created_at              INTEGER NOT NULL
            )
            "#,
            [],
        )?;

        // Schema from /sql/03_token_signals.sql
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS token_signals (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                mint            TEXT NOT NULL,
                signal_type     TEXT NOT NULL,
                window_seconds  INTEGER NOT NULL,
                severity        INTEGER NOT NULL DEFAULT 1,
                score           REAL,
                details_json    TEXT,
                created_at      INTEGER NOT NULL,
                sent_to_discord INTEGER NOT NULL DEFAULT 0,
                seen_in_terminal INTEGER NOT NULL DEFAULT 0
            )
            "#,
            [],
        )?;

        drop(conn); // Close connection before creating writer

        let writer = SqliteAggregateWriter::new(db_path)?;
        Ok((temp_file, writer))
    }

    /// Helper to create a minimal AggregatedTokenState for testing
    fn make_aggregate(mint: &str, net_flow_300s: f64, updated_at: i64) -> AggregatedTokenState {
        AggregatedTokenState {
            mint: mint.to_string(),
            source_program: "test_program".to_string(),
            last_trade_timestamp: Some(updated_at - 100),
            price_usd: None,
            price_sol: None,
            market_cap_usd: None,
            net_flow_60s_sol: Some(1.0),
            net_flow_300s_sol: Some(net_flow_300s),
            net_flow_900s_sol: Some(10.0),
            buy_count_60s: Some(5),
            sell_count_60s: Some(2),
            buy_count_300s: Some(20),
            sell_count_300s: Some(10),
            buy_count_900s: Some(50),
            sell_count_900s: Some(30),
            unique_wallets_300s: Some(10),
            bot_trades_300s: Some(3),
            bot_wallets_300s: Some(2),
            avg_trade_size_300s_sol: Some(0.5),
            volume_300s_sol: Some(15.0),
            updated_at,
            created_at: updated_at - 1000,
        }
    }

    #[tokio::test]
    async fn test_upsert_new_aggregate() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        let agg = make_aggregate("mint_new", 5.0, now);

        // Write new aggregate
        writer.write_aggregates(vec![agg.clone()]).await.unwrap();

        // Verify it was inserted
        let conn = writer.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT mint, net_flow_300s_sol, created_at FROM token_aggregates WHERE mint = ?")
            .unwrap();

        let result: (String, f64, i64) = stmt
            .query_row(["mint_new"], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap();

        assert_eq!(result.0, "mint_new");
        assert_eq!(result.1, 5.0);
        assert_eq!(result.2, agg.created_at);
    }

    #[tokio::test]
    async fn test_upsert_existing_aggregate() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        // Insert initial aggregate
        let agg1 = make_aggregate("mint_existing", 5.0, now);
        writer.write_aggregates(vec![agg1.clone()]).await.unwrap();

        // Update with new values (same mint, different net_flow, later updated_at)
        let agg2 = make_aggregate("mint_existing", 10.0, now + 100);
        writer.write_aggregates(vec![agg2.clone()]).await.unwrap();

        // Verify updated values
        let conn = writer.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT mint, net_flow_300s_sol, updated_at, created_at FROM token_aggregates WHERE mint = ?",
            )
            .unwrap();

        let result: (String, f64, i64, i64) = stmt
            .query_row(["mint_existing"], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap();

        assert_eq!(result.0, "mint_existing");
        assert_eq!(result.1, 10.0); // Updated net_flow
        assert_eq!(result.2, now + 100); // Updated updated_at
        assert_eq!(result.3, agg1.created_at); // created_at preserved from first insert
    }

    #[tokio::test]
    async fn test_insert_signal_allowed() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        let signal = TokenSignal::new("mint_allowed".to_string(), SignalType::Breakout, 60, now)
            .with_severity(3)
            .with_score(0.85)
            .with_details(r#"{"net_flow_60s":10.5,"unique_wallets":8}"#.to_string());

        // Write signal (mint not in blocklist)
        writer.write_signal(signal.clone()).await.unwrap();

        // Verify it was inserted
        let conn = writer.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT mint, signal_type, severity, score, details_json FROM token_signals WHERE mint = ?",
            )
            .unwrap();

        let result: (String, String, i32, f64, String) = stmt
            .query_row(["mint_allowed"], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })
            .unwrap();

        assert_eq!(result.0, "mint_allowed");
        assert_eq!(result.1, "BREAKOUT");
        assert_eq!(result.2, 3);
        assert_eq!(result.3, 0.85);
        assert_eq!(result.4, r#"{"net_flow_60s":10.5,"unique_wallets":8}"#);
    }

    #[tokio::test]
    async fn test_insert_signal_blocked() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        // Add mint to blocklist
        {
            let conn = writer.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_blocked", "spam", "admin", now - 1000, now + 10000],
            )
            .unwrap();
        }

        let signal =
            TokenSignal::new("mint_blocked".to_string(), SignalType::Surge, 300, now)
                .with_severity(4);

        // Attempt to write signal (should fail due to blocklist)
        let result = writer.write_signal(signal).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("mint_blocked is blocked"));

        // Verify signal was NOT inserted
        let conn = writer.conn.lock().unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM token_signals WHERE mint = ?",
                ["mint_blocked"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_batch_aggregates() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        // Create multiple aggregates
        let aggregates = vec![
            make_aggregate("mint_batch_1", 5.0, now),
            make_aggregate("mint_batch_2", 10.0, now),
            make_aggregate("mint_batch_3", 15.0, now),
        ];

        // Write all at once
        writer.write_aggregates(aggregates).await.unwrap();

        // Verify all were inserted
        let conn = writer.conn.lock().unwrap();
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM token_aggregates", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(count, 3);

        // Verify specific values
        let mint2_flow: f64 = conn
            .query_row(
                "SELECT net_flow_300s_sol FROM token_aggregates WHERE mint = ?",
                ["mint_batch_2"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(mint2_flow, 10.0);
    }

    #[tokio::test]
    async fn test_json_details_storage() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        let json_details =
            r#"{"net_flow_60s":25.5,"volume_ratio":4.2,"buy_count":15,"extra":{"nested":"value"}}"#;

        let signal = TokenSignal::new("mint_json".to_string(), SignalType::Surge, 60, now)
            .with_severity(5)
            .with_score(0.95)
            .with_details(json_details.to_string());

        // Write signal with JSON details
        writer.write_signal(signal).await.unwrap();

        // Verify JSON was stored correctly
        let conn = writer.conn.lock().unwrap();
        let stored_json: String = conn
            .query_row(
                "SELECT details_json FROM token_signals WHERE mint = ?",
                ["mint_json"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(stored_json, json_details);

        // Verify JSON can be parsed back
        let parsed: serde_json::Value = serde_json::from_str(&stored_json).unwrap();
        assert_eq!(parsed["net_flow_60s"], 25.5);
        assert_eq!(parsed["extra"]["nested"], "value");
    }

    #[tokio::test]
    async fn test_null_optional_fields() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        // Create aggregate with NULL optional fields (price_usd, price_sol, market_cap_usd)
        let mut agg = make_aggregate("mint_nulls", 5.0, now);
        agg.price_usd = None;
        agg.price_sol = None;
        agg.market_cap_usd = None;

        writer.write_aggregates(vec![agg]).await.unwrap();

        // Verify NULLs were stored correctly
        let conn = writer.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT price_usd, price_sol, market_cap_usd FROM token_aggregates WHERE mint = ?")
            .unwrap();

        let result: (Option<f64>, Option<f64>, Option<f64>) = stmt
            .query_row(["mint_nulls"], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap();

        assert!(result.0.is_none());
        assert!(result.1.is_none());
        assert!(result.2.is_none());
    }

    #[tokio::test]
    async fn test_invalid_json_rejected() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        let invalid_json = r#"{"incomplete": "#; // Malformed JSON

        let signal = TokenSignal::new("mint_invalid_json".to_string(), SignalType::Focused, 300, now)
            .with_details(invalid_json.to_string());

        // Attempt to write signal with invalid JSON (should fail)
        let result = writer.write_signal(signal).await;
        assert!(result.is_err());

        // Verify signal was NOT inserted
        let conn = writer.conn.lock().unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM token_signals WHERE mint = ?",
                ["mint_invalid_json"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_blocklist_expiration() {
        let (_temp, writer) = create_test_db().unwrap();
        let now = 1700000000;

        // Add mint to blocklist with expiration in the past
        {
            let conn = writer.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_expired", "temporary ban", "admin", now - 2000, now - 100],
            )
            .unwrap();
        }

        let signal =
            TokenSignal::new("mint_expired".to_string(), SignalType::Breakout, 60, now)
                .with_severity(2);

        // Signal should be allowed (blocklist expired)
        writer.write_signal(signal).await.unwrap();

        // Verify signal was inserted
        let conn = writer.conn.lock().unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM token_signals WHERE mint = ?",
                ["mint_expired"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);
    }
}
