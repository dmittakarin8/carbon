import { NextResponse } from 'next/server';
import { setBlocked } from '@/lib/queries';

export async function POST(request: Request) {
  try {
    const { mint } = await request.json();
    
    if (!mint) {
      return NextResponse.json({ error: 'Invalid mint' }, { status: 400 });
    }
    
    setBlocked(mint, true);
    
    return NextResponse.json({ ok: true });
  } catch (error) {
    console.error('Error blocking token:', error);
    return NextResponse.json({ error: 'Failed to block' }, { status: 500 });
  }
}
