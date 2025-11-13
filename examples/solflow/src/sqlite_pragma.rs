//! Optimized SQLite PRAGMA configuration
//!
//! Applies performance-optimized PRAGMAs to all SQLite connections.
//! These settings improve write performance, reduce lock contention, and optimize memory usage.

use rusqlite::{Connection, Result};

/// Apply all optimized PRAGMAs to a SQLite connection
///
/// Must be called immediately after Connection::open()
///
/// # PRAGMAs Applied
/// - `journal_mode = WAL`: Write-Ahead Logging for concurrent reads/writes
/// - `synchronous = NORMAL`: Balanced durability/performance (fsync at checkpoints)
/// - `temp_store = MEMORY`: Store temporary tables in RAM (faster)
/// - `mmap_size = 30000000000`: 30GB memory-mapped I/O for fast page access
/// - `cache_size = -20000`: 20MB page cache (negative = KB)
/// - `wal_autocheckpoint = 1000`: Auto-checkpoint every 1000 pages (~4MB)
///
/// # Example
/// ```no_run
/// use rusqlite::Connection;
/// use solflow::sqlite_pragma::apply_optimized_pragmas;
///
/// let conn = Connection::open("database.db")?;
/// apply_optimized_pragmas(&conn)?;
/// # Ok::<(), rusqlite::Error>(())
/// ```
pub fn apply_optimized_pragmas(conn: &Connection) -> Result<()> {
    // WAL mode for concurrent read/write
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    
    // NORMAL synchronous for balanced safety/performance
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    
    // Store temp tables in memory (faster)
    conn.execute_batch("PRAGMA temp_store = MEMORY;")?;
    
    // Memory-mapped I/O (30GB virtual address space)
    conn.execute_batch("PRAGMA mmap_size = 30000000000;")?;
    
    // Cache size: 20MB (negative = KB, positive = pages)
    conn.execute_batch("PRAGMA cache_size = -20000;")?;
    
    // Auto-checkpoint every 1000 pages (~4MB)
    conn.execute_batch("PRAGMA wal_autocheckpoint = 1000;")?;
    
    log::debug!("✅ SQLite PRAGMAs applied: WAL, NORMAL, MEMORY, mmap=30GB, cache=20MB, checkpoint=1000");
    
    Ok(())
}

/// Manually trigger WAL checkpoint with TRUNCATE mode
///
/// Shrinks WAL file to prevent unbounded growth.
/// Call this during maintenance windows or low-traffic periods.
///
/// # When to Use
/// - During scheduled maintenance
/// - Before database backups
/// - When WAL file grows too large (>100MB)
///
/// # Warning
/// This is an expensive operation. Never call automatically in hot paths.
///
/// # Example
/// ```no_run
/// use rusqlite::Connection;
/// use solflow::sqlite_pragma::checkpoint_truncate;
///
/// let conn = Connection::open("database.db")?;
/// checkpoint_truncate(&conn)?;
/// # Ok::<(), rusqlite::Error>(())
/// ```
pub fn checkpoint_truncate(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    log::info!("✅ WAL checkpoint TRUNCATE executed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pragmas_applied_correctly() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        
        apply_optimized_pragmas(&conn).unwrap();
        
        // Verify journal_mode
        let journal_mode: String = conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        
        // Verify synchronous
        let synchronous: i32 = conn.query_row(
            "PRAGMA synchronous",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(synchronous, 1); // NORMAL = 1
        
        // Verify temp_store
        let temp_store: i32 = conn.query_row(
            "PRAGMA temp_store",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(temp_store, 2); // MEMORY = 2
        
        // Verify mmap_size (may be clamped by system limits)
        let mmap_size: i64 = conn.query_row(
            "PRAGMA mmap_size",
            [],
            |row| row.get(0),
        ).unwrap();
        // SQLite may clamp to system limits, so verify it's > 0 (enabled)
        assert!(mmap_size > 0, "mmap_size should be enabled (got {})", mmap_size);
        
        // Verify cache_size
        let cache_size: i32 = conn.query_row(
            "PRAGMA cache_size",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(cache_size, -20000);
        
        // Verify wal_autocheckpoint
        let checkpoint: i32 = conn.query_row(
            "PRAGMA wal_autocheckpoint",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(checkpoint, 1000);
    }
    
    #[test]
    fn test_checkpoint_truncate() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        
        apply_optimized_pragmas(&conn).unwrap();
        
        // Create a table and insert some data
        conn.execute("CREATE TABLE test (id INTEGER)", []).unwrap();
        conn.execute("INSERT INTO test VALUES (1)", []).unwrap();
        
        // Checkpoint should succeed
        checkpoint_truncate(&conn).unwrap();
        
        // Verify data still exists
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM test",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }
}
