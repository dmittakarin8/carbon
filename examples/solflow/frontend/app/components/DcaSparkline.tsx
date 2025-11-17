'use client';

import { useEffect, useState } from 'react';
import { LineChart, Line, ResponsiveContainer, XAxis, YAxis } from 'recharts';
import { DcaSparklineResponse } from '@/lib/types';

interface DcaSparklineProps {
  mint: string;
  width?: number;
  height?: number;
}

export default function DcaSparkline({
  mint,
  width = 100,
  height = 20,
}: DcaSparklineProps) {
  const [data, setData] = useState<DcaSparklineResponse['dataPoints']>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchData() {
      try {
        const response = await fetch(`/api/dca-sparkline/${mint}`);
        if (response.ok) {
          const result: DcaSparklineResponse = await response.json();
          setData(result.dataPoints);
        }
      } catch (error) {
        console.error('Error fetching DCA sparkline data:', error);
      } finally {
        setLoading(false);
      }
    }

    fetchData();
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

  if (data.length === 0) {
    return (
      <div
        className="flex items-center justify-center text-gray-500 text-xs"
        style={{ width, height }}
      >
        —
      </div>
    );
  }

  // For DCA buys, we always use green color (positive activity)
  const color = '#10B981';

  // Transform data for Recharts (needs array of objects with value property)
  const chartData = data.map((point) => ({
    value: point.buyCount,
  }));

  return (
    <ResponsiveContainer width={width} height={height}>
      <LineChart data={chartData} margin={{ top: 0, right: 0, bottom: 0, left: 0 }}>
        <XAxis hide={true} />
        <YAxis hide={true} />
        <Line
          type="monotone"
          dataKey="value"
          stroke={color}
          strokeWidth={1.5}
          dot={false}
          isAnimationActive={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
}

