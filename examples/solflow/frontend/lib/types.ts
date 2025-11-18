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
  // Phase 6: DCA Rolling Windows (from token_aggregates)
  dcaBuys60s: number;      // DCA buys in last 60 seconds
  dcaBuys300sWindow: number;   // DCA buys in last 300 seconds
  dcaBuys900s: number;     // DCA buys in last 900 seconds
  dcaBuys3600s: number;    // DCA buys in last 3600 seconds (1 hour)
  dcaBuys14400s: number;   // DCA buys in last 14400 seconds (4 hours)
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

export interface DcaSparklineDataPoint {
  timestamp: number;
  buyCount: number;
}

export interface DcaSparklineResponse {
  dataPoints: DcaSparklineDataPoint[];
}

export interface TokenMetadata {
  mint: string;
  name?: string;
  symbol?: string;
  imageUrl?: string;
  priceUsd?: number;
  marketCap?: number;
  followPrice: boolean;
  blocked: boolean;
  updatedAt: number;
}

export interface MetadataResponse {
  metadata: TokenMetadata | null;
}

