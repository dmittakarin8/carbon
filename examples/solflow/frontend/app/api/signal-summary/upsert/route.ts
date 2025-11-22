import { NextRequest, NextResponse } from 'next/server';
import { upsertTokenSignalSummary } from '@/lib/queries';

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    
    const {
      tokenAddress,
      persistenceScore,
      patternTag,
      confidence,
      appearance24h,
      appearance72h,
    } = body;
    
    // Validate required fields
    if (!tokenAddress || typeof persistenceScore !== 'number') {
      return NextResponse.json(
        { error: 'Missing required fields: tokenAddress and persistenceScore' },
        { status: 400 }
      );
    }
    
    // Validate persistence_score range (0-10)
    if (persistenceScore < 0 || persistenceScore > 10) {
      return NextResponse.json(
        { error: 'persistenceScore must be between 0 and 10' },
        { status: 400 }
      );
    }
    
    upsertTokenSignalSummary({
      tokenAddress,
      persistenceScore,
      patternTag: patternTag || null,
      confidence: confidence || null,
      appearance24h: appearance24h || 0,
      appearance72h: appearance72h || 0,
    });
    
    return NextResponse.json({ success: true });
  } catch (error) {
    console.error('Error upserting token signal summary:', error);
    return NextResponse.json(
      { error: 'Failed to upsert signal summary' },
      { status: 500 }
    );
  }
}
