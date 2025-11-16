import { NextResponse } from 'next/server';
import { blockToken } from '@/lib/queries';
import { BlockResponse } from '@/lib/types';

export async function POST(
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
    
    const body = await request.json().catch(() => ({}));
    const reason = body.reason || 'Blocked via web UI';
    
    blockToken(mint, reason);
    
    const response: BlockResponse = { success: true };
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error blocking token:', error);
    const response: BlockResponse = {
      success: false,
      error: error instanceof Error ? error.message : 'Failed to block token',
    };
    return NextResponse.json(response, { status: 500 });
  }
}

