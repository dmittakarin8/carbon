'use client';

import { useState, useEffect } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { TokenMetrics, TokenMetadata } from '@/lib/types';

export default function BlockedTokensModal() {
  const [open, setOpen] = useState(false);
  const [blockedTokens, setBlockedTokens] = useState<TokenMetrics[]>([]);
  const [metadata, setMetadata] = useState<Record<string, TokenMetadata>>({});

  useEffect(() => {
    if (open) {
      // Fetch blocked tokens from API
      fetch('/api/metadata/blocked')
        .then(res => res.json())
        .then(data => {
          const tokens = data.tokens || [];
          setBlockedTokens(tokens);
          
          // Fetch metadata for each blocked token
          tokens.forEach(async (token: TokenMetrics) => {
            try {
              const response = await fetch(`/api/metadata/get?mint=${token.mint}`);
              if (response.ok) {
                const metaData = await response.json();
                if (metaData.metadata) {
                  setMetadata(prev => ({ ...prev, [token.mint]: metaData.metadata }));
                }
              }
            } catch (error) {
              console.error('Failed to fetch metadata:', error);
            }
          });
        })
        .catch(error => {
          console.error('Failed to fetch blocked tokens:', error);
        });
    }
  }, [open]);

  async function handleUnblock(mint: string) {
    try {
      const response = await fetch('/api/metadata/unblock', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint }),
      });
      
      if (response.ok) {
        setBlockedTokens(prev => prev.filter(t => t.mint !== mint));
      }
    } catch (error) {
      console.error('Failed to unblock:', error);
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm transition-colors">
          🚫 Blocked Tokens ({blockedTokens.length})
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg p-6 shadow-lg max-w-3xl w-full max-h-[80vh] overflow-y-auto">
          <Dialog.Title className="text-lg font-semibold text-white mb-4">
            Blocked Tokens
          </Dialog.Title>
          
          {blockedTokens.length === 0 ? (
            <p className="text-gray-400">No blocked tokens</p>
          ) : (
            <div className="space-y-2">
              {blockedTokens.map(token => {
                const meta = metadata[token.mint];
                return (
                  <div key={token.mint} className="flex items-center justify-between p-3 bg-gray-700/50 rounded">
                    <div className="flex items-center gap-3 flex-1">
                      {meta?.imageUrl && (
                        <img 
                          src={meta.imageUrl} 
                          alt={meta.symbol || 'Token'}
                          className="w-8 h-8 rounded-full opacity-70"
                        />
                      )}
                      <div>
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
                            {token.mint.slice(0, 8)}...{token.mint.slice(-8)}
                          </div>
                        )}
                      </div>
                    </div>
                    <button
                      onClick={() => handleUnblock(token.mint)}
                      className="px-3 py-1 bg-green-600 hover:bg-green-700 text-white rounded text-xs transition-colors"
                    >
                      Unblock
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
