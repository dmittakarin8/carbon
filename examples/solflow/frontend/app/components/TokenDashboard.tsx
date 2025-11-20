'use client';

import { useState, useMemo } from 'react';
import { DashboardData } from '@/lib/dashboard-client';
import DcaSparkline from './DcaSparkline';
import * as Tooltip from '@radix-ui/react-tooltip';
import { TrendingUp, Target, Zap, AlertTriangle, Minus, Star, RefreshCw, Download, Ban, Copy, Check } from 'lucide-react';

type SortField =
  | 'netFlow900s'
  | 'netFlow3600s'
  | 'netFlow14400s'
  | 'maxUniqueWallets'
  | 'dcaBuys3600s';

type SortDirection = 'asc' | 'desc';

interface TokenDashboardProps {
  dashboardData: DashboardData;
  onRefresh: () => void;
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
    <Tooltip.Provider delayDuration={200}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button
            onClick={handleCopy}
            className="text-gray-500 hover:text-gray-300 transition-colors"
          >
            {copied ? (
              <Check className="w-3.5 h-3.5 text-green-400" />
            ) : (
              <Copy className="w-3.5 h-3.5" />
            )}
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            className="bg-gray-900 text-gray-100 px-2.5 py-1 rounded text-xs shadow-lg border border-gray-700"
            sideOffset={5}
          >
            {copied ? 'Copied!' : 'Copy Address'}
            <Tooltip.Arrow className="fill-gray-700" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}

function SignalIcon({ signalType }: { signalType: string | null }) {
  if (!signalType) {
    return <Minus className="w-3.5 h-3.5 text-gray-600" />;
  }

  let Icon = Minus;
  let label = signalType;

  switch (signalType.toUpperCase()) {
    case 'BREAKOUT':
      Icon = TrendingUp;
      break;
    case 'FOCUSED':
      Icon = Target;
      break;
    case 'SURGE':
      Icon = Zap;
      break;
    case 'BOT_DROPOFF':
      Icon = AlertTriangle;
      break;
  }

  return (
    <Tooltip.Provider delayDuration={200}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button className="text-blue-400 hover:text-blue-300 transition-colors">
            <Icon className="w-3.5 h-3.5" />
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content
            className="bg-gray-900 text-gray-100 px-2.5 py-1 rounded text-xs shadow-lg border border-gray-700"
            sideOffset={5}
          >
            {label}
            <Tooltip.Arrow className="fill-gray-700" />
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}

export default function TokenDashboard({
  dashboardData,
  onRefresh,
}: TokenDashboardProps) {
  const [sortField, setSortField] = useState<SortField>('netFlow900s');
  const [sortDirection, setSortDirection] = useState<SortDirection>('desc');

  const { tokens, metadata, signals } = dashboardData;

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
        // Refresh dashboard to get updated state
        onRefresh();
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
        // Refresh dashboard to get updated metadata
        onRefresh();
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
            <th className="text-left px-5 py-3 text-xs font-semibold text-gray-400">
              Token
            </th>
            <th className="text-right px-5 py-3 text-xs font-semibold text-gray-400">
              Price (USD)
            </th>
            <th className="text-right px-5 py-3 text-xs font-semibold text-gray-400">
              Market Cap
            </th>
            <th className="text-center px-3 py-3 text-xs font-semibold text-gray-400">
              Actions
            </th>
            <th
              className="text-right px-5 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow900s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 15m <SortIcon field="netFlow900s" />
              </div>
            </th>
            <th
              className="text-right px-5 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow3600s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 1h <SortIcon field="netFlow3600s" />
              </div>
            </th>
            <th
              className="text-right px-5 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow14400s')}
            >
              <div className="flex items-center justify-end gap-1">
                Net Flow 4h <SortIcon field="netFlow14400s" />
              </div>
            </th>
            <th 
              className="text-center px-5 py-3 text-xs font-semibold text-gray-400"
              title="Raw DCA buy activity over the last hour (JupiterDCA BUY trades grouped per minute)."
            >
              DCA Buys
            </th>
            <th
              className="text-right px-5 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('dcaBuys3600s')}
              title="DCA buy count in the last hour (3600s rolling window) from JupiterDCA program. Higher values indicate sustained accumulation activity."
            >
              <div className="flex items-center justify-end gap-1">
                DCA (1h) <SortIcon field="dcaBuys3600s" />
              </div>
            </th>
            <th className="text-center px-2 py-3 text-xs font-semibold text-gray-400">
              Signal
            </th>
            <th
              className="text-right px-2 py-3 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('maxUniqueWallets')}
            >
              <div className="flex items-center justify-end gap-1">
                Wallets <SortIcon field="maxUniqueWallets" />
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
                  isFollowing ? 'bg-blue-950/30 border-l-2 border-l-blue-500 hover:bg-blue-950/40 hover:ring-1 hover:ring-blue-500/20' : ''
                }`}
              >
                {/* Token Column: Name + Symbol + Image */}
                <td className="px-5 py-3 text-xs">
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
                  </div>
                </td>

                {/* Price Column */}
                <td className="px-5 py-3 text-xs text-right">
                  {meta?.priceUsd ? (
                    <span className="text-gray-300">${meta.priceUsd.toFixed(6)}</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Market Cap Column */}
                <td className="px-5 py-3 text-xs text-right">
                  {meta?.marketCap ? (
                    <span className="text-gray-300">
                      ${(meta.marketCap / 1_000_000).toFixed(2)}M
                    </span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Actions Column */}
                <td className="px-3 py-3">
                  <div className="flex items-center justify-center gap-1.5">
                    {/* Copy Address */}
                    <CopyButton text={token.mint} mint={token.mint} />

                    {/* Follow Price Star */}
                    <Tooltip.Provider delayDuration={200}>
                      <Tooltip.Root>
                        <Tooltip.Trigger asChild>
                          <button
                            onClick={() => handleFollowPrice(token.mint, !isFollowing)}
                            className={`transition-colors ${
                              isFollowing 
                                ? 'text-yellow-400 hover:text-yellow-300' 
                                : 'text-gray-500 hover:text-gray-400'
                            }`}
                          >
                            <Star 
                              className="w-3.5 h-3.5" 
                              fill={isFollowing ? 'currentColor' : 'none'}
                              strokeWidth={2}
                            />
                          </button>
                        </Tooltip.Trigger>
                        <Tooltip.Portal>
                          <Tooltip.Content
                            className="bg-gray-900 text-gray-100 px-2.5 py-1 rounded text-xs shadow-lg border border-gray-700"
                            sideOffset={5}
                          >
                            {isFollowing ? 'Following' : 'Follow'}
                            <Tooltip.Arrow className="fill-gray-700" />
                          </Tooltip.Content>
                        </Tooltip.Portal>
                      </Tooltip.Root>
                    </Tooltip.Provider>

                    {/* Refresh Metadata */}
                    <Tooltip.Provider delayDuration={200}>
                      <Tooltip.Root>
                        <Tooltip.Trigger asChild>
                          <button
                            onClick={() => handleGetMetadata(token.mint)}
                            className="text-gray-500 hover:text-gray-300 transition-colors"
                          >
                            {hasMetadata ? (
                              <RefreshCw className="w-3.5 h-3.5" />
                            ) : (
                              <Download className="w-3.5 h-3.5" />
                            )}
                          </button>
                        </Tooltip.Trigger>
                        <Tooltip.Portal>
                          <Tooltip.Content
                            className="bg-gray-900 text-gray-100 px-2.5 py-1 rounded text-xs shadow-lg border border-gray-700"
                            sideOffset={5}
                          >
                            {hasMetadata ? 'Refresh' : 'Fetch'}
                            <Tooltip.Arrow className="fill-gray-700" />
                          </Tooltip.Content>
                        </Tooltip.Portal>
                      </Tooltip.Root>
                    </Tooltip.Provider>

                    {/* Block Token */}
                    <Tooltip.Provider delayDuration={200}>
                      <Tooltip.Root>
                        <Tooltip.Trigger asChild>
                          <button
                            onClick={() => handleBlockFixed(token.mint)}
                            className="text-gray-500 hover:text-gray-400 transition-colors"
                          >
                            <Ban className="w-3.5 h-3.5" />
                          </button>
                        </Tooltip.Trigger>
                        <Tooltip.Portal>
                          <Tooltip.Content
                            className="bg-gray-900 text-gray-100 px-2.5 py-1 rounded text-xs shadow-lg border border-gray-700"
                            sideOffset={5}
                          >
                            Block
                            <Tooltip.Arrow className="fill-gray-700" />
                          </Tooltip.Content>
                        </Tooltip.Portal>
                      </Tooltip.Root>
                    </Tooltip.Provider>
                  </div>
                </td>
                
                {/* Net Flow Columns */}
                <td className="px-5 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow900s} />
                </td>
                <td className="px-5 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow3600s} />
                </td>
                <td className="px-5 py-3 text-xs text-right">
                  <NetFlowCell value={token.netFlow14400s} />
                </td>

                {/* DCA Sparkline */}
                <td className="px-5 py-3 text-center">
                  <DcaSparkline 
                    mint={token.mint}
                    dataPoints={dashboardData.dcaSparklines[token.mint] || []}
                  />
                </td>

                {/* DCA Buys Count */}
                <td className="px-5 py-3 text-xs text-right text-gray-400">
                  {token.dcaBuys3600s > 0 ? (
                    <span>{token.dcaBuys3600s} buys</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>

                {/* Signal */}
                <td className="px-2 py-3 text-center">
                  <SignalIcon signalType={signals[token.mint]?.signalType || null} />
                </td>

                {/* Wallets */}
                <td className="px-2 py-3 text-xs text-right text-gray-400">
                  {token.maxUniqueWallets || '—'}
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

