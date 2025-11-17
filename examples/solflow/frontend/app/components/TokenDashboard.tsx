'use client';

import { useState, useEffect, useMemo } from 'react';
import { TokenMetrics, TokenSignal } from '@/lib/types';
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
  | 'dcaBuys300s';

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

function CopyButton({ text }: { text: string }) {
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
      className="ml-2 text-gray-500 hover:text-gray-300 transition-colors"
      title={copied ? 'Copied!' : 'Copy address'}
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

  return (
    <div className="w-full overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-gray-700">
            <th className="text-left p-2 text-xs font-semibold text-gray-400">
              Mint
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow60s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 1m <SortIcon field="netFlow60s" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow300s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 5m <SortIcon field="netFlow300s" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow900s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 15m <SortIcon field="netFlow900s" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow3600s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 1h <SortIcon field="netFlow3600s" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow7200s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 2h <SortIcon field="netFlow7200s" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('netFlow14400s')}
            >
              <div className="flex items-center gap-1">
                Net Flow 4h <SortIcon field="netFlow14400s" />
              </div>
            </th>
            <th 
              className="text-left p-2 text-xs font-semibold text-gray-400"
              title="Raw DCA buy activity over the last hour (JupiterDCA BUY trades grouped per minute)."
            >
              DCA Buys
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('dcaBuys300s')}
              title="DCA conviction signals occur when DCA buys overlap with spot buys, indicating coordinated accumulation."
            >
              <div className="flex items-center gap-1">
                DCA (1h) <SortIcon field="dcaBuys300s" />
              </div>
            </th>
            <th className="text-left p-2 text-xs font-semibold text-gray-400">
              Signal
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('maxUniqueWallets')}
            >
              <div className="flex items-center gap-1">
                Wallets <SortIcon field="maxUniqueWallets" />
              </div>
            </th>
            <th
              className="text-left p-2 text-xs font-semibold text-gray-400 cursor-pointer hover:text-gray-300"
              onClick={() => handleSort('totalVolume300s')}
            >
              <div className="flex items-center gap-1">
                Volume <SortIcon field="totalVolume300s" />
              </div>
            </th>
            <th className="text-left p-2 text-xs font-semibold text-gray-400">
              Block
            </th>
          </tr>
        </thead>
        <tbody>
          {sortedTokens.map((token) => (
            <tr
              key={token.mint}
              className="border-b border-gray-800 hover:bg-gray-800/50 transition-colors"
            >
              <td className="p-2 text-xs">
                <div className="flex items-center gap-2">
                  <a
                    href={`https://solscan.io/token/${token.mint}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-blue-400 hover:text-blue-300 font-mono"
                    title={token.mint}
                  >
                    {formatMint(token.mint)}
                  </a>
                  <CopyButton text={token.mint} />
                </div>
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow60s} />
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow300s} />
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow900s} />
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow3600s} />
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow7200s} />
              </td>
              <td className="p-2 text-xs">
                <NetFlowCell value={token.netFlow14400s} />
              </td>
              <td className="p-2">
                <DcaSparkline mint={token.mint} />
              </td>
              <td className="p-2 text-xs text-gray-400">
                {token.dcaBuys300s > 0 ? (
                  <div>
                    {token.dcaBuys300s} buys
                  </div>
                ) : (
                  <span className="text-gray-600">—</span>
                )}
              </td>
              <td className="p-2 text-xs">
                {signals[token.mint] ? (
                  <span className="px-2 py-1 bg-blue-600/20 text-blue-400 rounded text-xs">
                    {signals[token.mint]?.signalType}
                  </span>
                ) : (
                  <span className="text-gray-600">—</span>
                )}
              </td>
              <td className="p-2 text-xs text-gray-400">
                {token.maxUniqueWallets || '—'}
              </td>
              <td className="p-2 text-xs text-gray-400">
                {formatNetFlow(token.totalVolume300s)} SOL
              </td>
              <td className="p-2">
                <BlockButton mint={token.mint} onBlocked={onRefresh} />
              </td>
            </tr>
          ))}
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

