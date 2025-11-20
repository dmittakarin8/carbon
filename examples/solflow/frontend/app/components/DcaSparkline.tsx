'use client';

/**
 * DcaSparkline - Time-series DCA activity visualization
 * 
 * Phase 7: DCA Sparkline Foundation (feature/dca-sparkline-backend)
 * Phase 8: Batched Dashboard Architecture (feature/batched-dashboard-architecture)
 * 
 * Renders a true sparkline showing DCA BUY activity over the last 60 minutes
 * with 1-minute bucket resolution (60 data points).
 * 
 * Data source: dca_activity_buckets table (persistent time-series data)
 * 
 * Architecture:
 * - Backend: Writes 1-minute buckets to database on every flush cycle
 * - API: GET /api/dashboard returns ALL sparklines in one batched request
 * - Frontend: Receives data as props (no per-component fetching)
 * 
 * Features:
 * - True time-series visualization (not rolling-window snapshots)
 * - Handles sparse data gracefully (missing buckets = 0 activity)
 * - Persists across pipeline restarts (database-backed)
 * - Automatic cleanup (buckets older than 2 hours removed every 5 minutes)
 * - Zero N+1 queries (data comes from batched endpoint)
 */

interface DcaSparklineProps {
  mint: string;
  dataPoints: Array<{ timestamp: number; buyCount: number }>;
  width?: number;
  height?: number;
}

export default function DcaSparkline({
  mint,
  dataPoints,
  width = 100,
  height = 20,
}: DcaSparklineProps) {
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

