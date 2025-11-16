import { NextResponse } from 'next/server';
import { getTokens } from '@/lib/queries';
import { TokensResponse } from '@/lib/types';

export async function GET() {
  try {
    const tokens = getTokens(100);
    const response: TokensResponse = { tokens };
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error fetching tokens:', error);
    return NextResponse.json(
      { error: 'Failed to fetch tokens' },
      { status: 500 }
    );
  }
}

