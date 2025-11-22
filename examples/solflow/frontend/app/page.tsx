'use client';

import { useState, useEffect } from 'react';
import { fetchDashboardSafe, DashboardData } from '@/lib/dashboard-client';
import { useFollowedTokenRefresh } from '@/lib/use-followed-token-refresh';
import TokenDashboard from './components/TokenDashboard';
import BlockedTokensModal from './components/BlockedTokensModal';
import FollowedTokensModal from './components/FollowedTokensModal';

export default function Home() {
  const [dashboardData, setDashboardData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  async function fetchDashboard() {
    try {
      const data = await fetchDashboardSafe();
      if (data) {
        setDashboardData(data);
        setError(null);
      } else {
        setError('Failed to fetch dashboard data');
      }
    } catch (err) {
      console.error('Error fetching dashboard:', err);
      setError(err instanceof Error ? err.message : 'Failed to fetch dashboard');
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // Initial fetch
    fetchDashboard();
    
    // Auto-refresh every 10 seconds
    const interval = setInterval(fetchDashboard, 10000);
    
    return () => clearInterval(interval);
  }, []);

  // Auto-refresh followed tokens (staggered, 5s interval)
  useFollowedTokenRefresh(dashboardData?.counts.followed ?? 0);

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      <div className="container mx-auto px-4 py-8">
        <header className="mb-6 flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold mb-2">SolFlow Token Dashboard</h1>
            <p className="text-gray-400 text-sm">
              Real-time token metrics with net flow across multiple time windows
            </p>
          </div>
          <div className="flex items-center gap-3">
            <FollowedTokensModal 
              followedCount={dashboardData?.counts.followed ?? 0} 
              onCountChange={fetchDashboard}
              dashboardData={dashboardData ?? { tokens: [], metadata: {}, signals: {}, sparklines: {}, dcaSparklines: {}, counts: { followed: 0, blocked: 0 }, followedTokens: [], blockedTokens: [] }}
              followedTokens={dashboardData?.followedTokens ?? []}
            />
            <BlockedTokensModal 
              blockedCount={dashboardData?.counts.blocked ?? 0} 
              onCountChange={fetchDashboard}
              dashboardData={dashboardData ?? { tokens: [], metadata: {}, signals: {}, sparklines: {}, dcaSparklines: {}, counts: { followed: 0, blocked: 0 }, followedTokens: [], blockedTokens: [] }}
              blockedTokens={dashboardData?.blockedTokens ?? []}
            />
          </div>
        </header>

        {loading && !dashboardData ? (
          <div className="text-center py-12 text-gray-400">
            Loading dashboard...
          </div>
        ) : error ? (
          <div className="text-center py-12 text-red-400">
            Error: {error}
          </div>
        ) : dashboardData ? (
          <TokenDashboard 
            dashboardData={dashboardData}
            onRefresh={fetchDashboard} 
          />
        ) : null}

        <footer className="mt-8 text-center text-xs text-gray-500">
          <div>Auto-refreshing every 10 seconds • {dashboardData?.tokens.length ?? 0} tokens</div>
          {(dashboardData?.counts.followed ?? 0) > 0 && (
            <div className="mt-1 text-yellow-500/70">
              ⭐ Following {dashboardData?.counts.followed ?? 0} token{(dashboardData?.counts.followed ?? 0) !== 1 ? 's' : ''} • Price updates every ~{(dashboardData?.counts.followed ?? 0) * 5}s
            </div>
          )}
        </footer>
      </div>
    </div>
  );
}
