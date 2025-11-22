'use client';

import { useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { DashboardData } from '@/lib/dashboard-client';
import { X } from 'lucide-react';

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
        <Dialog.Overlay className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg shadow-2xl max-w-2xl w-full max-h-[85vh] flex flex-col z-50">
          {/* Header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-gray-700">
            <Dialog.Title className="text-base font-semibold text-white">
              Blocked Tokens ({blockedCount})
            </Dialog.Title>
            <Dialog.Close asChild>
              <button className="text-gray-400 hover:text-gray-200 transition-colors">
                <X className="w-5 h-5" />
              </button>
            </Dialog.Close>
          </div>
          
          {/* Body - Scrollable */}
          <div className="flex-1 overflow-y-auto px-5 py-3">
            {blockedTokens.length === 0 ? (
              <div className="text-center py-12 text-gray-400 text-sm">
                No blocked tokens. Click the block icon on any token to block it.
              </div>
            ) : (
              <div className="space-y-1">
                {blockedTokens.map(mint => {
                  const meta = dashboardData.metadata[mint];
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
                          className="w-7 h-7 rounded-full flex-shrink-0 opacity-70"
                        />
                      ) : (
                        <div className="w-7 h-7 rounded-full bg-gray-700 flex-shrink-0 opacity-70" />
                      )}
                      
                      {/* Token Name & Symbol */}
                      <div className="flex-1 min-w-0">
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
                      
                      {/* Unblock Button */}
                      <button
                        onClick={() => handleUnblock(mint)}
                        className="ml-2 px-2 py-1 bg-green-600 hover:bg-green-700 text-white rounded text-xs transition-colors flex-shrink-0"
                      >
                        Unblock
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          
          {/* Footer */}
          <div className="px-5 py-3 border-t border-gray-700 flex items-center justify-end">
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
