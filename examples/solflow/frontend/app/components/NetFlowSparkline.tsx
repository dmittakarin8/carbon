'use client';

import { useEffect, useState } from 'react';
import { LineChart, Line, ResponsiveContainer, XAxis, YAxis } from 'recharts';
import { SparklineResponse } from '@/lib/types';

interface NetFlowSparklineProps {
  mint: string;
  width?: number;
  height?: number;
}

export default function NetFlowSparkline({
  mint,
  width = 100,
  height = 20,
}: NetFlowSparklineProps) {
  const [data, setData] = useState<SparklineResponse['dataPoints']>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchData() {
      try {
        const response = await fetch(`/api/sparkline/${mint}`);
        if (response.ok) {
          const result: SparklineResponse = await response.json();
          setData(result.dataPoints);
        }
      } catch (error) {
        console.error('Error fetching sparkline data:', error);
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

  // Determine color based on trend (positive/negative)
  const firstValue = data[0]?.netFlowSol ?? 0;
  const lastValue = data[data.length - 1]?.netFlowSol ?? 0;
  const isPositive = lastValue >= firstValue;
  const color = isPositive ? '#10B981' : '#EF4444';

  // Transform data for Recharts (needs array of objects with value property)
  const chartData = data.map((point) => ({
    value: point.netFlowSol,
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

