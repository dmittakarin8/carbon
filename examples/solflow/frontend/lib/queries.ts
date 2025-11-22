import { getDb, getWriteDb } from './db';
import { TokenMetrics, SparklineDataPoint, DcaSparklineDataPoint, TokenMetadata } from './types';

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
  // Phase 7: Exclude blocked tokens via LEFT JOIN with token_metadata
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
    LEFT JOIN token_metadata tm ON ta.mint = tm.mint
    WHERE ta.dca_buys_3600s > 0
      AND (tm.blocked IS NULL OR tm.blocked = 0)
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
  
  // Phase 7: Query dca_activity_buckets table (time-series data)
  // Check if table exists (may not exist if pipeline hasn't run migration yet)
  if (!tableExists(db, 'dca_activity_buckets')) {
    console.warn('dca_activity_buckets table does not exist yet - pipeline migration pending');
    return [];
  }
  
  // Get last 60 minutes of 1-minute buckets (60 data points)
  const query = `
    SELECT 
      bucket_timestamp as timestamp,
      buy_count
    FROM dca_activity_buckets
    WHERE mint = ?
      AND bucket_timestamp > unixepoch() - 3600
    ORDER BY bucket_timestamp ASC
    LIMIT 60
  `;
  
  try {
    const stmt = db.prepare(query);
    const rows = stmt.all(mint) as Array<{
      timestamp: number;
      buy_count: number;
    }>;
    
    return rows.map(row => ({
      timestamp: row.timestamp,
      buyCount: row.buy_count,
    }));
  } catch (error) {
    console.error('Error querying dca_activity_buckets:', error);
    return [];
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

export function getTokenMetadata(mint: string): TokenMetadata | null {
  const db = getDb();
  
  const query = `
    SELECT 
      mint, name, symbol, image_url, price_usd, market_cap,
      follow_price, blocked, updated_at
    FROM token_metadata
    WHERE mint = ?
  `;
  
  const stmt = db.prepare(query);
  const row = stmt.get(mint) as {
    mint: string;
    name: string | null;
    symbol: string | null;
    image_url: string | null;
    price_usd: number | null;
    market_cap: number | null;
    follow_price: number;
    blocked: number;
    updated_at: number;
  } | undefined;
  
  if (!row) return null;
  
  return {
    mint: row.mint,
    name: row.name ?? undefined,
    symbol: row.symbol ?? undefined,
    imageUrl: row.image_url ?? undefined,
    priceUsd: row.price_usd ?? undefined,
    marketCap: row.market_cap ?? undefined,
    followPrice: row.follow_price === 1,
    blocked: row.blocked === 1,
    updatedAt: row.updated_at,
  };
}

export function setFollowPrice(mint: string, follow: boolean): void {
  const writeDb = getWriteDb();
  
  try {
    const now = Math.floor(Date.now() / 1000);
    
    // Insert or update - ensure row exists
    const query = `
      INSERT INTO token_metadata (mint, follow_price, updated_at, created_at, decimals, blocked)
      VALUES (?, ?, ?, ?, 0, 0)
      ON CONFLICT(mint) DO UPDATE SET
        follow_price = excluded.follow_price,
        updated_at = excluded.updated_at
    `;
    
    const stmt = writeDb.prepare(query);
    stmt.run(mint, follow ? 1 : 0, now, now);
  } finally {
    writeDb.close();
  }
}

export function setBlocked(mint: string, blocked: boolean): void {
  const writeDb = getWriteDb();
  
  try {
    const now = Math.floor(Date.now() / 1000);
    
    // Insert or update - ensure row exists
    const query = `
      INSERT INTO token_metadata (mint, blocked, updated_at, created_at, decimals, follow_price)
      VALUES (?, ?, ?, ?, 0, 0)
      ON CONFLICT(mint) DO UPDATE SET
        blocked = excluded.blocked,
        updated_at = excluded.updated_at
    `;
    
    const stmt = writeDb.prepare(query);
    stmt.run(mint, blocked ? 1 : 0, now, now);
  } finally {
    writeDb.close();
  }
}

export function getBlockedTokens(): TokenMetrics[] {
  const db = getDb();
  
  const query = `
    SELECT
      ta.mint,
      ta.net_flow_300s_sol,
      ta.updated_at
    FROM token_aggregates ta
    INNER JOIN token_metadata tm ON ta.mint = tm.mint
    WHERE tm.blocked = 1
    ORDER BY ta.updated_at DESC
    LIMIT 50
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all() as Array<{
    mint: string;
    net_flow_300s_sol: number | null;
    updated_at: number | null;
  }>;
  
  // Simplified return (full fields not needed for blocked view)
  return rows.map(row => ({
    mint: row.mint,
    netFlow60s: 0,
    netFlow300s: row.net_flow_300s_sol ?? 0,
    netFlow900s: 0,
    netFlow3600s: 0,
    netFlow7200s: 0,
    netFlow14400s: 0,
    totalBuys300s: 0,
    totalSells300s: 0,
    dcaBuys60s: 0,
    dcaBuys300sWindow: 0,
    dcaBuys900s: 0,
    dcaBuys3600s: 0,
    dcaBuys14400s: 0,
    maxUniqueWallets: 0,
    totalVolume300s: 0,
    lastUpdate: row.updated_at ?? 0,
  }));
}

export function getFollowedTokens(): TokenMetrics[] {
  const db = getDb();
  
  // Phase 3: Sort by persistence_score DESC (highest first)
  const query = `
    SELECT
      ta.mint,
      ta.net_flow_300s_sol,
      ta.updated_at
    FROM token_aggregates ta
    INNER JOIN token_metadata tm ON ta.mint = tm.mint
    LEFT JOIN token_signal_summary tss ON ta.mint = tss.token_address
    WHERE tm.follow_price = 1
    ORDER BY COALESCE(tss.persistence_score, 0) DESC, ta.updated_at DESC
    LIMIT 50
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all() as Array<{
    mint: string;
    net_flow_300s_sol: number | null;
    updated_at: number | null;
  }>;
  
  // Simplified return (full fields not needed for followed view)
  return rows.map(row => ({
    mint: row.mint,
    netFlow60s: 0,
    netFlow300s: row.net_flow_300s_sol ?? 0,
    netFlow900s: 0,
    netFlow3600s: 0,
    netFlow7200s: 0,
    netFlow14400s: 0,
    totalBuys300s: 0,
    totalSells300s: 0,
    dcaBuys60s: 0,
    dcaBuys300sWindow: 0,
    dcaBuys900s: 0,
    dcaBuys3600s: 0,
    dcaBuys14400s: 0,
    maxUniqueWallets: 0,
    totalVolume300s: 0,
    lastUpdate: row.updated_at ?? 0,
  }));
}

export function getFollowedCount(): number {
  const db = getDb();
  const result = db.prepare(`
    SELECT COUNT(*) as count 
    FROM token_metadata 
    WHERE follow_price = 1
  `).get() as { count: number };
  return result.count;
}

export function getBlockedCount(): number {
  const db = getDb();
  const result = db.prepare(`
    SELECT COUNT(*) as count 
    FROM token_metadata 
    WHERE blocked = 1
  `).get() as { count: number };
  return result.count;
}

// ============================================================================
// TOKEN SIGNAL SUMMARY (Phase 1: Persistent Signal Engine)
// ============================================================================

export function getTokenSignalSummary(tokenAddress: string): TokenMetadata & { summary: import('./types').TokenSignalSummary | null } | null {
  const db = getDb();
  
  // Check if table exists
  if (!tableExists(db, 'token_signal_summary')) {
    console.warn('token_signal_summary table does not exist yet');
    const metadata = getTokenMetadata(tokenAddress);
    return metadata ? { ...metadata, summary: null } : null;
  }
  
  const query = `
    SELECT 
      token_address,
      persistence_score,
      pattern_tag,
      confidence,
      appearance_24h,
      appearance_72h,
      updated_at
    FROM token_signal_summary
    WHERE token_address = ?
  `;
  
  const stmt = db.prepare(query);
  const row = stmt.get(tokenAddress) as {
    token_address: string;
    persistence_score: number;
    pattern_tag: string | null;
    confidence: string | null;
    appearance_24h: number;
    appearance_72h: number;
    updated_at: number;
  } | undefined;
  
  const metadata = getTokenMetadata(tokenAddress);
  
  if (!row) {
    return metadata ? { ...metadata, summary: null } : null;
  }
  
  return {
    ...(metadata || {
      mint: tokenAddress,
      followPrice: false,
      blocked: false,
      updatedAt: 0,
    }),
    summary: {
      tokenAddress: row.token_address,
      persistenceScore: row.persistence_score,
      patternTag: row.pattern_tag,
      confidence: row.confidence,
      appearance24h: row.appearance_24h,
      appearance72h: row.appearance_72h,
      updatedAt: row.updated_at,
    },
  };
}

export function upsertTokenSignalSummary(summary: {
  tokenAddress: string;
  persistenceScore: number;
  patternTag: string | null;
  confidence: string | null;
  appearance24h: number;
  appearance72h: number;
}): void {
  const writeDb = getWriteDb();
  
  try {
    const now = Math.floor(Date.now() / 1000);
    
    const query = `
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
    `;
    
    const stmt = writeDb.prepare(query);
    stmt.run(
      summary.tokenAddress,
      summary.persistenceScore,
      summary.patternTag,
      summary.confidence,
      summary.appearance24h,
      summary.appearance72h,
      now
    );
  } finally {
    writeDb.close();
  }
}

export function getAllTokenSignalSummaries(limit: number = 50): Array<import('./types').TokenSignalSummary> {
  const db = getDb();
  
  // Check if table exists
  if (!tableExists(db, 'token_signal_summary')) {
    console.warn('token_signal_summary table does not exist yet');
    return [];
  }
  
  const query = `
    SELECT 
      token_address,
      persistence_score,
      pattern_tag,
      confidence,
      appearance_24h,
      appearance_72h,
      updated_at
    FROM token_signal_summary
    ORDER BY persistence_score DESC, updated_at DESC
    LIMIT ?
  `;
  
  const stmt = db.prepare(query);
  const rows = stmt.all(limit) as Array<{
    token_address: string;
    persistence_score: number;
    pattern_tag: string | null;
    confidence: string | null;
    appearance_24h: number;
    appearance_72h: number;
    updated_at: number;
  }>;
  
  return rows.map(row => ({
    tokenAddress: row.token_address,
    persistenceScore: row.persistence_score,
    patternTag: row.pattern_tag,
    confidence: row.confidence,
    appearance24h: row.appearance_24h,
    appearance72h: row.appearance_72h,
    updatedAt: row.updated_at,
  }));
}

