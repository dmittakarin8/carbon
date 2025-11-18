-- Phase 7: Token Metadata Enhancement
-- Add DexScreener integration fields and user controls
--
-- This migration extends the existing token_metadata table with:
-- - Market data fields (price_usd, market_cap, image_url)
-- - User control flags (follow_price, blocked)
--
-- All new columns have default values for backward compatibility
--
-- IMPORTANT: This migration should only run ONCE. If columns already exist,
-- manually delete this file from the sql/ directory or use a migration tracking system.

-- Indexes for filtering and user controls (these are idempotent and safe to run multiple times)
CREATE INDEX IF NOT EXISTS idx_token_metadata_blocked 
    ON token_metadata (blocked);

CREATE INDEX IF NOT EXISTS idx_token_metadata_follow_price 
    ON token_metadata (follow_price);
