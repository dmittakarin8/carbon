-- token_signal_summary: Persistent signal scoring and pattern classification (Postgres version)
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
    updated_at          BIGINT NOT NULL,             -- Unix timestamp
    
    -- Foreign key constraint (token_address references token_metadata.mint)
    FOREIGN KEY (token_address) REFERENCES token_metadata(mint) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_persistence_score
    ON token_signal_summary (persistence_score DESC);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_pattern_tag
    ON token_signal_summary (pattern_tag);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_updated_at
    ON token_signal_summary (updated_at DESC);

-- Add constraint for persistence_score range (0-10)
ALTER TABLE token_signal_summary 
    ADD CONSTRAINT check_persistence_score_range 
    CHECK (persistence_score >= 0 AND persistence_score <= 10);

-- Add constraint for valid pattern_tag values
ALTER TABLE token_signal_summary 
    ADD CONSTRAINT check_pattern_tag_values 
    CHECK (pattern_tag IS NULL OR pattern_tag IN ('ACCUMULATION', 'MOMENTUM', 'DISTRIBUTION', 'WASHOUT', 'NOISE'));

-- Add constraint for valid confidence values
ALTER TABLE token_signal_summary 
    ADD CONSTRAINT check_confidence_values 
    CHECK (confidence IS NULL OR confidence IN ('LOW', 'MEDIUM', 'HIGH'));
