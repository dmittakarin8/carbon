'use client';

import { useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';

interface BlockButtonProps {
  mint: string;
  onBlocked: () => void;
}

export default function BlockButton({ mint, onBlocked }: BlockButtonProps) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);

  async function handleBlock() {
    setLoading(true);
    try {
      const response = await fetch(`/api/tokens/${mint}/block`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ reason: 'Blocked via web UI' }),
      });

      if (response.ok) {
        setOpen(false);
        onBlocked();
      } else {
        const error = await response.json();
        alert(`Failed to block token: ${error.error || 'Unknown error'}`);
      }
    } catch (error) {
      console.error('Error blocking token:', error);
      alert('Failed to block token');
    } finally {
      setLoading(false);
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button className="px-3 py-1 text-xs bg-red-600 hover:bg-red-700 text-white rounded transition-colors">
          Block
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg p-6 shadow-lg max-w-md w-full">
          <Dialog.Title className="text-lg font-semibold text-white mb-2">
            Block Token
          </Dialog.Title>
          <Dialog.Description className="text-gray-400 mb-4">
            Are you sure you want to block this token? It will be removed from
            the dashboard.
          </Dialog.Description>
          <div className="text-xs text-gray-500 mb-4 font-mono break-all">
            {mint}
          </div>
          <div className="flex gap-2 justify-end">
            <Dialog.Close asChild>
              <button
                className="px-4 py-2 text-sm bg-gray-700 hover:bg-gray-600 text-white rounded transition-colors"
                disabled={loading}
              >
                Cancel
              </button>
            </Dialog.Close>
            <button
              onClick={handleBlock}
              disabled={loading}
              className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded transition-colors disabled:opacity-50"
            >
              {loading ? 'Blocking...' : 'Block Token'}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

