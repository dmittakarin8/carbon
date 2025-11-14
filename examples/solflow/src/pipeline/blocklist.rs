//! Blocklist checking trait for signal filtering
//!
//! Phase 1: Trait definition only (no SQLite implementation)

use async_trait::async_trait;

/// Trait for checking if a mint is blocked
///
/// SQL reference: `/sql/01_mint_blocklist.sql`
///
/// Rule (from AGENTS.md):
/// Before writing signals, the system MUST check if a mint is blocked.
///
/// Query logic:
/// ```sql
/// SELECT mint FROM mint_blocklist
/// WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)
/// ```
///
/// If a row is returned, the mint is blocked and signals should NOT be written.
///
/// Expiration handling:
/// - `expires_at = NULL`: Permanently blocked
/// - `expires_at > now`: Temporarily blocked (not yet expired)
/// - `expires_at <= now`: Block has expired (mint is no longer blocked)
///
/// Phase 1: Trait definition only
/// Phase 2: Implement SqliteBlocklistProvider
#[async_trait]
pub trait BlocklistProvider {
    /// Returns true if mint is currently blocked
    ///
    /// # Arguments
    /// * `mint` - Token mint address to check
    /// * `now` - Current Unix timestamp for expiration check
    ///
    /// # Returns
    /// * `Ok(true)` - Mint is blocked (do not write signal)
    /// * `Ok(false)` - Mint is not blocked (safe to write signal)
    /// * `Err(...)` - Database error
    async fn is_blocked(
        &self,
        mint: &str,
        now: i64,
    ) -> Result<bool, Box<dyn std::error::Error>>;
}

// TODO: Phase 2 - Implement SqliteBlocklistProvider:
//
// pub struct SqliteBlocklistProvider {
//     pool: SqlitePool,
// }
//
// impl SqliteBlocklistProvider {
//     pub async fn new(pool: SqlitePool) -> Self { ... }
// }
//
// #[async_trait]
// impl BlocklistProvider for SqliteBlocklistProvider {
//     async fn is_blocked(&self, mint: &str, now: i64) -> Result<bool, ...> {
//         let result = sqlx::query_scalar!(
//             "SELECT mint FROM mint_blocklist 
//              WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)",
//             mint, now
//         )
//         .fetch_optional(&self.pool)
//         .await?;
//
//         Ok(result.is_some())
//     }
// }

// TODO: Phase 2 - Add caching layer:
// - Cache blocked mints in memory (HashMap<String, i64>)
// - Refresh cache periodically (every 60s)
// - Reduces database queries for frequently checked mints

// TODO: Phase 2 - Add admin operations:
// - fn add_to_blocklist(mint: &str, reason: &str, expires_at: Option<i64>) -> Result<...>
// - fn remove_from_blocklist(mint: &str) -> Result<...>
// - fn list_blocked_mints() -> Result<Vec<BlocklistEntry>, ...>
