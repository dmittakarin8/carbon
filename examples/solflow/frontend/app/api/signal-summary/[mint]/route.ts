import { NextRequest, NextResponse } from 'next/server';
import { getTokenSignalSummary } from '@/lib/queries';

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ mint: string }> }
) {
  try {
    const { mint } = await params;
    const result = getTokenSignalSummary(mint);
    
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
