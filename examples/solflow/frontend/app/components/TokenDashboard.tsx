'use client';

import { useState, useEffect, useMemo } from 'react';
import { TokenMetrics, TokenSignal, TokenMetadata } from '@/lib/types';
import DcaSparkline from './DcaSparkline';
import BlockButton from './BlockButton';

type SortField =
  | 'netFlow60s'
  | 'netFlow300s'
  | 'netFlow900s'
  | 'netFlow3600s'
  | 'netFlow7200s'
  | 'netFlow14400s'
  | 'totalVolume300s'
  | 'maxUniqueWallets'
  | 'dcaBuys3600s';

type SortDirection = 'asc' | 'desc';

interface TokenDashboardProps {
  tokens: TokenMetrics[];
  onRefresh: () => void;
}

interface TokenWithSignal extends TokenMetrics {
  signal?: TokenSignal | null;
}

function formatNetFlow(value: number): string {
  if (value === 0) return '0';
  if (Math.abs(value) < 0.001) return value.toFixed(6);
  if (Math.abs(value) < 1) return value.toFixed(3);
  return value.toFixed(2);
}

function formatMint(mint: string): string {
  return `${mint.slice(0, 4)}...${mint.slice(-4)}`;
}

function CopyButton({ text, mint }: { text: string; mint: string }) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      console.error('Failed to copy:', error);
    }
  }

  return (
    <button
      onClick={handleCopy}
      className="text-gray-500 hover:text-gray-300 transition-colors"
      title={copied ? 'Copied!' : `Copy address: ${mint}`}
    >
      {copied ? '✓' : '📋'}
    </button>
  );
}

export default function TokenDashboard({
  tokens,
  onRefresh,
}: TokenDashboardProps) {
  const [sortField, setSortField] = useState<SortField>('netFlow300s');
  const [sortDirection, setSortDirection] = useState<SortDirection>('desc');
  const [signals, setSignals] = useState<Record<string, TokenSignal | null>>({});
  const [metadata, setMetadata] = useState<Record<string, TokenMetadata>>({});

  useEffect(() => {
    // Fetch signals for all tokens
    async function fetchSignals() {
      const signalPromises = tokens.map(async (token) => {
        try {
          const response = await fetch(`/api/tokens/${token.mint}/signal`);
          if (response.ok) {
            const signal = await response.json();
            return { mint: token.mint, signal };
          }
        } catch (error) {
          console.error(`Error fetching signal for ${token.mint}:`, error);
        }
        return { mint: token.mint, signal: null };
      });

      const results = await Promise.all(signalPromises);
      const signalsMap: Record<string, TokenSignal | null> = {};
      results.forEach(({ mint, signal }) => {
        signalsMap[mint] = signal;
      });
      setSignals(signalsMap);
    }

    if (tokens.length > 0) {
      fetchSignals();
    }
  }, [tokens]);

  useEffect(() => {
    // Fetch metadata for all tokens
    async function fetchMetadata() {
      const metadataPromises = tokens.map(async (token) => {
        try {
          const response = await fetch(`/api/metadata/get?mint=${token.mint}`);
          if (response.ok) {
            const data = await response.json();
            return { mint: token.mint, metadata: data.metadata };
          }
        } catch (error) {
          console.error(`Error fetching metadata for ${token.mint}:`, error);
        }
        return { mint: token.mint, metadata: null };
      });

      const results = await Promise.all(metadataPromises);
      const metadataMap: Record<string, TokenMetadata> = {};
      results.forEach(({ mint, metadata }) => {
        if (metadata) metadataMap[mint] = metadata;
      });
      setMetadata(metadataMap);
    }

    if (tokens.length > 0) {
      fetchMetadata();
    }
  }, [tokens]);

  const sortedTokens = useMemo(() => {
    const sorted = [...tokens].sort((a, b) => {
      const aValue = a[sortField] ?? 0;
      const bValue = b[sortField] ?? 0;
      const comparison = aValue - bValue;
      return sortDirection === 'asc' ? comparison : -comparison;
    });
    return sorted;
  }, [tokens, sortField, sortDirection]);

  function handleSort(field: SortField) {
    if (sortField === field) {
      setSortDirection(sortDirection === 'asc' ? 'desc' : 'asc');
    } else {
      setSortField(field);
      setSortDirection('desc');
    }
  }

  function SortIcon({ field }: { field: SortField }) {
    if (sortField !== field) {
      return <span className="text-gray-500">↕</span>;
    }
    return sortDirection === 'asc' ? <span>↑</span> : <span>↓</span>;
  }

  function NetFlowCell({ value }: { value: number }) {
    const isPositive = value >= 0;
    const colorClass = isPositive ? 'text-green-400' : 'text-red-400';
    return <span className={colorClass}>{formatNetFlow(value)}</span>;
  }

  async function handleFollowPrice(mint: string, value: boolean) {
    try {
      const response = await fetch('/api/metadata/follow', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint, value }),
      });
      
      if (response.ok) {
        // Update local state
        setMetadata(prev => ({
          ...prev,
          [mint]: { ...prev[mint], followPrice: value, mint, blocked: false, updatedAt: Date.now() / 1000 },
        }));
      }
    } catch (error) {
      console.error('Failed to toggle follow price:', error);
    }
  }

  async function handleBlockFixed(mint: string) {
    try {
      const response = await fetch('/api/metadata/block', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint }),
      });
      
      if (response.ok) {
        onRefresh(); // Remove from main table immediately
      }
    } catch (error) {
      console.error('Failed to block:', error);
    }
  }

  async function handleGetMetadata(mint: string) {
    try {
      const response = await fetch('/api/metadata/update', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint }),
      });
      
      if (response.ok) {
        // Refetch metadata for this token
        const metaResponse = await fetch(`/api/metadata/get?mint=${mint}`);
        if (metaResponse.ok) {
          const data = await metaResponse.json();
          setMetadata(prev => ({
            ...prev,
            [mint]: data.metadata,
          }));
        }
      }
    } catch (error) {
      console.error('Failed to get metadata:', error);
    }
  }

  return (
    <div className="w-full overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-gray-700">
            <th className="text-left px-4 py-3 text-xs font-semibold text-gray-400">
              Token
            </th>
            <th className="text-right px-4 py-3 text-xs font-semibold text-gray-400">
              Price (USD)
            </th>
            <th className="text-right px-4 py-3 text-xs font-semibold text-gray-400">
              Market Cap
            </th>
            <th className="text-center px-4 py-3 text-xs font-semibold text-gray-400">
              Actions
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow60s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 1m <SortIcon field="netFlow60s" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow300s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 5m <SortIcon field="netFlow300s" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow900s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 15m <SortIcon field="netFlow900s" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow3600s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 1h <SortIcon field="netFlow3600s" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow7200s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 2h <SortIcon field="netFlow7200s" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow14400s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 4h <SortIcon field="netFlow14400s" />
              </div>
            </th>
            <th 
              className="text-center px-4 py-3 text-xs font-semibold text-gray-400"
              title="Raw DCA buy activity over the last hour (JupiterDCA BUY trades grouped per minute)."
            >
              DCA Buys
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('dcaBuys3600s')}
              title="DCA buy count in the last hour (3600s rolling window) from JupiterDCA program. Higher values indicate sustained accumulation activity."
            >
              <div className="flex items-center justify-end gap-1">
                DCA (1h) <SortIcon field="dcaBuys3600s" />
              </div>
            </th>
            <th className="text-center px-4 py-3 text-xs font-semibold text-gray-400">
              Signal
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('maxUniqueWallets')}
            >
              <div className="flex items-center justify-end gap-1">
                Wallets <SortIcon field="maxUniqueWallets" />
              </div>
            </th>
            <th
              className="text-right px-4 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('totalVolume300s')}
            >
              <div className="flex items-center justify-end gap-1">
                Volume <SortIcon field="totalVolume300s" />
              </div>
            </th>
          </tr>
        </thead>
        <tbody>
          {sortedTokens.map((token) => {
            const meta = metadata[token.mint];
            const hasMetadata = meta && (meta.name || meta.symbol);
            const isFollowing = meta?.followPrice ?? false;
            
            return (
              <tr
                key={token.mint}
                className={`border-b border-gray-800 hover:bg-gray-800/50 transition-colors ${
                  isFollowing ? 'bg-blue-900/10' : ''
                }`}
              >
                {/* Token Column: Name + Symbol + Image + Copy Button */}
                <td className="px-4 py-3 text-xs">
                  <div className="flex items-center gap-3">
                    {meta?.imageUrl && (
                      <img 
                        src={meta.imageUrl} 
                        alt={meta.symbol || 'Token'}
                        className="w-8 h-8 rounded-full opacity-70"
                      />
                    )}
                    <div className="flex-1 min-w-0">
                      {hasMetadata ? (
                        <>
                          <div className="font-semibold text-gray-200 truncate">
                            {meta.name || 'Unknown'}
                          </div>
                          <div className="text-gray-500 text-xs">
                            {meta.symbol || '—'}
                          </div>
                        </>
                      ) : (
                        <div className="text-gray-600 font-mono text-xs">
                          {formatMint(token.mint)}
                        </div>
                      )}
                    </div>
                    <CopyButton text={token.mint} mint={token.mint} />
                  </div>
                </td>

                {/* Price Column */}
                <td className="px-4 py-3 text-xs text-right">
                  {meta?.priceUsd ? (
                    <span className="text-gray-300">${meta.priceUsd.toFixed(6)}</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Market Cap Column */}
                <td className="px-4 py-3 text-xs text-right">
                  {meta?.marketCap ? (
                    <span className="text-gray-300">
                      ${(meta.marketCap / 1_000_000).toFixed(2)}M
                    </span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Actions Column: Get/Refresh Metadata, Follow Price, Block */}
                <td className="px-4 py-3">
                  <div className="flex items-center justify-center gap-2">
                    {/* Get/Refresh Metadata Button */}
                    <button
                      onClick={() => handleGetMetadata(token.mint)}
                      className="px-2 py-1 text-xs bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors"
                      title={hasMetadata ? 'Refresh metadata' : 'Get metadata'}
                    >
                      {hasMetadata ? '🔄' : '📥'}
                    </button>

                    {/* Follow Price Checkbox */}
                    <label className="flex items-center gap-1 cursor-pointer" title="Follow price updates">
                      <input
                        type="checkbox"
                        checked={isFollowing}
                        onChange={(e) => handleFollowPrice(token.mint, e.target.checked)}
                        className="w-4 h-4 cursor-pointer accent-blue-500"
                      />
                      <span className="text-gray-500 text-xs">Follow</span>
                    </label>

                    {/* Block Button */}
                    <button
                      onClick={() => handleBlockFixed(token.mint)}
                      className="px-2 py-1 text-xs bg-red-600 hover:bg-red-700 text-white rounded transition-colors"
                      title="Block this token"
                    >
                      🚫
                    </button>
                  </div>
                </td>
                {/* Net Flow Columns */}
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow60s} />
                </td>
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow300s} />
                </td>
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow900s} />
                </td>
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow3600s} />
                </td>
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow7200s} />
                </td>
                <td className="px-4 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow14400s} />
                </td>

                {/* DCA Sparkline */}
                <td className="px-4 py-3 text-center">
                  <DcaSparkline
                    dcaBuys60s={token.dcaBuys60s}
                    dcaBuys300s={token.dcaBuys300sWindow}
                    dcaBuys900s={token.dcaBuys900s}
                    dcaBuys3600s={token.dcaBuys3600s}
                    dcaBuys14400s={token.dcaBuys14400s}
                  />
                </td>

                {/* DCA Buys Count */}
                <td className="px-4 py-3 text-xs text-right text-gray-400">
                  {token.dcaBuys3600s > 0 ? (
                    <span>{token.dcaBuys3600s} buys</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Signal */}
                <td className="px-4 py-3 text-xs text-center">
                  {signals[token.mint] ? (
                    <span className="px-2 py-1 bg-blue-600/20 text-blue-400 rounded text-xs">
                      {signals[token.mint]?.signalType}
                    </span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Wallets */}
                <td className="px-4 py-3 text-xs text-right text-gray-400">
                  {token.maxUniqueWallets || '—'}
                </td>

                {/* Volume */}
                <td className="px-4 py-3 text-xs text-right text-gray-400">
                  {formatNetFlow(token.totalVolume300s)} SOL
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {sortedTokens.length === 0 && (
        <div className="text-center py-8 text-gray-500 text-sm">
          No tokens found
        </div>
      )}
    </div>
  );
}

