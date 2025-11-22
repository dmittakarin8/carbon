import { NextRequest, NextResponse } from 'next/server';
import { getTokenSignalSummary } from '@/lib/queries';

export async function GET(
  request: NextRequest,
  { params }: { params: { mint: string } }
) {
  try {
    const result = getTokenSignalSummary(params.mint);
    
    return NextResponse.json({
      summary: result?.summary || null,
    });
  } catch (error) {
    console.error('Error fetching token signal summary:', error);
    return NextResponse.json(
      { error: 'Failed to fetch signal summary' },
      { status: 500 }
    );
  }
}
