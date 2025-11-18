import { NextResponse } from 'next/server';

export async function POST(request: Request) {
  try {
    const { mint } = await request.json();
    
    if (!mint || typeof mint !== 'string') {
      return NextResponse.json({ error: 'Invalid mint' }, { status: 400 });
    }
    
    // Call DexScreener API directly to trigger metadata refresh
    // The backend price monitoring task will handle the database update
    const response = await fetch(`https://api.dexscreener.com/token-pairs/v1/solana/${mint}`);
    
    if (!response.ok) {
      return NextResponse.json({ error: 'DexScreener API failed' }, { status: 502 });
    }
    
    // Note: Backend price monitoring task will handle upsert on next cycle
    return NextResponse.json({ ok: true });
  } catch (error) {
    console.error('Error updating metadata:', error);
    return NextResponse.json({ error: 'Internal error' }, { status: 500 });
  }
}
