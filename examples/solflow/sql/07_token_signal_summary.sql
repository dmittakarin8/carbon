-- token_signal_summary: Persistent signal scoring and pattern classification
-- This table maintains a rolling summary of token behavior with persistence scores,
-- pattern tags, and confidence levels. Updated periodically by the scoring engine.

CREATE TABLE IF NOT EXISTS token_signal_summary (
    token_address       TEXT PRIMARY KEY,
    
    -- Core scoring metrics
    persistence_score   INTEGER NOT NULL DEFAULT 0,  -- 0-10 scale
    pattern_tag         TEXT,                        -- ACCUMULATION, MOMENTUM, DISTRIBUTION, WASHOUT, NOISE
    confidence          TEXT,                        -- LOW, MEDIUM, HIGH
    
    -- Appearance tracking
    appearance_24h      INTEGER NOT NULL DEFAULT 0,  -- Count of appearances in top lists (24h)
    appearance_72h      INTEGER NOT NULL DEFAULT 0,  -- Count of appearances in top lists (72h)
    
    -- Metadata
    updated_at          INTEGER NOT NULL             -- Unix timestamp
    
    -- Note: No foreign key constraint on token_address
    -- Rationale: Token aggregates can exist before metadata is fetched
    -- This allows persistence scoring to run independently of metadata enrichment
);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_persistence_score
    ON token_signal_summary (persistence_score DESC);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_pattern_tag
    ON token_signal_summary (pattern_tag);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_updated_at
    ON token_signal_summary (updated_at DESC);
