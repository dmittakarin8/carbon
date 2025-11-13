use rusqlite::{Connection, params};
use crate::streamer_core::{
    output_writer::TradeEvent,
    writer_backend::{WriterBackend, WriterError}
};
use crate::sqlite_pragma::apply_optimized_pragmas;
use async_trait::async_trait;
use std::path::Path;
use std::time::Instant;

pub struct SqliteWriter {
    conn: Connection,
    batch: Vec<TradeEvent>,
    batch_size: usize,
    last_flush: Instant,
    flush_interval_secs: u64,
}

impl SqliteWriter {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, WriterError> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.as_ref().parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                WriterError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create database directory {}: {}", parent.display(), e),
                ))
            })?;
        }
        
        let conn = Connection::open(db_path)?;
        
        // Apply optimized PRAGMAs (WAL, NORMAL, MEMORY, mmap, cache, autocheckpoint)
        apply_optimized_pragmas(&conn)
            .map_err(|e| WriterError::Database(e.to_string()))?;
        
        // Create table with optimized schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trades (
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
        )?;
        
        // Create indexes for common queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mint_timestamp ON trades(mint, timestamp DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_timestamp ON trades(timestamp DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_program ON trades(program, timestamp DESC)",
            [],
        )?;
        
        log::info!("✅ SQLite database initialized with WAL mode");
        
        Ok(Self {
            conn,
            batch: Vec::with_capacity(100),
            batch_size: 100,
            last_flush: Instant::now(),
            flush_interval_secs: 2,
        })
    }
    
    fn flush_batch(&mut self) -> Result<(), WriterError> {
        if self.batch.is_empty() {
            return Ok(());
        }
        
        let tx = self.conn.transaction()?;
        
        for event in &self.batch {
            tx.execute(
                "INSERT OR IGNORE INTO trades 
                 (program, program_name, mint, signature, action, sol_amount, 
                  token_amount, token_decimals, user_account, discriminator, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    event.program_id,
                    event.program_name,
                    event.mint,
                    event.signature,
                    event.action,
                    event.sol_amount,
                    event.token_amount,
                    event.token_decimals,
                    event.user_account,
                    event.discriminator,
                    event.timestamp,
                ],
            )?;
        }
        
        tx.commit()?;
        
        log::debug!("✅ Flushed {} trades to SQLite", self.batch.len());
        self.batch.clear();
        self.last_flush = Instant::now();
        
        Ok(())
    }
}

#[async_trait]
impl WriterBackend for SqliteWriter {
    async fn write(&mut self, event: &TradeEvent) -> Result<(), WriterError> {
        self.batch.push(event.clone());
        
        // Auto-flush if batch full or time elapsed
        if self.batch.len() >= self.batch_size 
           || self.last_flush.elapsed().as_secs() >= self.flush_interval_secs {
            self.flush_batch()?;
        }
        
        Ok(())
    }
    
    async fn flush(&mut self) -> Result<(), WriterError> {
        self.flush_batch()
    }
    
    fn backend_type(&self) -> &'static str {
        "SQLite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_event(signature: &str) -> TradeEvent {
        TradeEvent {
            timestamp: 1700000000,
            signature: signature.to_string(),
            program_id: "test_program".to_string(),
            program_name: "TestDEX".to_string(),
            action: "BUY".to_string(),
            mint: "test_mint".to_string(),
            sol_amount: 1.5,
            token_amount: 1000.0,
            token_decimals: 6,
            user_account: Some("user1".to_string()),
            discriminator: "0123456789abcdef".to_string(),
        }
    }
    
    #[tokio::test]
    async fn test_sqlite_basic_write() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteWriter::new(&db_path).unwrap();
        
        let event = create_test_event("test_sig_1");
        
        writer.write(&event).await.unwrap();
        writer.flush().await.unwrap();
        
        // Verify insert
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE signature = ?1",
            params![event.signature],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 1);
    }
    
    #[tokio::test]
    async fn test_duplicate_prevention() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteWriter::new(&db_path).unwrap();
        
        let event = create_test_event("dup_sig");
        
        writer.write(&event).await.unwrap();
        writer.write(&event).await.unwrap(); // Duplicate
        writer.flush().await.unwrap();
        
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trades",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 1); // Only one inserted
    }
    
    #[tokio::test]
    async fn test_batch_flush() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut writer = SqliteWriter::new(&db_path).unwrap();
        
        // Write 150 trades (should trigger 1 auto-flush at 100)
        for i in 0..150 {
            let event = create_test_event(&format!("sig_{}", i));
            writer.write(&event).await.unwrap();
        }
        
        writer.flush().await.unwrap();
        
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trades",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(count, 150);
    }
    
    #[tokio::test]
    async fn test_wal_checkpoint_configured() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let _writer = SqliteWriter::new(&db_path).unwrap();
        
        let conn = Connection::open(&db_path).unwrap();
        
        // Verify WAL mode enabled
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        
        // Verify autocheckpoint set to 1000 pages
        let checkpoint: i32 = conn.query_row(
            "PRAGMA wal_autocheckpoint",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(checkpoint, 1000);
    }
}
