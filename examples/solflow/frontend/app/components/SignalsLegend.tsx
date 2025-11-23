'use client';

import { useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { Info, X } from 'lucide-react';
import { Badge } from '@/components/ui/badge';

export default function SignalsLegend() {
  const [open, setOpen] = useState(false);

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button className="px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm transition-colors flex items-center gap-1.5">
          <Info className="w-4 h-4" />
          Signals Legend
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50 z-40" />
        <Dialog.Content className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-gray-800 rounded-lg shadow-2xl max-w-xl w-full max-h-[85vh] flex flex-col z-50">
          {/* Header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-gray-700">
            <Dialog.Title className="text-base font-semibold text-white">
              Signals Scoring Legend
            </Dialog.Title>
            <Dialog.Close asChild>
              <button className="text-gray-400 hover:text-gray-200 transition-colors">
                <X className="w-5 h-5" />
              </button>
            </Dialog.Close>
          </div>
          
          {/* Body - Scrollable */}
          <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5">
            {/* Pattern Tags Section */}
            <div>
              <h3 className="text-sm font-semibold text-gray-300 mb-3">Pattern Tags</h3>
              <div className="space-y-2.5">
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="success">ACCUMULATION</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Strong, consistent net inflows and structured wallet growth.
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="info">MOMENTUM</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Short-term spike in activity; early movement.
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="danger">DISTRIBUTION</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Net outflows and weakening wallet behavior.
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="warning">WASHOUT</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    High churn with conflicting flows (often bots).
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="neutral">NOISE</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    No meaningful trend or structure; not predictive.
                  </div>
                </div>
              </div>
            </div>
            
            {/* Confidence Levels Section */}
            <div>
              <h3 className="text-sm font-semibold text-gray-300 mb-3">Confidence Levels</h3>
              <div className="space-y-2.5">
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="default">HIGH</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Strong, consistent data across windows; reliable.
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="neutral">MEDIUM</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Moderately consistent; soft signal.
                  </div>
                </div>
                
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-32 flex items-center">
                    <Badge variant="neutral">LOW</Badge>
                  </div>
                  <div className="text-gray-400 text-sm flex-1 leading-relaxed">
                    Sparse or inconsistent data; weak signal.
                  </div>
                </div>
              </div>
            </div>
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
