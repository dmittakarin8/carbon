import { NextResponse } from 'next/server';
import { getSparklineData } from '@/lib/queries';
import { SparklineResponse } from '@/lib/types';

export async function GET(
  request: Request,
  { params }: { params: Promise<{ mint: string }> }
) {
  try {
    const { mint } = await params;
    
    if (!mint || typeof mint !== 'string') {
      return NextResponse.json(
        { error: 'Invalid mint address' },
        { status: 400 }
      );
    }
    
    const dataPoints = getSparklineData(mint, 30);
    const response: SparklineResponse = { dataPoints };
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error fetching sparkline data:', error);
    return NextResponse.json(
      { error: 'Failed to fetch sparkline data' },
      { status: 500 }
    );
  }
}

