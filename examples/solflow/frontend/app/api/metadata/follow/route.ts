import { NextResponse } from 'next/server';
import { setFollowPrice } from '@/lib/queries';

export async function POST(request: Request) {
  try {
    const { mint, value } = await request.json();
    
    if (!mint || typeof value !== 'boolean') {
      return NextResponse.json({ error: 'Invalid parameters' }, { status: 400 });
    }
    
    setFollowPrice(mint, value);
    
    return NextResponse.json({ ok: true });
  } catch (error) {
    console.error('Error setting follow price:', error);
    return NextResponse.json({ error: 'Failed to update' }, { status: 500 });
  }
}
