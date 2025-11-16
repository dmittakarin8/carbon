'use client';

import { useState, useEffect } from 'react';
import { TokenMetrics } from '@/lib/types';
import TokenDashboard from './components/TokenDashboard';

export default function Home() {
  const [tokens, setTokens] = useState<TokenMetrics[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function fetchTokens() {
    try {
      const response = await fetch('/api/tokens');
      if (!response.ok) {
        throw new Error('Failed to fetch tokens');
      }
      const data = await response.json();
      setTokens(data.tokens || []);
      setError(null);
    } catch (err) {
      console.error('Error fetching tokens:', err);
      setError(err instanceof Error ? err.message : 'Failed to fetch tokens');
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    fetchTokens();
    
    // Auto-refresh every 5 seconds
    const interval = setInterval(fetchTokens, 5000);
    
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      <div className="container mx-auto px-4 py-8">
        <header className="mb-6">
          <h1 className="text-2xl font-bold mb-2">SolFlow Token Dashboard</h1>
          <p className="text-gray-400 text-sm">
            Real-time token metrics with net flow across multiple time windows
          </p>
        </header>

        {loading && tokens.length === 0 ? (
          <div className="text-center py-12 text-gray-400">
            Loading tokens...
          </div>
        ) : error ? (
          <div className="text-center py-12 text-red-400">
            Error: {error}
          </div>
        ) : (
          <TokenDashboard tokens={tokens} onRefresh={fetchTokens} />
        )}

        <footer className="mt-8 text-center text-xs text-gray-500">
          Auto-refreshing every 5 seconds • {tokens.length} tokens
        </footer>
      </div>
    </div>
  );
}
