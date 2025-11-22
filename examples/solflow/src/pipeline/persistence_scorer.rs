//! Persistence Scoring Engine (Phase 2)
//!
//! Computes persistence scores, pattern tags, and confidence levels for tokens
//! based on multi-window presence, wallet growth, net flow strength, and bot activity.
//!
//! ## Scoring Algorithm
//!
//! **persistence_score** (0-10 scale):
//! - Multi-window presence (30%): Token appears in multiple rolling windows
//! - Wallet growth (25%): Unique wallet count increasing over time
//! - Net flow strength (25%): Consistent buy pressure across windows
//! - Behavioral consistency (10%): Repeat micro-signal confirmations
//! - Bot penalty (10%): Penalize excessive bot activity
//!
//! **pattern_tag**:
//! - ACCUMULATION: High DCA overlap + positive net flow
//! - MOMENTUM: Strong uptrend score + increasing velocity
//! - DISTRIBUTION: Negative net flow + high sell pressure
//! - WASHOUT: Declining across all metrics
//! - NOISE: Inconsistent or low-quality signals
//!
//! **confidence** (LOW/MEDIUM/HIGH):
//! - Based on data richness, consistency, token lifetime, and bot interference

use rusqlite::{Connection, Result as SqliteResult};
use std::collections::HashMap;

/// Token metrics snapshot from database
#[derive(Debug, Clone)]
pub struct TokenSnapshot {
    pub mint: String,
    pub net_flow_60s: f64,
    pub net_flow_300s: f64,
    pub net_flow_900s: f64,
    pub net_flow_3600s: f64,
    pub net_flow_7200s: f64,
    pub net_flow_14400s: f64,
    pub unique_wallets_300s: i64,
    pub bot_trades_300s: i64,
    pub buy_count_300s: i64,
    pub sell_count_300s: i64,
    pub dca_buys_3600s: i64,
    pub volume_300s_sol: f64,
    pub updated_at: i64,
    pub created_at: i64,
}

/// Signal summary for appearance tracking
#[derive(Debug, Clone)]
pub struct SignalHistory {
    pub mint: String,
    pub signal_count_24h: i64,
    pub signal_count_72h: i64,
    pub last_signal_type: Option<String>,
}

/// Computed persistence summary
#[derive(Debug, Clone)]
pub struct PersistenceSummary {
    pub token_address: String,
    pub persistence_score: i32,
    pub pattern_tag: String,
    pub confidence: String,
    pub appearance_24h: i32,
    pub appearance_72h: i32,
}

/// Persistence scoring engine
pub struct PersistenceScorer {
    db_path: String,
}

impl PersistenceScorer {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }

    /// Fetch active tokens from database (matches dashboard query for consistency)
    fn fetch_active_tokens(&self, conn: &Connection) -> SqliteResult<Vec<TokenSnapshot>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT
                ta.mint,
                ta.net_flow_60s_sol,
                ta.net_flow_300s_sol,
                ta.net_flow_900s_sol,
                ta.net_flow_3600s_sol,
                ta.net_flow_7200s_sol,
                ta.net_flow_14400s_sol,
                ta.unique_wallets_300s,
                ta.bot_trades_300s,
                ta.buy_count_300s,
                ta.sell_count_300s,
                ta.dca_buys_3600s,
                ta.volume_300s_sol,
                ta.updated_at,
                ta.created_at
            FROM token_aggregates ta
            LEFT JOIN token_metadata tm ON ta.mint = tm.mint
            WHERE ta.dca_buys_3600s > 0
              AND (tm.blocked IS NULL OR tm.blocked = 0)
            ORDER BY ta.net_flow_300s_sol DESC
            LIMIT 100
            "#,
        )?;

        let tokens = stmt
            .query_map([], |row| {
                Ok(TokenSnapshot {
                    mint: row.get(0)?,
                    net_flow_60s: row.get(1).unwrap_or(0.0),
                    net_flow_300s: row.get(2).unwrap_or(0.0),
                    net_flow_900s: row.get(3).unwrap_or(0.0),
                    net_flow_3600s: row.get(4).unwrap_or(0.0),
                    net_flow_7200s: row.get(5).unwrap_or(0.0),
                    net_flow_14400s: row.get(6).unwrap_or(0.0),
                    unique_wallets_300s: row.get(7).unwrap_or(0),
                    bot_trades_300s: row.get(8).unwrap_or(0),
                    buy_count_300s: row.get(9).unwrap_or(0),
                    sell_count_300s: row.get(10).unwrap_or(0),
                    dca_buys_3600s: row.get(11).unwrap_or(0),
                    volume_300s_sol: row.get(12).unwrap_or(0.0),
                    updated_at: row.get(13).unwrap_or(0),
                    created_at: row.get(14).unwrap_or(0),
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(tokens)
    }

    /// Fetch signal history for appearance tracking
    fn fetch_signal_history(&self, conn: &Connection) -> SqliteResult<HashMap<String, SignalHistory>> {
        let now = conn.query_row("SELECT unixepoch()", [], |row| row.get::<_, i64>(0))?;
        let cutoff_24h = now - 86400;
        let cutoff_72h = now - 259200;

        let mut stmt = conn.prepare(
            r#"
            SELECT
                mint,
                signal_type,
                created_at
            FROM token_signals
            WHERE created_at > ?
            ORDER BY mint, created_at DESC
            "#,
        )?;

        let mut history: HashMap<String, SignalHistory> = HashMap::new();

        let rows = stmt.query_map([cutoff_72h], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        for row in rows {
            let (mint, signal_type, created_at) = row?;

            let entry = history.entry(mint.clone()).or_insert_with(|| SignalHistory {
                mint: mint.clone(),
                signal_count_24h: 0,
                signal_count_72h: 0,
                last_signal_type: None,
            });

            entry.signal_count_72h += 1;
            if created_at >= cutoff_24h {
                entry.signal_count_24h += 1;
            }

            if entry.last_signal_type.is_none() {
                entry.last_signal_type = Some(signal_type);
            }
        }

        Ok(history)
    }

    /// Compute persistence score (0-10)
    fn compute_persistence_score(&self, token: &TokenSnapshot, lifetime_hours: f64, bot_ratio: f64) -> i32 {
        let mut score = 0.0;

        // 1. Multi-window presence (30 points): Token appears in multiple windows
        let window_presence = [
            token.net_flow_60s,
            token.net_flow_300s,
            token.net_flow_900s,
            token.net_flow_3600s,
            token.net_flow_7200s,
            token.net_flow_14400s,
        ]
        .iter()
        .filter(|&&v| v.abs() > 0.01)
        .count() as f64
            / 6.0;
        score += window_presence * 30.0;

        // 2. Wallet growth (25 points): Unique wallet count
        let wallet_score = (token.unique_wallets_300s as f64 / 50.0).min(1.0);
        score += wallet_score * 25.0;

        // 3. Net flow strength (25 points): Consistent buy pressure
        let avg_net_flow = (token.net_flow_300s + token.net_flow_900s + token.net_flow_3600s) / 3.0;
        let flow_score = if avg_net_flow > 0.0 {
            (avg_net_flow / 10.0).min(1.0)
        } else {
            0.0
        };
        score += flow_score * 25.0;

        // 4. Behavioral consistency (10 points): Lifetime normalization
        let lifetime_factor = if lifetime_hours > 0.0 {
            (lifetime_hours / 24.0).min(1.0)
        } else {
            0.0
        };
        score += lifetime_factor * 10.0;

        // 5. Bot penalty (10 points): Penalize excessive bot activity
        let bot_penalty = bot_ratio * 10.0;
        score -= bot_penalty;

        // Normalize to 0-10 scale
        (score / 10.0).clamp(0.0, 10.0).round() as i32
    }

    /// Classify pattern tag
    fn classify_pattern(&self, token: &TokenSnapshot, dca_overlap: bool) -> String {
        let total_trades = token.buy_count_300s + token.sell_count_300s;
        let buy_ratio = if total_trades > 0 {
            token.buy_count_300s as f64 / total_trades as f64
        } else {
            0.5
        };

        let avg_net_flow = (token.net_flow_300s + token.net_flow_900s + token.net_flow_3600s) / 3.0;

        if dca_overlap && avg_net_flow > 0.0 && buy_ratio > 0.6 {
            "ACCUMULATION".to_string()
        } else if avg_net_flow > 5.0 && buy_ratio > 0.7 {
            "MOMENTUM".to_string()
        } else if avg_net_flow < -2.0 && buy_ratio < 0.4 {
            "DISTRIBUTION".to_string()
        } else if avg_net_flow < -5.0 {
            "WASHOUT".to_string()
        } else {
            "NOISE".to_string()
        }
    }

    /// Compute confidence level
    fn compute_confidence(&self, token: &TokenSnapshot, lifetime_hours: f64, bot_ratio: f64) -> String {
        let total_trades = token.buy_count_300s + token.sell_count_300s;
        let data_richness = total_trades as f64 / 50.0;
        let lifetime_factor = (lifetime_hours / 24.0).min(1.0);

        let confidence_score =
            data_richness * 0.4 + lifetime_factor * 0.3 + (1.0 - bot_ratio) * 0.3;

        if confidence_score > 0.7 {
            "HIGH".to_string()
        } else if confidence_score > 0.4 {
            "MEDIUM".to_string()
        } else {
            "LOW".to_string()
        }
    }

    /// Run scoring engine and write results to database
    pub fn run_scoring_cycle(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let conn = Connection::open(&self.db_path)?;

        // Fetch data
        let tokens = self.fetch_active_tokens(&conn)?;
        let signal_history = self.fetch_signal_history(&conn)?;

        log::info!("📊 Scoring {} active tokens", tokens.len());

        let now = conn.query_row("SELECT unixepoch()", [], |row| row.get::<_, i64>(0))?;

        let mut summaries = Vec::new();

        for token in &tokens {
            // Calculate lifetime in hours
            let lifetime_seconds = now - token.created_at;
            let lifetime_hours = lifetime_seconds as f64 / 3600.0;

            // Calculate bot ratio
            let total_trades = token.buy_count_300s + token.sell_count_300s;
            let bot_ratio = if total_trades > 0 {
                token.bot_trades_300s as f64 / total_trades as f64
            } else {
                0.0
            };

            // Check DCA overlap (use DCA buys as proxy)
            let dca_overlap = token.dca_buys_3600s > 3;

            // Compute metrics
            let persistence_score = self.compute_persistence_score(token, lifetime_hours, bot_ratio);
            let pattern_tag = self.classify_pattern(token, dca_overlap);
            let confidence = self.compute_confidence(token, lifetime_hours, bot_ratio);

            // Get appearance counts
            let history = signal_history.get(&token.mint);
            let appearance_24h = history.map(|h| h.signal_count_24h).unwrap_or(0) as i32;
            let appearance_72h = history.map(|h| h.signal_count_72h).unwrap_or(0) as i32;

            summaries.push(PersistenceSummary {
                token_address: token.mint.clone(),
                persistence_score,
                pattern_tag,
                confidence,
                appearance_24h,
                appearance_72h,
            });
        }

        // Write to database
        let count = self.write_summaries(&conn, &summaries)?;

        log::info!("✅ Wrote {} persistence summaries to database", count);

        Ok(count)
    }

    /// Write persistence summaries to database
    fn write_summaries(
        &self,
        conn: &Connection,
        summaries: &[PersistenceSummary],
    ) -> SqliteResult<usize> {
        let now = conn.query_row("SELECT unixepoch()", [], |row| row.get::<_, i64>(0))?;

        let mut stmt = conn.prepare(
            r#"
            INSERT INTO token_signal_summary (
                token_address,
                persistence_score,
                pattern_tag,
                confidence,
                appearance_24h,
                appearance_72h,
                updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(token_address) DO UPDATE SET
                persistence_score = excluded.persistence_score,
                pattern_tag = excluded.pattern_tag,
                confidence = excluded.confidence,
                appearance_24h = excluded.appearance_24h,
                appearance_72h = excluded.appearance_72h,
                updated_at = excluded.updated_at
            "#,
        )?;

        let mut count = 0;
        for summary in summaries {
            stmt.execute(rusqlite::params![
                summary.token_address,
                summary.persistence_score,
                summary.pattern_tag,
                summary.confidence,
                summary.appearance_24h,
                summary.appearance_72h,
                now,
            ])?;
            count += 1;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistence_score_calculation() {
        let scorer = PersistenceScorer::new(":memory:".to_string());

        let token = TokenSnapshot {
            mint: "test_mint".to_string(),
            net_flow_60s: 1.0,
            net_flow_300s: 2.0,
            net_flow_900s: 3.0,
            net_flow_3600s: 4.0,
            net_flow_7200s: 5.0,
            net_flow_14400s: 6.0,
            unique_wallets_300s: 25,
            bot_trades_300s: 5,
            buy_count_300s: 20,
            sell_count_300s: 10,
            dca_buys_3600s: 5,
            volume_300s_sol: 10.0,
            updated_at: 1000000,
            created_at: 999000,
        };

        let lifetime_hours = 1000.0 / 3600.0;
        let bot_ratio = 5.0 / 30.0;

        let score = scorer.compute_persistence_score(&token, lifetime_hours, bot_ratio);

        assert!(score >= 0 && score <= 10, "Score should be 0-10");
    }

    #[test]
    fn test_pattern_classification() {
        let scorer = PersistenceScorer::new(":memory:".to_string());

        let accumulation_token = TokenSnapshot {
            mint: "test".to_string(),
            net_flow_60s: 1.0,
            net_flow_300s: 2.0,
            net_flow_900s: 3.0,
            net_flow_3600s: 4.0,
            net_flow_7200s: 0.0,
            net_flow_14400s: 0.0,
            unique_wallets_300s: 10,
            bot_trades_300s: 2,
            buy_count_300s: 18,
            sell_count_300s: 10,
            dca_buys_3600s: 5,
            volume_300s_sol: 5.0,
            updated_at: 1000,
            created_at: 900,
        };

        let pattern = scorer.classify_pattern(&accumulation_token, true);
        assert_eq!(pattern, "ACCUMULATION");
    }

    #[test]
    fn test_confidence_calculation() {
        let scorer = PersistenceScorer::new(":memory:".to_string());

        let high_confidence_token = TokenSnapshot {
            mint: "test".to_string(),
            net_flow_60s: 0.0,
            net_flow_300s: 0.0,
            net_flow_900s: 0.0,
            net_flow_3600s: 0.0,
            net_flow_7200s: 0.0,
            net_flow_14400s: 0.0,
            unique_wallets_300s: 50,
            bot_trades_300s: 5,
            buy_count_300s: 50,
            sell_count_300s: 50,
            dca_buys_3600s: 0,
            volume_300s_sol: 10.0,
            updated_at: 1000000,
            created_at: 900000,
        };

        let lifetime_hours = 100000.0 / 3600.0;
        let bot_ratio = 5.0 / 100.0;

        let confidence = scorer.compute_confidence(&high_confidence_token, lifetime_hours, bot_ratio);
        assert_eq!(confidence, "HIGH");
    }
}
