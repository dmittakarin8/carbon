CREATE TABLE IF NOT EXISTS token_signals (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    mint            TEXT NOT NULL,
    signal_type     TEXT NOT NULL,
    window_seconds  INTEGER NOT NULL,
    severity        INTEGER NOT NULL DEFAULT 1,
    score           REAL,
    details_json    TEXT,
    created_at      INTEGER NOT NULL,

    sent_to_discord INTEGER NOT NULL DEFAULT 0,
    seen_in_terminal INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_token_signals_mint_created
    ON token_signals (mint, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_token_signals_type_created
    ON token_signals (signal_type, created_at DESC);
