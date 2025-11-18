import { NextResponse } from 'next/server';
import { setBlocked } from '@/lib/queries';

export async function POST(request: Request) {
  try {
    const { mint } = await request.json();
    
    if (!mint) {
      return NextResponse.json({ error: 'Invalid mint' }, { status: 400 });
    }
    
    setBlocked(mint, false);
    
    return NextResponse.json({ ok: true });
  } catch (error) {
    console.error('Error unblocking token:', error);
    return NextResponse.json({ error: 'Failed to unblock' }, { status: 500 });
  }
}
