# SolFlow Frontend Architecture Guide

**Created:** 2025-11-16  
**Purpose:** Complete architectural documentation for frontend developers  
**Status:** Current (Pipeline-Based Architecture)

---

## Table of Contents

1. [Overview](#overview)
2. [Components & Binaries](#components--binaries)
3. [Pipeline Architecture (Current)](#pipeline-architecture-current)
4. [SQLite Schema](#sqlite-schema)
5. [Streamers → Pipeline → Tables Mapping](#streamers--pipeline--tables-mapping)
6. [Frontend Integration Guidance](#frontend-integration-guidance)
7. [Legacy vs New](#legacy-vs-new)
8. [Verification & Open Questions](#verification--open-questions)

---

## Overview

### Executive Summary

SolFlow is a real-time Solana DEX analytics system built on the Carbon framework. The architecture follows a **pipeline-based design** where streamer binaries ingest on-chain trades, process them through an in-memory rolling-window aggregator, and persist aggregated metrics to SQLite.

**Key Mental Model:** `Streamers → PipelineEngine → SQLite (token_aggregates + token_signals)`

**NOT:** `Streamers → trades table → aggregator → output` (this is the legacy/separate path)

### Critical Architectural Decision

The codebase contains **two distinct processing paths**:

1. **PRIMARY (Current):** `pipeline_runtime` binary with integrated streamers
   - In-memory rolling windows (60s/300s/900s)
   - Writes to `token_aggregates` and `token_signals` tables
   - ✅ **This is what the frontend should use**

2. **SEPARATE (Standalone):** `aggregator` binary with `aggregator_core` module
   - Reads from `trades` table (raw events)
   - Performs PumpSwap × Jupiter DCA correlation analysis
   - Outputs enriched metrics (JSONL or separate SQLite path)
   - ⚠️ **This is functional but NOT part of the primary runtime**

### What Frontend Developers Need to Know

**Use These Tables:**
- `token_aggregates` - Primary metrics source (rolling-window data)
- `token_signals` - Real-time trading signals and alerts
- `token_metadata` - Token information (symbol, decimals, name)
- `mint_blocklist` - Blocked/filtered tokens

**Avoid These Tables:**
- `trades` - Raw trade events (transitional, not aggregated)
  - Only used by standalone `aggregator` and `solflow_signals` binaries
  - Will cause unbounded queries and poor performance
  - Not written by primary pipeline runtime

---

## Components & Binaries

### Current Pipeline Components (ACTIVE)

#### 1. `pipeline_runtime` (Binary)

**File:** `src/bin/pipeline_runtime.rs` (171 lines)

**Purpose:** Main orchestrator for the pipeline-based architecture

**Responsibilities:**
- Spawns all 4 streamer binaries with dual-channel integration
- Creates shared `PipelineEngine` instance (in-memory aggregator)
- Initializes SQLite database with schema migrations
- Spawns background ingestion task (processes trade channel)
- Manages unified flush loop (writes to database every 5 seconds)

**Runtime Flow:**
1. Load configuration from environment (`ENABLE_PIPELINE=true` required)
2. Initialize SQLite database at `SOLFLOW_DB_PATH` (default: `/var/lib/solflow/solflow.db`)
3. Run schema migrations from `/sql` directory
4. Create mpsc channel for trade events (10,000 buffer)
5. Spawn 4 streamers (PumpSwap, BonkSwap, Moonshot, Jupiter DCA)
6. Each streamer sends trades to pipeline channel via `try_send()`
7. Ingestion loop processes trades through `PipelineEngine`
8. Periodic flush (every 5s) computes metrics and writes to SQLite

**Key Environment Variables:**
- `ENABLE_PIPELINE=true` - Master switch (required)
- `SOLFLOW_DB_PATH` - Database location
- `AGGREGATE_FLUSH_INTERVAL_MS` - Flush frequency (default: 5000ms)
- `STREAMER_CHANNEL_BUFFER` - Channel size (default: 10000)

**Status:** ✅ **This is the primary runtime binary**

---

#### 2. Streamer Binaries (4 Active)

All streamers follow the same pattern defined in `src/streamer_core/lib.rs`:

| Binary | Program ID | Program Name | Output Path (JSONL) | Pipeline Channel |
|--------|-----------|--------------|---------------------|------------------|
| `pumpswap_streamer` | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | PumpSwap | `streams/pumpswap/events.jsonl` | ✅ Yes (via `pipeline_tx`) |
| `bonkswap_streamer` | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | BonkSwap | `streams/bonkswap/events.jsonl` | ✅ Yes |
| `moonshot_streamer` | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | Moonshot | `streams/moonshot/events.jsonl` | ✅ Yes |
| `jupiter_dca_streamer` | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | JupiterDCA | `streams/jupiter_dca/events.jsonl` | ✅ Yes |

**Common Behavior:**
- Connect to Yellowstone gRPC stream (Solana transaction feed)
- Extract trades from `TransactionStatusMeta` (metadata-based, no instruction decoding)
- Dual-channel writes:
  - **Channel 1:** Pipeline mpsc channel (non-blocking `try_send()`)
  - **Channel 2:** JSONL files (optional, legacy compatibility)

**Trade Detection Logic:**
```rust
// From TransactionStatusMeta (pre/post balances)
let sol_deltas = extract_sol_changes(&metadata.meta);
let token_deltas = extract_token_changes(&metadata.meta);

// Determine BUY/SELL based on SOL flow direction
if sol_is_outflow { action = "BUY" }   // User spent SOL
if sol_is_inflow  { action = "SELL" }  // User received SOL
```

**Key Fields Extracted:**
- `mint` - Token address being traded
- `sol_amount` - SOL value of trade
- `token_amount` - Token quantity
- `token_decimals` - Token decimal precision
- `user_account` - Trader wallet address
- `action` - "BUY" or "SELL"
- `timestamp` - Unix epoch seconds
- `signature` - Transaction signature

**Status:** ✅ **These are the current data ingestion components**

---

#### 3. `PipelineEngine` (Core Module)

**File:** `src/pipeline/engine.rs` (937 lines)

**Purpose:** In-memory rolling-window aggregator and signal detector

**Architecture:**
```
PipelineEngine
├── states: HashMap<Mint, TokenRollingState>
│   └── Per-token rolling windows (60s/300s/900s)
├── last_bot_counts: HashMap<Mint, i32>
│   └── Bot detection state tracking
├── last_signal_state: HashMap<Mint, HashMap<SignalType, bool>>
│   └── Signal deduplication (prevents duplicate signals)
└── metadata_cache: HashMap<Mint, TokenMetadata>
    └── Token metadata for enrichment
```

**Key Methods:**

**`process_trade(trade: TradeEvent)`**
- Adds trade to appropriate rolling windows
- Updates per-token state
- Tracks unique wallets
- Evicts trades older than 900s

**`compute_metrics(mint: &str, now: i64)`**
- Returns: `(RollingMetrics, Vec<TokenSignal>, AggregatedTokenState)`
- Computes net flow across all windows
- Detects trading signals (BREAKOUT, SURGE, FOCUSED, BOT_DROPOFF)
- Builds aggregate state for database persistence

**Signal Detection Logic:**

| Signal | Condition | Meaning |
|--------|-----------|---------|
| `BREAKOUT` | `net_flow_300s > 50 SOL` AND `buy_count_300s > 10` | Large buying pressure |
| `SURGE` | `buy_count_60s > 5` AND `net_flow_60s > 10 SOL` | Rapid accumulation |
| `FOCUSED` | `unique_wallets_300s < 5` AND `volume_300s > 100 SOL` | Concentrated buying (possible insider activity) |
| `BOT_DROPOFF` | Previous bot count > 5 AND current bot count ≤ 2 | Bot activity ceased (potential organic interest) |

**Memory Management:**
- Auto-eviction of trades older than 900s (15 minutes)
- Constant memory footprint per active token
- Target: <300 MB for 50 active tokens

**Status:** ✅ **This is the core analytics engine**

---

#### 4. Pipeline Ingestion (Background Task)

**File:** `src/pipeline/ingestion.rs` (327 lines)

**Purpose:** Unified flush loop that processes trades and writes to database

**Architecture:**
```
Ingestion Loop
├── Trade Receiver (mpsc channel)
│   └── Receives TradeEvent from all 4 streamers
├── Flush Timer (every 5 seconds)
│   ├── Lock PipelineEngine (single lock acquisition)
│   ├── Compute metrics for all active mints
│   ├── Release lock
│   └── Write to database (engine unlocked)
└── Health Monitoring
    └── Channel utilization, throughput metrics
```

**Critical Design Decision:**
- **Single lock acquisition per flush cycle** (not per mint)
- Compute all metrics while holding lock
- Release lock BEFORE database writes
- Non-blocking design prevents engine stalls

**Performance Characteristics:**
- Flush interval: 5000ms (configurable via `AGGREGATE_FLUSH_INTERVAL_MS`)
- Typical flush duration: 20-50ms for 50 active tokens
- Channel warning: >50% capacity triggers alert
- Throughput logging: Every 10 seconds

**Status:** ✅ **This is the primary database writer**

---

### Separate/Standalone Components

#### 5. `aggregator` Binary (STANDALONE)

**File:** `src/bin/aggregator.rs` (296 lines)

**Purpose:** PumpSwap × Jupiter DCA correlation analyzer

**Key Distinction:** ⚠️ **This is NOT part of `pipeline_runtime`**

**Architecture:**
```
Aggregator Binary (runs separately)
├── SqliteTradeReader (reads from `trades` table)
├── TimeWindowAggregator (15m/1h/2h/4h windows)
├── CorrelationEngine (matches PumpSwap BUYs with DCA fills)
├── SignalScorer (uptrend score computation)
├── SignalDetector (UPTREND/ACCUMULATION signals)
└── AggregatorWriter (outputs to JSONL or separate SQLite)
```

**Functionality:**
- Reads raw trade events from `trades` table (not in-memory)
- Computes DCA overlap percentage (PumpSwap BUYs within ±60s of DCA fills)
- Emits `ACCUMULATION` signal when `dca_overlap > 25%`
- Outputs enriched metrics to `streams/aggregates/*.jsonl`

**Why This Exists:**
- Implements advanced correlation logic not in primary pipeline
- Useful for backtesting and research
- Can run offline against historical `trades` data

**Why NOT Primary:**
- Requires `trades` table to be populated (separate streamer mode)
- Not integrated into `pipeline_runtime`
- Different output schema than `token_aggregates`

**Status:** ⚠️ **Functional but separate from primary pipeline**

**Recommendation:** If you need DCA correlation features, this must run as a **separate process** alongside `pipeline_runtime`.

---

#### 6. `solflow_signals` Binary (TRANSITIONAL)

**File:** `src/bin/solflow_signals.rs` (304 lines)

**Purpose:** Standalone signal analyzer using `trades` table

**Architecture:**
- Queries `trades` table every 10 seconds
- Computes "REAL DEMAND BREAKOUT" scoring model
- Combines PumpSwap flow + DCA events + Aggregator flow + wallet diversity
- Writes to `signals` table (separate from `token_signals`)

**Status:** ⚠️ **Transitional component, may be deprecated**

**Note:** This appears to be an older analytics engine. Check with backend team if this is still required.

---

#### 7. `grpc_verify` Binary (UTILITY)

**File:** `src/bin/grpc_verify.rs` (597 lines)

**Purpose:** gRPC connection diagnostics and manual trade monitoring

**Use Cases:**
- Verify Yellowstone gRPC connectivity
- Debug trade extraction logic
- View raw transaction metadata

**Status:** ✅ **Utility tool for debugging**

---

### Module Organization

**Pipeline Modules (Current Architecture):**
```
src/pipeline/
├── mod.rs            - Public API exports
├── engine.rs         - PipelineEngine orchestrator
├── state.rs          - TokenRollingState (per-mint windows)
├── types.rs          - Data structures (TradeEvent, AggregatedTokenState)
├── db.rs             - SQLite writer trait + implementation
├── signals.rs        - Signal type definitions
├── ingestion.rs      - Channel processing and unified flush
├── config.rs         - Environment variable loading
├── windows.rs        - Rolling window abstraction (unused)
└── blocklist.rs      - Blocklist trait (stubbed)
```

**Streamer Core Modules:**
```
src/streamer_core/
├── lib.rs            - Main streaming logic (TradeProcessor)
├── config.rs         - Streamer configuration
├── balance_extractor.rs - SOL/token balance changes
├── trade_detector.rs - Trade identification
├── sqlite_writer.rs  - Writes to `trades` table (optional backend)
├── output_writer.rs  - JSONL writer (legacy compatibility)
└── grpc_client.rs    - Yellowstone gRPC client
```

**Aggregator Core Modules (Separate System):**
```
src/aggregator_core/
├── mod.rs            - Public API exports
├── normalizer.rs     - Trade struct parsing
├── sqlite_reader.rs  - Reads `trades` table (ID-based cursor)
├── window.rs         - TimeWindowAggregator (15m/1h/2h/4h)
├── correlator.rs     - PumpSwap × DCA matching
├── scorer.rs         - Uptrend score computation
├── detector.rs       - Signal detection (UPTREND/ACCUMULATION)
├── writer.rs         - Unified writer router
├── jsonl_writer.rs   - JSONL output backend
└── sqlite_writer.rs  - SQLite output backend
```

---

## Pipeline Architecture (Current)

### End-to-End Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│          Yellowstone gRPC (Solana Blockchain)                │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         4 Streamer Binaries (Carbon Pipelines)               │
│  ┌──────────┬──────────┬──────────┬─────────────────┐      │
│  │ PumpSwap │ BonkSwap │ Moonshot │ Jupiter DCA     │      │
│  └──────────┴──────────┴──────────┴─────────────────┘      │
│                                                               │
│  Each streamer:                                               │
│  1. Extracts trades from TransactionStatusMeta               │
│  2. try_send() TradeEvent to pipeline channel (primary)      │
│  3. Optional: Write to JSONL (legacy, can be disabled)       │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         mpsc::channel<TradeEvent> (10,000 buffer)            │
│         Non-blocking try_send() from all streamers           │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         Ingestion Task (src/pipeline/ingestion.rs)           │
│                                                               │
│  Main Loop:                                                   │
│  ├─ rx.recv() → PipelineEngine.process_trade()              │
│  └─ Flush Timer (every 5s):                                  │
│     ├─ Lock engine ONCE                                      │
│     ├─ compute_metrics() for all active mints                │
│     ├─ Release lock                                          │
│     └─ Write to SQLite (engine unlocked)                     │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         PipelineEngine (in-memory aggregator)                │
│                                                               │
│  Per-Token State (HashMap<Mint, TokenRollingState>):        │
│  ├─ trades_60s: Vec<TradeEvent>                             │
│  ├─ trades_300s: Vec<TradeEvent>                            │
│  ├─ trades_900s: Vec<TradeEvent>                            │
│  ├─ unique_wallets_300s: HashSet<String>                    │
│  └─ bot_detection state                                      │
│                                                               │
│  Every 5 seconds (flush):                                    │
│  ├─ Compute RollingMetrics (net flow, counts, etc.)         │
│  ├─ Detect signals (BREAKOUT, SURGE, FOCUSED, BOT_DROPOFF)  │
│  └─ Build AggregatedTokenState                              │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│         SQLite Database (rusqlite)                           │
│         /var/lib/solflow/solflow.db                          │
│                                                               │
│  ┌──────────────────────────────────────────────┐           │
│  │ token_aggregates (UPSERT on mint)            │           │
│  │ ─────────────────────────────────────────    │           │
│  │ - mint (PK), source_program                  │           │
│  │ - net_flow_60s_sol, net_flow_300s_sol, ...   │           │
│  │ - buy_count_60s, sell_count_60s, ...         │           │
│  │ - unique_wallets_300s, bot_trades_300s       │           │
│  │ - price_sol, market_cap_usd                  │           │
│  │ - updated_at, created_at                     │           │
│  └──────────────────────────────────────────────┘           │
│                                                               │
│  ┌──────────────────────────────────────────────┐           │
│  │ token_signals (INSERT append-only)           │           │
│  │ ─────────────────────────────────────────    │           │
│  │ - id (PK AUTOINCREMENT)                      │           │
│  │ - mint, signal_type, window_seconds          │           │
│  │ - severity, score, details_json              │           │
│  │ - created_at                                 │           │
│  │ - sent_to_discord, seen_in_terminal          │           │
│  └──────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────┘
```

### Dual-Channel Design

**Why Two Outputs?**

1. **Pipeline Channel (Primary):**
   - Non-blocking `try_send()` to mpsc channel
   - Feeds directly into `PipelineEngine` (in-memory)
   - Zero disk I/O for trade ingestion
   - ✅ **This is the primary data path**

2. **JSONL Files (Legacy/Backup):**
   - Optional file writes to `streams/{program}/events.jsonl`
   - Historical compatibility (some scripts may depend on it)
   - Can be disabled via environment variables
   - ⚠️ **Not used by primary pipeline**

**Configuration:**
```bash
# Disable JSONL writes (pipeline-only mode)
ENABLE_JSONL=false cargo run --release --bin pipeline_runtime
```

### Rolling Window Logic

**Window Sizes:**
- **60 seconds** (1 minute) - Rapid signal detection
- **300 seconds** (5 minutes) - Short-term momentum
- **900 seconds** (15 minutes) - Medium-term trend

**Eviction Strategy:**
- Every time a trade is processed, check all windows
- Remove trades where `now - trade.timestamp > window_size`
- Maintains constant memory per token

**Example Metrics Computed:**

For a token with 20 trades in the last 5 minutes (300s window):
```
net_flow_300s_sol = sum(buy_sol_amounts) - sum(sell_sol_amounts)
buy_count_300s = count(trades where action == "BUY")
sell_count_300s = count(trades where action == "SELL")
unique_wallets_300s = unique(trades.map(t => t.user_account))
volume_300s_sol = sum(all trades sol_amount)
avg_trade_size_300s_sol = volume_300s_sol / total_trade_count
```

---

## SQLite Schema

### Database Location

**Default:** `/var/lib/solflow/solflow.db`  
**Configuration:** `SOLFLOW_DB_PATH` environment variable

**Database Mode:**
- WAL (Write-Ahead Logging) enabled
- Multiple readers, single writer
- Auto-checkpointing every 1000 pages

### Schema Migrations

**Location:** `/sql/` directory (5 files)

**Migration Process:**
1. `pipeline_runtime` binary runs `run_schema_migrations()` on startup
2. All `.sql` files executed in alphabetical order (00_, 01_, 02_, ...)
3. All DDL uses `CREATE TABLE IF NOT EXISTS` (idempotent)
4. Schema version not tracked (rely on idempotency)

---

### PRIMARY TABLES (Frontend Use These)

#### 1. `token_aggregates` (Primary Metrics Source)

**File:** `sql/02_token_aggregates.sql`

**Purpose:** Rolling-window aggregate metrics for all active tokens

**Writer:** `PipelineEngine` via `SqliteAggregateWriter` (every 5 seconds)

**Operation:** UPSERT on `mint` (INSERT ... ON CONFLICT DO UPDATE)

**Schema:**
```sql
CREATE TABLE token_aggregates (
    -- Primary Key
    mint                    TEXT PRIMARY KEY,
    
    -- Source & Timing
    source_program          TEXT NOT NULL,        -- Program that generated this trade
    last_trade_timestamp    INTEGER,              -- Unix timestamp of most recent trade
    
    -- Price & Market Data
    price_usd               REAL,                 -- Token price in USD (future enrichment)
    price_sol               REAL,                 -- Token price in SOL (future enrichment)
    market_cap_usd          REAL,                 -- Market capitalization (future)
    
    -- Net Flow Metrics (Rolling Windows)
    net_flow_60s_sol        REAL,                 -- Net SOL inflow over 1 minute
    net_flow_300s_sol       REAL,                 -- Net SOL inflow over 5 minutes
    net_flow_900s_sol       REAL,                 -- Net SOL inflow over 15 minutes
    
    -- Trade Counts (60s Window)
    buy_count_60s           INTEGER,              -- Number of BUY trades in 1 minute
    sell_count_60s          INTEGER,              -- Number of SELL trades in 1 minute
    
    -- Trade Counts (300s Window)
    buy_count_300s          INTEGER,              -- BUY trades in 5 minutes
    sell_count_300s         INTEGER,              -- SELL trades in 5 minutes
    
    -- Trade Counts (900s Window)
    buy_count_900s          INTEGER,              -- BUY trades in 15 minutes
    sell_count_900s         INTEGER,              -- SELL trades in 15 minutes
    
    -- Advanced Metrics (300s Window)
    unique_wallets_300s     INTEGER,              -- Count of unique trader addresses
    bot_trades_300s         INTEGER,              -- Suspected bot trades (heuristic-based)
    bot_wallets_300s        INTEGER,              -- Unique bot wallet addresses
    
    -- Volume Metrics (300s Window)
    avg_trade_size_300s_sol REAL,                 -- Average SOL per trade
    volume_300s_sol         REAL,                 -- Total volume (buy + sell)
    
    -- Timestamps
    updated_at              INTEGER NOT NULL,     -- Last update timestamp
    created_at              INTEGER NOT NULL      -- First seen timestamp
);

-- Indexes for Common Queries
CREATE INDEX idx_token_aggregates_updated_at 
    ON token_aggregates (updated_at);

CREATE INDEX idx_token_aggregates_source_program 
    ON token_aggregates (source_program);

CREATE INDEX idx_token_aggregates_netflow_300s 
    ON token_aggregates (net_flow_300s_sol DESC);
```

**Key Columns for Frontend:**

| Column | Type | Description | Use Case |
|--------|------|-------------|----------|
| `mint` | TEXT | Token address (Solana pubkey) | Primary identifier |
| `net_flow_60s_sol` | REAL | 1-min net flow | Rapid trend detection |
| `net_flow_300s_sol` | REAL | 5-min net flow | Standard momentum indicator |
| `net_flow_900s_sol` | REAL | 15-min net flow | Longer trend confirmation |
| `buy_count_300s` | INTEGER | 5-min buy count | Activity level |
| `sell_count_300s` | INTEGER | 5-min sell count | Sell pressure |
| `unique_wallets_300s` | INTEGER | Unique traders | Diversity indicator (anti-wash trading) |
| `bot_trades_300s` | INTEGER | Bot trades detected | Filter low-quality volume |
| `volume_300s_sol` | REAL | Total 5-min volume | Overall activity |
| `updated_at` | INTEGER | Last update time | Data freshness check |

**Update Frequency:** Every 5 seconds (flush interval)

**Retention:** Indefinite (rows only exist for active tokens)

**Status:** ✅ **PRIMARY TABLE FOR FRONTEND QUERIES**

---

#### 2. `token_signals` (Alerts & Events)

**File:** `sql/03_token_signals.sql`

**Purpose:** Append-only log of all trading signals detected by the pipeline

**Writer:** `PipelineEngine` via `SqliteAggregateWriter`

**Operation:** INSERT (append-only, never updated)

**Schema:**
```sql
CREATE TABLE token_signals (
    -- Primary Key
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- Signal Identification
    mint            TEXT NOT NULL,               -- Token address
    signal_type     TEXT NOT NULL,               -- "BREAKOUT", "SURGE", "FOCUSED", "BOT_DROPOFF"
    window_seconds  INTEGER NOT NULL,            -- Window size (60, 300, or 900)
    
    -- Signal Metadata
    severity        INTEGER NOT NULL DEFAULT 1,  -- Importance level (1-5)
    score           REAL,                        -- Numeric score (signal strength)
    details_json    TEXT,                        -- JSON payload with additional data
    created_at      INTEGER NOT NULL,            -- Unix timestamp
    
    -- UI State Tracking
    sent_to_discord INTEGER NOT NULL DEFAULT 0,  -- 0=not sent, 1=sent
    seen_in_terminal INTEGER NOT NULL DEFAULT 0  -- 0=not seen, 1=seen
);

-- Indexes for Common Queries
CREATE INDEX idx_token_signals_mint_created 
    ON token_signals (mint, created_at DESC);

CREATE INDEX idx_token_signals_type_created 
    ON token_signals (signal_type, created_at DESC);
```

**Signal Types:**

| Type | Description | Trigger Condition |
|------|-------------|-------------------|
| `BREAKOUT` | Large buying pressure | `net_flow_300s > 50 SOL` AND `buy_count_300s > 10` |
| `SURGE` | Rapid accumulation | `buy_count_60s > 5` AND `net_flow_60s > 10 SOL` |
| `FOCUSED` | Concentrated buying | `unique_wallets_300s < 5` AND `volume_300s > 100 SOL` |
| `BOT_DROPOFF` | Bot activity ceased | Previous bot count > 5 AND current ≤ 2 |

**Key Columns for Frontend:**

| Column | Type | Description | Use Case |
|--------|------|-------------|----------|
| `id` | INTEGER | Auto-incrementing ID | Pagination |
| `mint` | TEXT | Token address | Filter by token |
| `signal_type` | TEXT | Signal category | Filter by type |
| `window_seconds` | INTEGER | Time window | Context (60s, 300s, 900s) |
| `severity` | INTEGER | Importance (1-5) | Sort by priority |
| `score` | REAL | Signal strength | Ranking |
| `created_at` | INTEGER | Timestamp | Sort chronologically |
| `seen_in_terminal` | INTEGER | UI state flag | Mark as read/unread |

**Update Frequency:** Emitted when conditions met (varies per token)

**Retention:** Append-only (grows over time, may need archival strategy)

**Deduplication:** Pipeline tracks last signal state per mint to prevent duplicate emissions

**Status:** ✅ **PRIMARY TABLE FOR ALERTS/NOTIFICATIONS**

---

### METADATA TABLES (Supporting Data)

#### 3. `token_metadata` (Token Information)

**File:** `sql/00_token_metadata.sql`

**Purpose:** Cache of token metadata (name, symbol, decimals)

**Writer:** Future metadata enrichment services (not yet implemented)

**Schema:**
```sql
CREATE TABLE token_metadata (
    mint              TEXT PRIMARY KEY,
    symbol            TEXT,                      -- Ticker symbol (e.g., "SOL", "USDC")
    name              TEXT,                      -- Full name (e.g., "Solana")
    decimals          INTEGER NOT NULL,          -- Token decimals (typically 6 or 9)
    launch_platform   TEXT,                      -- "pump.fun", "raydium", etc.
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
```

**Status:** ⚠️ **Defined but not actively populated by current pipeline**

**Future Use:** Enrich UI with human-readable token names

---

#### 4. `mint_blocklist` (Filtered Tokens)

**File:** `sql/01_mint_blocklist.sql`

**Purpose:** Blacklist of tokens to exclude from analytics

**Writer:** Manual/Admin tools (not part of pipeline)

**Schema:**
```sql
CREATE TABLE mint_blocklist (
    mint         TEXT PRIMARY KEY,
    reason       TEXT NOT NULL,                  -- "scam", "rug pull", "test token"
    blocked_by   TEXT,                           -- Admin identifier
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER                         -- NULL = permanent, otherwise Unix timestamp
);
```

**Enforcement:**
- Signal writer checks this table BEFORE writing to `token_signals`
- Frontend should filter queries to exclude blocked mints:
  ```sql
  WHERE mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())
  ```

**Status:** ✅ **Active blocklist (currently empty, requires manual population)**

---

#### 5. `system_metrics` (Optional Health Data)

**File:** `sql/04_system_metrics.sql`

**Purpose:** System health and heartbeat metrics

**Schema:**
```sql
CREATE TABLE system_metrics (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    metric_name   TEXT NOT NULL,
    metric_value  REAL NOT NULL,
    timestamp     INTEGER NOT NULL
);
```

**Status:** ⚠️ **Optional, not currently used by pipeline**

---

### TRANSITIONAL TABLE (Not Recommended for Frontend)

#### `trades` Table (Raw Trade Events)

**File:** `src/streamer_core/sqlite_writer.rs` (not in `/sql` directory)

**Purpose:** Raw trade event storage (used by standalone `aggregator` binary)

**Writer:** Streamers when run with `--backend sqlite` flag

**Schema:**
```sql
CREATE TABLE trades (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    program         TEXT NOT NULL,               -- Program ID
    program_name    TEXT NOT NULL,               -- "PumpSwap", "JupiterDCA", etc.
    mint            TEXT NOT NULL,
    signature       TEXT UNIQUE NOT NULL,
    action          TEXT NOT NULL,               -- "BUY" or "SELL"
    sol_amount      REAL NOT NULL,
    token_amount    REAL NOT NULL,
    token_decimals  INTEGER NOT NULL,
    user_account    TEXT,
    discriminator   TEXT NOT NULL,
    timestamp       INTEGER NOT NULL
);

CREATE INDEX idx_mint_timestamp ON trades(mint, timestamp DESC);
CREATE INDEX idx_timestamp ON trades(timestamp DESC);
CREATE INDEX idx_program ON trades(program, timestamp DESC);
```

**Key Differences from `token_aggregates`:**
- Stores individual trades (not aggregated)
- Unbounded growth (millions of rows)
- Requires complex queries to compute metrics
- Not written by `pipeline_runtime` (only when streamers run standalone)

**Who Uses This:**
- `aggregator` binary (via `SqliteTradeReader`)
- `solflow_signals` binary
- Backtesting/research tools

**Why NOT Recommended:**
- ❌ Raw data (requires aggregation on every query)
- ❌ Unbounded size (performance degrades over time)
- ❌ Not written by primary pipeline runtime
- ❌ Duplicates data (pipeline keeps trades in memory)

**Status:** ⚠️ **TRANSITIONAL - Do NOT query this from frontend**

**Exception:** If you need historical raw trades for specific analysis, this table exists. But for real-time metrics, **always use `token_aggregates`**.

---

## Streamers → Pipeline → Tables Mapping

### Complete Data Flow Mapping

| On-Chain Program | Streamer Binary | Program ID | Data Extracted | Pipeline Stage | Output Tables |
|------------------|----------------|-----------|----------------|----------------|---------------|
| **PumpSwap** | `pumpswap_streamer` | `pAMMBay6oce...` | BUY/SELL trades | PipelineEngine rolling windows | `token_aggregates`, `token_signals` |
| **BonkSwap** | `bonkswap_streamer` | `LanMV9sAd7w...` | BUY/SELL trades | PipelineEngine rolling windows | `token_aggregates`, `token_signals` |
| **Moonshot** | `moonshot_streamer` | `MoonCVVNZFS...` | BUY/SELL trades | PipelineEngine rolling windows | `token_aggregates`, `token_signals` |
| **Jupiter DCA** | `jupiter_dca_streamer` | `DCA265Vj8a9...` | DCA fill events | PipelineEngine rolling windows | `token_aggregates`, `token_signals` |

### Per-Streamer Details

#### PumpSwap Streamer

**Program ID:** `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`

**What It Monitors:**
- Automated Market Maker (AMM) swap transactions
- Primary DEX for new token launches

**Extracted Fields:**
- `mint` - Token being swapped
- `sol_amount` - SOL value
- `token_amount` - Token quantity
- `action` - "BUY" (user spent SOL) or "SELL" (user received SOL)
- `user_account` - Trader wallet
- `timestamp` - Transaction time

**Pipeline Processing:**
1. Trade sent to pipeline channel via `try_send()`
2. `PipelineEngine.process_trade()` adds to rolling windows
3. Every 5 seconds: `compute_metrics()` aggregates all PumpSwap trades
4. Writes to `token_aggregates` (net flow, counts, etc.)
5. Emits signals to `token_signals` if thresholds met

**Frontend Queries:**
```sql
-- Get top PumpSwap tokens by 5-min net flow
SELECT mint, net_flow_300s_sol, buy_count_300s, sell_count_300s
FROM token_aggregates
WHERE source_program = 'PumpSwap'
  AND updated_at > unixepoch() - 60  -- Updated in last minute
ORDER BY net_flow_300s_sol DESC
LIMIT 20;
```

---

#### BonkSwap Streamer

**Program ID:** `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`

**What It Monitors:**
- LetsBonk Launchpad swap transactions
- Secondary DEX for meme tokens

**Pipeline Processing:** Same as PumpSwap (unified in `PipelineEngine`)

**Frontend Queries:**
```sql
-- Compare BonkSwap vs PumpSwap activity
SELECT 
    source_program,
    COUNT(*) as active_tokens,
    SUM(volume_300s_sol) as total_volume
FROM token_aggregates
WHERE updated_at > unixepoch() - 300
GROUP BY source_program;
```

---

#### Moonshot Streamer

**Program ID:** `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG`

**What It Monitors:**
- Moonshot platform swap transactions
- Another token launch platform

**Pipeline Processing:** Same as PumpSwap (unified)

---

#### Jupiter DCA Streamer

**Program ID:** `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M`

**What It Monitors:**
- Dollar-Cost Averaging (DCA) fill events
- Scheduled recurring buy orders

**Why Important:**
- DCA orders represent long-term conviction (not speculative)
- High DCA activity = institutional/informed traders accumulating
- Useful for filtering noise from bot trading

**Pipeline Processing:**
- DCA trades flow through same pipeline as DEX trades
- `source_program` field = "JupiterDCA"
- Aggregated into same `token_aggregates` table
- ⚠️ **Note:** DCA correlation (overlap % with PumpSwap) is NOT implemented in primary pipeline

**Missing Feature:** The `aggregator` binary implements DCA correlation, but it's a **separate process**. Primary pipeline treats DCA trades as regular trades.

**Frontend Queries:**
```sql
-- Find tokens with recent DCA activity
SELECT mint, buy_count_300s, net_flow_300s_sol
FROM token_aggregates
WHERE source_program = 'JupiterDCA'
  AND buy_count_300s > 0
  AND updated_at > unixepoch() - 300
ORDER BY buy_count_300s DESC;
```

---

### Cross-Program Aggregation

**How Multiple Sources Are Combined:**

If a token `MINT_XYZ` has trades from multiple DEX programs:

1. **Separate Rows:** Each `source_program` gets its own row in `token_aggregates`
   - Row 1: `mint=MINT_XYZ, source_program=PumpSwap`
   - Row 2: `mint=MINT_XYZ, source_program=BonkSwap`

2. **Frontend Aggregation Required:**
   ```sql
   -- Total activity across all DEXes for a token
   SELECT 
       mint,
       SUM(net_flow_300s_sol) as total_net_flow,
       SUM(buy_count_300s) as total_buys,
       SUM(sell_count_300s) as total_sells
   FROM token_aggregates
   WHERE mint = 'MINT_XYZ'
   GROUP BY mint;
   ```

**Note:** Pipeline does NOT merge across `source_program` automatically. Each DEX maintains separate metrics.

---

## Frontend Integration Guidance

### Recommended Query Patterns

#### 1. Top Tokens by Net Flow (5-Minute Window)

**Use Case:** Dashboard "Hot Tokens" list

```sql
SELECT 
    mint,
    net_flow_300s_sol,
    buy_count_300s,
    sell_count_300s,
    unique_wallets_300s,
    volume_300s_sol,
    updated_at
FROM token_aggregates
WHERE updated_at > unixepoch() - 60  -- Updated in last minute (data freshness check)
  AND mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())
ORDER BY net_flow_300s_sol DESC
LIMIT 50;
```

**Returns:** Top 50 tokens by buying pressure (net inflow)

**Refresh Rate:** Every 5-10 seconds (matches pipeline flush interval)

---

#### 2. Token Detail View

**Use Case:** Individual token analytics page

```sql
SELECT 
    mint,
    source_program,
    net_flow_60s_sol,
    net_flow_300s_sol,
    net_flow_900s_sol,
    buy_count_60s,
    sell_count_60s,
    buy_count_300s,
    sell_count_300s,
    buy_count_900s,
    sell_count_900s,
    unique_wallets_300s,
    bot_trades_300s,
    avg_trade_size_300s_sol,
    volume_300s_sol,
    last_trade_timestamp,
    updated_at
FROM token_aggregates
WHERE mint = ?
ORDER BY updated_at DESC;
```

**Returns:** All time windows and metrics for a specific token

**Multiple Rows:** If token trades on multiple DEXes, you'll get one row per `source_program`

---

#### 3. Recent Signals/Alerts

**Use Case:** Real-time alert feed

```sql
SELECT 
    id,
    mint,
    signal_type,
    window_seconds,
    severity,
    score,
    created_at,
    seen_in_terminal
FROM token_signals
WHERE created_at > unixepoch() - 3600  -- Last hour
  AND mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())
ORDER BY created_at DESC, severity DESC
LIMIT 100;
```

**Returns:** Recent signals sorted by time and importance

**Mark as Seen:**
```sql
UPDATE token_signals
SET seen_in_terminal = 1
WHERE id IN (?, ?, ?, ...);
```

---

#### 4. Signals for Specific Token

**Use Case:** Token detail page "Recent Signals" section

```sql
SELECT 
    signal_type,
    window_seconds,
    severity,
    score,
    created_at
FROM token_signals
WHERE mint = ?
ORDER BY created_at DESC
LIMIT 20;
```

---

#### 5. Cross-DEX Token Activity

**Use Case:** Aggregate token metrics across all DEXes

```sql
SELECT 
    mint,
    SUM(net_flow_300s_sol) as total_net_flow,
    SUM(buy_count_300s) as total_buys,
    SUM(sell_count_300s) as total_sells,
    SUM(volume_300s_sol) as total_volume,
    MAX(unique_wallets_300s) as max_unique_wallets,  -- Or SUM if you want total across DEXes
    MAX(updated_at) as latest_update
FROM token_aggregates
WHERE mint = ?
GROUP BY mint;
```

**Note:** `unique_wallets_300s` aggregation strategy depends on your UX:
- `MAX(unique_wallets_300s)` - Show most active DEX's wallet count
- `SUM(unique_wallets_300s)` - May double-count wallets trading on multiple DEXes

---

#### 6. Activity Heatmap (Time-Based)

**Use Case:** Show trading activity distribution over time

```sql
SELECT 
    strftime('%Y-%m-%d %H:%M', datetime(created_at, 'unixepoch')) as time_bucket,
    COUNT(*) as signal_count,
    signal_type
FROM token_signals
WHERE created_at > unixepoch() - 86400  -- Last 24 hours
GROUP BY time_bucket, signal_type
ORDER BY time_bucket DESC;
```

---

### Data Freshness & Polling

**Update Intervals:**
- `token_aggregates`: Updated every 5 seconds (flush interval)
- `token_signals`: Emitted when conditions met (variable)

**Recommended Frontend Polling:**
- **Dashboard:** Poll every 5-10 seconds
- **Token Detail:** Poll every 5 seconds
- **Alert Feed:** Poll every 3-5 seconds
- **Historical Charts:** No polling (query on-demand)

**Freshness Check:**
```sql
SELECT mint, updated_at, (unixepoch() - updated_at) as seconds_ago
FROM token_aggregates
WHERE (unixepoch() - updated_at) > 60;  -- Find stale data (>1 min old)
```

If many rows are stale, check if `pipeline_runtime` is running.

---

### Important Constraints & Caveats

#### 1. No Historical Raw Trades in Primary Pipeline

**Limitation:** `token_aggregates` only stores current rolling-window metrics. Past trades are evicted from memory.

**Implication:** You CANNOT query "all trades for token X in the last 24 hours" from `token_aggregates`.

**Workarounds:**
- Use `token_signals` for historical event timeline
- Run standalone `aggregator` binary to populate `trades` table (separate process)
- Implement separate trade archival system

---

#### 2. Per-DEX Metrics (Not Merged)

**Behavior:** Each `source_program` (PumpSwap, BonkSwap, etc.) gets a separate row in `token_aggregates`.

**Frontend Responsibility:** Aggregate across DEXes in your queries (see query patterns above).

**Why:** Pipeline design keeps DEX data isolated for granular analysis.

---

#### 3. Signal Deduplication

**Behavior:** Pipeline tracks last signal state to prevent duplicate emissions.

**Implication:** If a token meets `BREAKOUT` conditions continuously for 10 minutes, you'll only get ONE signal (at transition time).

**Reset Logic:** Signal resets when conditions no longer met, then triggers again on next transition.

---

#### 4. Price & Market Cap (Future Feature)

**Current Status:** `price_usd`, `price_sol`, `market_cap_usd` columns exist but are NOT populated.

**Planned:** Future enrichment pipeline will fetch prices from external APIs.

**For Now:** Frontend must fetch prices separately (e.g., from Jupiter/Birdeye APIs).

---

#### 5. Bot Detection Heuristics

**Current Logic:** Simple heuristics based on trade patterns (not ML-based).

**Metrics:**
- `bot_trades_300s` - Count of suspected bot trades
- `bot_wallets_300s` - Unique bot wallet addresses

**Use With Caution:** Bot detection is rudimentary. False positives/negatives expected.

---

### Example Frontend Workflow

**Dashboard Implementation:**

1. **Initial Load:**
   ```sql
   -- Fetch top 50 tokens by net flow
   SELECT * FROM token_aggregates 
   WHERE updated_at > unixepoch() - 60
   ORDER BY net_flow_300s_sol DESC LIMIT 50;
   ```

2. **Every 5 Seconds (Polling):**
   ```sql
   -- Refresh token list
   SELECT * FROM token_aggregates 
   WHERE updated_at > [last_fetch_timestamp]
   ORDER BY updated_at DESC;
   
   -- Check for new signals
   SELECT * FROM token_signals
   WHERE created_at > [last_signal_timestamp]
   ORDER BY created_at DESC;
   ```

3. **User Clicks Token:**
   ```sql
   -- Fetch detailed metrics
   SELECT * FROM token_aggregates WHERE mint = ?;
   
   -- Fetch recent signals
   SELECT * FROM token_signals WHERE mint = ? ORDER BY created_at DESC LIMIT 20;
   
   -- Fetch metadata (if available)
   SELECT * FROM token_metadata WHERE mint = ?;
   ```

4. **Handle Blocklist:**
   ```javascript
   // Filter out blocked tokens in UI
   const blocklist = await db.all(
     "SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch()"
   );
   const blockedMints = new Set(blocklist.map(r => r.mint));
   
   tokens = tokens.filter(t => !blockedMints.has(t.mint));
   ```

---

## Legacy vs New

### Summary Table

| Component | Type | Status | Frontend Should Use? |
|-----------|------|--------|---------------------|
| `pipeline_runtime` | Binary | ✅ Current | N/A (runtime only) |
| `PipelineEngine` | Module | ✅ Current | N/A (backend only) |
| `token_aggregates` table | SQLite | ✅ Current | ✅ **YES - Primary metrics** |
| `token_signals` table | SQLite | ✅ Current | ✅ **YES - Primary alerts** |
| `aggregator` binary | Binary | ⚠️ Separate | ❌ No (runs standalone) |
| `aggregator_core` module | Module | ⚠️ Separate | ❌ No (separate system) |
| `trades` table | SQLite | ⚠️ Transitional | ❌ **NO - Raw data only** |
| `solflow_signals` binary | Binary | ⚠️ Transitional | ❌ No (may be deprecated) |
| JSONL files | Files | ⚠️ Legacy | ❌ No (backup only) |

---

### Detailed Legacy Component Analysis

#### 1. `aggregator` Binary (Standalone Analyzer)

**Location:** `src/bin/aggregator.rs` (296 lines)

**Purpose:** Advanced correlation analysis (PumpSwap × Jupiter DCA)

**Architecture:**
```
aggregator binary
├── Reads `trades` table (SqliteTradeReader)
├── TimeWindowAggregator (15m/1h/2h/4h windows)
├── CorrelationEngine (matches PumpSwap BUYs with DCA fills within ±60s)
├── SignalScorer (uptrend score computation)
├── SignalDetector (UPTREND/ACCUMULATION signals)
└── Outputs to streams/aggregates/*.jsonl OR separate SQLite path
```

**Why It Exists:**
- Implements DCA correlation logic NOT in primary pipeline
- Useful for research and backtesting
- Can run offline against historical `trades` data

**Why NOT Primary:**
- Requires `trades` table to be populated (streamers must run with `--backend sqlite`)
- Not integrated into `pipeline_runtime`
- Different output format than `token_aggregates`
- Separate execution: `cargo run --release --bin aggregator`

**Frontend Impact:**
- ❌ Do NOT query `aggregator` outputs directly
- If you need DCA correlation features, coordinate with backend team
- May become integrated into primary pipeline in future

**Status:** ⚠️ **Functional but separate - coordination required if needed**

---

#### 2. `aggregator_core` Module

**Location:** `src/aggregator_core/` (12 files, ~1,500 lines)

**Purpose:** Multi-stream correlation system for advanced analytics

**Submodules:**
- `normalizer.rs` - Trade struct parsing
- `sqlite_reader.rs` - Reads `trades` table incrementally
- `window.rs` - TimeWindowAggregator (15m/1h/2h/4h)
- `correlator.rs` - PumpSwap × DCA matching algorithm
- `scorer.rs` - Uptrend score computation
- `detector.rs` - Signal detection (UPTREND/ACCUMULATION)
- `writer.rs` - Unified writer router
- `jsonl_writer.rs` - JSONL output backend
- `sqlite_writer.rs` - SQLite output backend

**Key Algorithm (DCA Correlation):**
```rust
// Match PumpSwap BUYs with Jupiter DCA fills within ±60s
fn compute_dca_overlap(pumpswap_buys: &[Trade], dca_buys: &[Trade]) -> f64 {
    let dca_index: BTreeMap<i64, &Trade> = dca_buys.iter().map(|t| (t.timestamp, t)).collect();
    
    let overlapping_volume: f64 = pumpswap_buys.iter()
        .filter(|t| dca_index.range(t.timestamp - 60..=t.timestamp + 60).next().is_some())
        .map(|t| t.sol_amount)
        .sum();
    
    let total_pumpswap_volume: f64 = pumpswap_buys.iter().map(|t| t.sol_amount).sum();
    
    (overlapping_volume / total_pumpswap_volume) * 100.0  // Returns percentage
}
```

**Why This Isn't in Primary Pipeline:**
- Requires reading from `trades` table (different data source)
- More complex windowing (15m/1h/2h/4h vs 60s/300s/900s)
- Separate signal types (UPTREND/ACCUMULATION vs BREAKOUT/SURGE)
- Performance considerations (raw trade queries)

**Status:** ⚠️ **Separate subsystem - not part of `pipeline_runtime`**

---

#### 3. `trades` Table (Transitional)

**Created By:** `src/streamer_core/sqlite_writer.rs`

**When Populated:**
- Streamers run with `--backend sqlite` flag
- Example: `cargo run --release --bin pumpswap_streamer -- --backend sqlite`

**Schema:** (See SQLite Schema section for full details)

**Readers:**
- `aggregator` binary (via `SqliteTradeReader`)
- `solflow_signals` binary
- Research/backtesting tools

**Why Transitional:**
- Primary pipeline (`pipeline_runtime`) does NOT write to this table
- Stores raw trades (unbounded growth, performance issues)
- Duplicates data already in memory (`PipelineEngine` rolling windows)

**Migration Path:**
- Short term: Keep for backward compatibility with `aggregator` binary
- Long term: Integrate DCA correlation into primary pipeline, deprecate `trades` table

**Frontend Guidance:**
- ❌ **Do NOT query this table for production UI**
- Use `token_aggregates` instead (aggregated, performant)
- Exception: Historical analysis tools may need raw trades

---

#### 4. `solflow_signals` Binary (Transitional)

**Location:** `src/bin/solflow_signals.rs` (304 lines)

**Purpose:** Standalone signal analyzer using "REAL DEMAND BREAKOUT" model

**Architecture:**
```
solflow_signals binary
├── Queries `trades` table every 10 seconds
├── Loads PumpSwap flow, DCA events, Aggregator flow, wallet diversity
├── Computes score: (pumpswap * 0.6) + (dca_vol * 2.0) + (dca_events * 1.0) + (agg * 0.4) + (wallets * 0.2)
├── Emits to `signals` table (NOTE: different from `token_signals`)
└── Trims old trades (>24 hours)
```

**Key Differences from Primary Pipeline:**
- Queries raw `trades` table (not in-memory windows)
- Different scoring model than `PipelineEngine`
- Writes to `signals` table (separate from `token_signals`)
- Runs independently (not spawned by `pipeline_runtime`)

**Status:** ⚠️ **Likely deprecated or experimental - check with backend team**

**Frontend Impact:**
- ❌ Do NOT use `signals` table (different schema/purpose)
- Use `token_signals` table from primary pipeline instead

---

#### 5. JSONL Files (Legacy Output)

**Location:** `streams/{program}/events.jsonl`

**Written By:** All streamers (dual-channel writes)

**Purpose:**
- Historical compatibility with scripts/tools
- Backup data source
- Debugging/manual inspection

**Why Legacy:**
- Primary pipeline operates in-memory (no JSONL reads)
- Unbounded file growth (no auto-rotation in current implementation)
- Slower than in-memory aggregation
- Duplicates data in SQLite

**Can Be Disabled:**
```bash
# Run pipeline without JSONL writes
ENABLE_JSONL=false cargo run --release --bin pipeline_runtime
```

**Frontend Impact:**
- ❌ Do NOT read JSONL files from frontend
- Use SQLite tables only

---

### Cleanup Recommendations

**For Backend Team (Future Work):**

1. **Deprecate `trades` table:**
   - Integrate DCA correlation into `PipelineEngine`
   - Remove dependency on raw trade storage
   - Migrate `aggregator` binary logic to pipeline

2. **Consolidate Signal Systems:**
   - Merge `solflow_signals` scoring model into `PipelineEngine`
   - Single `token_signals` table (no separate `signals` table)

3. **Disable JSONL by Default:**
   - Make JSONL writes opt-in (not default)
   - Reduce disk I/O overhead

4. **Documentation Updates:**
   - Mark `aggregator_core` as "advanced/research" module
   - Clear separation between "production runtime" and "analysis tools"

---

## Verification & Open Questions

### Known Gaps & Limitations

#### 1. Missing Feature: DCA Correlation in Primary Pipeline

**Issue:** The `aggregator` binary implements PumpSwap × Jupiter DCA correlation, but this logic is **NOT** in `pipeline_runtime`.

**Impact:**
- Frontend cannot query DCA overlap % from `token_aggregates`
- `ACCUMULATION` signals (based on DCA correlation) are NOT emitted by primary pipeline
- Requires running separate `aggregator` binary to get DCA metrics

**Workaround:**
- Run `aggregator` binary in parallel with `pipeline_runtime`
- Coordinate between two systems (different databases/outputs)

**Long-Term Fix:**
- Integrate `aggregator_core` correlation logic into `PipelineEngine`
- Add `dca_overlap_pct` column to `token_aggregates` table
- Emit `ACCUMULATION` signals from primary pipeline

---

#### 2. Schema Inconsistency: `trades` Table Not in `/sql`

**Issue:** The `trades` table is defined in `src/streamer_core/sqlite_writer.rs`, not in the canonical `/sql` directory.

**Impact:**
- Schema migrations (`run_schema_migrations()`) do NOT create `trades` table
- Streamers create it on-demand (may lead to schema drift)
- `/sql/readme.md` states "raw trades are never stored" but `trades` table exists

**Verification Needed:**
1. Is `trades` table intentionally excluded from schema migrations?
2. Should it be added to `/sql/05_trades.sql` for consistency?
3. Or should it be fully deprecated (migrate to pipeline-only architecture)?

---

#### 3. Dual-Channel Necessity

**Question:** Why maintain JSONL writes if pipeline is in-memory?

**Possible Reasons:**
- Backward compatibility with existing scripts/tools
- Disaster recovery (SQLite corruption → restore from JSONL)
- Debugging (manual inspection of raw events)

**Recommendation:**
- Document rationale in `AGENTS.md`
- Consider making JSONL opt-in (not default)
- Add auto-rotation if JSONL is kept long-term

---

#### 4. Price Enrichment Pipeline (Stubbed)

**Issue:** Columns exist (`price_usd`, `price_sol`, `market_cap_usd`) but are NOT populated.

**Implication:**
- Frontend must fetch prices from external APIs (Jupiter, Birdeye, etc.)
- Cannot compute market cap from database alone

**Planned Implementation:**
- Background task in `pipeline_runtime` to fetch prices periodically
- Update `token_aggregates` with price data
- Requires API keys and rate limiting logic

**Frontend Impact:**
- For now, implement price fetching in frontend
- Watch for backend updates that populate these columns

---

#### 5. Token Metadata Population

**Issue:** `token_metadata` table exists but is NOT populated by current pipeline.

**Implication:**
- Frontend must fetch token names/symbols from external sources
- Cannot display human-readable token names from database

**Planned Implementation:**
- Metadata enrichment service (separate from pipeline)
- Fetch from Metaplex/Solana RPC
- Populate `token_metadata` table

---

#### 6. Signal Severity Levels

**Issue:** `token_signals.severity` column exists (1-5 scale) but all signals currently emit `severity=1`.

**Implication:**
- Cannot prioritize signals by importance
- Sorting by severity has no effect

**Recommendation:**
- Define severity rules per signal type:
  - `BREAKOUT` with high volume → severity 3-4
  - `FOCUSED` (potential insider) → severity 5
  - `BOT_DROPOFF` → severity 2
  - etc.

---

#### 7. Blocklist Enforcement

**Issue:** `mint_blocklist` table exists but is NOT checked by `PipelineEngine`.

**Current Behavior:**
- `SqliteAggregateWriter.write_signal()` checks blocklist BEFORE writing
- BUT `token_aggregates` rows are written regardless of blocklist

**Implication:**
- Blocked tokens still appear in `token_aggregates` table
- Frontend must filter queries manually

**Recommendation:**
- Add blocklist check to `PipelineEngine.process_trade()` or `compute_metrics()`
- Skip processing for blocked tokens entirely (save CPU)

---

### Open Questions for Backend Team

1. **Aggregator Integration:**
   - Is there a plan to merge `aggregator_core` into `pipeline_runtime`?
   - Timeline for DCA correlation in primary pipeline?

2. **Trades Table Deprecation:**
   - Should `trades` table be added to `/sql` schema or removed entirely?
   - What is the long-term vision (aggregate-only vs raw trades)?

3. **solflow_signals Binary:**
   - Is this still actively used or can it be deprecated?
   - Should its scoring model be merged into `PipelineEngine`?

4. **JSONL Persistence:**
   - Can JSONL writes be disabled by default (opt-in only)?
   - Should auto-rotation be implemented if kept?

5. **Price Enrichment:**
   - Which API will be used (Jupiter, Birdeye, other)?
   - ETA for price/market cap population?

6. **Historical Data:**
   - How should frontend query historical metrics (>15 minutes ago)?
   - Is there a plan for time-series data retention?

---

### Verification Checklist (For QA/Testing)

**Test `pipeline_runtime` in isolation:**
```bash
# 1. Start pipeline
ENABLE_PIPELINE=true SOLFLOW_DB_PATH=/tmp/test.db cargo run --release --bin pipeline_runtime

# 2. Verify SQLite tables created
sqlite3 /tmp/test.db ".tables"
# Expected: token_aggregates, token_signals, token_metadata, mint_blocklist, system_metrics

# 3. Verify data flow (after 30 seconds)
sqlite3 /tmp/test.db "SELECT COUNT(*) FROM token_aggregates;"
# Expected: > 0 if trades detected

# 4. Verify signals emitted
sqlite3 /tmp/test.db "SELECT * FROM token_signals ORDER BY created_at DESC LIMIT 5;"
# Expected: Signals if thresholds met
```

**Test `aggregator` binary separately:**
```bash
# 1. Populate trades table (run streamer with SQLite backend)
cargo run --release --bin pumpswap_streamer -- --backend sqlite &

# 2. Start aggregator (reads trades table)
cargo run --release --bin aggregator -- --backend sqlite

# 3. Verify enriched metrics output
cat streams/aggregates/1h.jsonl | jq 'select(.signal != null)'
# Expected: ACCUMULATION/UPTREND signals
```

**Test frontend queries:**
```sql
-- 1. Top tokens query (should return results)
SELECT * FROM token_aggregates ORDER BY net_flow_300s_sol DESC LIMIT 10;

-- 2. Recent signals (should return results if conditions met)
SELECT * FROM token_signals WHERE created_at > unixepoch() - 3600;

-- 3. Blocklist filter (should exclude blocked tokens)
SELECT * FROM token_aggregates 
WHERE mint NOT IN (SELECT mint FROM mint_blocklist)
LIMIT 10;

-- 4. Data freshness (should be <10 seconds if pipeline running)
SELECT MAX(unixepoch() - updated_at) as max_staleness FROM token_aggregates;
```

---

## Appendix: Quick Reference

### Environment Variables

**Pipeline Runtime:**
- `ENABLE_PIPELINE` - Master switch (default: false, **set to true**)
- `SOLFLOW_DB_PATH` - Database path (default: `/var/lib/solflow/solflow.db`)
- `AGGREGATE_FLUSH_INTERVAL_MS` - Flush frequency (default: 5000)
- `STREAMER_CHANNEL_BUFFER` - Channel size (default: 10000)

**Streamers:**
- `GEYSER_URL` - Yellowstone gRPC endpoint (required)
- `X_TOKEN` - Authentication token (required)
- `ENABLE_JSONL` - Enable JSONL writes (default: true)

**Aggregator (Standalone):**
- `AGGREGATOR_POLL_INTERVAL_MS` - SQLite poll frequency (default: 500)
- `CORRELATION_WINDOW_SECS` - DCA match window (default: 60)
- `UPTREND_THRESHOLD` - Score threshold (default: 0.7)
- `ACCUMULATION_THRESHOLD` - DCA overlap % (default: 25.0)

---

### Key File Locations

**Binaries:**
- `src/bin/pipeline_runtime.rs` - Primary runtime orchestrator
- `src/bin/pumpswap_streamer.rs` - PumpSwap ingestion
- `src/bin/bonkswap_streamer.rs` - BonkSwap ingestion
- `src/bin/moonshot_streamer.rs` - Moonshot ingestion
- `src/bin/jupiter_dca_streamer.rs` - Jupiter DCA ingestion
- `src/bin/aggregator.rs` - Standalone analyzer (separate)
- `src/bin/solflow_signals.rs` - Transitional analyzer
- `src/bin/grpc_verify.rs` - Diagnostic tool

**Modules:**
- `src/pipeline/` - Primary pipeline architecture (11 files)
- `src/streamer_core/` - Streamer shared logic (10 files)
- `src/aggregator_core/` - Separate correlation system (12 files)

**Schema:**
- `sql/` - Canonical schema definitions (5 tables)
- `sql/readme.md` - Schema documentation

**Documentation:**
- `AGENTS.md` - Agent guide (comprehensive rules)
- `ARCHITECTURE.md` - Original architecture notes
- `docs/` - Timestamped design documents

---

### Command Reference

**Start Primary Runtime:**
```bash
ENABLE_PIPELINE=true cargo run --release --bin pipeline_runtime
```

**Start Standalone Aggregator:**
```bash
cargo run --release --bin aggregator -- --backend sqlite
```

**Run Individual Streamer (SQLite backend):**
```bash
cargo run --release --bin pumpswap_streamer -- --backend sqlite
```

**Query Database:**
```bash
sqlite3 /var/lib/solflow/solflow.db
> .tables
> SELECT * FROM token_aggregates LIMIT 5;
> SELECT * FROM token_signals ORDER BY created_at DESC LIMIT 10;
```

**Monitor Logs:**
```bash
RUST_LOG=info cargo run --release --bin pipeline_runtime 2>&1 | tee pipeline.log
```

---

## Conclusion

**For Frontend Developers:**

✅ **Use These:**
- `token_aggregates` table - Primary metrics source
- `token_signals` table - Real-time alerts
- `token_metadata` table - Token info (when populated)
- `mint_blocklist` table - Filtering

❌ **Avoid These:**
- `trades` table - Raw data (not aggregated)
- `aggregator` binary outputs - Separate system
- JSONL files - Legacy backup

🔄 **Architecture Flow:**
```
Streamers → PipelineEngine (in-memory) → SQLite (every 5s) → Frontend
```

**Key Takeaway:** The pipeline-based architecture is the **current and recommended** approach. The `aggregator_core` module exists as a **separate research/analysis tool** and should not be confused with the primary runtime.

For questions or clarifications, refer to:
- `AGENTS.md` - Comprehensive agent rules
- `docs/20251114T18-feature-pipeline-architecture-review.md` - Detailed review
- Backend team for integration coordination

**Document Version:** 1.0 (2025-11-16)
