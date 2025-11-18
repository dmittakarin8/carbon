import { NextResponse } from 'next/server';
import { getFollowedCount, getBlockedCount } from '@/lib/queries';

export async function GET() {
  try {
    const followedCount = getFollowedCount();
    const blockedCount = getBlockedCount();
    
    return NextResponse.json({ 
      followedCount,
      blockedCount 
    });
  } catch (error) {
    console.error('Error fetching counts:', error);
    return NextResponse.json({ error: 'Failed to fetch counts' }, { status: 500 });
  }
}
