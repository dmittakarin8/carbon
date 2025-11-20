'use client';

import { useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { DashboardData } from '@/lib/dashboard-client';

interface BlockedTokensModalProps {
  blockedCount: number;
  onCountChange: () => void;
  dashboardData: DashboardData;
  blockedTokens: string[];
}

export default function BlockedTokensModal({ 
  blockedCount, 
  onCountChange, 
  dashboardData,
  blockedTokens
}: BlockedTokensModalProps) {
  const [open, setOpen] = useState(false);

  async function handleUnblock(mint: string) {
    try {
      const response = await fetch('/api/metadata/unblock', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mint }),
      });
      
      if (response.ok) {
        // Trigger dashboard refresh to get updated state
        onCountChange();
      }
    } catch (error) {
      console.error('Failed to unblock:', error);
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm transition-colors">
          🚫 Blocked Tokens ({blockedCount})
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
              {blockedTokens.map(mint => {
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
                            {mint.slice(0, 8)}...{mint.slice(-8)}
                          </div>
                        )}
                      </div>
                    </div>
                    <button
                      onClick={() => handleUnblock(mint)}
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
