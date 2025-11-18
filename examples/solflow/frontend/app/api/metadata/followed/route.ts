import { NextResponse } from 'next/server';
import { getFollowedTokens } from '@/lib/queries';

export async function GET() {
  try {
    const followedTokens = getFollowedTokens();
    return NextResponse.json({ tokens: followedTokens });
  } catch (error) {
    console.error('Error fetching followed tokens:', error);
    return NextResponse.json({ error: 'Failed to fetch followed tokens' }, { status: 500 });
  }
}
