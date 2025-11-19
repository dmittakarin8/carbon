'use client';

import { useEffect, useState } from 'react';

/**
 * DcaSparkline - Time-series DCA activity visualization
 * 
 * Phase 7: DCA Sparkline Foundation (feature/dca-sparkline-backend)
 * 
 * Renders a true sparkline showing DCA BUY activity over the last 60 minutes
 * with 1-minute bucket resolution (60 data points).
 * 
 * Data source: dca_activity_buckets table (persistent time-series data)
 * 
 * Architecture:
 * - Backend: Writes 1-minute buckets to database on every flush cycle
 * - API: GET /api/dca-sparkline/[mint] returns last 60 buckets
 * - Frontend: Renders continuous sparkline with consistent bucket spacing
 * 
 * Features:
 * - True time-series visualization (not rolling-window snapshots)
 * - Handles sparse data gracefully (missing buckets = 0 activity)
 * - Persists across pipeline restarts (database-backed)
 * - Automatic cleanup (buckets older than 2 hours removed every 5 minutes)
 */

interface DcaSparklineProps {
  mint: string;
  width?: number;
  height?: number;
}

interface DataPoint {
  timestamp: number;
  buyCount: number;
}

export default function DcaSparkline({
  mint,
  width = 100,
  height = 20,
}: DcaSparklineProps) {
  const [dataPoints, setDataPoints] = useState<DataPoint[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchData() {
      try {
        const response = await fetch(`/api/dca-sparkline/${mint}`);
        if (!response.ok) {
          throw new Error('Failed to fetch sparkline data');
        }
        const data = await response.json();
        setDataPoints(data.dataPoints || []);
      } catch (error) {
        console.error('Error fetching DCA sparkline:', error);
        setDataPoints([]);
      } finally {
        setLoading(false);
      }
    }

    fetchData();
    // Refresh every 60 seconds
    const interval = setInterval(fetchData, 60000);
    return () => clearInterval(interval);
  }, [mint]);

  if (loading) {
    return (
      <div
        className="flex items-center justify-center text-gray-500 text-xs"
        style={{ width, height }}
      >
        ...
      </div>
    );
  }

  if (dataPoints.length === 0) {
    return (
      <div
        className="flex items-center justify-center text-gray-500 text-xs"
        style={{ width, height }}
      >
        —
      </div>
    );
  }

  // Fill gaps: create 60-bucket array with 0 for missing buckets
  const now = Math.floor(Date.now() / 1000);
  const startTime = Math.floor((now - 3600) / 60) * 60; // Floor to minute boundary
  const bucketArray = new Array(60).fill(0);
  
  dataPoints.forEach(point => {
    const bucketIndex = Math.floor((point.timestamp - startTime) / 60);
    if (bucketIndex >= 0 && bucketIndex < 60) {
      bucketArray[bucketIndex] = point.buyCount;
    }
  });

  const maxValue = Math.max(...bucketArray, 1); // Avoid division by zero
  const color = '#10B981'; // Green for DCA activity (positive signal)

  return (
    <div
      className="flex items-end justify-between gap-px"
      style={{ width, height }}
      title={`DCA activity over last 60 minutes (${dataPoints.length} active buckets)`}
    >
      {bucketArray.map((value, index) => {
        const barHeight = (value / maxValue) * height;
        return (
          <div
            key={index}
            className="flex-1"
            style={{
              height: `${Math.max(barHeight, 1)}px`,
              backgroundColor: value > 0 ? color : '#E5E7EB',
              opacity: value > 0 ? 1 : 0.3,
            }}
          />
        );
      })}
    </div>
  );
}

