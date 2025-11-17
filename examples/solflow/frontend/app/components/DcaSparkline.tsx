'use client';

/**
 * DcaSparkline - Static DCA activity visualization
 * 
 * Phase 6: DCA Rolling Windows (feature/dca-rolling-windows)
 * 
 * Replaced dynamic sparkline with static bar chart showing DCA activity
 * across 5 time windows (60s, 300s, 900s, 3600s, 14400s).
 * 
 * Data source: token_aggregates.dca_buys_* columns (no database query needed)
 * 
 * Why static?
 * - Eliminates per-token database queries (1 query per mint → 0 queries)
 * - Uses pre-aggregated data from pipeline (already computed)
 * - Reduces page load time significantly (40 tokens × 100ms = 4s saved)
 * - More accurate: reflects actual pipeline state, not reconstructed data
 */

interface DcaSparklineProps {
  dcaBuys60s: number;
  dcaBuys300s: number;
  dcaBuys900s: number;
  dcaBuys3600s: number;
  dcaBuys14400s: number;
  width?: number;
  height?: number;
}

export default function DcaSparkline({
  dcaBuys60s,
  dcaBuys300s,
  dcaBuys900s,
  dcaBuys3600s,
  dcaBuys14400s,
  width = 100,
  height = 20,
}: DcaSparklineProps) {
  // Build 5-bar mini chart (60s, 300s, 900s, 3600s, 14400s)
  const values = [dcaBuys60s, dcaBuys300s, dcaBuys900s, dcaBuys3600s, dcaBuys14400s];
  const maxValue = Math.max(...values, 1); // Avoid division by zero
  const hasActivity = values.some(v => v > 0);

  if (!hasActivity) {
    return (
      <div
        className="flex items-center justify-center text-gray-500 text-xs"
        style={{ width, height }}
      >
        —
      </div>
    );
  }

  // Color: green for DCA activity (positive signal)
  const color = '#10B981';

  return (
    <div
      className="flex items-end justify-between gap-0.5"
      style={{ width, height }}
    >
      {values.map((value, index) => {
        const barHeight = (value / maxValue) * height;
        return (
          <div
            key={index}
            className="flex-1"
            style={{
              height: `${barHeight}px`,
              backgroundColor: value > 0 ? color : '#E5E7EB',
              minHeight: value > 0 ? '2px' : '1px',
            }}
            title={`Window ${index + 1}: ${value} DCA buys`}
          />
        );
      })}
    </div>
  );
}

