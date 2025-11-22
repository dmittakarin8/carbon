import { 
  TokenMetrics, 
  TokenMetadata, 
  TokenSignal, 
  SparklineDataPoint, 
  DcaSparklineDataPoint,
  TokenSignalSummary 
} from './types';

/**
 * Dashboard Data Interface - Complete dashboard state from batched API
 */
export interface DashboardData {
  tokens: TokenMetrics[];
  metadata: Record<string, TokenMetadata>;
  signals: Record<string, TokenSignal | null>;
  signalSummaries: Record<string, TokenSignalSummary | null>;
  sparklines: Record<string, SparklineDataPoint[]>;
  dcaSparklines: Record<string, DcaSparklineDataPoint[]>;
  counts: {
    followed: number;
    blocked: number;
  };
  followedTokens: string[];
  blockedTokens: string[];
}

/**
 * Fetch complete dashboard data from batched endpoint
 * 
 * This replaces all individual API calls (tokens, metadata, signals, sparklines)
 * with a single efficient request that returns all data needed for the UI.
 * 
 * Should be called:
 * - Once on initial load
 * - Every 10 seconds via setInterval
 */
export async function fetchDashboard(): Promise<DashboardData> {
  const response = await fetch('/api/dashboard');
  
  if (!response.ok) {
    throw new Error(`Failed to fetch dashboard: ${response.statusText}`);
  }
  
  return response.json();
}

/**
 * Centralized error handling for dashboard fetch
 */
export async function fetchDashboardSafe(): Promise<DashboardData | null> {
  try {
    return await fetchDashboard();
  } catch (error) {
    console.error('Error fetching dashboard:', error);
    return null;
  }
}
