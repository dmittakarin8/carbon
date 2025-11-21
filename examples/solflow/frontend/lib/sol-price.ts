/**
 * Utility function to fetch SOL/USD price from Dexscreener API.
 * Returns current price and 24h percentage change.
 */

export interface SolPriceData {
  priceUsd: number;
  priceChange24h: number;
}

const DEXSCREENER_API_URL = 'https://api.dexscreener.com/latest/dex/tokens/So11111111111111111111111111111111111111112';

export async function fetchSolPrice(): Promise<SolPriceData | null> {
  try {
    const response = await fetch(DEXSCREENER_API_URL, {
      method: 'GET',
      headers: {
        'Accept': 'application/json',
      },
      // Add cache: 'no-store' to ensure fresh data on each request
      cache: 'no-store',
    });

    if (!response.ok) {
      console.error('Dexscreener API error:', response.status, response.statusText);
      return null;
    }

    const data = await response.json();

    // Dexscreener returns pairs array - find SOL/USD pair
    if (!data.pairs || data.pairs.length === 0) {
      console.error('No pairs found in Dexscreener response');
      return null;
    }

    // Get the first pair (most liquid SOL/USD pair)
    const solPair = data.pairs[0];

    return {
      priceUsd: parseFloat(solPair.priceUsd || '0'),
      priceChange24h: parseFloat(solPair.priceChange?.h24 || '0'),
    };
  } catch (error) {
    console.error('Failed to fetch SOL price from Dexscreener:', error);
    return null;
  }
}
