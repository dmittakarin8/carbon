import { getDb, getWriteDb } from './db';
import { TokenMetrics, SparklineDataPoint } from './types';

export function getTokens(limit: number = 100): TokenMetrics[] {
  const db = getDb();
  
  // Use CTE pattern matching the working Grafana query
  // DCA data comes from token_signals with signal_type = 'DCA_CONVICTION'
  // Time-windowed to last 1 hour (3600 seconds) to match 1h net flow window
  const query = `
    WITH dca AS (
      SELECT 
        mint,
        COUNT(*) AS dca_count,
        MAX(created_at) AS last_dca_ts,
        SUM(CAST(json_extract(details_json, '$.net_flow_sol') AS REAL)) AS dca_net_flow
      FROM token_signals
      WHERE signal_type = 'DCA_CONVICTION'
        AND created_at > unixepoch() - 3600
      GROUP BY mint
    )
    SELECT 
      ta.mint,
      SUM(ta.net_flow_60s_sol) as net_flow_60s,
      SUM(ta.net_flow_300s_sol) as net_flow_300s,
      SUM(ta.net_flow_900s_sol) as net_flow_900s,
      SUM(ta.net_flow_3600s_sol) as net_flow_3600s,
      SUM(ta.net_flow_7200s_sol) as net_flow_7200s,
      SUM(ta.net_flow_14400s_sol) as net_flow_14400s,
      SUM(ta.buy_count_300s) as total_buys_300s,
      SUM(ta.sell_count_300s) as total_sells_300s,
      COALESCE(dca.dca_count, 0) as dca_buys_300s,
      COALESCE(dca.dca_net_flow, 0) as dca_net_flow_300s,
      MAX(ta.unique_wallets_300s) as max_unique_wallets,
      SUM(ta.volume_300s_sol) as total_volume_300s,
      MAX(ta.updated_at) as last_update
    FROM token_aggregates ta
    LEFT JOIN dca ON ta.mint = dca.mint
    WHERE ta.updated_at > unixepoch() - 60
      AND ta.mint NOT IN (
        SELECT mint FROM mint_blocklist 
        WHERE expires_at IS NULL OR expires_at > unixepoch()
      )
    GROUP BY ta.mint
    ORDER BY SUM(ta.net_flow_300s_sol) DESC
    LIMIT ?
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all(limit) as Array<{
    mint: string;
    net_flow_60s: number | null;
    net_flow_300s: number | null;
    net_flow_900s: number | null;
    net_flow_3600s: number | null;
    net_flow_7200s: number | null;
    net_flow_14400s: number | null;
    total_buys_300s: number | null;
    total_sells_300s: number | null;
    dca_buys_300s: number | null;
    dca_net_flow_300s: number | null;
    max_unique_wallets: number | null;
    total_volume_300s: number | null;
    last_update: number | null;
  }>;
  
  return rows.map(row => ({
    mint: row.mint,
    netFlow60s: row.net_flow_60s ?? 0,
    netFlow300s: row.net_flow_300s ?? 0,
    netFlow900s: row.net_flow_900s ?? 0,
    netFlow3600s: row.net_flow_3600s ?? 0,
    netFlow7200s: row.net_flow_7200s ?? 0,
    netFlow14400s: row.net_flow_14400s ?? 0,
    totalBuys300s: row.total_buys_300s ?? 0,
    totalSells300s: row.total_sells_300s ?? 0,
    dcaBuys300s: row.dca_buys_300s ?? 0,
    dcaNetFlow300s: row.dca_net_flow_300s ?? 0,
    maxUniqueWallets: row.max_unique_wallets ?? 0,
    totalVolume300s: row.total_volume_300s ?? 0,
    lastUpdate: row.last_update ?? 0,
  }));
}

export function getSparklineData(mint: string, limit: number = 30): SparklineDataPoint[] {
  const db = getDb();
  
  // Try to get historical net flow from token_signals details_json
  // This extracts net_flow_sol from signal details if available
  // Note: token_aggregates only stores current state (PRIMARY KEY on mint),
  // so we can't get historical time series from it
  const query = `
    SELECT 
        created_at as timestamp,
        CAST(json_extract(details_json, '$.net_flow_sol') AS REAL) as net_flow_sol,
        CAST(json_extract(details_json, '$.net_flow_300s') AS REAL) as net_flow_300s
    FROM token_signals
    WHERE mint = ?
        AND created_at > unixepoch() - 3600
        AND (
            json_extract(details_json, '$.net_flow_sol') IS NOT NULL
            OR json_extract(details_json, '$.net_flow_300s') IS NOT NULL
        )
    ORDER BY created_at DESC
    LIMIT ?
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all(mint, limit) as Array<{
    timestamp: number;
    net_flow_sol: number | null;
    net_flow_300s: number | null;
  }>;
  
  // Reverse to get chronological order and use net_flow_sol or net_flow_300s as fallback
  return rows
    .reverse()
    .map(row => ({
      timestamp: row.timestamp,
      netFlowSol: row.net_flow_sol ?? row.net_flow_300s ?? 0,
    }))
    .filter(point => point.netFlowSol !== 0 || point.timestamp > 0);
}

export function blockToken(mint: string, reason: string = 'Blocked via web UI'): void {
  const writeDb = getWriteDb();
  
  try {
    const now = Math.floor(Date.now() / 1000);
    const query = `
      INSERT OR REPLACE INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at)
      VALUES (?, ?, 'web-ui', ?, NULL)
    `;
    
    const stmt = writeDb.prepare(query);
    stmt.run(mint, reason, now);
  } finally {
    writeDb.close();
  }
}

export function unblockToken(mint: string): void {
  const writeDb = getWriteDb();
  
  try {
    const query = `DELETE FROM mint_blocklist WHERE mint = ?`;
    const stmt = writeDb.prepare(query);
    stmt.run(mint);
  } finally {
    writeDb.close();
  }
}

export function getLatestSignal(mint: string): { signalType: string; createdAt: number } | null {
  const db = getDb();
  
  const query = `
    SELECT signal_type, created_at
    FROM token_signals
    WHERE mint = ?
    ORDER BY created_at DESC
    LIMIT 1
  `;
  
  const stmt = db.prepare(query);
  const row = stmt.get(mint) as { signal_type: string; created_at: number } | undefined;
  
  if (!row) {
    return null;
  }
  
  return {
    signalType: row.signal_type,
    createdAt: row.created_at,
  };
}

