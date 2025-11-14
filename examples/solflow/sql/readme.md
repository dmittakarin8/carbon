# Solflow Aggregate-Only SQLite Schema

This folder contains the canonical DDL for the Solflow aggregate-only database.

The system uses an in-memory rolling-window aggregator and stores only
aggregated metrics and signal events in SQLite. Raw trades are never stored.

## Files

- `00_token_metadata.sql`  
  One row per token mint. Stores symbol, name, decimals, launch platform,
  and timestamps. Used by all UIs and the aggregator.

- `01_mint_blocklist.sql`  
  Maintains a blacklist of mints. The aggregator MUST check this table before
  writing signals. UIs MUST filter out blocked mints unless explicitly showing them.

- `02_token_aggregates.sql`  
  The core rolling-window table. Stores 1m/5m/15m net flows, counts, unique
  wallets, and price/market cap data. Updated continuously by the aggregator.

- `03_token_signals.sql`  
  Append-only event table for all signals (BREAKOUT, FOCUSED, SURGE, BOT_DROPOFF).
  Used for Discord alerts and historical data analysis.

- `04_system_metrics.sql`  
  Optional table for system-wide health/heartbeat metrics.

## Agent Rules

When generating code that interacts with SQLite:
- Agents must reference these DDL files as the source of truth.
- DO NOT create new schema outside this folder without approval.
- DO NOT modify table structure unless explicitly instructed.
- Use the exact column names defined in these SQL files.
- Always check `mint_blocklist` before writing signals.
- Aggregator must write only to:
    - `token_aggregates`
    - `token_signals`
    - `system_metrics`
- Metadata fetchers write to `token_metadata`.
