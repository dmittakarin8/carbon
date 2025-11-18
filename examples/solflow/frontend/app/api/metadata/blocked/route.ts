import { NextResponse } from 'next/server';
import { getBlockedTokens } from '@/lib/queries';

export async function GET() {
  try {
    const blockedTokens = getBlockedTokens();
    return NextResponse.json({ tokens: blockedTokens });
  } catch (error) {
    console.error('Error fetching blocked tokens:', error);
    return NextResponse.json({ error: 'Failed to fetch blocked tokens' }, { status: 500 });
  }
}
