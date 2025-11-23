import { NextResponse } from 'next/server';
import { getWriteDb } from '@/lib/db';

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
  pairCreatedAt?: number;
  info?: {
    imageUrl?: string;
  };
}

export async function POST(request: Request) {
  try {
    const { mint } = await request.json();
    
    if (!mint || typeof mint !== 'string') {
      return NextResponse.json({ error: 'Invalid mint' }, { status: 400 });
    }
    
    // Fetch token metadata from DexScreener API
    const response = await fetch(`https://api.dexscreener.com/token-pairs/v1/solana/${mint}`);
    
    if (!response.ok) {
      return NextResponse.json({ error: 'DexScreener API failed' }, { status: 502 });
    }
    
    const pairs: DexScreenerPair[] = await response.json();
    
    // Find first pair with SOL quote token
    const pair = pairs.find(p => p.quoteToken.symbol === 'SOL');
    
    if (!pair) {
      return NextResponse.json({ error: 'No SOL pair found' }, { status: 404 });
    }
    
    // Parse metadata
    const name = pair.baseToken.name;
    const symbol = pair.baseToken.symbol;
    const imageUrl = pair.info?.imageUrl || null;
    const priceUsd = parseFloat(pair.priceUsd) || 0;
    const marketCap = pair.marketCap || null;
    // Convert pairCreatedAt from milliseconds to seconds for consistency with other timestamps
    const pairCreatedAt = pair.pairCreatedAt ? Math.floor(pair.pairCreatedAt / 1000) : null;
    const now = Math.floor(Date.now() / 1000);
    
    // Update database
    const writeDb = getWriteDb();
    
    try {
      writeDb.exec('BEGIN TRANSACTION');
      
      const stmt = writeDb.prepare(`
        INSERT INTO token_metadata 
          (mint, name, symbol, image_url, price_usd, market_cap, pair_created_at, updated_at, created_at, decimals, blocked, follow_price)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0)
        ON CONFLICT(mint) DO UPDATE SET
          name = excluded.name,
          symbol = excluded.symbol,
          image_url = excluded.image_url,
          price_usd = excluded.price_usd,
          market_cap = excluded.market_cap,
          pair_created_at = COALESCE(token_metadata.pair_created_at, excluded.pair_created_at),
          updated_at = excluded.updated_at
      `);
      
      stmt.run(mint, name, symbol, imageUrl, priceUsd, marketCap, pairCreatedAt, now, now);
      
      writeDb.exec('COMMIT');
      
      return NextResponse.json({ ok: true, metadata: { name, symbol, imageUrl, priceUsd, marketCap, pairCreatedAt } });
    } catch (dbError) {
      writeDb.exec('ROLLBACK');
      throw dbError;
    } finally {
      writeDb.close();
    }
  } catch (error) {
    console.error('Error updating metadata:', error);
    return NextResponse.json({ error: 'Internal error' }, { status: 500 });
  }
}
