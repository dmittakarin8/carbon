//! Blocklist checking for GRPC ingestion layer
//!
//! This module provides early-stage filtering of blocked tokens at the
//! GRPC stream ingestion point, before any aggregation, metrics, or signals.
//!
//! Architecture:
//! - Reads from `mint_blocklist` table in SQLite database
//! - Checks are performed synchronously (fast, index-backed query)
//! - Blocked tokens are discarded immediately (no processing, no writes)
//!
//! Usage:
//! ```rust
//! let checker = BlocklistChecker::new("/var/lib/solflow/solflow.db")?;
//! 
//! if checker.is_blocked("mint_address")? {
//!     // Discard trade event
//!     return Ok(());
//! }
//! ```
//!
//! Hot reload:
//! - Each check queries the database directly (no caching)
//! - Updates to mint_blocklist are reflected immediately
//! - No restart required for blocklist changes

use rusqlite::{Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

/// Blocklist checker for GRPC ingestion filtering
///
/// Thread-safe SQLite connection wrapper for checking if mints are blocked.
/// Uses Arc<Mutex<Connection>> for concurrent access from multiple streamers.
#[derive(Debug)]
pub struct BlocklistChecker {
    conn: Arc<Mutex<Connection>>,
}

impl BlocklistChecker {
    /// Create a new blocklist checker
    ///
    /// Arguments:
    /// - `db_path`: Path to SQLite database containing mint_blocklist table
    ///
    /// Returns: BlocklistChecker instance or error if database cannot be opened
    pub fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;
        
        // Verify mint_blocklist table exists
        let table_exists: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='mint_blocklist'",
            [],
            |_| Ok(true),
        ).optional()?.unwrap_or(false);
        
        if !table_exists {
            return Err("mint_blocklist table not found in database".into());
        }
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Check if a mint is currently blocked
    ///
    /// Query logic (matches AGENTS.md specification):
    /// ```sql
    /// SELECT mint FROM mint_blocklist
    /// WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)
    /// ```
    ///
    /// Returns:
    /// - `Ok(true)` - Mint is blocked (discard trade)
    /// - `Ok(false)` - Mint is not blocked (process trade)
    /// - `Err(...)` - Database error
    pub fn is_blocked(&self, mint: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        let mut stmt = conn.prepare_cached(
            "SELECT mint FROM mint_blocklist 
             WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)"
        )?;

        let blocked = stmt.exists(rusqlite::params![mint, now])?;
        
        Ok(blocked)
    }
}

impl Clone for BlocklistChecker {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    /// Helper to create a test database with mint_blocklist table
    fn create_test_db() -> Result<(NamedTempFile, String), Box<dyn std::error::Error>> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap().to_string();

        let conn = Connection::open(&db_path)?;
        
        // Create mint_blocklist table (schema from /sql/01_mint_blocklist.sql)
        conn.execute(
            r#"
            CREATE TABLE mint_blocklist (
                mint            TEXT PRIMARY KEY,
                reason          TEXT,
                blocked_by      TEXT,
                created_at      INTEGER NOT NULL,
                expires_at      INTEGER
            )
            "#,
            [],
        )?;

        drop(conn);
        Ok((temp_file, db_path))
    }

    #[test]
    fn test_new_checker_success() {
        let (_temp, db_path) = create_test_db().unwrap();
        let checker = BlocklistChecker::new(&db_path);
        assert!(checker.is_ok());
    }

    #[test]
    fn test_new_checker_missing_table() {
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();
        
        // Create database without mint_blocklist table
        let conn = Connection::open(db_path).unwrap();
        conn.execute("CREATE TABLE dummy (id INTEGER)", []).unwrap();
        drop(conn);

        let checker = BlocklistChecker::new(db_path);
        assert!(checker.is_err());
        assert!(checker.unwrap_err().to_string().contains("mint_blocklist"));
    }

    #[test]
    fn test_is_blocked_not_in_list() {
        let (_temp, db_path) = create_test_db().unwrap();
        let checker = BlocklistChecker::new(&db_path).unwrap();

        let blocked = checker.is_blocked("mint_not_blocked").unwrap();
        assert!(!blocked);
    }

    #[test]
    fn test_is_blocked_permanent() {
        let (_temp, db_path) = create_test_db().unwrap();
        
        // Add permanently blocked mint (expires_at = NULL)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) 
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_permanent", "spam", "admin", 1700000000, rusqlite::types::Null],
            ).unwrap();
        }

        let checker = BlocklistChecker::new(&db_path).unwrap();
        let blocked = checker.is_blocked("mint_permanent").unwrap();
        assert!(blocked);
    }

    #[test]
    fn test_is_blocked_temporary_active() {
        let (_temp, db_path) = create_test_db().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Add temporarily blocked mint (expires in future)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) 
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_temp", "temporary ban", "admin", now - 1000, now + 10000],
            ).unwrap();
        }

        let checker = BlocklistChecker::new(&db_path).unwrap();
        let blocked = checker.is_blocked("mint_temp").unwrap();
        assert!(blocked);
    }

    #[test]
    fn test_is_blocked_temporary_expired() {
        let (_temp, db_path) = create_test_db().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Add expired block (expires_at in past)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) 
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_expired", "temporary ban", "admin", now - 2000, now - 100],
            ).unwrap();
        }

        let checker = BlocklistChecker::new(&db_path).unwrap();
        let blocked = checker.is_blocked("mint_expired").unwrap();
        assert!(!blocked); // Should not be blocked (expired)
    }

    #[test]
    fn test_checker_clone() {
        let (_temp, db_path) = create_test_db().unwrap();
        let checker1 = BlocklistChecker::new(&db_path).unwrap();
        let checker2 = checker1.clone();

        // Both checkers should work independently
        let result1 = checker1.is_blocked("test_mint").unwrap();
        let result2 = checker2.is_blocked("test_mint").unwrap();
        
        assert_eq!(result1, result2);
        assert!(!result1); // Mint not in list
    }

    #[test]
    fn test_hot_reload_simulation() {
        let (_temp, db_path) = create_test_db().unwrap();
        let checker = BlocklistChecker::new(&db_path).unwrap();

        // Initially not blocked
        assert!(!checker.is_blocked("mint_dynamic").unwrap());

        // Add to blocklist (simulating UI update)
        {
            let conn = Connection::open(&db_path).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            conn.execute(
                "INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) 
                 VALUES (?, ?, ?, ?, ?)",
                rusqlite::params!["mint_dynamic", "added during runtime", "web-ui", now, rusqlite::types::Null],
            ).unwrap();
        }

        // Now blocked (no restart needed)
        assert!(checker.is_blocked("mint_dynamic").unwrap());
    }
}
