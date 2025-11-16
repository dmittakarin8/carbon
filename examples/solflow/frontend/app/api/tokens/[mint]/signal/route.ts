import { NextResponse } from 'next/server';
import { getLatestSignal } from '@/lib/queries';
import { TokenSignal } from '@/lib/types';

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
    
    const signal = getLatestSignal(mint);
    return NextResponse.json(signal);
  } catch (error) {
    console.error('Error fetching signal:', error);
    return NextResponse.json(
      { error: 'Failed to fetch signal' },
      { status: 500 }
    );
  }
}

