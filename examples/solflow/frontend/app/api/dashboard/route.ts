import { NextResponse } from 'next/server';
import { getDb } from '@/lib/db';
import { 
  TokenMetrics, 
  TokenMetadata, 
  TokenSignal, 
  SparklineDataPoint, 
  DcaSparklineDataPoint,
  TokenSignalSummary 
} from '@/lib/types';

/**
 * Batched Dashboard API - Single Endpoint for All Dashboard Data
 * 
 * Replaces the N+1 query pattern with a single efficient endpoint that returns:
 * - All active tokens (with metrics)
 * - Metadata for all tokens
 * - Latest signal for all tokens
 * - Sparkline data for all tokens
 * - DCA sparkline data for all tokens
 * - Followed/blocked counts
 * 
 * This endpoint is called once every 10 seconds by the frontend.
 */

interface DashboardResponse {
  tokens: TokenMetrics[];
  metadata: Record<string, TokenMetadata>;
  signals: Record<string, TokenSignal | null>;
  signalSummaries: Record<string, TokenSignalSummary | null>;
  sparklines: Record<string, SparklineDataPoint[]>;
  dcaSparklines: Record<string, DcaSparklineDataPoint[]>;
  counts: {
    followed: number;
    blocked: number;
  };
  followedTokens: string[];
  blockedTokens: string[];
}

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

export async function GET() {
  try {
    const db = getDb();
    
    // 1. Get all active tokens (same query as /api/tokens)
    const tokensQuery = `
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
    
    const tokenRows = db.prepare(tokensQuery).all() as Array<{
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
    
    const tokens: TokenMetrics[] = tokenRows.map(row => ({
      mint: row.mint,
      netFlow60s: row.net_flow_60s_sol ?? 0,
      netFlow300s: row.net_flow_300s_sol ?? 0,
      netFlow900s: row.net_flow_900s_sol ?? 0,
      netFlow3600s: row.net_flow_3600s_sol ?? 0,
      netFlow7200s: row.net_flow_7200s_sol ?? 0,
      netFlow14400s: row.net_flow_14400s_sol ?? 0,
      totalBuys300s: 0,
      totalSells300s: 0,
      dcaBuys60s: row.dca_buys_60s ?? 0,
      dcaBuys300sWindow: row.dca_buys_300s ?? 0,
      dcaBuys900s: row.dca_buys_900s ?? 0,
      dcaBuys3600s: row.dca_buys_3600s ?? 0,
      dcaBuys14400s: row.dca_buys_14400s ?? 0,
      maxUniqueWallets: row.unique_wallets_300s ?? 0,
      totalVolume300s: row.volume_300s_sol ?? 0,
      lastUpdate: row.updated_at ?? 0,
    }));
    
    // Extract token mints from dashboard
    const tokenMints = tokens.map(t => t.mint);
    
    // Get followed/blocked tokens early (needed for unified metadata query)
    const followedTokensRows = db.prepare(`
      SELECT mint 
      FROM token_metadata 
      WHERE follow_price = 1
    `).all() as Array<{ mint: string }>;
    
    const blockedTokensRows = db.prepare(`
      SELECT mint 
      FROM token_metadata 
      WHERE blocked = 1
    `).all() as Array<{ mint: string }>;
    
    const followedTokens = followedTokensRows.map(row => row.mint);
    const blockedTokens = blockedTokensRows.map(row => row.mint);
    
    // Create unified mint list: dashboard tokens + blocked tokens + followed tokens
    const allMints = Array.from(new Set([...tokenMints, ...blockedTokens, ...followedTokens]));
    
    // 2. Get metadata for ALL relevant tokens in one query
    const metadata: Record<string, TokenMetadata> = {};
    if (allMints.length > 0) {
      const metadataQuery = `
        SELECT 
          mint, name, symbol, image_url, price_usd, market_cap,
          follow_price, blocked, updated_at
        FROM token_metadata
        WHERE mint IN (${allMints.map(() => '?').join(',')})
      `;
      
      const metadataRows = db.prepare(metadataQuery).all(...allMints) as Array<{
        mint: string;
        name: string | null;
        symbol: string | null;
        image_url: string | null;
        price_usd: number | null;
        market_cap: number | null;
        follow_price: number;
        blocked: number;
        updated_at: number;
      }>;
      
      metadataRows.forEach(row => {
        metadata[row.mint] = {
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
      });
    }
    
    // 3. Get latest signal for dashboard tokens only
    const signals: Record<string, TokenSignal | null> = {};
    if (tokenMints.length > 0) {
      // Get latest signal per mint using window function
      const signalsQuery = `
        WITH latest_signals AS (
          SELECT 
            mint,
            signal_type,
            created_at,
            ROW_NUMBER() OVER (PARTITION BY mint ORDER BY created_at DESC) as rn
          FROM token_signals
          WHERE mint IN (${tokenMints.map(() => '?').join(',')})
        )
        SELECT mint, signal_type, created_at
        FROM latest_signals
        WHERE rn = 1
      `;
      
      try {
        const signalRows = db.prepare(signalsQuery).all(...tokenMints) as Array<{
          mint: string;
          signal_type: string;
          created_at: number;
        }>;
        
        signalRows.forEach(row => {
          signals[row.mint] = {
            signalType: row.signal_type,
            createdAt: row.created_at,
          };
        });
      } catch (error) {
        console.error('Error fetching signals:', error);
        // Continue without signals
      }
      
      // Set null for mints without signals
      tokenMints.forEach(mint => {
        if (!signals[mint]) {
          signals[mint] = null;
        }
      });
    }
    
    // 4. Get sparkline data for dashboard tokens only (last hour of net flow)
    const sparklines: Record<string, SparklineDataPoint[]> = {};
    if (tokenMints.length > 0) {
      const sparklineQuery = `
        SELECT 
          mint,
          created_at as timestamp,
          CAST(json_extract(details_json, '$.net_flow_sol') AS REAL) as net_flow_sol,
          CAST(json_extract(details_json, '$.net_flow_300s') AS REAL) as net_flow_300s
        FROM token_signals
        WHERE mint IN (${tokenMints.map(() => '?').join(',')})
          AND created_at > unixepoch() - 3600
          AND (
            json_extract(details_json, '$.net_flow_sol') IS NOT NULL
            OR json_extract(details_json, '$.net_flow_300s') IS NOT NULL
          )
        ORDER BY mint, created_at DESC
      `;
      
      try {
        const sparklineRows = db.prepare(sparklineQuery).all(...tokenMints) as Array<{
          mint: string;
          timestamp: number;
          net_flow_sol: number | null;
          net_flow_300s: number | null;
        }>;
        
        // Group by mint and limit to 30 points per token
        const sparklinesByMint: Record<string, SparklineDataPoint[]> = {};
        sparklineRows.forEach(row => {
          if (!sparklinesByMint[row.mint]) {
            sparklinesByMint[row.mint] = [];
          }
          if (sparklinesByMint[row.mint].length < 30) {
            sparklinesByMint[row.mint].push({
              timestamp: row.timestamp,
              netFlowSol: row.net_flow_sol ?? row.net_flow_300s ?? 0,
            });
          }
        });
        
        // Reverse to chronological order
        Object.entries(sparklinesByMint).forEach(([mint, points]) => {
          sparklines[mint] = points.reverse();
        });
      } catch (error) {
        console.error('Error fetching sparklines:', error);
      }
      
      // Set empty array for mints without sparkline data
      tokenMints.forEach(mint => {
        if (!sparklines[mint]) {
          sparklines[mint] = [];
        }
      });
    }
    
    // 5. Get DCA sparkline data for dashboard tokens only (last 60 minutes of 1-min buckets)
    const dcaSparklines: Record<string, DcaSparklineDataPoint[]> = {};
    if (tokenMints.length > 0 && tableExists(db, 'dca_activity_buckets')) {
      const dcaSparklineQuery = `
        SELECT 
          mint,
          bucket_timestamp as timestamp,
          buy_count
        FROM dca_activity_buckets
        WHERE mint IN (${tokenMints.map(() => '?').join(',')})
          AND bucket_timestamp > unixepoch() - 3600
        ORDER BY mint, bucket_timestamp ASC
      `;
      
      try {
        const dcaSparklineRows = db.prepare(dcaSparklineQuery).all(...tokenMints) as Array<{
          mint: string;
          timestamp: number;
          buy_count: number;
        }>;
        
        // Group by mint and limit to 60 points per token
        const dcaSparklinesByMint: Record<string, DcaSparklineDataPoint[]> = {};
        dcaSparklineRows.forEach(row => {
          if (!dcaSparklinesByMint[row.mint]) {
            dcaSparklinesByMint[row.mint] = [];
          }
          if (dcaSparklinesByMint[row.mint].length < 60) {
            dcaSparklinesByMint[row.mint].push({
              timestamp: row.timestamp,
              buyCount: row.buy_count,
            });
          }
        });
        
        Object.assign(dcaSparklines, dcaSparklinesByMint);
      } catch (error) {
        console.error('Error fetching DCA sparklines:', error);
      }
      
      // Set empty array for mints without DCA sparkline data
      tokenMints.forEach(mint => {
        if (!dcaSparklines[mint]) {
          dcaSparklines[mint] = [];
        }
      });
    } else {
      // Table doesn't exist - set empty arrays for all mints
      tokenMints.forEach(mint => {
        dcaSparklines[mint] = [];
      });
    }
    
    // 6. Get signal summaries for all tokens (Phase 3)
    const signalSummaries: Record<string, TokenSignalSummary | null> = {};
    if (allMints.length > 0 && tableExists(db, 'token_signal_summary')) {
      const summariesQuery = `
        SELECT 
          token_address,
          persistence_score,
          pattern_tag,
          confidence,
          appearance_24h,
          appearance_72h,
          updated_at
        FROM token_signal_summary
        WHERE token_address IN (${allMints.map(() => '?').join(',')})
      `;
      
      try {
        const summaryRows = db.prepare(summariesQuery).all(...allMints) as Array<{
          token_address: string;
          persistence_score: number;
          pattern_tag: string | null;
          confidence: string | null;
          appearance_24h: number;
          appearance_72h: number;
          updated_at: number;
        }>;
        
        summaryRows.forEach(row => {
          signalSummaries[row.token_address] = {
            tokenAddress: row.token_address,
            persistenceScore: row.persistence_score,
            patternTag: row.pattern_tag,
            confidence: row.confidence,
            appearance24h: row.appearance_24h,
            appearance72h: row.appearance_72h,
            updatedAt: row.updated_at,
          };
        });
      } catch (error) {
        console.error('Error fetching signal summaries:', error);
      }
      
      // Set null for mints without summaries
      allMints.forEach(mint => {
        if (!signalSummaries[mint]) {
          signalSummaries[mint] = null;
        }
      });
    } else {
      // Table doesn't exist - set null for all mints
      allMints.forEach(mint => {
        signalSummaries[mint] = null;
      });
    }
    
    // 7. Build response (blocked/followed tokens already fetched earlier)
    const response: DashboardResponse = {
      tokens,
      metadata,
      signals,
      signalSummaries,
      sparklines,
      dcaSparklines,
      counts: {
        followed: followedTokens.length,
        blocked: blockedTokens.length,
      },
      followedTokens,
      blockedTokens,
    };
    
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error fetching dashboard data:', error);
    return NextResponse.json(
      { error: 'Failed to fetch dashboard data' },
      { status: 500 }
    );
  }
}
