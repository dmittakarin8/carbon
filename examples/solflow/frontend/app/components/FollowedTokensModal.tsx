'use client';

import { useState, useEffect } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { DashboardData } from '@/lib/dashboard-client';
import { Star, X } from 'lucide-react';
import { Badge } from '@/components/ui/badge';

interface FollowedTokensModalProps {
  followedCount: number;
  onCountChange: () => void;
  dashboardData: DashboardData;
  followedTokens: string[];
}

function formatTimeAgo(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestamp;
  
  if (diff < 10) return `${diff}s ago`;
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function formatPrice(price: number): string {
  if (price >= 1) return `$${price.toFixed(4)}`;
  if (price >= 0.0001) return `$${price.toFixed(6)}`;
  return `$${price.toExponential(2)}`;
}

function formatMarketCap(mcap: number): string {
  if (mcap >= 1_000_000_000) return `$${(mcap / 1_000_000_000).toFixed(2)}B`;
  if (mcap >= 1_000_000) return `$${(mcap / 1_000_000).toFixed(2)}M`;
  if (mcap >= 1_000) return `$${(mcap / 1_000).toFixed(1)}K`;
  return `$${mcap.toFixed(0)}`;
}

export default function FollowedTokensModal({ 
  followedCount, 
  onCountChange,
  dashboardData,
  followedTokens
}: FollowedTokensModalProps) {
  const [open, setOpen] = useState(false);
  const [currentTime, setCurrentTime] = useState(Math.floor(Date.now() / 1000));

  // Update current time every second to refresh "ago" timestamps
  useEffect(() => {
    if (!open) return;
    
    const interval = setInterval(() => {
      setCurrentTime(Math.floor(Date.now() / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [open]);

  async function handleUnfollow(mint: string) {
    try {
      const response = await fetch('/api/metadata/follow', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint, value: false }),
      });
      
      if (response.ok) {
        // Trigger dashboard refresh to get updated state
        onCountChange();
      }
    } catch (error) {
      console.error('Failed to unfollow:', error);
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm transition-colors">
          ⭐ Followed Tokens ({followedCount})
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg shadow-2xl max-w-2xl w-full max-h-[85vh] flex flex-col z-50">
          {/* Header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-gray-700">
            <Dialog.Title className="text-base font-semibold text-white">
              Followed Tokens ({followedCount})
            </Dialog.Title>
            <Dialog.Close asChild>
              <button className="text-gray-400 hover:text-gray-200 transition-colors">
                <X className="w-5 h-5" />
              </button>
            </Dialog.Close>
          </div>
          
          {/* Body - Scrollable */}
          <div className="flex-1 overflow-y-auto px-5 py-3">
            {followedTokens.length === 0 ? (
              <div className="text-center py-12 text-gray-400 text-sm">
                No followed tokens. Click the star icon on any token to follow it.
              </div>
            ) : (
              <div className="space-y-1">
                {followedTokens.map(mint => {
                  const meta = dashboardData.metadata[mint];
                  const summary = dashboardData.signalSummaries[mint];
                  const hasMetadata = meta && (meta.name || meta.symbol);
                  
                  return (
                    <div 
                      key={mint} 
                      className="flex items-center gap-3 px-3 py-1.5 bg-gray-700/30 hover:bg-gray-700/50 rounded transition-colors border border-gray-700/50"
                    >
                      {/* Token Image */}
                      {meta?.imageUrl ? (
                        <img 
                          src={meta.imageUrl} 
                          alt={meta.symbol || 'Token'}
                          className="w-7 h-7 rounded-full flex-shrink-0"
                        />
                      ) : (
                        <div className="w-7 h-7 rounded-full bg-gray-700 flex-shrink-0" />
                      )}
                      
                      {/* Token Name & Symbol */}
                      <div className="flex-1 min-w-0 max-w-[180px]">
                        {hasMetadata ? (
                          <>
                            <div className="font-medium text-gray-100 text-sm truncate leading-tight">
                              {meta.symbol || 'Unknown'}
                            </div>
                            <div className="text-gray-500 text-xs truncate leading-tight">
                              {meta.name || '—'}
                            </div>
                          </>
                        ) : (
                          <div className="font-mono text-xs text-gray-400 truncate">
                            {mint.slice(0, 12)}...{mint.slice(-6)}
                          </div>
                        )}
                      </div>
                      
                      {/* Price */}
                      <div className="text-right min-w-[90px]">
                        {meta?.priceUsd ? (
                          <>
                            <div className="text-gray-100 text-sm font-medium leading-tight">
                              {formatPrice(meta.priceUsd)}
                            </div>
                            <div className="text-gray-500 text-xs leading-tight">Price</div>
                          </>
                        ) : (
                          <div className="text-gray-600 text-xs">No price</div>
                        )}
                      </div>
                      
                      {/* Market Cap */}
                      <div className="text-right min-w-[80px]">
                        {meta?.marketCap ? (
                          <>
                            <div className="text-gray-100 text-sm font-medium leading-tight">
                              {formatMarketCap(meta.marketCap)}
                            </div>
                            <div className="text-gray-500 text-xs leading-tight">MCap</div>
                          </>
                        ) : (
                          <div className="text-gray-600 text-xs">—</div>
                        )}
                      </div>
                      
                      {/* Persistence Score */}
                      <div className="min-w-[225px]">
                        {summary ? (
                          <>
                            <div className="inline-block bg-gray-700/30 rounded-md px-2 py-1.5">
                              <div className="grid grid-cols-[50px_110px_65px] gap-0 items-center text-sm font-medium leading-tight">
                                <span className={`text-center ${
                                  summary.persistenceScore >= 7 ? 'text-green-400' :
                                  summary.persistenceScore >= 4 ? 'text-yellow-400' :
                                  'text-gray-400'
                                }`}>
                                  {summary.persistenceScore}/10
                                </span>
                                <div className="flex justify-center">
                                  <Badge variant={
                                    summary.patternTag === 'ACCUMULATION' ? 'success' :
                                    summary.patternTag === 'MOMENTUM' ? 'info' :
                                    summary.patternTag === 'DISTRIBUTION' ? 'danger' :
                                    summary.patternTag === 'WASHOUT' ? 'warning' :
                                    'neutral'
                                  }>
                                    {summary.patternTag || 'NOISE'}
                                  </Badge>
                                </div>
                                <div className="flex justify-center">
                                  <Badge variant={
                                    summary.confidence === 'HIGH' ? 'default' : 'neutral'
                                  } className="text-[9px]">
                                    {summary.confidence || 'LOW'}
                                  </Badge>
                                </div>
                              </div>
                            </div>
                            <div className="text-gray-500 text-xs leading-tight mt-1 text-center">
                              {formatTimeAgo(summary.updatedAt)}
                            </div>
                          </>
                        ) : (
                          <div className="text-gray-600 text-xs text-center">No score</div>
                        )}
                      </div>
                      
                      {/* Unfollow Button */}
                      <button
                        onClick={() => handleUnfollow(mint)}
                        className="ml-2 px-2 py-1 bg-yellow-600/90 hover:bg-yellow-600 text-white rounded text-xs transition-colors flex items-center gap-1 flex-shrink-0"
                        title="Unfollow"
                      >
                        <Star className="w-3 h-3" fill="currentColor" />
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          
          {/* Footer */}
          <div className="px-5 py-3 border-t border-gray-700 flex items-center justify-between">
            <div className="text-xs text-gray-500">
              Auto-refreshing every ~{followedCount * 5}s
            </div>
            <Dialog.Close asChild>
              <button className="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 rounded text-xs transition-colors">
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
