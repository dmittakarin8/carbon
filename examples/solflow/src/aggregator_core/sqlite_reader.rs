//! SQLite-based trade reader with incremental cursor
//!
//! Replaced JSONL file-tailing with database queries for Aggregator input pipeline.
//! Uses ID-based cursor to incrementally read new trades from the unified trades table.

use super::normalizer::{Trade, TradeAction};
use crate::sqlite_pragma::apply_optimized_pragmas;
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
use rusqlite::params;

#[derive(Debug)]
pub enum ReaderError {
    Database(rusqlite::Error),
    InvalidAction(String),
}

impl From<rusqlite::Error> for ReaderError {
    fn from(err: rusqlite::Error) -> Self {
        ReaderError::Database(err)
    }
}

impl std::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReaderError::Database(e) => write!(f, "Database error: {}", e),
            ReaderError::InvalidAction(a) => write!(f, "Invalid action: {}", a),
        }
    }
}

impl std::error::Error for ReaderError {}

/// SQLite trade reader with incremental cursor
pub struct SqliteTradeReader {
    conn: Connection,
    last_read_id: i64,
    poll_interval: Duration,
}

impl SqliteTradeReader {
    /// Create a new SQLite trade reader
    ///
    /// Initializes cursor from MAX(id) to start reading from current position
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, ReaderError> {
        let conn = Connection::open(db_path)?;
        
        // Apply optimized PRAGMAs (WAL, NORMAL, MEMORY, mmap, cache, autocheckpoint)
        apply_optimized_pragmas(&conn)
            .map_err(ReaderError::Database)?;
        
        // Enable read-only mode to prevent write locks (must be after PRAGMAs)
        conn.execute("PRAGMA query_only = ON", [])?;
        
        // Initialize cursor from highest existing id
        let last_id: i64 = conn.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM trades 
             WHERE program_name IN ('PumpSwap', 'JupiterDCA')",
            [],
            |row| row.get(0)
        )?;
        
        log::info!("📥 SQLite reader initialized: starting from cursor id={}", last_id);
        
        Ok(Self {
            conn,
            last_read_id: last_id,
            poll_interval: Duration::from_millis(500),
        })
    }
    
    /// Create reader with custom poll interval
    pub fn with_poll_interval(db_path: impl AsRef<Path>, poll_interval: Duration) -> Result<Self, ReaderError> {
        let mut reader = Self::new(db_path)?;
        reader.poll_interval = poll_interval;
        Ok(reader)
    }
    
    /// Read new trades since last cursor position
    ///
    /// Returns up to 1000 trades per call, ordered by id ASC.
    /// Filters for PumpSwap and JupiterDCA only (excludes Aggregator rows).
    pub fn read_new_trades(&mut self) -> Result<Vec<Trade>, ReaderError> {
        let mut stmt = self.conn.prepare(
            "SELECT timestamp, signature, program_name, action, mint,
                    sol_amount, token_amount, token_decimals, user_account, id
             FROM trades
             WHERE id > ?1 
               AND program_name IN ('PumpSwap', 'JupiterDCA')
             ORDER BY id ASC
             LIMIT 1000"
        )?;
        
        let trade_iter = stmt.query_map([self.last_read_id], |row| {
            let action_str: String = row.get(3)?;
            let action = match action_str.as_str() {
                "BUY" => TradeAction::Buy,
                "SELL" => TradeAction::Sell,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            
            Ok((
                Trade {
                    timestamp: row.get(0)?,
                    signature: row.get(1)?,
                    program_name: row.get(2)?,
                    action,
                    mint: row.get(4)?,
                    sol_amount: row.get(5)?,
                    token_amount: row.get(6)?,
                    token_decimals: row.get(7)?,
                    user_account: row.get(8)?,
                },
                row.get::<_, i64>(9)?, // id column
            ))
        })?;
        
        let mut trades = Vec::new();
        let mut max_id = self.last_read_id;
        
        for result in trade_iter {
            let (trade, id) = result?;
            trades.push(trade);
            max_id = max_id.max(id);
        }
        
        // Update cursor to highest processed id
        if max_id > self.last_read_id {
            self.last_read_id = max_id;
            log::debug!("📥 Read {} new trades, cursor updated to id={}", trades.len(), max_id);
        }
        
        Ok(trades)
    }
    
    /// Get current cursor position
    pub fn cursor_position(&self) -> i64 {
        self.last_read_id
    }
    
    /// Get poll interval
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;
    
    fn setup_test_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let conn = Connection::open(&db_path).unwrap();
        
        // Create schema
        conn.execute(
            "CREATE TABLE trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                program TEXT NOT NULL,
                program_name TEXT NOT NULL,
                mint TEXT NOT NULL,
                signature TEXT UNIQUE NOT NULL,
                action TEXT NOT NULL,
                sol_amount REAL NOT NULL,
                token_amount REAL NOT NULL,
                token_decimals INTEGER NOT NULL,
                user_account TEXT,
                discriminator TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        ).unwrap();
        
        (dir, db_path)
    }
    
    fn insert_trade(conn: &Connection, id: Option<i64>, program_name: &str, action: &str, mint: &str, signature: &str) {
        if let Some(id) = id {
            conn.execute(
                "INSERT INTO trades (id, program, program_name, mint, signature, action, 
                 sol_amount, token_amount, token_decimals, user_account, discriminator, timestamp)
                 VALUES (?1, 'program_id', ?2, ?3, ?4, ?5, 1.0, 1000.0, 6, 'user1', 'disc', 1000)",
                params![id, program_name, mint, signature, action],
            ).unwrap();
        } else {
            conn.execute(
                "INSERT INTO trades (program, program_name, mint, signature, action, 
                 sol_amount, token_amount, token_decimals, user_account, discriminator, timestamp)
                 VALUES ('program_id', ?1, ?2, ?3, ?4, 1.0, 1000.0, 6, 'user1', 'disc', 1000)",
                params![program_name, mint, signature, action],
            ).unwrap();
        }
    }
    
    #[test]
    fn test_read_new_trades_incremental() {
        let (_dir, db_path) = setup_test_db();
        let conn = Connection::open(&db_path).unwrap();
        
        // Insert 5 initial trades
        for i in 1..=5 {
            insert_trade(&conn, Some(i), "PumpSwap", "BUY", "mint1", &format!("sig{}", i));
        }
        drop(conn);
        
        // Create reader (should start at id=5)
        let mut reader = SqliteTradeReader::new(&db_path).unwrap();
        assert_eq!(reader.cursor_position(), 5);
        
        // No new trades yet
        let trades = reader.read_new_trades().unwrap();
        assert_eq!(trades.len(), 0);
        assert_eq!(reader.cursor_position(), 5);
        
        // Insert 2 new trades
        let conn = Connection::open(&db_path).unwrap();
        insert_trade(&conn, Some(6), "JupiterDCA", "SELL", "mint2", "sig6");
        insert_trade(&conn, Some(7), "PumpSwap", "BUY", "mint3", "sig7");
        drop(conn);
        
        // Read new trades
        let trades = reader.read_new_trades().unwrap();
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].signature, "sig6");
        assert_eq!(trades[1].signature, "sig7");
        assert_eq!(reader.cursor_position(), 7);
    }
    
    #[test]
    fn test_filters_aggregator_rows() {
        let (_dir, db_path) = setup_test_db();
        let conn = Connection::open(&db_path).unwrap();
        
        // Insert mixed trades
        insert_trade(&conn, Some(1), "PumpSwap", "BUY", "mint1", "sig1");
        insert_trade(&conn, Some(2), "Aggregator", "UPTREND", "mint1", "sig2"); // Should be filtered
        insert_trade(&conn, Some(3), "JupiterDCA", "BUY", "mint2", "sig3");
        insert_trade(&conn, Some(4), "PumpSwap", "SELL", "mint1", "sig4");
        drop(conn);
        
        let mut reader = SqliteTradeReader::new(&db_path).unwrap();
        // Cursor should be at id=4 (highest PumpSwap/JupiterDCA row)
        assert_eq!(reader.cursor_position(), 4);
        
        // Read from beginning
        reader.last_read_id = 0;
        let trades = reader.read_new_trades().unwrap();
        
        // Should return 3 trades (exclude Aggregator)
        assert_eq!(trades.len(), 3);
        assert_eq!(trades[0].program_name, "PumpSwap");
        assert_eq!(trades[1].program_name, "JupiterDCA");
        assert_eq!(trades[2].program_name, "PumpSwap");
    }
    
    #[test]
    fn test_batch_limit() {
        let (_dir, db_path) = setup_test_db();
        let conn = Connection::open(&db_path).unwrap();
        
        // Insert 1500 trades
        for i in 1..=1500 {
            insert_trade(&conn, Some(i), "PumpSwap", "BUY", "mint1", &format!("sig{}", i));
        }
        drop(conn);
        
        let mut reader = SqliteTradeReader::new(&db_path).unwrap();
        reader.last_read_id = 0; // Read from beginning
        
        // First call should return 1000 (LIMIT)
        let trades = reader.read_new_trades().unwrap();
        assert_eq!(trades.len(), 1000);
        assert_eq!(reader.cursor_position(), 1000);
        
        // Second call should return remaining 500
        let trades = reader.read_new_trades().unwrap();
        assert_eq!(trades.len(), 500);
        assert_eq!(reader.cursor_position(), 1500);
        
        // Third call should return 0
        let trades = reader.read_new_trades().unwrap();
        assert_eq!(trades.len(), 0);
    }
    
    #[test]
    fn test_read_only_mode() {
        let (_dir, db_path) = setup_test_db();
        let conn = Connection::open(&db_path).unwrap();
        insert_trade(&conn, Some(1), "PumpSwap", "BUY", "mint1", "sig1");
        drop(conn);
        
        let reader = SqliteTradeReader::new(&db_path).unwrap();
        
        // Attempt to write should fail
        let result = reader.conn.execute(
            "INSERT INTO trades (program, program_name, mint, signature, action, 
             sol_amount, token_amount, token_decimals, user_account, discriminator, timestamp)
             VALUES ('p', 'PN', 'm', 's', 'BUY', 1.0, 1.0, 6, 'u', 'd', 1000)",
            [],
        );
        
        assert!(result.is_err());
    }
}
