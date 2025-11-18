import { NextResponse } from 'next/server';
import { getTokenMetadata } from '@/lib/queries';
import { MetadataResponse } from '@/lib/types';

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const mint = searchParams.get('mint');
    
    if (!mint) {
      return NextResponse.json({ error: 'Mint required' }, { status: 400 });
    }
    
    const metadata = getTokenMetadata(mint);
    
    const response: MetadataResponse = { metadata };
    return NextResponse.json(response);
  } catch (error) {
    console.error('Error fetching metadata:', error);
    return NextResponse.json({ error: 'Failed to fetch' }, { status: 500 });
  }
}
