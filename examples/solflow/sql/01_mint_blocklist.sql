CREATE TABLE IF NOT EXISTS mint_blocklist (
    mint            TEXT PRIMARY KEY,
    reason          TEXT,
    blocked_by      TEXT,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER
);

CREATE INDEX IF NOT EXISTS idx_mint_blocklist_created_at
    ON mint_blocklist (created_at);
