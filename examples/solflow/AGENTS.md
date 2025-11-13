# PumpSwap Terminal - Agent Guide

**Last Updated:** 2025-11-11  
**Repository Type:** Rust Workspace (Multi-Binary with Database-Backed Architecture)

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

```toml
[[bin]]
name = "terminal_ui"        # Interactive terminal UI (primary)
path = "src/bin/terminal_ui.rs"

[[bin]]
name = "token_indexer"      # Background enrichment worker
path = "src/bin/token_indexer.rs"

[[bin]]
name = "health_check"       # Database diagnostics
path = "src/bin/health_check.rs"
```

**Total: 3 binaries** (no more may be added without user approval)

### Verification Commands

**Before any commit, Droid should verify:**
```bash
# Count [[bin]] entries (should be 3 or less)
grep -c '^\[\[bin\]\]' Cargo.toml

# Count shell scripts (should be 0)
find . -name "*.sh" -type f | wc -l

# Check for unapproved binaries
git status | grep "src/bin/"
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

---

## 📋 Project Snapshot

**Stack:** Rust + Carbon Framework + Yellowstone gRPC + VibeStation APIs + BirdEye API + SQLite  

**Binaries:**
- `pumpswap-alerts` - Main streaming terminal (lib + binary)
- `terminal_ui` - Interactive TUI dashboard with enriched token data
- `token_indexer` - **NEW** Background token enrichment worker (24/7)
- `health_check` - **NEW** Database and indexer health diagnostics
- `transaction_diagnostic` - Transaction analysis tool

**Core Modules:**
- `main.rs` - Metadata-based volume processor (using TransactionStatusMeta)
- `rpc_client.rs` - **NEW** Direct Solana RPC client (metadata, supply, decimals)
- `persistence.rs` - SQLite-backed token cache and mint queue
- `volume_aggregator.rs` - Rolling time-window volume tracking
- `metadata_fetcher.rs` - Token metadata enrichment (RPC-based)
- `pricing_fetcher.rs` - Price fetching with fallback (VibeStation → BirdEye)
- `balances.rs` - Extract SOL/token changes from transaction metadata
- `ui.rs` - Terminal UI renderer (ratatui)
- `metrics_store.rs` - In-memory token metrics aggregation
- `bot_detector.rs` - Wash trade detection
- `shared_state.rs` - Thread-safe data structures for IPC

**Data Flow:**
```
Terminal (gRPC → Metrics) ──INSERT──> mint_queue (SQLite)
                                           ↓
Indexer (24/7 Worker) ──SELECT──> mint_queue ──API Enrich──> token_cache
                                                                    ↓
Terminal UI ──SELECT──> token_cache ──Display──> Dashboard
```

**Data Sources:**
- **Primary:** Yellowstone gRPC (Geyser) - Live transaction stream with TransactionStatusMeta
- **On-Chain Data:** Solana RPC - Token metadata, decimals, supply (direct from blockchain)
- **Pricing:** VibeStation API - Price data (primary), BirdEye API (fallback)
- **Persistence:** SQLite - Shared token cache and processing queue

**Architecture Note:** All trade volumes are derived from Carbon's `TransactionStatusMeta` (pre/post balances and token balances). Token metadata is fetched directly from Solana RPC (no external APIs). Only price data uses external APIs. Token enrichment is decoupled from UI via database-backed queue system.

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
2. Use timestamp prefix: `YYYYMMDD-HHMM-descriptive-name.md`
3. Example: `20251109-1420-volume-aggregation.md`

**Exceptions:** Root-level guides (AGENTS.md, README.md, USAGE.md)

### Module Organization
- **Shared logic** → `src/` as library modules
- **Binary entrypoints** → `src/main.rs` (main), `src/bin/*.rs` (tools)
- **Tests** → `#[cfg(test)] mod tests` at bottom of each file
- **Large features** → Separate files in `src/`

---

## 🔐 Secrets & Environment

### Required Variables (.env)
```bash
GEYSER_URL=https://basic.grpc.solanavibestation.com  # gRPC endpoint
RPC_URL=https://public.rpc.solanavibestation.com     # Solana RPC
X_TOKEN=<your_geyser_token>                          # Authentication
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

### Add New Module
```bash
# 1. Create file
touch src/my_module.rs

# 2. Add to main.rs
echo "mod my_module;" >> src/main.rs

# 3. Build to check
cargo check

# 4. Document
# Create docs/YYYYMMDD-HHMM-my-module.md
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
- [Phase 10: Smart Indexing](docs/20251110-smart-indexing-phase10.md) - Freshness windows & queue optimization ✅
- [Phase 9: RPC Client](docs/20251110-1115-rpc-client-solana-sdk-migration.md) - Direct RPC integration ✅
- [Phase 8: Database Architecture](docs/20251110-0751-database-indexer-architecture-transition.md) - Indexer/DB design ✅

**Architecture:**
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

**Current Phase:** Phase 1.5 - Data Integrity Upgrade (Always-On Filters)  
**Architecture:** Metadata-based volume extraction + Direct RPC integration + SQLite persistence + Smart background indexer + Data integrity filters  
**Status:** ✅ Implementation Complete, Testing Pending  
**Last Updated:** 2025-11-11  

**New in Phase 1.5 (Data Integrity Upgrade):**
- ✅ **Duplicate Signature Filter** - 5-minute TTL cache prevents double-counting
- ✅ **Quote Mint Validation** - Only SOL/WSOL pairs accepted (no USDC/USDT)
- ✅ **Minimum Trade Size Filter** - 0.15 SOL threshold removes dust/spam
- ✅ **Filter Statistics Tracking** - Real-time effectiveness monitoring
- ✅ **Always-On Architecture** - No feature flags, permanent validation
- ✅ **Performance:** < 1% CPU overhead, < 100 KB memory
- ✅ **Expected Impact:** 10-25% event reduction, cleaner metrics

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

**Architecture Changes (Phase 9 → Phase 10):**
- **Before Phase 9:** VibeStation APIs for metadata + supply + price
- **Phase 9:** Solana RPC for metadata + supply, APIs for price only
- **Phase 10:** Smart indexing with freshness windows and selective refresh

**Phase 10 Benefits:**
- ✅ 85% reduction in API calls through smart caching
- ✅ Database queue stays at steady state (no unbounded growth)
- ✅ Per-field refresh intervals (price: 60s, supply: 3h, metadata: never)
- ✅ Automatic queue cleanup prevents database bloat
- ✅ Enhanced metrics show skipped/updated token counts

**Verification Status:**
- [x] All binaries compile successfully
- [x] Database schema updated with freshness columns
- [x] Queue deduplication logic implemented
- [x] Automatic cleanup task running
- [x] Enhanced metrics and logging active
- [ ] 30-minute stress test pending (`./verify_indexer.sh`)
- [ ] Database growth verification pending (24h monitor)
- [ ] API rate limit verification pending

**Pending Future Phases:**
- Phase 11: Advanced analytics (trend detection, anomaly alerts)
- Phase 12: Multi-terminal support and portfolio tracking
- Phase 13: Historical data replay and backtesting
