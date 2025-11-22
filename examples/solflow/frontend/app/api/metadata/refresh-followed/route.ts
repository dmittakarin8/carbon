import { NextResponse } from 'next/server';
import { getDb, getWriteDb } from '@/lib/db';

interface DexScreenerPair {
  baseToken: {
    name: string;
    symbol: string;
  };
  quoteToken: {
    symbol: string;
  };
  priceUsd: string;
  marketCap?: number;
  info?: {
    imageUrl?: string;
  };
}

/**
 * Refresh Followed Tokens API
 * 
 * Automatically updates price and market cap for all followed tokens
 * from DexScreener API. Intended to be called periodically.
 * 
 * Implements staggered refresh to avoid rate limiting:
 * - Fetches one token per call
 * - Returns token count and next mint to refresh
 * - Client should call repeatedly with delays
 */
export async function POST() {
  try {
    const db = getDb();
    
    // Get all followed tokens ordered by updated_at (oldest first)
    const followedTokens = db.prepare(`
      SELECT mint, updated_at
      FROM token_metadata
      WHERE follow_price = 1
      ORDER BY updated_at ASC
      LIMIT 1
    `).all() as Array<{ mint: string; updated_at: number }>;
    
    if (followedTokens.length === 0) {
      return NextResponse.json({ 
        ok: true, 
        message: 'No followed tokens to refresh',
        refreshed: 0 
      });
    }
    
    const { mint } = followedTokens[0];
    
    // Fetch token metadata from DexScreener API
    let response;
    try {
      response = await fetch(`https://api.dexscreener.com/token-pairs/v1/solana/${mint}`);
    } catch (fetchError) {
      console.error(`DexScreener API request failed for ${mint}:`, fetchError);
      return NextResponse.json({ 
        error: 'Network error',
        mint,
        refreshed: 0 
      }, { status: 502 });
    }
    
    if (!response.ok) {
      console.error(`DexScreener API returned ${response.status} for ${mint}`);
      return NextResponse.json({ 
        error: `DexScreener API failed with status ${response.status}`,
        mint,
        refreshed: 0 
      }, { status: 502 });
    }
    
    const pairs: DexScreenerPair[] = await response.json();
    
    // Find first pair with SOL quote token
    const pair = pairs.find(p => p.quoteToken.symbol === 'SOL');
    
    if (!pair) {
      console.warn(`No SOL pair found for followed token ${mint}`);
      // Still update timestamp to avoid getting stuck on this token
      const writeDb = getWriteDb();
      try {
        const now = Math.floor(Date.now() / 1000);
        writeDb.prepare(`
          UPDATE token_metadata 
          SET updated_at = ?
          WHERE mint = ?
        `).run(now, mint);
      } finally {
        writeDb.close();
      }
      
      return NextResponse.json({ 
        ok: true,
        message: 'No SOL pair found',
        mint,
        refreshed: 0 
      });
    }
    
    // Parse metadata
    const name = pair.baseToken.name;
    const symbol = pair.baseToken.symbol;
    const imageUrl = pair.info?.imageUrl || null;
    const priceUsd = parseFloat(pair.priceUsd) || 0;
    const marketCap = pair.marketCap || null;
    const now = Math.floor(Date.now() / 1000);
    
    // Update database
    const writeDb = getWriteDb();
    
    try {
      writeDb.exec('BEGIN TRANSACTION');
      
      const stmt = writeDb.prepare(`
        UPDATE token_metadata 
        SET 
          name = ?,
          symbol = ?,
          image_url = ?,
          price_usd = ?,
          market_cap = ?,
          updated_at = ?
        WHERE mint = ?
      `);
      
      stmt.run(name, symbol, imageUrl, priceUsd, marketCap, now, mint);
      
      writeDb.exec('COMMIT');
      
      console.log(`[Followed Token Refresh] Updated ${symbol} (${mint}): $${priceUsd.toFixed(6)}, MCap: ${marketCap ? '$' + (marketCap / 1_000_000).toFixed(2) + 'M' : 'N/A'}`);
      
      return NextResponse.json({ 
        ok: true, 
        mint,
        symbol,
        priceUsd,
        marketCap,
        refreshed: 1
      });
    } catch (dbError) {
      writeDb.exec('ROLLBACK');
      throw dbError;
    } finally {
      writeDb.close();
    }
  } catch (error) {
    console.error('Error refreshing followed tokens:', error);
    return NextResponse.json({ 
      error: 'Internal error', 
      details: error instanceof Error ? error.message : 'Unknown error'
    }, { status: 500 });
  }
}
