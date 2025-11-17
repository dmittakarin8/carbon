import { NextResponse } from 'next/server';
import { getDcaSparklineData } from '@/lib/queries';
import { DcaSparklineResponse } from '@/lib/types';

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
    
    const dataPoints = getDcaSparklineData(mint);
    const response: DcaSparklineResponse = { dataPoints };
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error fetching DCA sparkline data:', error);
    return NextResponse.json(
      { error: 'Failed to fetch DCA sparkline data' },
      { status: 500 }
    );
  }
}

