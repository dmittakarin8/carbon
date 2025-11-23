CREATE TABLE IF NOT EXISTS token_metadata (
    mint                TEXT PRIMARY KEY,
    symbol              TEXT,
    name                TEXT,
    decimals            INTEGER NOT NULL,
    launch_platform     TEXT,
    pair_created_at     INTEGER,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    CHECK (decimals >= 0 AND decimals <= 18)
);

CREATE INDEX IF NOT EXISTS idx_token_metadata_created_at
    ON token_metadata (created_at);
