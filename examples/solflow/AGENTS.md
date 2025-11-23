# PumpSwap Terminal - Agent Guide

**Last Updated:** 2025-11-13  
**Repository Type:** Rust Workspace (Multi-Streamer with Aggregator Architecture)

---

## 🚫 CRITICAL: Single Binary Architecture Rule

**MANDATORY CONSTRAINT:** This project maintains a **strictly controlled binary architecture**.

### The Rule

**Droid MUST NEVER:**
- ❌ Create new `[[bin]]` entries in Cargo.toml without explicit user approval
- ❌ Create new files in `src/bin/` directory
- ❌ Create `.sh` shell scripts for testing, automation, or verification
- ❌ Create separate test binaries or "helper" executables
- ❌ Create "backup" or "old" binary files (use git history instead)

**Droid MUST INSTEAD:**
- ✅ Propose functionality as CLI flags to existing binaries
- ✅ Implement features using Rust code and environment variables
- ✅ Use `RUST_LOG` for debugging instead of separate debug binaries
- ✅ Integrate verification/testing into existing binaries
- ✅ Ask user before adding any new binary entry points

### Current Binaries (Approved)

**Core Streamers:**
```toml
[[bin]]
name = "pumpswap_streamer"
path = "src/bin/pumpswap_streamer.rs"

[[bin]]
name = "bonkswap_streamer"
path = "src/bin/bonkswap_streamer.rs"

[[bin]]
name = "moonshot_streamer"
path = "src/bin/moonshot_streamer.rs"

[[bin]]
name = "jupiter_dca_streamer"
path = "src/bin/jupiter_dca_streamer.rs"
```

**Aggregation & Analysis:**
```toml
[[bin]]
name = "aggregator"
path = "src/bin/aggregator.rs"
```

**Utilities:**
```toml
[[bin]]
name = "grpc_verify"
path = "src/bin/grpc_verify.rs"
```

**Total: 6 binaries** (4 streamers, 1 aggregator, 1 utility)

**Note:** All binaries follow the same pattern:
- Must include `dotenv::dotenv().ok()` at the start of `main()`
- Must work when run from `examples/solflow` directory
- Must load `.env` file from current working directory

### Verification Commands

**Before any commit, Droid should verify:**
```bash
# Count [[bin]] entries (should be 6 or less)
grep -c '^\[\[bin\]\]' Cargo.toml

# Count shell scripts (should be 0)
find . -name "*.sh" -type f | wc -l

# Check for unapproved binaries
git status | grep "src/bin/"

# Verify all streamers have dotenv
grep -l "dotenv::dotenv" src/bin/*_streamer.rs | wc -l
# Should equal number of streamers (4)
```

### How to Add Features

**❌ WRONG (Creating new binary):**
```bash
# DON'T DO THIS
touch src/bin/my_new_feature.rs
# Add [[bin]] to Cargo.toml
```

**✅ CORRECT (Adding CLI flag to existing binary):**
```rust
// In src/bin/terminal_ui.rs
fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.contains(&"--my-feature".to_string()) {
        run_my_feature();
    } else {
        run_normal_mode();
    }
}
```

**✅ CORRECT (Using environment variable):**
```bash
# Enable feature via env var
ENABLE_MY_FEATURE=true cargo run --release --bin terminal_ui
```

---

## 🗄️ CRITICAL: Schema Source of Truth

**MANDATORY CONSTRAINT:** This project maintains a **canonical SQL schema** in the `/sql` directory.

### The Rule

**Droid MUST:**
- ✅ **Always reference `/sql` directory as the single source of truth for all database schema**
- ✅ Use exact column names, types, and constraints defined in SQL files
- ✅ Check `/sql/readme.md` for schema documentation and agent rules
- ✅ Verify blocklist table (`mint_blocklist`) before inserting signals
- ✅ Use only the tables defined in `/sql` for queries and data operations

**Droid MUST NEVER:**
- ❌ Create new tables outside `/sql` directory without explicit user approval
- ❌ Modify table structure unless explicitly instructed
- ❌ Use different column names than those defined in `/sql`
- ❌ Create duplicate schema definitions in code
- ❌ Ignore the blocklist when writing signals
- ❌ Store raw trades in database (aggregate-only architecture)

### Canonical Schema Files

**Location:** `/sql` directory (aggregate-only architecture)

| File | Table | Purpose | Written By |
|------|-------|---------|------------|
| `00_token_metadata.sql` | `token_metadata` | Token mint metadata (symbol, name, decimals, launch platform) | Metadata fetchers |
| `01_mint_blocklist.sql` | `mint_blocklist` | Blacklist of blocked mints with reasons and expiration | Manual/Admin tools |
| `02_token_aggregates.sql` | `token_aggregates` | Rolling-window aggregate metrics (60s/300s/900s net flows, counts, wallets) | Aggregator |
| `03_token_signals.sql` | `token_signals` | Append-only signal events (BREAKOUT, FOCUSED, SURGE, BOT_DROPOFF) | Aggregator |
| `04_system_metrics.sql` | `system_metrics` | System health and heartbeat metrics (optional) | Aggregator/Monitor |

### Architecture: Aggregate-Only System

**This system does NOT store raw trades.** Instead:
1. **In-memory aggregator** processes streaming trades in real-time
2. **Rolling windows** (60s, 300s, 900s) compute aggregate metrics
3. **SQLite database** stores only:
   - Aggregated metrics (net flows, counts, averages)
   - Signal events (BREAKOUT, SURGE, etc.)
   - Token metadata (symbol, decimals, etc.)
   - System metrics (optional)

**Why aggregate-only?**
- ✅ Minimal disk I/O (no raw trade storage)
- ✅ Constant memory footprint (rolling windows)
- ✅ Fast queries (pre-aggregated data)
- ✅ Historical analysis via signal events (not raw trades)

### Data Write Rules

**Aggregator writes to:**
- `token_aggregates` - UPDATE or INSERT rolling-window metrics
- `token_signals` - INSERT signal events (append-only)
- `system_metrics` - INSERT heartbeat/health metrics

**Metadata fetchers write to:**
- `token_metadata` - INSERT or UPDATE token information

**Before writing signals:**
```sql
-- MUST check blocklist first
SELECT mint FROM mint_blocklist WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?);
-- If row exists, DO NOT write signal
```

**UIs/Terminals:**
- Read from all tables
- Filter out blocked mints unless explicitly showing them
- Never write to database

### Schema Validation

**Before any commit involving database code:**
```bash
# Verify SQL files are unmodified (unless explicitly changing schema)
git diff sql/

# Check that Rust code uses correct table/column names
grep -r "token_aggregates\|token_signals\|token_metadata" src/

# Verify blocklist checks exist in signal writers
grep -r "mint_blocklist" src/
```

**Schema modification workflow:**
1. Get explicit user approval
2. Update SQL file in `/sql` directory
3. Update Rust code to match new schema
4. Update this section of AGENTS.md
5. Test with both SQLite and (if applicable) Postgres
6. Document migration path in `/docs`

### Column Reference (Quick Lookup)

**token_aggregates:**
- `mint` (TEXT, PK), `source_program` (TEXT), `last_trade_timestamp` (INTEGER)
- `price_usd`, `price_sol`, `market_cap_usd` (REAL)
- `net_flow_60s_sol`, `net_flow_300s_sol`, `net_flow_900s_sol` (REAL)
- `buy_count_60s`, `sell_count_60s`, `buy_count_300s`, `sell_count_300s`, `buy_count_900s`, `sell_count_900s` (INTEGER)
- `unique_wallets_300s`, `bot_trades_300s`, `bot_wallets_300s` (INTEGER)
- `avg_trade_size_300s_sol`, `volume_300s_sol` (REAL)
- `updated_at`, `created_at` (INTEGER)

**token_signals:**
- `id` (INTEGER, PK AUTOINCREMENT)
- `mint` (TEXT), `signal_type` (TEXT), `window_seconds` (INTEGER)
- `severity` (INTEGER, default 1), `score` (REAL), `details_json` (TEXT)
- `created_at` (INTEGER), `sent_to_discord` (INTEGER, default 0), `seen_in_terminal` (INTEGER, default 0)

**token_metadata:**
- `mint` (TEXT, PK), `symbol` (TEXT), `name` (TEXT), `decimals` (INTEGER)
- `launch_platform` (TEXT), `created_at` (INTEGER), `updated_at` (INTEGER)

**mint_blocklist:**
- `mint` (TEXT, PK), `reason` (TEXT), `blocked_by` (TEXT)
- `created_at` (INTEGER), `expires_at` (INTEGER, nullable)

### Integration Examples

**Aggregator writes aggregate:**
```rust
// Update rolling-window metrics
sqlx::query!(
    r#"
    INSERT INTO token_aggregates (mint, source_program, net_flow_300s_sol, buy_count_300s, updated_at, created_at)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(mint) DO UPDATE SET
        net_flow_300s_sol = excluded.net_flow_300s_sol,
        buy_count_300s = excluded.buy_count_300s,
        updated_at = excluded.updated_at
    "#,
    mint, source_program, net_flow, buy_count, now, now
).execute(&pool).await?;
```

**Aggregator writes signal (with blocklist check):**
```rust
// Check blocklist first
let blocked = sqlx::query_scalar!(
    "SELECT mint FROM mint_blocklist WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)",
    mint, now
).fetch_optional(&pool).await?;

if blocked.is_none() {
    // Safe to write signal
    sqlx::query!(
        r#"INSERT INTO token_signals (mint, signal_type, window_seconds, severity, score, created_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        mint, signal_type, window_secs, severity, score, now
    ).execute(&pool).await?;
}
```

**UI reads aggregates:**
```rust
let aggregates = sqlx::query_as!(
    TokenAggregate,
    r#"SELECT mint, net_flow_300s_sol, buy_count_300s, unique_wallets_300s
       FROM token_aggregates
       WHERE mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > ?)
       ORDER BY net_flow_300s_sol DESC
       LIMIT 50"#,
    now
).fetch_all(&pool).await?;
```

---

## 📋 Project Snapshot

**Stack:** Rust + Carbon Framework + Yellowstone gRPC + VibeStation APIs + BirdEye API + SQLite  

**Binaries:**
- `pumpswap_streamer` - PumpSwap DEX trade monitor → streams/pumpswap/events.jsonl
- `bonkswap_streamer` - BonkSwap DEX trade monitor → streams/bonkswap/events.jsonl
- `moonshot_streamer` - Moonshot DEX trade monitor → streams/moonshot/events.jsonl
- `jupiter_dca_streamer` - **NEW** Jupiter DCA fill monitor → streams/jupiter_dca/events.jsonl
- `aggregator` - **NEW** Multi-stream correlation engine → streams/aggregates/*.jsonl
- `grpc_verify` - gRPC connection diagnostics

**Core Modules:**
- `main.rs` - Library entrypoint (exports all modules)
- `streamer_core/` - **NEW** Shared JSONL streaming infrastructure
  - `lib.rs` - Main streaming logic with Carbon pipeline
  - `config.rs` - Configuration and environment validation
  - `output_writer.rs` - JSONL file writer with rotation
  - `trade_detector.rs` - Metadata-based trade extraction
  - `balance_extractor.rs` - SOL/token balance change detection
  - `grpc_client.rs` - Yellowstone gRPC client with reconnection
- `aggregator_core/` - **NEW** Multi-stream correlation system
  - `mod.rs` - Public API exports
  - `normalizer.rs` - Trade struct parsing (source-agnostic)
  - `sqlite_reader.rs` - Incremental SQLite cursor reader (ID-based)
  - `window.rs` - Rolling time windows (15m, 1h, 2h, 4h)
  - `correlator.rs` - Cross-stream correlation (PumpSwap + Jupiter DCA)
  - `scorer.rs` - Uptrend score computation (multi-factor)
  - `detector.rs` - Signal detection (UPTREND, ACCUMULATION)
  - `writer.rs` - Unified writer router (JSONL or SQLite backend)
  - `writer_backend.rs` - Writer trait abstraction
  - `jsonl_writer.rs` - JSONL output implementation
  - `sqlite_writer.rs` - SQLite output implementation
- `state.rs` - Legacy state management (terminal UI)
- `empty_decoder.rs` - Minimal decoder for metadata-only processing

**Data Flow (Multi-Streamer Architecture):**
```
Yellowstone gRPC Stream
    ↓
PumpSwap/BonkSwap/Moonshot/JupiterDCA Streamers
    ↓
SQLite Database (/var/lib/solflow/solflow.db - unified trades table)
    ↓
Aggregator (SqliteTradeReader - incremental cursor)
    ↓
TimeWindowAggregator (15m, 1h, 2h, 4h windows)
    ↓
CorrelationEngine (PumpSwap × Jupiter DCA)
    ↓
SignalScorer + SignalDetector
    ↓
Enriched Metrics (SQLite or JSONL backend)
    ↓
Terminal UI (future integration)
```

**Data Sources:**
- **Primary:** Yellowstone gRPC (Geyser) - Live transaction stream
- **On-Chain Data:** Solana RPC - Token metadata via TransactionStatusMeta
- **Enrichment:** VibeStation API (price), BirdEye API (fallback)
- **Persistence:** JSONL files (trade events) + SQLite (optional token cache)

**Architecture Note:** All trade volumes are extracted from Carbon's `TransactionStatusMeta` (pre/post balances). No instruction decoding required. Streamers emit unified JSONL schema. Aggregator correlates multiple streams to detect accumulation patterns.

---

## 🚀 Quick Start

### Build & Run

**Recommended Production Setup:**

```bash
# 1. Build all binaries
cargo build --release

# 2. Start background indexer (keep running 24/7)
cargo run --release --bin token_indexer &

# 3. Start terminal UI (interactive dashboard)
cargo run --release --bin terminal_ui

# 4. Health check (run periodically or on-demand)
cargo run --release --bin health_check
```

**Alternative Commands:**

```bash
# Run main terminal (simple text output, no database)
cargo run --release --bin pumpswap-alerts

# Run diagnostic tool
cargo run --release --bin transaction_diagnostic -- <SIGNATURE>

# Run with debug logs
RUST_LOG=debug cargo run --release --bin token_indexer

# Save terminal output
cargo run --release --bin pumpswap-alerts | tee volume_log.txt

# Run verification test (30 minutes)
./verify_indexer.sh
```

### Environment Setup
```bash
cp .env.example .env
# Edit .env with your credentials:
# GEYSER_URL=https://basic.grpc.solanavibestation.com
# RPC_URL=https://public.rpc.solanavibestation.com
# X_TOKEN=<your_geyser_token>

# RPC Endpoints (on-chain data fetching)
# RPC_PRIMARY=https://public.rpc.solanavibestation.com
# RPC_BACKUP_1=https://api.mainnet-beta.solana.com
# RPC_BACKUP_2=https://rpc.ankr.com/solana
# RPC_RPS=20  # RPC requests per second limit

# VibeStation API endpoints (PRICE ONLY)
# VIBE_PRICE_API=https://beta-api.solanavibestation.com/price

# BirdEye API (pricing fallback)
# BIRDEYE_API_KEY=<your_birdeye_key>

# Database Configuration
# DB_PATH=data/terminal_store.db

# Indexer Configuration (Background Worker)
# INDEXER_PRICE_INTERVAL=60        # Refresh prices every 60s
# INDEXER_METADATA_INTERVAL=3600   # Refresh metadata every 1h
# INDEXER_SUPPLY_INTERVAL=10800    # Refresh supply every 3h
# INDEXER_MAX_CONCURRENT=20        # Max concurrent API requests
# INDEXER_BATCH_SIZE=50            # Process N tokens per cycle
# VIBESTATION_RPS=25               # Rate limit (req/s)
# BIRDEYE_RPS=5                    # Fallback rate limit
```

---

## 🏗️ Database Architecture (Phase 8)

**Overview:** Token enrichment is now handled by a dedicated background indexer process, with terminal and indexer communicating via SQLite database.

### Tables

**token_cache** - Enriched token data
```sql
mint TEXT PRIMARY KEY           -- Token mint address
name TEXT NOT NULL              -- Token name
symbol TEXT NOT NULL            -- Token symbol (e.g., "USDC")
decimals INTEGER NOT NULL       -- Token decimals
supply REAL                     -- Total supply
price REAL                      -- Latest price in SOL
price_source TEXT               -- "VibeStation" or "BirdEye"
market_cap REAL                 -- price × supply
last_updated TEXT NOT NULL      -- ISO 8601 timestamp
```

**mint_queue** - Indexer processing queue
```sql
mint TEXT PRIMARY KEY           -- Token mint address
discovered_at TEXT NOT NULL     -- When first seen
processed INTEGER DEFAULT 0     -- 0=pending, 1=done
last_attempt TEXT               -- Last enrichment attempt
attempt_count INTEGER DEFAULT 0 -- Retry counter
```

### Binaries

| Binary | Role | Reads | Writes | Runs |
|--------|------|-------|--------|------|
| `terminal_ui` | UI + Metrics | token_cache | mint_queue | Interactive |
| `token_indexer` | Enrichment Worker | mint_queue | token_cache, mint_queue | 24/7 |
| `health_check` | Diagnostics | Both tables | None | On-demand |

### Flow Diagram

```
┌─────────────────┐
│  New Trade      │
│  Detected       │
└────────┬────────┘
         │
         ▼
┌─────────────────────────┐
│  Terminal Processor     │
│  - Extract mint address │
│  - Store metrics        │
│  - INSERT mint_queue    │
└─────────────────────────┘
         │
         ▼
┌─────────────────────────┐
│  SQLite Database        │
│  mint_queue table       │
│  (processed=0)          │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│  Indexer (Background)   │
│  - SELECT unprocessed   │
│  - Fetch metadata/price │
│  - Retry on failure     │
│  - UPDATE token_cache   │
│  - Mark processed=1     │
└─────────────────────────┘
         │
         ▼
┌─────────────────────────┐
│  Terminal UI Renderer   │
│  - SELECT token_cache   │
│  - Display enriched data│
└─────────────────────────┘
```

### Why This Architecture?

**Before (Phase 7):**
- ✗ UI froze during API calls
- ✗ No persistence between restarts
- ✗ Redundant API requests

**After (Phase 8):**
- ✓ UI never blocks (reads from DB)
- ✓ Data persists between restarts
- ✓ Centralized rate limiting in indexer
- ✓ Retry logic isolated from UI
- ✓ Multiple terminals can share one indexer

**See:** [Database-Indexer Architecture Transition](docs/20251110-0751-database-indexer-architecture-transition.md)

---

## 📁 Directory Map

```
solflow/
├── AGENTS.md                    # This file
├── src/
│   ├── AGENTS.md               # Detailed module guide → /src/AGENTS.md
│   ├── main.rs                 # Event processor (lib + binary)
│   ├── volume_aggregator.rs    # Rolling volume tracking
│   ├── token_normalizer.rs     # Decimal normalization
│   ├── transaction_diagnostic.rs
│   └── bin/
│       └── transaction_diagnostic.rs
├── diagnostic/                 # Standalone diagnostic tool
│   └── AGENTS.md              # Diagnostic tool guide
├── docs/                       # Timestamped documentation
│   └── [YYYYMMDD-HHMM]-*.md
└── Cargo.toml                 # Workspace configuration
```

**Sub-Guides:**
- [Detailed Module Guide](src/AGENTS.md) - All Rust modules
- [Diagnostic Tool Guide](diagnostic/AGENTS.md) - Transaction analysis
- [Architecture Docs](docs/) - Timestamped design decisions

---

## 🌊 Multi-Streamer Architecture

### Overview

The SolFlow system uses a **multi-streamer architecture** where each DEX program is monitored by a dedicated streamer binary that outputs to a unified JSONL schema.

### Streamer Pattern

**All streamers follow the same structure:**

```rust
use carbon_terminal::streamer_core::{run, config::StreamerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();  // ← CRITICAL: Must load .env first

    let config = StreamerConfig {
        program_id: "PROGRAM_ADDRESS_HERE".to_string(),
        program_name: "StreamerName".to_string(),
        output_path: std::env::var("OUTPUT_PATH_VAR")
            .unwrap_or_else(|_| "streams/default/events.jsonl".to_string()),
    };

    run(config).await
}
```

**Critical Requirements:**
1. ✅ **Must include `dotenv::dotenv().ok()`** at the start of `main()`
2. ✅ Must work when run from `examples/solflow` directory
3. ✅ Must load environment variables from `.env` in current working directory
4. ✅ Must use `streamer_core::run()` for consistent behavior
5. ✅ Must emit unified JSONL schema (see below)

### Unified JSONL Schema

**All streamers output the same schema:**

```json
{
  "timestamp": 1731491200,
  "signature": "...",
  "program_id": "...",
  "program_name": "PumpSwap|BonkSwap|Moonshot|JupiterDCA",
  "action": "BUY|SELL",
  "mint": "...",
  "sol_amount": 1.5,
  "token_amount": 1000000.0,
  "token_decimals": 6,
  "user_account": "...",
  "discriminator": "..."
}
```

**Why unified schema?**
- Aggregator can process all streams identically
- Easy to add new DEX programs (just add streamer)
- Consistent data format for downstream consumers

### Current Streamers

| Streamer | Program ID | Output Path | Status |
|----------|-----------|-------------|--------|
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | `streams/pumpswap/events.jsonl` | ✅ Active |
| BonkSwap | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | `streams/bonkswap/events.jsonl` | ✅ Active |
| Moonshot | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | `streams/moonshot/events.jsonl` | ✅ Active |
| Jupiter DCA | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | `streams/jupiter_dca/events.jsonl` | ✅ Active |

### Adding a New Streamer

**DO NOT** create a new streamer without user approval. If approved:

1. Create binary: `src/bin/NEW_streamer.rs`
2. Use exact pattern above with `dotenv::dotenv().ok()`
3. Add `[[bin]]` entry to `Cargo.toml`
4. Create output directory: `mkdir -p streams/NEW`
5. Test from solflow directory: `cargo run --release --bin NEW_streamer`
6. Document in this section

---

## 🎯 Aggregator Enrichment System

### Purpose

The aggregator correlates multiple trade streams to detect **accumulation patterns** and **uptrend signals** by analyzing cross-stream activity.

### Architecture

**Core Concept:** Detect when PumpSwap spot buying aligns with Jupiter DCA recurring buys → Strong accumulation signal

**Components:**
1. **SqliteTradeReader** - Incremental cursor-based reader from SQLite database
2. **TimeWindowAggregator** - Rolling windows (15m, 1h, 2h, 4h)
3. **CorrelationEngine** - Matches PumpSwap BUYs with Jupiter DCA BUYs within ±60s
4. **SignalScorer** - Multi-factor uptrend scoring (net flow, ratio, velocity, diversity)
5. **SignalDetector** - Emits UPTREND or ACCUMULATION signals based on thresholds
6. **AggregatorWriter** - Outputs enriched metrics to JSONL or SQLite backend

### Why Jupiter DCA?

**Jupiter DCA** (`DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M`) is a Dollar-Cost Averaging program where users set up recurring token buys.

**Significance:**
- DCA orders represent **long-term conviction** (not speculative)
- High DCA activity + high spot activity = **coordinated accumulation**
- More reliable signal than just spot trading volume

### Correlation Logic

**Goal:** Measure % of PumpSwap BUY volume that occurs within ±60 seconds of Jupiter DCA fills.

**Algorithm:**
1. Build BTreeMap index of Jupiter DCA BUY timestamps (O(D log D))
2. For each PumpSwap BUY, check if DCA trade exists in [timestamp - 60s, timestamp + 60s] (O(log D))
3. Sum overlapping PumpSwap volume
4. Return: `(overlapping_volume / total_pumpswap_volume) * 100`

**Example:**
```
PumpSwap BUYs: 100 SOL total
Jupiter DCA fills within ±60s: 30 SOL overlaps with PumpSwap
DCA Overlap: (30 / 100) * 100 = 30%
```

### Signal Types

**ACCUMULATION** (priority signal):
- Condition: `dca_overlap_pct > 25%` AND `net_flow_sol > 0`
- Meaning: Smart money (DCA) and spot traders are accumulating together

**UPTREND** (generic signal):
- Condition: `uptrend_score > 0.7`
- Meaning: Strong buying pressure across multiple factors

**Uptrend Score Components:**
- Net Flow (30%): Positive = more buys than sells
- Buy Ratio (30%): Buy volume / total volume
- Trade Velocity (20%): Trades per minute
- Wallet Diversity (20%): Unique buyers / total buys (anti-wash trading)

### Output Schema

**Files:** `streams/aggregates/{15m,1h,2h,4h}.jsonl`

**Schema:**
```json
{
  "mint": "...",
  "window": "1h",
  "net_flow_sol": 123.45,
  "buy_sell_ratio": 0.68,
  "dca_overlap_pct": 27.3,
  "uptrend_score": 0.82,
  "signal": "ACCUMULATION",
  "timestamp": 1731491200
}
```

### Usage

**Start aggregator:**
```bash
cd ~/projects/carbon/examples/solflow
cargo run --release --bin aggregator
```

**Monitor signals:**
```bash
tail -f streams/aggregates/1h.jsonl | jq 'select(.signal != null)'
```

**Environment Variables:**
- `SOLFLOW_DB_PATH` - SQLite database path for input (default: /var/lib/solflow/solflow.db)
- `AGGREGATES_OUTPUT_PATH` - Output directory for JSONL backend (default: streams/aggregates)
- `AGGREGATOR_POLL_INTERVAL_MS` - SQLite poll frequency in milliseconds (default: 500)
- `CORRELATION_WINDOW_SECS` - Time window for correlation (default: 60)
- `UPTREND_THRESHOLD` - Uptrend score threshold (default: 0.7)
- `ACCUMULATION_THRESHOLD` - DCA overlap threshold % (default: 25.0)
- `EMISSION_INTERVAL_SECS` - Metrics emission interval (default: 60)

**Deprecated Variables:**
- `PUMPSWAP_STREAM_PATH` - No longer used (replaced by SQLite input)
- `JUPITER_DCA_STREAM_PATH` - No longer used (replaced by SQLite input)

### Memory Management

**Target:** < 300 MB for 50 active tokens

**Strategy:**
- Evict trades older than 4 hours every 60 seconds
- Each window stores only trades within its time range
- Auto-cleanup prevents unbounded growth

---

## 🎯 Conventions

### Code Style
- **Naming:** snake_case for files, PascalCase for structs, SCREAMING_SNAKE for constants
- **Logging:** Use `log::info!`, `log::debug!`, `log::warn!` with descriptive emojis
- **Async:** All processor methods are async with `#[async_trait]`
- **Errors:** Use `CarbonResult<()>` for Carbon pipeline integration

### Commit Style
```bash
git commit -m "Add feature: description

- Bullet point changes
- Reference issue/PR if applicable

Co-authored-by: factory-droid[bot] <138933559+factory-droid[bot]@users.noreply.github.com>"
```

### Documentation Rule
**All new `.md` files MUST:**
1. Be saved in `/docs/` directory
2. Use timestamp prefix: `YYYYMMDDThh-descriptive-name.md` (ISO 8601 format with 'T' separator)
3. Include hour (00-23) for proper chronological sorting
4. Example: `20251113T10-architecture-aggregator-enrichment.md`
5. Never create markdown files in root directory (except AGENTS.md, README.md, ARCHITECTURE.md)

**Format Breakdown:**
- `YYYY` = 4-digit year (e.g., 2025)
- `MM` = 2-digit month (01-12)
- `DD` = 2-digit day (01-31)
- `T` = ISO 8601 separator (literal 'T' character)
- `hh` = 2-digit hour in 24-hour format (00-23)
- `-descriptive-name.md` = kebab-case description

**Exceptions:** Root-level guides (AGENTS.md, README.md, ARCHITECTURE.md)

### Module Organization
- **Shared logic** → `src/` as library modules
- **Binary entrypoints** → `src/main.rs` (main), `src/bin/*.rs` (tools)
- **Tests** → `#[cfg(test)] mod tests` at bottom of each file
- **Large features** → Separate files in `src/`

---

## 🚫 GRPC-Level Token Blocking Architecture

**Phase 11:** Token blocking has been moved from UI-level filtering to the earliest point in the pipeline: the GRPC stream ingestion layer.

### Architecture Overview

**Data Flow:**
```
Yellowstone gRPC Stream
    ↓
GRPC Ingestion Layer (TradeProcessor)
    ↓
BlocklistChecker.is_blocked(mint)?
    ├─ YES → Discard trade immediately (no processing)
    └─ NO  → Continue to aggregation/metrics/DB
    ↓
PipelineEngine (aggregation, metrics)
    ↓
Database (token_aggregates, token_signals)
    ↓
UI (frontend dashboard)
```

### Blocklist Storage

**Primary Table:** `mint_blocklist` (SQLite)

Schema (from `/sql/01_mint_blocklist.sql`):
```sql
CREATE TABLE mint_blocklist (
    mint            TEXT PRIMARY KEY,
    reason          TEXT,
    blocked_by      TEXT,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER
);
```

**Fields:**
- `mint` - Token mint address (primary key)
- `reason` - Human-readable reason for blocking
- `blocked_by` - User/system that added the block
- `created_at` - Unix timestamp when blocked
- `expires_at` - NULL (permanent) or future timestamp (temporary)

### Blocklist Checking

**Implementation:** `src/streamer_core/blocklist_checker.rs`

**Query Logic:**
```sql
SELECT mint FROM mint_blocklist 
WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)
```

**Behavior:**
- If row exists: Token is blocked → discard trade
- If no row: Token is allowed → process trade
- If query fails: Fail-open (allow trade, log warning)

### Integration Points

**1. GRPC Ingestion Layer** (`src/streamer_core/lib.rs`)
- BlocklistChecker initialized at startup
- Checked BEFORE any processing (earliest possible point)
- Blocked trades never reach aggregation/metrics/DB/signals

**2. UI Layer** (`frontend/lib/queries.ts`)
- `blockToken()` - Writes to `mint_blocklist` table
- `unblockToken()` - Removes from `mint_blocklist` table
- `getTokens()` - Defense-in-depth filter (checks `mint_blocklist`)

**3. Database Layer**
- No foreign key constraints (allows hot-reloading)
- Indexed on `created_at` for efficient queries
- Supports both permanent (NULL) and temporary (timestamp) blocks

### Hot Reload Support

**No restart required:**
- BlocklistChecker queries database on every trade check
- Updates to `mint_blocklist` are reflected immediately
- New blocks take effect within ~1 second (next trade)

### Usage

**Block a token:**
```typescript
// Frontend (TypeScript)
blockToken("mint_address", "reason: spam");

// Result:
// 1. Row added to mint_blocklist table
// 2. All future trades for this mint discarded at GRPC layer
// 3. No aggregates/signals generated
// 4. Token disappears from UI within ~10s (next refresh)
```

**Unblock a token:**
```typescript
// Frontend (TypeScript)
unblockToken("mint_address");

// Result:
// 1. Row removed from mint_blocklist table
// 2. Future trades for this mint processed normally
// 3. Aggregates/signals resume generation
// 4. Token reappears in UI if active
```

**Temporary block (expires after 1 hour):**
```sql
-- Direct SQL (for automation/scripts)
INSERT INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at)
VALUES ('mint_address', 'temporary ban', 'script', unixepoch(), unixepoch() + 3600);
```

### Environment Configuration

**Required for blocklist functionality:**
```bash
SOLFLOW_DB_PATH=/var/lib/solflow/solflow.db
```

If `SOLFLOW_DB_PATH` is not set:
- BlocklistChecker is disabled
- All trades are processed (no filtering)
- Log warning emitted at startup

### Verification

**Check if token is blocked:**
```bash
sqlite3 /var/lib/solflow/solflow.db "
  SELECT mint, reason, blocked_by, created_at, expires_at 
  FROM mint_blocklist 
  WHERE mint = 'YOUR_MINT_ADDRESS'
"
```

**List all blocked tokens:**
```bash
sqlite3 /var/lib/solflow/solflow.db "
  SELECT mint, reason, blocked_by, 
         datetime(created_at, 'unixepoch') as blocked_at,
         CASE 
           WHEN expires_at IS NULL THEN 'permanent' 
           ELSE datetime(expires_at, 'unixepoch') 
         END as expires
  FROM mint_blocklist 
  ORDER BY created_at DESC
"
```

**Check GRPC logs for blocked trades:**
```bash
# Look for debug messages (requires RUST_LOG=debug)
tail -f /path/to/logs | grep "🚫 Blocked token detected"
```

### Benefits

✅ **Earliest-possible filtering** - Blocked trades never enter the system  
✅ **Zero overhead** - No wasted CPU/memory on blocked tokens  
✅ **Immediate effect** - No restart required, hot-reload supported  
✅ **Defense-in-depth** - Checked at GRPC layer AND UI layer  
✅ **Flexible expiration** - Supports both permanent and temporary blocks  
✅ **Fail-open** - Database errors don't block legitimate trades  
✅ **Audit trail** - All blocks logged with reason, timestamp, and user  

### Legacy Note

**Phase 10 and earlier:** Blocklist was stored in `token_metadata.blocked` column and filtered at UI layer only. This meant blocked trades still consumed pipeline resources.

**Phase 11:** Blocklist moved to dedicated `mint_blocklist` table and checked at GRPC layer. Blocked trades are discarded before any processing.

---

## 🔐 Secrets & Environment

### Required Variables (.env)
```bash
GEYSER_URL=https://basic.grpc.solanavibestation.com  # gRPC endpoint
RPC_URL=https://public.rpc.solanavibestation.com     # Solana RPC
X_TOKEN=<your_geyser_token>                          # Authentication
SOLFLOW_DB_PATH=/var/lib/solflow/solflow.db         # SQLite database path (required for blocklist)
```

### Optional Variables
```bash
RUST_LOG=info           # Logging level (debug, info, warn, error)
DATABASE_PATH=token_decimals.db  # SQLite cache location
COMMITMENT_LEVEL=finalized  # Transaction commitment (processed, confirmed, finalized)
```

### Commitment Level Configuration (Important!)
```bash
# Valid options: processed, confirmed, finalized
# Default: finalized (guarantees transactions are irreversible)
COMMITMENT_LEVEL=finalized

# Latency expectations:
# - processed: ~400ms (not finalized, high revert risk)
# - confirmed: ~1s (not finalized, ~0.1% revert risk)
# - finalized: 10-20s (irreversible, 0% revert risk) ← RECOMMENDED
```

**Production Requirement:** Always use `finalized` for production to guarantee data integrity.

### Loading Pattern
```rust
use dotenv::dotenv;

dotenv().ok();
let rpc_url = env::var("RPC_URL")
    .or_else(|_| env::var("SOLANA_RPC_URL"))
    .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
```

---

## 🌐 External Integrations

### Solana RPC (Primary On-Chain Data)

**Purpose:** Direct blockchain queries for token metadata, decimals, and supply

**Endpoints:**
- **Primary:** `RPC_PRIMARY` (VibeStation RPC)
- **Backup 1:** `RPC_BACKUP_1` (Solana Foundation)
- **Backup 2:** `RPC_BACKUP_2` (Ankr)

**RPC Methods Used:**
1. **getAccountInfo** (with `jsonParsed` encoding)
   - Fetches mint account data (decimals, supply)
   - Fetches Metaplex metadata PDA (name, symbol, URI)
2. **getTokenSupply**
   - Returns total supply with decimals

**Features:**
- ✅ Automatic failover (3 endpoints)
- ✅ Rate limiting (20 req/s default via Semaphore)
- ✅ Exponential backoff (1s → 2s → 4s)
- ✅ Per-endpoint health tracking

**Cache Strategy:**
- Metadata: 3600s TTL (rarely changes)
- Supply: 10800s TTL (updated less frequently)
- Decimals: Permanent (never changes)

### VibeStation API (Price Data Only)

**Base URL:** `https://beta-api.solanavibestation.com`

**Price API:**
- **Endpoint:** `/price?address=<mint>`
- **Returns:** Latest price, 1m/15m/1h/24h averages (in SOL)
- **Use:** Real-time price display and market cap computation
- **Cache:** 60 seconds TTL

### BirdEye API (Pricing Fallback)

**Endpoint:** `https://public-api.birdeye.so/defi/price?address=<mint>&ui_amount_mode=raw`  
**Headers:** `X-API-KEY`, `x-chain: solana`  
**Returns:** Latest price only (no averages)  
**Use:** Fallback when VibeStation price unavailable

### Integration Rules

**Data Source Hierarchy:**
1. **Metadata/Decimals/Supply:** Always use Solana RPC (source of truth)
2. **Price:** Try VibeStation first, fallback to BirdEye if unavailable
3. **Failover:** RPC automatically switches endpoints after 3 consecutive failures

**Rate Limiting:**
- RPC: 20 requests/second (configurable via `RPC_RPS`)
- Price APIs: 25 requests/second combined (VibeStation + BirdEye)

**Caching Strategy:**
- On-chain data: Long TTL (1-3 hours) - data rarely changes
- Price data: Short TTL (60 seconds) - needs freshness
- All cache entries tagged with `metadata_source='rpc'` in database

**Error Handling:**
- RPC failures trigger automatic failover to backup endpoints
- Metaplex metadata may not exist for all tokens (use mint address as fallback)
- All errors logged with structured context for debugging

---

## 🧪 Definition of Done

Before committing:
- [ ] `cargo check` passes
- [ ] `cargo clippy` has no warnings
- [ ] `cargo test` all tests pass
- [ ] `cargo fmt --check` (or run `cargo fmt`)
- [ ] Logs use appropriate levels (debug for internals, info for events)
- [ ] New `.md` docs saved in `/docs/` with timestamp
- [ ] **Commitment level verification** - Startup logs show "FINALIZED" (see below)
- [ ] Manual test: Run for 30 seconds, verify output looks correct
- [ ] **Diagnostic verification passes with ≥ 95% accuracy** (see below)

Before PR:
- [ ] Update relevant AGENTS.md if adding modules
- [ ] Add doc comments for public functions
- [ ] No secrets in code or logs
- [ ] Performance: No unbounded memory growth

---

## 🔍 Self-Diagnostic Verification

All agents must verify system accuracy after any code or logic changes that affect volume calculation, metadata enrichment, or market cap computation.

### Self-Check Rule for Metadata Processing

**Before processing or visualizing any token, confirm:**
1. ✅ Token name and symbol are cached (or fetched from VibeStation)
2. ✅ Token decimals are available (for normalization)
3. ✅ Latest price is cached or fetchable
4. ✅ If computing market cap: token supply is available via `/mint_info`

**If any data is missing:**
- Query VibeStation `/metadata` once and cache results
- Query `/mint_info` if supply needed (market cap = price × supply)
- Use cached data for all subsequent operations (60s TTL)
- Display "Loading..." or "—" in UI if fetch fails or times out

### Commitment Level Verification (Critical!)

Before running diagnostic verification, **ALWAYS** verify the commitment level is set correctly:

1. **Check startup logs** when running terminal:
   ```bash
   cargo run --release --bin pumpswap-alerts | head -10
   ```

2. **Expected output**:
   ```
   🚀 Starting PumpSwap Alerts (Carbon Wrapper)...
   📊 Monitoring program: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
   🔒 COMMITMENT LEVEL: Finalized
   ⏱️  Expected latency: 10-20s (irreversible)
   ✅ Data integrity: GUARANTEED (finalized)
   ```

3. **Failure modes**:
   - ⚠️ If you see "Confirmed" → Set `COMMITMENT_LEVEL=finalized` in .env
   - ⚠️ If you see "Processed" → Set `COMMITMENT_LEVEL=finalized` in .env
   - ⚠️ If you see "AT RISK" → Data may be reverted, use finalized!

**Why this matters:** Non-finalized transactions may be reverted, causing inaccurate volume data and failed verifications.

### Volume and Metadata Verification

**Volume Accuracy (Metadata-Based):**

1. **Run the terminal** for several minutes to generate live trade data:
   ```bash
   cargo run --release --bin pumpswap-alerts | tee pumpswap-terminal.log
   ```

2. **Verify metadata extraction** using diagnostic tool:
   ```bash
   cargo run --release --bin transaction_diagnostic -- <SIGNATURE> --check meta
   ```
   
   This verifies:
   - ✅ Pre/post SOL balances extracted from `TransactionStatusMeta`
   - ✅ Token balance changes match on-chain data
   - ✅ Primary token mint correctly identified
   - ✅ No instruction decoding used (pure metadata approach)

3. **Manual cross-check** (sample 3-5 transactions):
   - Copy signature from log
   - Open: `https://solscan.io/tx/<SIGNATURE>`
   - Verify SOL amounts match terminal output (±0.000001 rounding)
   - Confirm token amounts match (with decimal normalization)
   - Validate market cap: `price × supply` (if displayed in UI)

**Market Cap Verification:**

4. **Check market cap computation**:
   ```bash
   # Expected formula: market_cap = latest_price (SOL) × total_supply
   # Example: 0.000123 SOL × 1,000,000,000 tokens = 123,000 SOL market cap
   ```
   
   - Verify price source indicator (V = VibeStation, B = BirdEye)
   - Confirm supply fetched from `/mint_info` API
   - Check calculation accuracy in UI display

### Acceptance Criteria

**Commitment Level:** MUST show "Finalized" in startup logs (0% revert risk)

**Volume Accuracy:** ≥ 95% of trades must match on-chain `TransactionStatusMeta` within ±0.000001 SOL

**Metadata Enrichment:** ≥ 90% of active tokens must have name/symbol cached within 60 seconds

**Price Availability:** ≥ 85% of tokens must have price data (VibeStation or BirdEye)

**Market Cap Accuracy:** If displayed, market cap must equal `price × supply` within ±0.1%

### What to Check

✅ **SOL Volumes:**
- Terminal: `user_quote_amount_in/out` (actual user paid/received)
- SolScan: Sum of base amount + fees should equal terminal amount

✅ **Mint Addresses:**
- Terminal mint should match token shown on SolScan
- Use token link to verify: `https://solscan.io/token/<MINT>`

✅ **Token Amounts:**
- May differ slightly due to decimal normalization
- SOL amounts are primary verification (always accurate)

### Automation Note

Agents may invoke the diagnostic tool automatically as part of post-build validation when working within trade-processing modules. The verification script (`verify_volumes.sh`) automates extraction and provides SolScan links for manual verification.

---

## 🖥️ Terminal UI Behavior

### Data Flow Architecture

```
Carbon Stream (gRPC)
    ↓ TransactionStatusMeta (pre/post balances)
In-Memory Aggregator (MetricsStore)
    ↓ Per-token volume, wallet counts
Metadata/Price Enrichment (async, 60s cache)
    ↓ VibeStation (primary) → BirdEye (fallback)
UI Renderer (ratatui, 3-5s refresh)
    ↓ Interactive dashboard display
```

### UI Layout

The terminal UI (`terminal_ui` binary) provides a real-time, enriched dashboard:

**Columns:**
1. **Name/Symbol** - Token name and ticker (from VibeStation `/metadata`)
2. **Price (◎)** - Latest price with source indicator (`V` = VibeStation, `B` = BirdEye)
3. **Net Vol (1m/5m/15m)** - Net inflow/outflow over rolling time windows
4. **Wallets** - Unique wallet counts `(buyers/sellers)`
5. **Market Cap** - `price × supply` (if supply available from `/mint_info`)
6. **Last** - Time since last trade

**Color Scheme:**
- 🟢 **Green** - Positive net inflow (buying pressure)
- 🔴 **Red** - Negative net flow (selling pressure)
- ⚪ **Gray** - Neutral or inactive

**Refresh Rate:** 3-5 seconds (configurable, depends on API latency)

### Keyboard Bindings

| Key           | Action                           |
|---------------|----------------------------------|
| `q` / `Esc`   | Quit terminal                    |
| `↑` / `k`     | Scroll up (navigate tokens)      |
| `↓` / `j`     | Scroll down                      |
| `Space`       | Toggle pause/resume (planned)    |
| `c`           | Copy selected mint to clipboard (planned) |
| `PgUp/PgDn`   | Fast scroll (5 rows)             |
| `Home/End`    | Jump to top/bottom               |

### In-Memory Architecture

**Storage:**
- All data is in-memory only (no database writes for UI binary)
- Token metrics stored in `HashMap<Mint, TokenMetrics>`
- Metadata and prices cached with 60s TTL
- Volume windows: 1m, 5m, 15m rolling aggregations

**Performance:**
- Memory footprint: ~50-70 MB (typical for 20-30 active tokens)
- CPU usage: Low (< 5% on modern systems)
- API requests: ~2-5 per minute steady state (after initial burst)
- Network: Minimal (only API calls for cache misses)

**Data Lifecycle:**
1. Transaction arrives → extract balances from metadata
2. Update in-memory metrics (volume, wallets, direction)
3. UI refresh cycle (every 3-5s):
   - Enrich top 20 tokens with metadata/prices
   - Render dashboard
   - Handle keyboard input
4. Cache expiration → refetch stale entries on next cycle

---

## 🔍 Common Tasks

### Run Multi-Streamer System
```bash
# Terminal 1: PumpSwap streamer
cargo run --release --bin pumpswap_streamer

# Terminal 2: Jupiter DCA streamer
cargo run --release --bin jupiter_dca_streamer

# Terminal 3: Aggregator
cargo run --release --bin aggregator

# Terminal 4: Monitor signals
tail -f streams/aggregates/1h.jsonl | jq 'select(.signal != null)'
```

### Debug Aggregator
```bash
# Enable debug logs for specific module
RUST_LOG=carbon_terminal::aggregator_core::correlator=debug cargo run --release --bin aggregator

# Check correlation for specific token
cat streams/aggregates/1h.jsonl | jq 'select(.mint == "YOUR_MINT") | {mint, dca_overlap_pct, signal}'

# Monitor memory usage
watch -n 5 'ps aux | grep aggregator | grep -v grep'
```

### Test Correlation Accuracy
```bash
# Extract PumpSwap BUYs for a token
cat streams/pumpswap/events.jsonl | jq 'select(.mint == "MINT" and .action == "BUY") | .timestamp'

# Extract Jupiter DCA BUYs for same token
cat streams/jupiter_dca/events.jsonl | jq 'select(.mint == "MINT" and .action == "BUY") | .timestamp'

# Manually verify ±60s overlap matches aggregator output
```

### Add New Module
```bash
# 1. Create file
touch src/my_module.rs

# 2. Add to main.rs
echo "mod my_module;" >> src/main.rs

# 3. Build to check
cargo check

# 4. Document
# Create docs/YYYYMMDDThh-my-module.md
```

### Debug Live Stream
```bash
# Enable debug logs
RUST_LOG=debug cargo run --release --bin pumpswap-alerts 2>&1 | tee debug.log

# Search for errors
grep -i "error\|warn" debug.log

# Check specific module
RUST_LOG=pumpswap_alerts::volume_aggregator=trace cargo run --release --bin pumpswap-alerts
```

### Test Transaction Analysis
```bash
# Get signature from live stream
cargo run --release --bin pumpswap-alerts | head -10

# Analyze with diagnostic
cargo run --release --bin transaction_diagnostic -- <SIGNATURE>
```

### Verify Volume Accuracy
```bash
# Capture trades
cargo run --release --bin pumpswap-alerts | tee volume_log.txt

# Run verification
./verify_volumes.sh volume_log.txt

# Manual check on SolScan
# Copy signature from log, open: https://solscan.io/tx/<SIGNATURE>
```

---

## 📚 Key Documentation

**Latest:**
- [Aggregator Enrichment System](docs/20251113T10-architecture-aggregator-enrichment.md) - Multi-stream correlation ✅
- [Phase 10: Smart Indexing](docs/20251110-smart-indexing-phase10.md) - Freshness windows & queue optimization ✅
- [Phase 9: RPC Client](docs/20251110-1115-rpc-client-solana-sdk-migration.md) - Direct RPC integration ✅
- [Phase 8: Database Architecture](docs/20251110-0751-database-indexer-architecture-transition.md) - Indexer/DB design ✅

**Architecture:**
- [Multi-Streamer System](docs/20251113T08-architecture-streamer-system.md) - Streamer pattern and extension guide
- [Streamer Patterns](docs/20251113T08-streamer-patterns-and-extension.md) - How to add new streamers
- [Volume Aggregation](docs/20251109_VOLUME_AGGREGATION.md) - Rolling window implementation
- [Diagnostic Integration](docs/20251109_DIAGNOSTIC_INTEGRATION.md) - Discriminator matching
- [Mint Extraction](docs/20251109_MINT_EXTRACTION_FIX.md) - Pool → mint cache
- [Commitment Enforcement](docs/20251109-1445-commitment-level-enforcement.md) - Specification
- [Commitment Verification](docs/20251109-1730-commitment-verification.md) - Phase 2 ✅
- [SQLite Write Lock Elimination](docs/20251110-sqlite-write-lock-elimination.md) - WAL mode optimization

**Operational:**
- [How to Use Indexer](docs/20251110-1125-how-to-use-indexer.md) - Indexer usage guide
- [Volume Tracking Guide](docs/VOLUME_TRACKING_GUIDE.md) - User guide
- [Volume Verification](docs/VOLUME_VERIFICATION.md) - Accuracy validation

---

## 🐛 Common Issues

### gRPC Connection Fails
```
Error: Connection refused
```
**Fix:** Check GEYSER_URL in .env, verify X_TOKEN is valid

### High Unknown Mint Rate
```
📊 VOLUME SUMMARY | Active Mints: 2 | Total Trades: 47
```
**Fix:** Cache needs warm-up. Run for 5+ minutes. Rate improves over time.

### Token Amounts Look Wrong
```
SELL | 14226820.64 tokens → 5.453728 ◎
```
**Check:** Verify token decimals in RPC logs. May be non-standard (not 6 decimals).

### Memory Growing
```
$ ps aux | grep pumpswap
... 500MB+ memory
```
**Fix:** Check cleanup runs every minute. Enable debug: `RUST_LOG=debug` and look for cleanup logs.

---

## 🎓 Learning Path

**For New Developers:**
1. Read [main.rs](src/main.rs) - Understand event processing flow
2. Study [volume_aggregator.rs](src/volume_aggregator.rs) - Rolling windows
3. Review [token_normalizer.rs](src/token_normalizer.rs) - Decimal handling
4. Explore [diagnostic tool](diagnostic/) - Transaction structure
5. Check [docs/](docs/) - Design decisions (chronological)

**For Contributors:**
1. Review [src/AGENTS.md](src/AGENTS.md) - Module details
2. Run terminal for 5 minutes - Understand output
3. Test diagnostic on known transaction
4. Read verification docs - Accuracy validation
5. Make small change, test Definition of Done

---

## 🔗 External Resources

**Carbon Framework:**
- https://github.com/sevenlabs-hq/carbon - Core framework docs

**Solana:**
- https://docs.solana.com/developing/clients/jsonrpc-api - RPC API
- https://solscan.io - Transaction explorer

**PumpSwap:**
- Program ID: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`

---

## 📞 Contact & Support

- **Issues:** Check [STREAMING_STATUS.md](STREAMING_STATUS.md) first
- **Questions:** Review [docs/](docs/) chronologically
- **Bugs:** Include logs (`RUST_LOG=debug`), transaction signatures

---

## 📌 Version Information

**Current Phase:** Phase 11.2 - Aggregator SQLite Reader Migration  
**Architecture:** Multi-streamer SQLite → Aggregator (SQLite input) → Enriched metrics  
**Status:** ✅ Implementation Complete, Tested  
**Last Updated:** 2025-11-13  

**New in Phase 11.2 (Aggregator SQLite Reader):**
- ✅ **SqliteTradeReader** - Incremental cursor-based reader (replaces TailReader)
- ✅ **ID-Based Cursor** - Monotonic sequence prevents data loss
- ✅ **Batch Processing** - 1-1000 trades per cycle (500ms poll interval)
- ✅ **Read-Only Mode** - Zero write lock contention with streamers
- ✅ **Unified Input** - Single reader for PumpSwap + JupiterDCA (not 2 separate)
- ✅ **Code Reduction** - Net -100 lines (removed JSONL tailing complexity)
- ✅ **Unit Tests** - 4 comprehensive tests (incremental, filtering, batching, read-only)
- ✅ **Integration Tested** - Live run with 22,878 existing trades

**Phase 11.2 Changes:**
- ❌ Removed: `reader.rs` (209 lines of JSONL tailing code)
- ✅ Added: `sqlite_reader.rs` (155 lines)
- ✅ Refactored: `aggregator.rs` (simplified main loop)
- ✅ Updated: Environment variables (deprecated JSONL paths)

**Previous Phase 11.1 (Aggregator Enrichment System):**
- ✅ **Jupiter DCA Streamer** - Monitors DCA fill events (DCA265...M)
- ✅ **Aggregator Core** - 10 modules for multi-stream correlation
- ✅ **TimeWindowAggregator** - Rolling windows (15m, 1h, 2h, 4h)
- ✅ **CorrelationEngine** - PumpSwap × Jupiter DCA matching (±60s)
- ✅ **SignalScorer** - Multi-factor uptrend scoring
- ✅ **SignalDetector** - UPTREND/ACCUMULATION signal detection
- ✅ **AggregatorWriter** - Dual backend support (JSONL or SQLite)
- ✅ **Architecture Documentation** - Complete spec with verification plan
- ✅ **Memory Management** - < 300 MB for 50 tokens (auto-eviction)

**Key Metrics:**
- Total code: 1,213 lines (7 modules + 2 binaries)
- Correlation complexity: O(P log D) via BTreeMap
- Memory footprint: < 300 MB steady state
- Emission interval: 60 seconds per window
- Supported windows: 15m, 1h, 2h, 4h

**Previous Phase 10:**
- ✅ Freshness-based refresh intervals (per-field timestamps)
- ✅ Queue deduplication (60s window prevents duplicates)
- ✅ Automatic queue cleanup (removes old entries every 5 min)
- ✅ Selective refresh strategy (price/supply/metadata intervals)
- ✅ Enhanced metrics (skipped tokens, updates by type)
- ✅ 85% API call reduction through smart caching
- ✅ Database steady-state growth (queue size stabilized)

**Previous Phase 9:**
- ✅ Direct Solana RPC integration (`rpc_client.rs` module)
- ✅ Eliminated dependency on VibeStation metadata/supply APIs
- ✅ 3-endpoint RPC failover system with automatic recovery
- ✅ Rate-limited RPC calls via Semaphore (20 req/s default)
- ✅ Metaplex metadata PDA parsing for token name/symbol
- ✅ External APIs used **only** for price data

**Architecture Evolution:**
- **Phase 1-7:** Single-binary terminal with inline processing
- **Phase 8:** Database-backed indexer (decoupled enrichment)
- **Phase 9:** Direct RPC integration (eliminated VibeStation metadata API)
- **Phase 10:** Smart indexing (freshness windows, queue optimization)
- **Phase 11.1:** Multi-streamer + Aggregator (cross-stream correlation, JSONL input)
- **Phase 11.2:** Aggregator SQLite migration (database-backed input pipeline) ← **Current**

**Phase 10 Benefits:**
- ✅ 85% reduction in API calls through smart caching
- ✅ Database queue stays at steady state (no unbounded growth)
- ✅ Per-field refresh intervals (price: 60s, supply: 3h, metadata: never)
- ✅ Automatic queue cleanup prevents database bloat
- ✅ Enhanced metrics show skipped/updated token counts

**Verification Status:**
- [x] All binaries compile successfully
- [x] Jupiter DCA streamer tested (dotenv fix applied)
- [x] Aggregator modules implemented with unit tests
- [x] Documentation complete (20251113T10-architecture-aggregator-enrichment.md)
- [ ] 30-minute live test pending (all streamers + aggregator)
- [ ] Signal detection validation pending
- [ ] Cross-stream correlation accuracy verification pending

**Next Steps:**
- Phase 12: Terminal UI integration (display ACCUMULATION/UPTREND alerts)
- Phase 13: Historical data replay (backtest correlation algorithm)
- Phase 14: Advanced analytics (trend detection, anomaly alerts)
