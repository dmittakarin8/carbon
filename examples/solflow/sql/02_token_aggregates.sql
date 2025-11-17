CREATE TABLE IF NOT EXISTS token_aggregates (
    mint                    TEXT PRIMARY KEY,

    source_program          TEXT NOT NULL,
    last_trade_timestamp    INTEGER,

    price_usd               REAL,
    price_sol               REAL,
    market_cap_usd          REAL,

    net_flow_60s_sol        REAL,
    net_flow_300s_sol       REAL,
    net_flow_900s_sol       REAL,
    net_flow_3600s_sol      REAL,
    net_flow_7200s_sol      REAL,
    net_flow_14400s_sol     REAL,

    buy_count_60s           INTEGER,
    sell_count_60s          INTEGER,

    buy_count_300s          INTEGER,
    sell_count_300s         INTEGER,

    buy_count_900s          INTEGER,
    sell_count_900s         INTEGER,

    unique_wallets_300s     INTEGER,
    bot_trades_300s         INTEGER,
    bot_wallets_300s        INTEGER,

    avg_trade_size_300s_sol REAL,
    volume_300s_sol         REAL,

    -- DCA buy counts (rolling windows)
    dca_buys_60s            INTEGER NOT NULL DEFAULT 0,
    dca_buys_300s           INTEGER NOT NULL DEFAULT 0,
    dca_buys_900s           INTEGER NOT NULL DEFAULT 0,
    dca_buys_3600s          INTEGER NOT NULL DEFAULT 0,
    dca_buys_14400s         INTEGER NOT NULL DEFAULT 0,

    updated_at              INTEGER NOT NULL,
    created_at              INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_updated_at
    ON token_aggregates (updated_at);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_source_program
    ON token_aggregates (source_program);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_netflow_300s
    ON token_aggregates (net_flow_300s_sol DESC);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_dca_buys_3600s
    ON token_aggregates (dca_buys_3600s DESC);
