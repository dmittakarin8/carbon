import { NextRequest, NextResponse } from 'next/server';
import { getAllTokenSignalSummaries } from '@/lib/queries';

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const limit = parseInt(searchParams.get('limit') || '50', 10);
    
    const summaries = getAllTokenSignalSummaries(limit);
    
    return NextResponse.json({ summaries });
  } catch (error) {
    console.error('Error fetching all token signal summaries:', error);
    return NextResponse.json(
      { error: 'Failed to fetch signal summaries' },
      { status: 500 }
    );
  }
}
