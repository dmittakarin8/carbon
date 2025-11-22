'use client';

import { useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { DashboardData } from '@/lib/dashboard-client';
import { Star } from 'lucide-react';

interface FollowedTokensModalProps {
  followedCount: number;
  onCountChange: () => void;
  dashboardData: DashboardData;
  followedTokens: string[];
}

function formatTimeAgo(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestamp;
  
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export default function FollowedTokensModal({ 
  followedCount, 
  onCountChange,
  dashboardData,
  followedTokens
}: FollowedTokensModalProps) {
  const [open, setOpen] = useState(false);

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
        <Dialog.Overlay className="fixed inset-0 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg p-6 shadow-lg max-w-3xl w-full max-h-[80vh] overflow-y-auto">
          <Dialog.Title className="text-lg font-semibold text-white mb-4">
            Followed Tokens
          </Dialog.Title>
          
          {followedTokens.length === 0 ? (
            <p className="text-gray-400">No followed tokens</p>
          ) : (
            <div className="space-y-2">
              {followedTokens.map(mint => {
                const meta = dashboardData.metadata[mint];
                return (
                  <div key={mint} className="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                    <div className="flex items-center gap-3 flex-1">
                      {meta?.imageUrl && (
                        <img 
                          src={meta.imageUrl} 
                          alt={meta.symbol || 'Token'}
                          className="w-8 h-8 rounded-full opacity-70"
                        />
                      )}
                      <div className="flex-1 min-w-0">
                        {meta?.name || meta?.symbol ? (
                          <>
                            <div className="font-semibold text-gray-200">
                              {meta.name || 'Unknown'}
                            </div>
                            <div className="text-gray-500 text-xs">
                              {meta.symbol || '—'}
                            </div>
                          </>
                        ) : (
                          <div className="font-mono text-sm text-gray-300">
                            {mint.slice(0, 8)}...{mint.slice(-8)}
                          </div>
                        )}
                      </div>
                      <div className="text-right">
                        {meta?.priceUsd && (
                          <div className="mb-1">
                            <div className="text-gray-400 text-xs">Price</div>
                            <div className="text-gray-200 text-sm">
                              ${meta.priceUsd.toFixed(6)}
                            </div>
                          </div>
                        )}
                        {meta?.marketCap && (
                          <div>
                            <div className="text-gray-400 text-xs">Market Cap</div>
                            <div className="text-gray-200 text-sm">
                              ${(meta.marketCap / 1_000_000).toFixed(2)}M
                            </div>
                          </div>
                        )}
                        {meta?.updatedAt && (
                          <div className="text-gray-500 text-xs mt-1">
                            Updated {formatTimeAgo(meta.updatedAt)}
                          </div>
                        )}
                      </div>
                    </div>
                    <button
                      onClick={() => handleUnfollow(mint)}
                      className="ml-4 px-3 py-1 bg-yellow-600 hover:bg-yellow-700 text-white rounded text-xs transition-colors flex items-center gap-1.5"
                    >
                      <Star className="w-3.5 h-3.5" fill="currentColor" />
                      Unfollow
                    </button>
                  </div>
                );
              })}
            </div>
          )}
          
          <Dialog.Close asChild>
            <button className="mt-4 px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm transition-colors">
              Close
            </button>
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
