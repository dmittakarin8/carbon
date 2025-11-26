-- Phase X: Price Monitoring Optimization
-- Add composite index for backend price update queries
--
-- This index optimizes the query used by the price update task:
-- SELECT mint, updated_at FROM token_metadata WHERE follow_price = 1
--
-- The partial index (WHERE follow_price = 1) reduces index size and improves
-- query performance for the most common access pattern.

CREATE INDEX IF NOT EXISTS idx_token_metadata_follow_updated
    ON token_metadata (follow_price, updated_at)
    WHERE follow_price = 1;
