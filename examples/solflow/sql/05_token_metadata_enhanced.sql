-- Phase 7: Token Metadata Enhancement
-- Add DexScreener integration fields and user controls
--
-- This migration extends the existing token_metadata table with:
-- - Market data fields (price_usd, market_cap, image_url)
-- - User control flags (follow_price, blocked)
--
-- All new columns have default values for backward compatibility

-- Add new columns to existing token_metadata table
ALTER TABLE token_metadata ADD COLUMN image_url TEXT;
ALTER TABLE token_metadata ADD COLUMN price_usd REAL;
ALTER TABLE token_metadata ADD COLUMN market_cap REAL;
ALTER TABLE token_metadata ADD COLUMN follow_price INTEGER NOT NULL DEFAULT 0;
ALTER TABLE token_metadata ADD COLUMN blocked INTEGER NOT NULL DEFAULT 0;

-- Indexes for filtering and user controls
CREATE INDEX IF NOT EXISTS idx_token_metadata_blocked 
    ON token_metadata (blocked);

CREATE INDEX IF NOT EXISTS idx_token_metadata_follow_price 
    ON token_metadata (follow_price);
