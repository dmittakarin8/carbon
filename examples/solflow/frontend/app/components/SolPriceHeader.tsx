'use client';

import { useState, useEffect } from 'react';
import { fetchSolPrice, SolPriceData } from '@/lib/sol-price';

const REFRESH_INTERVAL_MS = 5 * 60 * 1000; // 5 minutes

export default function SolPriceHeader() {
  const [priceData, setPriceData] = useState<SolPriceData | null>(null);
  const [loading, setLoading] = useState(true);

  async function updatePrice() {
    try {
      const data = await fetchSolPrice();
      if (data) {
        setPriceData(data);
      }
    } catch (error) {
      console.error('Error fetching SOL price:', error);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    // Initial fetch
    updatePrice();

    // Setup 5-minute refresh interval
    const interval = setInterval(updatePrice, REFRESH_INTERVAL_MS);

    // Cleanup on unmount
    return () => clearInterval(interval);
  }, []);

  if (loading || !priceData) {
    return (
      <div className="bg-gray-950 border-b border-gray-800 px-6 py-2">
        <div className="flex items-center justify-end gap-6 text-xs text-gray-500">
          <span>Loading SOL price...</span>
        </div>
      </div>
    );
  }

  const isPositive = priceData.priceChange24h >= 0;
  const changeColor = isPositive ? 'text-green-400' : 'text-red-400';
  const changeSymbol = isPositive ? '+' : '';

  return (
    <div className="bg-gray-950 border-b border-gray-800 px-6 py-2">
      <div className="flex items-center justify-end gap-6 text-xs">
        <div className="flex items-center gap-2">
          <span className="text-gray-400">SOL/USD:</span>
          <span className="text-gray-200 font-semibold">
            ${priceData.priceUsd.toFixed(2)}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-gray-400">24h:</span>
          <span className={`font-semibold ${changeColor}`}>
            {changeSymbol}{priceData.priceChange24h.toFixed(2)}%
          </span>
        </div>
      </div>
    </div>
  );
}
