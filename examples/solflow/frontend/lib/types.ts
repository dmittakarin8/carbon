export interface TokenMetrics {
  mint: string;
  netFlow60s: number;      // 1-minute net flow
  netFlow300s: number;     // 5-minute net flow (primary sort)
  netFlow900s: number;     // 15-minute net flow
  netFlow3600s: number;    // 1-hour net flow
  netFlow7200s: number;    // 2-hour net flow
  netFlow14400s: number;   // 4-hour net flow
  totalBuys300s: number;
  totalSells300s: number;
  dcaBuys300s: number;     // DCA buy count (JupiterDCA only)
  dcaNetFlow300s: number;  // DCA net flow (JupiterDCA only)
  maxUniqueWallets: number;
  totalVolume300s: number;
  lastUpdate: number;
}

export interface TokensResponse {
  tokens: TokenMetrics[];
}

export interface SparklineDataPoint {
  timestamp: number;
  netFlowSol: number;
}

export interface SparklineResponse {
  dataPoints: SparklineDataPoint[];
}

export interface BlockResponse {
  success: boolean;
  error?: string;
}

export interface TokenSignal {
  signalType: string;
  createdAt: number;
}

