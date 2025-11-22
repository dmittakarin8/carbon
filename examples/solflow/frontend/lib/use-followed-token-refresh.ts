import { useEffect, useRef } from 'react';

/**
 * Hook to automatically refresh followed tokens' price and market cap data
 * 
 * Strategy:
 * - Calls /api/metadata/refresh-followed every 5 seconds
 * - Each call updates ONE token (oldest first)
 * - Staggers requests to avoid DexScreener rate limits
 * - For N followed tokens, full refresh takes N * 5 seconds
 * 
 * Example: 10 followed tokens = 50 seconds for complete cycle
 */
export function useFollowedTokenRefresh(followedCount: number) {
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const isRefreshingRef = useRef(false);

  useEffect(() => {
    // Only run if there are followed tokens
    if (followedCount === 0) {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    async function refreshNextToken() {
      // Prevent concurrent requests
      if (isRefreshingRef.current) {
        return;
      }

      isRefreshingRef.current = true;

      try {
        const response = await fetch('/api/metadata/refresh-followed', {
          method: 'POST',
        });

        if (!response.ok) {
          console.error('Failed to refresh followed token:', response.statusText);
        } else {
          const data = await response.json();
          if (data.refreshed > 0) {
            console.log(`[Followed Token Refresh] Updated ${data.symbol || data.mint}`);
          }
        }
      } catch (error) {
        console.error('Error refreshing followed token:', error);
      } finally {
        isRefreshingRef.current = false;
      }
    }

    // Initial refresh after 2 seconds (avoid competing with initial dashboard load)
    const initialTimeout = setTimeout(refreshNextToken, 2000);

    // Setup recurring refresh every 5 seconds
    intervalRef.current = setInterval(refreshNextToken, 5000);

    // Cleanup on unmount or when followedCount changes
    return () => {
      clearTimeout(initialTimeout);
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [followedCount]);
}
