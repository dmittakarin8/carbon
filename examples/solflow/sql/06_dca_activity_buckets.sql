-- DCA Activity Buckets: Time-series storage for sparkline visualization
--
-- Purpose: Store 1-minute bucketed DCA buy counts for historical sparkline rendering
--
-- Retention: 1 hour of data (60 buckets × 60 seconds)
-- Cleanup: Automatic pruning of buckets older than 2 hours
--
-- Phase 7: DCA Sparkline Foundation (feature/dca-sparkline-backend)

CREATE TABLE IF NOT EXISTS dca_activity_buckets (
    mint TEXT NOT NULL,
    bucket_timestamp INTEGER NOT NULL,  -- Unix timestamp floored to 60s boundary
    buy_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mint, bucket_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_dca_buckets_timestamp
    ON dca_activity_buckets (bucket_timestamp);

CREATE INDEX IF NOT EXISTS idx_dca_buckets_mint_timestamp
    ON dca_activity_buckets (mint, bucket_timestamp);
