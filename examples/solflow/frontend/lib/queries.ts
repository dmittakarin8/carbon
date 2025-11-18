import { getDb, getWriteDb } from './db';
import { TokenMetrics, SparklineDataPoint, DcaSparklineDataPoint } from './types';

function tableExists(db: ReturnType<typeof getDb>, tableName: string): boolean {
  try {
    const result = db.prepare(`
      SELECT name FROM sqlite_master 
      WHERE type='table' AND name=?
    `).get(tableName);
    return result !== undefined;
  } catch {
    return false;
  }
}

export function getTokens(limit: number = 100): TokenMetrics[] {
  const db = getDb();
  
  // Phase 6: DCA Rolling Windows - query dca_buys_* columns directly from token_aggregates
  // Removed deprecated CTE joins for raw_dca and dca_count from token_signals
  const query = `
    SELECT
      ta.mint,
      ta.net_flow_60s_sol,
      ta.net_flow_300s_sol,
      ta.net_flow_900s_sol,
      ta.net_flow_3600s_sol,
      ta.net_flow_7200s_sol,
      ta.net_flow_14400s_sol,
      ta.unique_wallets_300s,
      ta.volume_300s_sol,
      ta.dca_buys_60s,
      ta.dca_buys_300s,
      ta.dca_buys_900s,
      ta.dca_buys_3600s,
      ta.dca_buys_14400s,
      ta.updated_at
    FROM token_aggregates ta
    WHERE ta.dca_buys_3600s > 0
    ORDER BY ta.net_flow_300s_sol DESC
    LIMIT 40
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all() as Array<{
    mint: string;
    net_flow_60s_sol: number | null;
    net_flow_300s_sol: number | null;
    net_flow_900s_sol: number | null;
    net_flow_3600s_sol: number | null;
    net_flow_7200s_sol: number | null;
    net_flow_14400s_sol: number | null;
    unique_wallets_300s: number | null;
    volume_300s_sol: number | null;
    dca_buys_60s: number | null;
    dca_buys_300s: number | null;
    dca_buys_900s: number | null;
    dca_buys_3600s: number | null;
    dca_buys_14400s: number | null;
    updated_at: number | null;
  }>;
  
  return rows.map(row => ({
    mint: row.mint,
    netFlow60s: row.net_flow_60s_sol ?? 0,
    netFlow300s: row.net_flow_300s_sol ?? 0,
    netFlow900s: row.net_flow_900s_sol ?? 0,
    netFlow3600s: row.net_flow_3600s_sol ?? 0,
    netFlow7200s: row.net_flow_7200s_sol ?? 0,
    netFlow14400s: row.net_flow_14400s_sol ?? 0,
    totalBuys300s: 0,
    totalSells300s: 0,
    // Phase 6: DCA rolling-window fields from token_aggregates
    dcaBuys60s: row.dca_buys_60s ?? 0,
    dcaBuys300sWindow: row.dca_buys_300s ?? 0,
    dcaBuys900s: row.dca_buys_900s ?? 0,
    dcaBuys3600s: row.dca_buys_3600s ?? 0,
    dcaBuys14400s: row.dca_buys_14400s ?? 0,
    maxUniqueWallets: row.unique_wallets_300s ?? 0,
    totalVolume300s: row.volume_300s_sol ?? 0,
    lastUpdate: row.updated_at ?? 0,
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

export function getDcaSparklineData(mint: string): DcaSparklineDataPoint[] {
  const db = getDb();
  
  // Check if trades table exists (it may not be present in all database setups)
  if (!tableExists(db, 'trades')) {
    return [];
  }
  
  // Get DCA BUY trades grouped by minute for last 60 minutes
  const query = `
    SELECT 
      (timestamp / 60) * 60 AS minute_timestamp,
      COUNT(*) AS buy_count
    FROM trades
    WHERE mint = ?
      AND program_name = 'JupiterDCA'
      AND action = 'BUY'
      AND timestamp > unixepoch() - 3600
    GROUP BY minute_timestamp
    ORDER BY minute_timestamp ASC
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all(mint) as Array<{
    minute_timestamp: number;
    buy_count: number;
  }>;
  
  return rows.map(row => ({
    timestamp: row.minute_timestamp,
    buyCount: row.buy_count,
  }));
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

