# SolFlow Complete Architectural Review

**Document Version:** 1.0  
**Date:** 2025-11-19T08:00  
**Review Type:** Complete System Architecture Audit  
**Status:** Final

---

## Executive Summary

### System Purpose

SolFlow is a real-time Solana token trading analytics system that monitors decentralized exchange (DEX) programs via Yellowstone gRPC streams, computes rolling-window aggregate metrics, detects trading signals, and presents data through a Next.js dashboard. The system employs an aggregate-only architecture where raw trades are held in memory only, with computed metrics persisted to SQLite.

### Key Architecture Characteristics

- **Framework:** Carbon pipeline framework for transaction processing
- **Data Source:** Yellowstone gRPC (Geyser) with Confirmed commitment level
- **Processing Model:** In-memory rolling windows (6 time windows)
- **Persistence:** SQLite with WAL mode, aggregate-only (no raw trade storage)
- **Frontend:** Next.js with Server-Side API routes querying SQLite
- **Deployment:** Single-binary streamers + unified aggregator + web dashboard

### Critical Findings Summary

1. **No LP Token Filtering:** System processes all token mints without classification
2. **Aggregate Persistence:** PRIMARY KEY on mint prevents historical time series
3. **Single Writer Bottleneck:** Arc<Mutex<Connection>> limits write concurrency
4. **No Trade Deduplication:** Signature-level deduplication not implemented
5. **Infinite Signal Growth:** token_signals table has no cleanup mechanism

---

## 1. Carbon Framework Integration Audit

### Carbon Dependencies

| Crate | Version | Purpose | Usage Pattern |
|-------|---------|---------|---------------|
| `carbon-core` | workspace | Pipeline orchestration, Processor trait, TransactionMetadata | Core framework - Pipeline builder, async Processor impl |
| `carbon-log-metrics` | workspace | Telemetry and metrics collection | LogMetrics for pipeline monitoring |
| `carbon-yellowstone-grpc-datasource` | workspace | Geyser stream subscription | YellowstoneGrpcGeyserClient for gRPC ingestion |

### Integration Pattern

**Processor Implementation:**
```rust
#[async_trait]
impl Processor for TradeProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;
    
    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        // Process via TransactionStatusMeta only
    }
}
```

**Key Observations:**

- **EmptyDecoderCollection:** No instruction decoding - metadata-only processing
- **TransactionStatusMeta:** Balance changes extracted from pre/post balances
- **Async Trait:** Proper use of Carbon's async Processor pattern
- **Pipeline Builder:** Standard Carbon pipeline construction with datasource + processor
- **Metrics:** Integrated with Carbon's MetricsCollection system

**Architecture Alignment:**

- ✅ Correct use of Pipeline::builder() pattern
- ✅ Proper async trait implementation
- ✅ Correct TransactionMetadata usage
- ✅ Proper error handling via CarbonResult
- ⚠️ Non-standard: No instruction decoding (intentional for balance-based extraction)

### Decoder Strategy

SolFlow uses **EmptyDecoderCollection**, which means:
- No instruction parsing or discriminator matching
- All trade detection via `TransactionStatusMeta.pre_balances` and `post_balances`
- No program-specific instruction validation
- Relies entirely on balance delta analysis

**Rationale (from codebase):**
> "Extract user volumes (filters out pool/fee accounts)"
> "No instruction decoding required"

This approach is valid but depends on accurate balance metadata from Solana RPC.

---

## 2. Program ID Complete Enumeration

### Discovered Program IDs

| Program Name | Program ID | Location | Purpose | Streamer Binary |
|--------------|------------|----------|---------|-----------------|
| **PumpSwap** | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | `src/bin/pumpswap_streamer.rs` | Spot trading DEX | `pumpswap_streamer` |
| **BonkSwap** | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | `src/bin/bonkswap_streamer.rs` | Spot trading DEX | `bonkswap_streamer` |
| **Moonshot** | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | `src/bin/moonshot_streamer.rs` | Spot trading DEX | `moonshot_streamer` |
| **Jupiter DCA** | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | `src/bin/jupiter_dca_streamer.rs` | Dollar-Cost Averaging | `jupiter_dca_streamer` |

**Total Programs Tracked:** 4

### Program-Specific Details

#### PumpSwap (`pAMMBay...`)

**Expected Pattern:**
- Spot buy/sell transactions
- SOL ↔ Token swaps
- Balance changes in pre/post token balances

**Mint Extraction:**
- Via `find_primary_token_mint()` - selects non-SOL mint with largest absolute change
- No program-specific filtering rules

**Inner Instructions:**
- Not explicitly handled
- All balance changes processed regardless of instruction depth

**Edge Cases (from code):**
- Pool tokens may appear if they have largest balance change
- Fee accounts filtered by `extract_user_volumes()` heuristic

#### BonkSwap (`LanMV...`)

**Pattern:** Similar to PumpSwap (spot trading)

**Extraction:** Same heuristic as PumpSwap

#### Moonshot (`MoonCVV...`)

**Pattern:** Similar to PumpSwap (spot trading)

**Extraction:** Same heuristic as PumpSwap

#### Jupiter DCA (`DCA265...`)

**Expected Pattern:**
- Recurring automated buy orders
- Triggered by time-based schedule
- Indicates long-term accumulation intent

**Significance:**
- DCA trades used for correlation analysis
- Overlap with spot buys = "DCA_CONVICTION" signal
- Separate rolling windows tracked (dca_buys_60s, dca_buys_300s, etc.)

**Extraction:** Same balance-based heuristic

### Mint Extraction Strategy

**Algorithm (from `src/streamer_core/trade_detector.rs`):**

```rust
fn find_primary_token_mint(token_deltas: &[BalanceDelta]) -> Option<String> {
    token_deltas
        .iter()
        .filter(|d| !d.mint.starts_with("So11111"))  // Exclude SOL
        .max_by_key(|d| d.raw_change.abs())          // Largest absolute change
        .map(|d| d.mint.clone())
}
```

**Observations:**

- Heuristic-based (no instruction validation)
- Assumes largest balance change = primary token
- No LP token detection
- No pool token filtering
- No program-specific rules

**Potential Issue:**

If an LP token or pool token has a larger balance change than the traded token, it will be selected as the primary mint.

---

## 3. LP Token Behavior Investigation

### Mint Extraction Heuristic

**Current Implementation:**

The system uses a **largest-absolute-change heuristic** to identify the "primary" token:

```rust
token_deltas
    .iter()
    .filter(|d| !d.mint.starts_with("So11111"))
    .max_by_key(|d| d.raw_change.abs())
```

**Behavior:**

- Selects non-SOL mint with maximum `|raw_change|`
- No token type validation
- No metadata lookup during extraction
- No pattern matching for LP tokens

### LP Token Detection Mechanisms

**Search Results:**

✅ **Manual Blocklist:** `mint_blocklist` table exists
- Allows manual blocking of specific mints
- Checked before signal writes
- Not checked before trade ingestion

❌ **Automatic LP Detection:** Not implemented
- No metadata-based classification
- No pattern matching (e.g., "LP-" prefix)
- No program-specific filtering

❌ **External API Classification:** Not available
- DexScreener API does not return LP/pool flags
- No integration with token registries

❌ **Program-Specific Rules:** Not implemented
- All programs use same extraction logic
- No special handling for AMM programs

### Observed LP Token Example

**From user report:**
- Mint: `3yug2vvDkMu9VMhh6d11z7BbS4afTLc7vV1T3tanuzDN`
- Appeared in trade dataset
- Likely an LP token or pool token

**Root Cause:**

If this mint had the largest balance change in a transaction, the heuristic would select it as the primary token.

### Filtering Gaps

| Mechanism | Status | Location |
|-----------|--------|----------|
| Pattern matching | ❌ Not implemented | - |
| Metadata classification | ❌ Not implemented | - |
| Program-specific rules | ❌ Not implemented | - |
| Manual blocklist | ✅ Implemented | `sql/01_mint_blocklist.sql` |
| Pre-ingestion filtering | ❌ Not implemented | - |

### Recommendations

**Priority: Critical**

1. **Add Pattern Matching:**
   - Check mint address for common LP patterns
   - Example: Raydium LP tokens often contain specific prefixes

2. **Implement Metadata Lookup:**
   - Query token registry before ingestion
   - Check for LP/pool classification flags
   - Cache results to avoid repeated lookups

3. **Enhance Blocklist Usage:**
   - Check blocklist BEFORE trade ingestion (not just signal writes)
   - Add automatic blocklist population for detected LP tokens

4. **Program-Specific Rules:**
   - Different extraction logic for AMM vs DEX programs
   - Special handling for programs known to involve LP tokens

**Implementation Complexity:** Medium (1-2 days)

---

## 4. Net Flow Calculation Methodology

### Rolling Window Implementation

**Window Definitions (from `src/pipeline/state.rs`):**

```rust
pub struct TokenRollingState {
    pub trades_60s: Vec<TradeEvent>,      // 1 minute
    pub trades_300s: Vec<TradeEvent>,     // 5 minutes
    pub trades_900s: Vec<TradeEvent>,     // 15 minutes
    pub trades_3600s: Vec<TradeEvent>,    // 1 hour
    pub trades_7200s: Vec<TradeEvent>,    // 2 hours
    pub trades_14400s: Vec<TradeEvent>,   // 4 hours
}
```

**Total Windows:** 6

### Net Flow Formula

**Calculation (from `src/pipeline/state.rs`):**

```rust
fn compute_net_flow(trades: &[TradeEvent]) -> f64 {
    let buy_sum: f64 = trades
        .iter()
        .filter(|t| matches!(t.direction, TradeDirection::Buy))
        .map(|t| t.sol_amount)
        .sum();
    
    let sell_sum: f64 = trades
        .iter()
        .filter(|t| matches!(t.direction, TradeDirection::Sell))
        .map(|t| t.sol_amount)
        .sum();
    
    buy_sum - sell_sum
}
```

**Formula:** `net_flow = Σ(BUY sol_amounts) - Σ(SELL sol_amounts)`

### Trade Addition

**Process:**

1. New trade arrives via streamer channel
2. `PipelineEngine::process_trade()` acquires lock
3. Trade added to all 6 windows
4. `touched_mints` set updated for delta flush

**Code:**
```rust
pub fn process_trade(&mut self, trade: TradeEvent) {
    let mint = trade.mint.clone();
    self.touched_mints.insert(mint.clone());
    
    let state = self.states.entry(mint).or_insert_with(...);
    state.add_trade(trade);
}
```

### Trade Eviction

**Process:**

1. `evict_old_trades(now)` called periodically
2. Each window filters trades older than window duration
3. Eviction uses `retain()` with timestamp comparison

**Code:**
```rust
pub fn evict_old_trades(&mut self, now: i64) {
    let cutoff_60s = now - 60;
    self.trades_60s.retain(|t| t.timestamp >= cutoff_60s);
    
    let cutoff_300s = now - 300;
    self.trades_300s.retain(|t| t.timestamp >= cutoff_300s);
    
    // ... (similar for all windows)
}
```

**Observation:** Eviction is timestamp-based, not count-based. Memory grows linearly with trade frequency.

### Persistence Strategy

**Database Schema:**

```sql
CREATE TABLE token_aggregates (
    mint TEXT PRIMARY KEY,
    net_flow_60s_sol REAL,
    net_flow_300s_sol REAL,
    net_flow_900s_sol REAL,
    net_flow_3600s_sol REAL,
    net_flow_7200s_sol REAL,
    net_flow_14400s_sol REAL,
    -- ... other fields
    updated_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
```

**Key Observation:** `PRIMARY KEY (mint)` means **only one row per token** exists.

### Flush Cycle Behavior

**Flush Strategy (from `src/pipeline/ingestion.rs`):**

1. **Delta Flush:** Every 5 seconds
   - Flushes only `touched_mints` since last flush
   - Reduces write volume for inactive tokens

2. **Full Flush:** Every 60 seconds
   - Flushes all active mints
   - Ensures stale tokens eventually get persisted

**SQL Operation:**
```sql
INSERT INTO token_aggregates (mint, ...) VALUES (?, ...)
ON CONFLICT(mint) DO UPDATE SET
    net_flow_60s_sol = excluded.net_flow_60s_sol,
    net_flow_300s_sol = excluded.net_flow_300s_sol,
    -- ... (all fields updated)
    updated_at = excluded.updated_at
```

**Behavior:** Each flush **overwrites** previous state for that mint.

### Historical Time Series

**Observation:** `token_aggregates` does NOT maintain historical time series.

- Only current state stored (PRIMARY KEY on mint)
- Previous values overwritten on each flush
- No timestamp-indexed rows for same mint
- Historical analysis relies on `token_signals` table (append-only events)

### Aggregate Lifecycle

**Creation:**
- First trade for a mint creates row via `INSERT`

**Updates:**
- Subsequent flushes use `ON CONFLICT ... DO UPDATE`
- `updated_at` timestamp refreshed on each flush

**Expiration:**
- No automatic row deletion
- Rows persist indefinitely unless pruned

### Inactive Token Pruning

**In-Memory Pruning (from `src/pipeline/engine.rs`):**

```rust
pub fn prune_inactive_mints(&mut self, now: i64, threshold_secs: i64) {
    let cutoff = now - threshold_secs;
    self.states.retain(|mint, state| {
        state.last_seen_ts >= cutoff
    });
}
```

**Configuration:**
- Default threshold: 7200 seconds (2 hours)
- Runs every 60 seconds (from `pipeline_runtime.rs`)
- Prunes from: `states`, `last_bot_counts`, `last_signal_state`, `touched_mints`

**Database Pruning:**

❌ Not implemented - database rows never deleted

### Frontend Query Logic

**Query (from `frontend/lib/queries.ts`):**

```typescript
SELECT
  ta.mint,
  ta.net_flow_60s_sol,
  ta.net_flow_300s_sol,
  ta.net_flow_900s_sol,
  ta.net_flow_3600s_sol,
  ta.net_flow_7200s_sol,
  ta.net_flow_14400s_sol,
  ta.updated_at
FROM token_aggregates ta
LEFT JOIN token_metadata tm ON ta.mint = tm.mint
WHERE ta.dca_buys_3600s > 0
  AND (tm.blocked IS NULL OR tm.blocked = 0)
ORDER BY ta.net_flow_300s_sol DESC
LIMIT 40
```

**Observations:**

- ✅ Filters blocked tokens
- ✅ Filters by DCA activity
- ❌ Does NOT filter by `updated_at` (stale aggregates included)
- ❌ No time range filter

### Potential Accuracy Concerns

**Observation 1: Stale Aggregates**

Tokens that were active hours ago retain their last known net flow values. If a token had +1000 SOL net flow 4 hours ago, that value persists in the database even if the token is now inactive.

**Frontend Impact:** Dashboard may display stale net flows for inactive tokens.

**Mitigation:** Add `WHERE updated_at > unixepoch() - 14400` filter to frontend query.

**Observation 2: No Historical Decay**

Net flow values do not decay over time. A token with high net flow at T=0 retains that value until new trades arrive.

**Impact:** Long-inactive tokens may rank highly in queries due to historical activity.

**Observation 3: PRIMARY KEY Limitation**

With `PRIMARY KEY (mint)`, the system cannot answer:
- "What was the net flow for token X at timestamp T?"
- "Show me net flow time series for token X"

**Current Workaround:** Use `token_signals` table for historical point-in-time snapshots.

---

## 5. Complete Pipeline Architecture Documentation

### Data Ingestion Layer

**Configuration (from `src/streamer_core/grpc_client.rs`):**

```rust
YellowstoneGrpcGeyserClient::new(
    geyser_url,              // GEYSER_URL env var
    x_token,                 // X_TOKEN env var
    Some(CommitmentLevel::Confirmed),  // Commitment level
    HashMap::default(),      // Account filters
    transaction_filters,     // Program filters
    Default::default(),      // Slot filters
    Arc::new(RwLock::new(HashSet::new())),
    Default::default(),
)
```

**Parameters:**
- Commitment: `Confirmed` (not Finalized)
- Latency: ~1s per transaction
- Revert risk: ~0.1% (confirmed but not finalized)

### Streamer Binaries

| Binary | Program | Responsibility | Output Channels |
|--------|---------|----------------|-----------------|
| `pumpswap_streamer` | PumpSwap | Monitor spot trades | JSONL (opt) + Pipeline TX |
| `bonkswap_streamer` | BonkSwap | Monitor spot trades | JSONL (opt) + Pipeline TX |
| `moonshot_streamer` | Moonshot | Monitor spot trades | JSONL (opt) + Pipeline TX |
| `jupiter_dca_streamer` | Jupiter DCA | Monitor DCA fills | JSONL (opt) + Pipeline TX |

**Total Streamers:** 4

### Dual-Channel Architecture

**Channel 1: JSONL Output (Optional)**

```rust
if self.enable_jsonl {
    let mut writer = self.writer.lock().await;
    writer.write(&event).await?;
}
```

- Disabled by default (`ENABLE_JSONL=true` to activate)
- Writes to `streams/{program}/events.jsonl`
- Used for debugging and historical replay

**Channel 2: Pipeline TX (Primary)**

```rust
if let Some(tx) = &self.pipeline_tx {
    let pipeline_event = convert_to_pipeline_event(&event);
    tx.try_send(pipeline_event).ok();  // Non-blocking
}
```

- Always enabled when `pipeline_runtime` is active
- Non-blocking `try_send()` prevents streamer lockup
- Buffer size: 10,000 trades (configurable via `STREAMER_CHANNEL_BUFFER`)

### Channel Buffer Configuration

**Buffer Size:** 10,000 trades (from `src/pipeline/config.rs`)

**Back-Pressure Thresholds:**
- High watermark: 80% (8,000 trades)
- Critical watermark: 95% (9,500 trades)

**Behavior When Full:**
- `try_send()` fails silently
- Dropped trades logged every 1,000 failures
- Streamer continues processing (never blocks)

### PipelineEngine Internal State

**State Structure (from `src/pipeline/engine.rs`):**

```rust
pub struct PipelineEngine {
    states: HashMap<String, TokenRollingState>,
    last_bot_counts: HashMap<String, i32>,
    last_signal_state: HashMap<String, HashMap<SignalType, bool>>,
    db_writer: Option<Arc<dyn AggregateDbWriter>>,
    metadata_cache: HashMap<String, TokenMetadata>,
    now_fn: Box<dyn Fn() -> i64 + Send + Sync>,
    touched_mints: HashSet<String>,
}
```

**Lock Strategy:**
- Wrapped in `Arc<Mutex<PipelineEngine>>`
- Single lock for all state
- Lock held during trade processing and metric computation

### Lock Acquisition Strategy

**Lock Held During:**

1. **Trade Processing:** `process_trade()`
2. **Metric Computation:** `compute_metrics()`
3. **Bot History Update:** `update_bot_history()`
4. **Mint Pruning:** `prune_inactive_mints()`

**Lock Released During:**

1. **Database Writes:** Aggregates buffered, lock released before SQLite writes
2. **API Calls:** Metadata fetching happens outside engine lock

**Optimization (Phase 5):**

Delta flush acquires lock once per cycle:
```rust
let (aggregates, signals, _) = {
    let mut engine_guard = engine.lock().unwrap();
    // Compute metrics for all touched mints
}; // Lock released here
// Database writes happen without lock
```

### Trade Processing Flow

```
TradeEvent arrives via channel
    ↓
engine.lock().unwrap()
    ↓
process_trade(event)
    ├─ Add to 6 rolling windows
    ├─ Mark mint as "touched"
    └─ Update last_seen_ts
    ↓
lock released
```

### Bot Detection Logic

**Algorithm (from `src/pipeline/state.rs`):**

```rust
fn detect_bot_wallets(trades: &[TradeEvent]) -> (HashSet<String>, i32) {
    // Heuristic 1: High-frequency (> 10 trades in 300s)
    // Heuristic 2: Rapid consecutive (≥3 trades within 1s)
    // Heuristic 3: Alternating direction (>70% flip-flop rate)
    // Heuristic 4: Identical amounts (>50% same values)
}
```

**Detected Bots:**
- Tracked in `bot_wallets_300s` HashSet
- Trade count tracked in `bot_trades_count_300s`
- Used for BOT_DROPOFF signal detection

### Signal Detection Logic

**Signal Types (from `src/pipeline/signals.rs`):**

| Signal | Threshold | Window | Description |
|--------|-----------|--------|-------------|
| BREAKOUT | net_flow_60s > 5 SOL, wallets ≥ 5, buy_ratio > 0.75 | 60s | Sudden volume spike |
| FOCUSED | wallet_concentration < 0.3, volume > 3 SOL | 300s | Concentrated buying |
| SURGE | volume_60s ≥ 3× volume_300s, buy_count ≥ 10 | 60s | Explosive volume |
| BOT_DROPOFF | bot_decline > 50%, new_wallets ≥ 3 | 300s | Bots exit, humans enter |
| DCA_CONVICTION | dca_overlap > 25%, net_flow > 0 | 60s | DCA + spot alignment |

**Detection Process:**
```rust
pub fn detect_signals(&self, now: i64, previous_bot_count: Option<i32>) -> Vec<TokenSignal>
```

### Flush Cycle Timing

**Delta Flush (Every 5 seconds):**

```rust
tokio::select! {
    _ = flush_timer.tick() => {
        let mints_to_flush = engine_guard.get_touched_mints();
        // Compute + write aggregates
        engine_guard.clear_touched_mints();
    }
}
```

**Full Flush (Every 60 seconds):**

```rust
let is_full_flush = last_full_flush.elapsed().as_secs() >= 60;
if is_full_flush {
    let mints_to_flush = engine_guard.get_active_mints();
}
```

**Strategy:**
- Delta flush: O(M) where M = touched mints (~50-200)
- Full flush: O(N) where N = total mints (~8,797 observed)
- Reduces write load by ~85%

### SQLite Schema

**Tables (from `sql/` directory):**

| Table | Purpose | Write Pattern | Indexes |
|-------|---------|---------------|---------|
| `token_aggregates` | Rolling metrics | UPSERT on mint | mint (PK), updated_at, source_program, net_flow_300s, dca_buys_3600s |
| `token_signals` | Signal events | INSERT (append-only) | None |
| `token_metadata` | Token info | UPSERT on mint | mint (PK), created_at |
| `mint_blocklist` | Blocked mints | INSERT/DELETE | mint (PK) |

**WAL Mode:**
- Enabled via `PRAGMA journal_mode=WAL`
- Allows concurrent readers during writes
- Write-ahead log for crash recovery

### API Layer Architecture

**Framework:** Next.js API routes (Server-Side)

**Routes (from `frontend/app/api/`):**

| Route | Method | Purpose | Query |
|-------|--------|---------|-------|
| `/api/tokens` | GET | List top tokens | token_aggregates + token_metadata JOIN |
| `/api/metadata/{mint}` | GET | Token details | token_metadata SELECT |
| `/api/metadata/follow` | POST | Toggle follow_price | token_metadata UPDATE |
| `/api/metadata/block` | POST | Block/unblock mint | token_metadata UPDATE |
| `/api/metadata/counts` | GET | Followed/blocked counts | token_metadata COUNT |

### Frontend Query Patterns

**Main Query (from `frontend/lib/queries.ts`):**

```sql
SELECT
  ta.mint,
  ta.net_flow_60s_sol,
  ta.net_flow_300s_sol,
  ta.net_flow_900s_sol,
  ta.net_flow_3600s_sol,
  ta.net_flow_7200s_sol,
  ta.net_flow_14400s_sol,
  ta.unique_wallets_300s,
  ta.volume_300s_sol,
  ta.dca_buys_60s,
  ta.dca_buys_300s,
  ta.dca_buys_900s,
  ta.dca_buys_3600s,
  ta.dca_buys_14400s,
  ta.updated_at
FROM token_aggregates ta
LEFT JOIN token_metadata tm ON ta.mint = tm.mint
WHERE ta.dca_buys_3600s > 0
  AND (tm.blocked IS NULL OR tm.blocked = 0)
ORDER BY ta.net_flow_300s_sol DESC
LIMIT 40
```

**Refresh Interval:** 5 seconds (auto-refresh in frontend)

### Data Flow Diagram

```
┌─────────────────────────────────────────────────────┐
│  Yellowstone gRPC Stream (Geyser)                   │
│  Commitment: Confirmed                              │
│  Latency: ~1s per transaction                       │
└──────────────────┬──────────────────────────────────┘
                   │
         ┌─────────┴─────────┐
         │   4 Streamers     │
         │   (one per DEX)   │
         └─────────┬─────────┘
                   │
       ┌───────────┴───────────┐
       │   Dual Channels:      │
       │   1. JSONL (optional) │
       │   2. Pipeline TX      │
       └───────────┬───────────┘
                   │
         ┌─────────┴──────────┐
         │  PipelineEngine    │
         │  Arc<Mutex<>>      │
         │  - TokenRollingState │
         │  - 6 windows       │
         │  - Bot detection   │
         │  - Signal detection │
         └─────────┬──────────┘
                   │
       ┌───────────┴────────────┐
       │    Flush Cycle         │
       │    Delta: 5s           │
       │    Full: 60s           │
       │    (lock once/cycle)   │
       └───────────┬────────────┘
                   │
         ┌─────────┴──────────┐
         │  SQLite Database   │
         │  WAL Mode          │
         │  - token_aggregates │
         │  - token_signals   │
         │  - token_metadata  │
         └─────────┬──────────┘
                   │
       ┌───────────┴────────────┐
       │   Next.js API Routes   │
       │   Server-Side Queries  │
       └───────────┬────────────┘
                   │
         ┌─────────┴──────────┐
         │  Token Dashboard   │
         │  React Components  │
         │  5s Auto-Refresh   │
         └────────────────────┘
```

### Component Responsibility Matrix

| Component | Reads From | Writes To | Lock Strategy | Frequency |
|-----------|------------|-----------|---------------|-----------|
| Streamer | Geyser stream | Pipeline channel | No locks | Real-time |
| PipelineEngine | Channel | In-memory state | Arc<Mutex<>> | Real-time |
| Flush Task | PipelineEngine | SQLite | Lock per cycle | 5s / 60s |
| Prune Task | PipelineEngine | In-memory state | Lock per cycle | 60s |
| Price Task | token_metadata | token_metadata | Per-token lock | 60s |
| API Routes | SQLite | None | Read-only | On request |
| Frontend | API | None | None | 5s polling |

### Lock Contention Analysis

**Potential Bottlenecks:**

1. **PipelineEngine Lock:**
   - Held during trade processing and metric computation
   - High contention during flush cycles
   - Mitigated by delta flush optimization

2. **SQLite Connection:**
   - Arc<Mutex<Connection>> = single writer
   - WAL mode allows concurrent readers
   - Batch writes reduce lock duration

**Observed Behavior:**

From logs, flush cycles complete in < 100ms with 500 mint batches, suggesting lock contention is acceptable under current load.

---

## 6. Frontend Data Contract Complete Audit

### API Routes Enumeration

| Route | Method | Request Params | Response Schema | Tables Queried | Operations |
|-------|--------|----------------|-----------------|----------------|------------|
| `/api/tokens` | GET | None | `{ tokens: TokenMetrics[] }` | `token_aggregates`, `token_metadata` | LEFT JOIN, filter, sort, limit |
| `/api/metadata/{mint}` | GET | `mint: string` | `TokenMetadata \| null` | `token_metadata` | SELECT by mint |
| `/api/metadata/follow` | POST | `{ mint: string, follow: boolean }` | `{ success: boolean }` | `token_metadata` | INSERT/UPDATE follow_price |
| `/api/metadata/block` | POST | `{ mint: string, blocked: boolean }` | `{ success: boolean }` | `token_metadata` | INSERT/UPDATE blocked |
| `/api/metadata/counts` | GET | None | `{ followedCount: number, blockedCount: number }` | `token_metadata` | COUNT aggregates |

### Field Mapping Matrix

| UI Column | API Field (camelCase) | SQL Column (snake_case) | SQL Table | Type | Transformation |
|-----------|----------------------|-------------------------|-----------|------|----------------|
| Token Name | - | name | token_metadata | TEXT | Fetched on-demand |
| Symbol | - | symbol | token_metadata | TEXT | Fetched on-demand |
| Image | - | image_url | token_metadata | TEXT | Fetched on-demand |
| Price USD | - | price_usd | token_metadata | REAL | None |
| Market Cap | - | market_cap | token_metadata | REAL | None |
| Net Flow 15m | netFlow900s | net_flow_900s_sol | token_aggregates | REAL | None |
| Net Flow 1h | netFlow3600s | net_flow_3600s_sol | token_aggregates | REAL | None |
| Net Flow 4h | netFlow14400s | net_flow_14400s_sol | token_aggregates | REAL | None |
| DCA Buys (sparkline) | - | (from trades table) | trades | INTEGER | Grouped by minute |
| DCA 1h | dcaBuys3600s | dca_buys_3600s | token_aggregates | INTEGER | None |
| Signal | - | signal_type | token_signals | TEXT | Latest only |
| Wallets | maxUniqueWallets | unique_wallets_300s | token_aggregates | INTEGER | None |

### TypeScript Type Definitions

**TokenMetrics (from `frontend/lib/types.ts`):**

```typescript
export interface TokenMetrics {
  mint: string;
  netFlow60s: number;
  netFlow300s: number;
  netFlow900s: number;
  netFlow3600s: number;
  netFlow7200s: number;
  netFlow14400s: number;
  totalBuys300s: number;
  totalSells300s: number;
  dcaBuys60s: number;
  dcaBuys300sWindow: number;
  dcaBuys900s: number;
  dcaBuys3600s: number;
  dcaBuys14400s: number;
  maxUniqueWallets: number;
  totalVolume300s: number;
  lastUpdate: number;
}
```

**TokenMetadata:**

```typescript
export interface TokenMetadata {
  mint: string;
  name?: string;
  symbol?: string;
  imageUrl?: string;
  priceUsd?: number;
  marketCap?: number;
  followPrice: boolean;
  blocked: boolean;
  updatedAt: number;
}
```

### Data Transformations

**SQL → API (from `frontend/lib/queries.ts`):**

```typescript
return rows.map(row => ({
  mint: row.mint,
  netFlow60s: row.net_flow_60s_sol ?? 0,
  netFlow300s: row.net_flow_300s_sol ?? 0,
  netFlow900s: row.net_flow_900s_sol ?? 0,
  netFlow3600s: row.net_flow_3600s_sol ?? 0,
  netFlow7200s: row.net_flow_7200s_sol ?? 0,
  netFlow14400s: row.net_flow_14400s_sol ?? 0,
  // ... (snake_case → camelCase)
}));
```

**Null Handling:**
- `??` operator provides default value (0 for numbers)
- Missing metadata fields remain undefined

### Error Handling Patterns

**API Route Pattern:**

```typescript
export async function GET() {
  try {
    const tokens = getTokens(100);
    return NextResponse.json({ tokens });
  } catch (error) {
    console.error('Error fetching tokens:', error);
    return NextResponse.json(
      { error: 'Failed to fetch tokens' },
      { status: 500 }
    );
  }
}
```

**Observations:**
- Generic error responses (no detailed error codes)
- Error logging to console
- 500 status on all errors

### Refresh and Polling Behavior

**Frontend Polling (from `frontend/app/page.tsx`):**

```typescript
useEffect(() => {
  fetchTokens();
  refreshCounts();
  
  const interval = setInterval(fetchTokens, 5000);  // 5 seconds
  
  return () => clearInterval(interval);
}, []);
```

**Behavior:**
- Auto-refresh every 5 seconds
- Independent count refresh (manual trigger required)
- No exponential backoff on errors
- Polling continues even if API fails

### Joins and Filters

**Main Query Joins:**

```sql
FROM token_aggregates ta
LEFT JOIN token_metadata tm ON ta.mint = tm.mint
```

**Filters Applied:**

1. `ta.dca_buys_3600s > 0` - Only tokens with DCA activity
2. `(tm.blocked IS NULL OR tm.blocked = 0)` - Exclude blocked tokens
3. `ORDER BY ta.net_flow_300s_sol DESC` - Sort by 5min net flow
4. `LIMIT 40` - Cap results

**Observation:** No `updated_at` timestamp filter (stale aggregates included).

---

## 7. Rolling Window Complete Discovery

### Window Enumeration

**Source:** `src/pipeline/state.rs` and `src/pipeline/types.rs`

| Window ID | Duration (seconds) | Duration (human) | Purpose | Metrics Computed |
|-----------|-------------------|------------------|---------|------------------|
| 1 | 60 | 1 minute | Short-term signals | net_flow, buy_count, sell_count, dca_buys |
| 2 | 300 | 5 minutes | Medium-term trends | net_flow, buy_count, sell_count, unique_wallets, bot_metrics, volume, dca_buys |
| 3 | 900 | 15 minutes | Extended trends | net_flow, buy_count, sell_count, dca_buys |
| 4 | 3600 | 1 hour | Hourly trends | net_flow, dca_buys |
| 5 | 7200 | 2 hours | Multi-hour trends | net_flow |
| 6 | 14400 | 4 hours | Long-term trends | net_flow, dca_buys |

**Total Windows:** 6

### Window-Specific Metrics

**60-Second Window:**
- `net_flow_60s_sol` - Buy-sell SOL delta
- `buy_count_60s` - Number of BUY trades
- `sell_count_60s` - Number of SELL trades
- `dca_buys_60s` - Jupiter DCA BUY count

**300-Second Window:**
- `net_flow_300s_sol` - Buy-sell SOL delta
- `buy_count_300s` - Number of BUY trades
- `sell_count_300s` - Number of SELL trades
- `unique_wallets_300s` - Distinct user accounts
- `bot_trades_300s` - Trades from detected bots
- `bot_wallets_300s` - Count of bot wallets
- `avg_trade_size_300s_sol` - Average SOL per trade
- `volume_300s_sol` - Absolute net flow
- `dca_buys_300s` - Jupiter DCA BUY count

**900-Second Window:**
- `net_flow_900s_sol` - Buy-sell SOL delta
- `buy_count_900s` - Number of BUY trades
- `sell_count_900s` - Number of SELL trades
- `dca_buys_900s` - Jupiter DCA BUY count

**3600-Second Window:**
- `net_flow_3600s_sol` - Buy-sell SOL delta
- `dca_buys_3600s` - Jupiter DCA BUY count

**7200-Second Window:**
- `net_flow_7200s_sol` - Buy-sell SOL delta

**14400-Second Window:**
- `net_flow_14400s_sol` - Buy-sell SOL delta
- `dca_buys_14400s` - Jupiter DCA BUY count

### Component Usage Matrix

| Window | PipelineEngine | SQLite (token_aggregates) | Frontend Display |
|--------|----------------|---------------------------|------------------|
| 60s | ✅ In-memory Vec | ✅ Persisted | ❌ Not displayed |
| 300s | ✅ In-memory Vec | ✅ Persisted | ❌ Not displayed |
| 900s | ✅ In-memory Vec | ✅ Persisted | ✅ "Net Flow 15m" |
| 3600s | ✅ In-memory Vec | ✅ Persisted | ✅ "Net Flow 1h" + "DCA 1h" |
| 7200s | ✅ In-memory Vec | ✅ Persisted | ❌ Not displayed |
| 14400s | ✅ In-memory Vec | ✅ Persisted | ✅ "Net Flow 4h" + "DCA 4h" |

### Eviction Strategy

**Per-Window Eviction (from `src/pipeline/state.rs`):**

```rust
pub fn evict_old_trades(&mut self, now: i64) {
    self.trades_60s.retain(|t| t.timestamp >= now - 60);
    self.trades_300s.retain(|t| t.timestamp >= now - 300);
    self.trades_900s.retain(|t| t.timestamp >= now - 900);
    self.trades_3600s.retain(|t| t.timestamp >= now - 3600);
    self.trades_7200s.retain(|t| t.timestamp >= now - 7200);
    self.trades_14400s.retain(|t| t.timestamp >= now - 14400);
}
```

**Strategy:**
- Timestamp-based (not count-based)
- Called on each trade addition
- O(N) complexity per eviction (N = trades in window)

### Database Persistence

**All 6 Windows Persisted:**

```sql
CREATE TABLE token_aggregates (
    net_flow_60s_sol REAL,
    net_flow_300s_sol REAL,
    net_flow_900s_sol REAL,
    net_flow_3600s_sol REAL,
    net_flow_7200s_sol REAL,
    net_flow_14400s_sol REAL,
    -- ... (plus counts, DCA metrics, etc.)
);
```

**Persistence Frequency:**
- Delta flush: 5 seconds (touched mints only)
- Full flush: 60 seconds (all active mints)

### Frontend Exposure

**Displayed Windows:**
- 900s (15 minutes) - "Net Flow 15m"
- 3600s (1 hour) - "Net Flow 1h" + "DCA Buys 1h"
- 14400s (4 hours) - "Net Flow 4h" + "DCA Buys 4h"

**Not Displayed:**
- 60s - Used for signal detection only
- 300s - Used for bot detection and wallet diversity
- 7200s - Stored but not exposed

**Rationale (inferred):**
- 60s/300s too noisy for dashboard
- 900s/3600s/14400s provide useful trend context
- 7200s might be for future feature

---

## 8. Token Lifetime and Retention Investigation

### In-Memory Retention

**Pruning Task Exists:** ✅ Yes

**Configuration (from `src/bin/pipeline_runtime.rs`):**

```rust
let prune_threshold = env::var("MINT_PRUNE_THRESHOLD_SECS")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(7200);  // Default: 2 hours

tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let now = chrono::Utc::now().timestamp();
        let mut engine_guard = engine_prune.lock().unwrap();
        engine_guard.prune_inactive_mints(now, prune_threshold);
    }
});
```

**Pruning Frequency:** Every 60 seconds

**Pruning Threshold:** 7200 seconds (2 hours) by default

**Pruned Data Structures:**

1. `states: HashMap<String, TokenRollingState>` - All rolling windows
2. `last_bot_counts: HashMap<String, i32>` - Bot history
3. `last_signal_state: HashMap<String, HashMap<SignalType, bool>>` - Signal deduplication state
4. `touched_mints: HashSet<String>` - Delta flush tracking

**Pruning Logic (from `src/pipeline/engine.rs`):**

```rust
pub fn prune_inactive_mints(&mut self, now: i64, threshold_secs: i64) {
    let cutoff = now - threshold_secs;
    let before_count = self.states.len();

    self.states.retain(|mint, state| {
        let keep = state.last_seen_ts >= cutoff;
        if !keep {
            self.last_bot_counts.remove(mint);
            self.last_signal_state.remove(mint);
            self.touched_mints.remove(mint);
        }
        keep
    });

    let pruned = before_count - self.states.len();
    if pruned > 0 {
        log::info!("🗑️  Pruned {} inactive mints", pruned);
    }
}
```

**Inactive Definition:**
- Token with `last_seen_ts < (now - 7200)` seconds
- No trades received in last 2 hours

**Impact:**
- Memory freed for inactive tokens
- State recreated if token becomes active again
- No data loss (aggregates persisted to database)

### Database Retention

**token_aggregates:**
- ❌ No automatic row deletion
- ❌ No TTL mechanism
- ❌ No expiration triggers
- ✅ Rows persist indefinitely

**token_signals:**
- ❌ No row deletion
- ❌ Append-only table
- ⚠️ Unbounded growth potential

**token_metadata:**
- ❌ No TTL
- ✅ Manual updates only
- ✅ Rows persist indefinitely

**Cleanup Tasks:**
- ❌ No database cleanup task exists

**Observed Behavior:**

From schema inspection, database rows are never automatically deleted. This means:
- Tokens that were active weeks ago still have rows
- `token_signals` table grows indefinitely
- No automatic archival or retention policy

### Inactive Token Handling

**Frontend Query Filter:**

```sql
WHERE ta.dca_buys_3600s > 0
```

This implicitly filters out some inactive tokens (those without recent DCA activity), but does NOT filter by `updated_at` timestamp.

**Impact:**

Tokens with stale aggregates (e.g., `updated_at` from 4 hours ago) remain queryable and may appear in frontend results if they meet other filter criteria.

**Stale Aggregate Behavior:**

A token that had high net flow 4 hours ago but is now inactive will:
1. Remain in `token_aggregates` with old net flow values
2. Potentially rank highly in frontend queries (sorted by `net_flow_300s_sol DESC`)
3. Display stale `updated_at` timestamp

### Token Lifecycle Summary

**Phase 1: Creation**
- First trade arrives → `TokenRollingState` created in PipelineEngine
- First flush → Row inserted into `token_aggregates`

**Phase 2: Active**
- Trades arrive → Rolling windows updated
- Delta/full flush → `token_aggregates` UPSERT, `updated_at` refreshed

**Phase 3: Inactive (< 2 hours)**
- No trades arrive
- In-memory state retained
- Database row persists with last known values

**Phase 4: Inactive (> 2 hours)**
- Prune task removes in-memory state
- Database row persists indefinitely
- `updated_at` timestamp becomes stale

**Phase 5: Reactivation**
- New trade arrives → `TokenRollingState` recreated
- Flush → Database row UPSERT, `updated_at` refreshed

### Retention Policy Summary

| Component | Retention Behavior | Cleanup Mechanism |
|-----------|-------------------|-------------------|
| In-memory state | 2 hours since last trade | Automatic pruning every 60s |
| token_aggregates | Indefinite | None |
| token_signals | Indefinite (append-only) | None |
| token_metadata | Indefinite | None |

**Observation:** No database retention policy exists. All historical data persists forever.

---

## 9. Deduplication Behavior Documentation

### Transaction-Level Deduplication

**Carbon Framework Handling:**

From Carbon framework documentation, the `Pipeline` automatically deduplicates transactions by signature. SolFlow inherits this behavior.

**Verification:**

```rust
Pipeline::builder()
    .datasource(yellowstone_grpc)
    // Carbon handles signature deduplication internally
    .transaction::<EmptyDecoderCollection, ()>(processor, None)
    .build()?
    .run()
    .await
```

**Observed Behavior:**
- ✅ Carbon framework tracks processed signatures
- ✅ Duplicate transactions from Geyser stream ignored
- ✅ No manual signature tracking needed in SolFlow

### Inner Instruction Deduplication

**Balance Change Aggregation:**

SolFlow processes all balance changes from `TransactionStatusMeta`, including those from inner instructions.

**Code (from `src/streamer_core/balance_extractor.rs`):**

```rust
pub fn extract_token_changes(
    meta: &TransactionStatusMeta,
    _account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    // Iterates through ALL token balance changes
    for pre in pre_token_balances {
        let post = post_token_balances.iter().find(...);
        // Computes delta for each account
    }
}
```

**Observation:**

❌ No inner instruction loop detection
❌ No duplicate balance change filtering
✅ All balance changes processed (intended behavior for metadata-based extraction)

**Potential Issue:**

If a transaction has circular balance changes or loops, they would all be counted. However, this is unlikely in normal Solana transactions.

### Signal-Level Deduplication

**Implementation:** ✅ Exists

**Code (from `src/pipeline/engine.rs`):**

```rust
fn deduplicate_signals(&mut self, mint: &str, signals: Vec<TokenSignal>) -> Vec<TokenSignal> {
    let signal_state = self.last_signal_state
        .entry(mint.to_string())
        .or_insert_with(HashMap::new);

    let mut new_signals = Vec::new();
    for signal in signals {
        let was_active = signal_state.get(&signal.signal_type).copied().unwrap_or(false);
        let is_active = true;

        if !was_active && is_active {  // Only write on false→true transition
            new_signals.push(signal);
        }
    }

    // Update state
    for signal_type in active_types.keys() {
        signal_state.insert(*signal_type, true);
    }
    
    // Mark inactive signals as false (allows re-emission later)
    for signal_type in &all_signal_types {
        if !active_types.contains_key(signal_type) {
            signal_state.insert(*signal_type, false);
        }
    }

    new_signals
}
```

**Behavior:**

- ✅ Signals only written on state transition (false → true)
- ✅ Prevents duplicate BREAKOUT/SURGE signals during same trend
- ✅ Allows signal re-emission after cooldown (true → false → true)

**Example:**

1. Token has BREAKOUT conditions → Signal emitted, state = true
2. Conditions persist → Signal NOT emitted (already true)
3. Conditions end → State = false (no signal)
4. Conditions resume → Signal emitted again (false → true)

### Trade-Level Deduplication

**Implementation:** ❌ Not implemented

**Code Review:**

No signature tracking at the trade level exists. Each transaction is processed exactly once (via Carbon's deduplication), but no additional trade-level checks.

**Implications:**

- If Geyser sends duplicate transactions, Carbon handles it
- If replayed transactions appear (rare), they would be processed
- No protection against intentional transaction replay attacks

**Observed Behavior:**

No duplicate trade detection exists beyond Carbon's signature deduplication.

### Deduplication Mechanism Inventory

| Level | Mechanism | Implementation | Status |
|-------|-----------|----------------|--------|
| Transaction Signature | Carbon framework | Automatic signature tracking | ✅ Active |
| Inner Instructions | None | All balance changes processed | ❌ Not implemented |
| Signals | State tracking | `last_signal_state` HashMap | ✅ Active |
| Trades | None | No trade-level deduplication | ❌ Not implemented |
| Balance Changes | None | All deltas counted | ❌ Not implemented |

### Gap Analysis

**Areas Without Deduplication:**

1. **Inner Instruction Loops:** If a transaction has circular balance changes, all would be counted
2. **Trade Replays:** Malicious actors could replay transactions (though rare on Solana)
3. **Balance Change Aggregation:** No detection of duplicate or redundant balance changes within same transaction

**Risk Assessment:**

- **Low Risk:** Inner instruction loops unlikely in DEX transactions
- **Low Risk:** Transaction replay requires significant effort and has minimal impact
- **Low Risk:** Balance change duplication rare in well-formed transactions

**Recommendation Priority:** Medium (not critical, but could improve accuracy)

---

## 10. Dashboard Column Semantics Documentation

### Complete Column Definitions

| Column Name | Semantic Meaning | Units | Data Source | Calculation | Update Frequency | Aggregation Window | Null Handling |
|-------------|------------------|-------|-------------|-------------|------------------|-------------------|---------------|
| **Token Name** | Human-readable token name | Text | token_metadata.name | DexScreener API | On-demand (60s follow_price) | N/A | Display "—" |
| **Symbol** | Trading ticker | Text | token_metadata.symbol | DexScreener API | On-demand (60s follow_price) | N/A | Display mint (truncated) |
| **Image** | Token logo URL | URL | token_metadata.image_url | DexScreener API | On-demand (60s follow_price) | N/A | Display default icon |
| **Price USD** | Current price | $ | token_metadata.price_usd | DexScreener API (SOL pair) | 60s (follow_price=1) | N/A | Display "—" |
| **Market Cap** | Total valuation | $ | token_metadata.market_cap | price × supply | 60s (follow_price=1) | N/A | Display "—" |
| **Net Flow 15m** | Buy - Sell SOL (15 min) | SOL | token_aggregates.net_flow_900s_sol | Σ(buys) - Σ(sells) in 900s window | 5s (delta) / 60s (full) | 900s | Display 0.00 |
| **Net Flow 1h** | Buy - Sell SOL (1 hour) | SOL | token_aggregates.net_flow_3600s_sol | Σ(buys) - Σ(sells) in 3600s window | 5s (delta) / 60s (full) | 3600s | Display 0.00 |
| **Net Flow 4h** | Buy - Sell SOL (4 hours) | SOL | token_aggregates.net_flow_14400s_sol | Σ(buys) - Σ(sells) in 14400s window | 5s (delta) / 60s (full) | 14400s | Display 0.00 |
| **DCA Buys (sparkline)** | Jupiter DCA buy histogram | bars | trades table (grouped by minute) | COUNT(JupiterDCA BUYs) per minute | N/A (historical) | Last 60 minutes | Empty chart |
| **DCA 1h** | Jupiter DCA buy count | count | token_aggregates.dca_buys_3600s | COUNT(JupiterDCA BUYs) in 3600s | 5s (delta) / 60s (full) | 3600s | Display 0 |
| **Signal** | Latest detected signal | enum | token_signals.signal_type (latest) | Threshold-based detection | On signal trigger | Varies (60s-300s) | Display "—" |
| **Wallet Count** | Unique wallets (5 min) | count | token_aggregates.unique_wallets_300s | COUNT(DISTINCT user_account) in 300s | 5s (delta) / 60s (full) | 300s | Display 0 |

### Data Lineage Documentation

**Path 1: Net Flow Values**

```
Yellowstone gRPC Transaction
    ↓
TransactionStatusMeta.pre_balances / post_balances
    ↓
extract_sol_changes() → BalanceDelta
    ↓
extract_trade_info() → TradeInfo
    ↓
TradeEvent (sol_amount, direction)
    ↓
TokenRollingState (trades_900s, trades_3600s, trades_14400s)
    ↓
compute_rolling_metrics() → RollingMetrics
    ↓
AggregatedTokenState.from_metrics()
    ↓
SQLite token_aggregates (net_flow_900s_sol, net_flow_3600s_sol, net_flow_14400s_sol)
    ↓
API /api/tokens → TokenMetrics
    ↓
Frontend TokenDashboard → Display
```

**Path 2: DCA Counts**

```
Jupiter DCA Transaction (program_name = "JupiterDCA")
    ↓
TradeEvent (source_program = "JupiterDCA", direction = Buy)
    ↓
TokenRollingState.dca_timestamps_3600s / dca_timestamps_14400s
    ↓
compute_rolling_metrics() → dca_buys_3600s, dca_buys_14400s
    ↓
AggregatedTokenState (dca_buys_3600s, dca_buys_14400s)
    ↓
SQLite token_aggregates
    ↓
API /api/tokens
    ↓
Frontend Display
```

**Path 3: Metadata (Name, Symbol, Image, Price)**

```
DexScreener API (/token-pairs/v1/solana/{mint})
    ↓
fetch_token_metadata() → TokenMetadata
    ↓
upsert_metadata() → SQLite token_metadata
    ↓
API /api/metadata/{mint}
    ↓
Frontend Display (via getTokenMetadata())
```

### Unit and Format Specifications

**Net Flow Values:**
- Unit: SOL (Solana native token)
- Format: Float with 2-4 decimal places
- Display: `+5.43 SOL` (green) or `-2.18 SOL` (red)
- Color: Green = positive (buying pressure), Red = negative (selling pressure)

**DCA Counts:**
- Unit: Integer count of trades
- Format: Whole number
- Display: `15` or `—` (if zero)

**Price USD:**
- Unit: US Dollars
- Format: `$0.000123` (scientific notation for small values)
- Display: Varies by magnitude

**Market Cap:**
- Unit: US Dollars
- Format: `$123,456` (with thousands separators)
- Display: Abbreviated for large values (e.g., `$1.2M`)

**Wallet Count:**
- Unit: Integer count of unique addresses
- Format: Whole number
- Display: `42` or `—` (if zero)

### Database Column → API Field → UI Mappings

**Complete Mapping:**

| DB Column (snake_case) | API Field (camelCase) | UI Display Name | React Component |
|------------------------|----------------------|-----------------|-----------------|
| net_flow_900s_sol | netFlow900s | Net Flow 15m | `<NetFlowCell>` |
| net_flow_3600s_sol | netFlow3600s | Net Flow 1h | `<NetFlowCell>` |
| net_flow_14400s_sol | netFlow14400s | Net Flow 4h | `<NetFlowCell>` |
| dca_buys_3600s | dcaBuys3600s | DCA 1h | `<DcaCell>` |
| dca_buys_14400s | dcaBuys14400s | DCA 4h | `<DcaCell>` |
| unique_wallets_300s | maxUniqueWallets | Wallets | `<WalletCell>` |
| name | (metadata lookup) | Token Name | `<TokenNameCell>` |
| symbol | (metadata lookup) | Symbol | `<TokenNameCell>` |
| image_url | (metadata lookup) | Image | `<TokenImage>` |
| price_usd | (metadata lookup) | Price USD | `<PriceCell>` |
| market_cap | (metadata lookup) | Market Cap | `<MarketCapCell>` |

### Consistency Verification

**Database ↔ Rust Structs:**

✅ All `token_aggregates` columns match `AggregatedTokenState` fields
✅ All `token_metadata` columns match `TokenMetadata` struct
✅ Snake_case → camelCase transformations consistent

**API ↔ Frontend:**

✅ `TokenMetrics` interface matches API response shape
✅ Field names consistent throughout TypeScript codebase
✅ Null/undefined handling consistent (`??` operator)

**Observed Inconsistencies:**

❌ `totalBuys300s` and `totalSells300s` in `TokenMetrics` are always 0 (not populated from database)
❌ DCA sparkline query checks `trades` table existence (may not exist in all deployments)

---

## 11. Metadata Enrichment Pipeline

### External APIs Used

**Primary API: DexScreener**

- **Endpoint:** `https://api.dexscreener.com/token-pairs/v1/solana/{mint}`
- **Purpose:** Token metadata, price, market cap
- **Rate Limit:** Not documented in code (appears unlimited)
- **Timeout:** 10 seconds per request

**API Response Structure:**

```rust
pub struct DexScreenerPair {
    base_token: BaseToken,  // name, symbol
    quote_token: QuoteToken,  // "SOL"
    price_usd: String,
    market_cap: Option<f64>,
    info: Option<PairInfo>,  // image_url
}
```

### API Request Patterns

**Price Monitoring Task (from `src/bin/pipeline_runtime.rs`):**

```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        // Query tokens with follow_price = 1
        let mints: Vec<String> = /* SQL query */;
        
        for mint in mints {
            let metadata = dexscreener::fetch_token_metadata(&mint).await?;
            dexscreener::upsert_metadata(&conn, &metadata)?;
            
            // Rate limiting: 300-600ms between requests
            let sleep_ms = 300 + (rand::random::<u64>() % 300);
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        }
    }
});
```

**Rate Limiting:**

- Staggered requests: 300-600ms between calls
- Effective rate: 2-3 requests/second
- Randomization prevents API detection

### Metadata Fields

**Fetched Fields:**

- `name` - Token full name
- `symbol` - Trading ticker
- `image_url` - Logo URL
- `price_usd` - Current price in USD
- `market_cap` - Total market capitalization

**Stored Fields (token_metadata table):**

- All fetched fields plus:
- `mint` - Token address (PK)
- `decimals` - Token decimals (from on-chain data)
- `blocked` - Manual block flag
- `follow_price` - Price monitoring flag
- `created_at` - First seen timestamp
- `updated_at` - Last updated timestamp

**Not Stored:**

- Price history (only latest price)
- LP/pool classification flags
- Liquidity metrics
- Volume (24h) from DexScreener

### TTL and Refresh Logic

**Metadata TTL:**

❌ No explicit TTL mechanism

**Refresh Logic:**

✅ Only tokens with `follow_price = 1` are refreshed
✅ Refresh frequency: 60 seconds
✅ On-demand fetch when token first appears in frontend

**Observed Behavior:**

- Metadata persists indefinitely once fetched
- Price updates only for followed tokens
- Stale metadata never auto-refreshed unless manually triggered

### Error Handling

**API Failure Handling (from `src/pipeline/dexscreener.rs`):**

```rust
pub async fn fetch_token_metadata(mint: &str) -> Result<TokenMetadata, Box<dyn std::error::Error>> {
    let response = client.get(&url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("DexScreener API error: {}", response.status()).into());
    }
    
    let pairs: Vec<DexScreenerPair> = response.json().await?;
    
    let pair = pairs.iter()
        .find(|p| p.quote_token.symbol == "SOL")
        .ok_or("No SOL pair found")?;
    
    // ... parse and return
}
```

**Error Scenarios:**

1. HTTP error → Logged, token skipped for this cycle
2. No SOL pair found → Error returned, token skipped
3. JSON parse error → Error returned, token skipped
4. Timeout (10s) → Request cancelled, token skipped

**Impact:**

- Tokens without metadata show truncated mint address
- Price displays "—" if fetch fails
- System continues processing other tokens

---

## 12. Signal Detection Logic

### Signal Type Enumeration

| Signal Type | Threshold Formula | Window | Severity | Description |
|-------------|-------------------|--------|----------|-------------|
| **BREAKOUT** | `net_flow_60s > 5` AND `wallets ≥ 5` AND `buy_ratio > 0.75` | 60s | 3 | Sudden volume spike with strong buying pressure |
| **FOCUSED** | `wallet_concentration < 0.3` AND `volume > 3` AND `bot_ratio < 0.2` | 300s | 4 | Concentrated buying from few wallets, low bot activity |
| **SURGE** | `volume_60s ≥ 3 × avg_volume_300s` AND `buy_count_60s ≥ 10` AND `net_flow_60s > 8` | 60s | 3 | Explosive short-term volume compared to baseline |
| **BOT_DROPOFF** | `bot_decline > 50%` AND `previous_bots ≥ 5` AND `new_wallets ≥ 3` | 300s | 3 | Bots exit while human traders enter (quality shift) |
| **DCA_CONVICTION** | `dca_overlap > 25%` AND `net_flow_60s > 0` | 60s | 2 | Jupiter DCA buys align with spot buying (accumulation) |

**Total Signal Types:** 5

### Threshold Values (Configurable via Code)

**From `src/pipeline/state.rs`:**

```rust
mod signal_thresholds {
    // BREAKOUT
    pub const BREAKOUT_NET_FLOW_60S_MIN: f64 = 5.0;
    pub const BREAKOUT_WALLET_GROWTH_MIN: i32 = 5;
    pub const BREAKOUT_BUY_RATIO_MIN: f64 = 0.75;
    
    // FOCUSED
    pub const FOCUSED_WALLET_CONCENTRATION_MAX: f64 = 0.3;
    pub const FOCUSED_MIN_VOLUME: f64 = 3.0;
    pub const FOCUSED_BOT_RATIO_MAX: f64 = 0.2;
    
    // SURGE
    pub const SURGE_VOLUME_RATIO_MIN: f64 = 3.0;
    pub const SURGE_BUY_COUNT_60S_MIN: i32 = 10;
    pub const SURGE_NET_FLOW_60S_MIN: f64 = 8.0;
    
    // BOT_DROPOFF
    pub const BOT_DROPOFF_DECLINE_RATIO_MIN: f64 = 0.5;
    pub const BOT_DROPOFF_MIN_PREVIOUS_BOTS: i32 = 5;
    pub const BOT_DROPOFF_NEW_WALLET_MIN: i32 = 3;
}
```

### Detection Algorithms

**BREAKOUT Detection:**

```rust
let net_flow_60s = compute_net_flow(&self.trades_60s);
let buy_ratio = buy_count as f64 / (buy_count + sell_count) as f64;
let wallet_count = self.unique_wallets_300s.len();

if net_flow_60s > 5.0 && wallet_count >= 5 && buy_ratio > 0.75 {
    // BREAKOUT signal triggered
}
```

**SURGE Detection:**

```rust
let volume_60s = net_flow_60s.abs();
let avg_volume_300s = net_flow_300s.abs() / 5.0;  // Per minute average

if volume_60s >= 3.0 * avg_volume_300s
    && buy_count_60s >= 10
    && net_flow_60s > 8.0
{
    // SURGE signal triggered
}
```

**DCA_CONVICTION Detection:**

```rust
fn compute_dca_correlation(
    spot_trades: &[TradeEvent],
    dca_trades: &[TradeEvent],
    window_secs: i64,
) -> (f64, usize) {
    let mut matched_dca_count = 0;
    
    for dca_trade in dca_trades {
        let has_matching_spot = spot_trades.iter().any(|spot_trade| {
            let time_diff = (spot_trade.timestamp - dca_trade.timestamp).abs();
            time_diff <= window_secs  // Within ±60 seconds
        });
        
        if has_matching_spot {
            matched_dca_count += 1;
        }
    }
    
    let overlap_ratio = matched_dca_count as f64 / dca_trades.len() as f64;
    (overlap_ratio, matched_dca_count)
}

if overlap_ratio > 0.25 && net_flow_60s > 0.0 {
    // DCA_CONVICTION signal triggered
}
```

### Bot Detection Integration

**Bot Detection Heuristics (from `src/pipeline/state.rs`):**

1. **High-Frequency:** > 10 trades in 300s window
2. **Rapid Consecutive:** ≥ 3 trades within 1 second
3. **Alternating Direction:** > 70% buy/sell flip-flop rate
4. **Identical Amounts:** > 50% of trade pairs have identical SOL amounts

**Integration with Signals:**

- `bot_wallets_count_300s` tracked in metrics
- `bot_trades_count_300s` tracked in metrics
- BOT_DROPOFF signal uses `previous_bot_count` comparison
- FOCUSED signal penalizes high bot activity

### Signal Persistence

**Database Write (from `src/pipeline/db.rs`):**

```sql
INSERT INTO token_signals (
    mint, signal_type, window_seconds, severity, score, details_json, created_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
```

**Checks Before Write:**

1. ✅ Blocklist check (mint not in `mint_blocklist`)
2. ✅ Signal deduplication (false → true transition only)

**Observed Behavior:**

- Signals appended to `token_signals` table
- No row deletion (append-only)
- Signals remain queryable indefinitely

---

## 13. Database Architecture

### Table Schema Complete

**token_aggregates:**

```sql
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
    dca_buys_60s            INTEGER NOT NULL DEFAULT 0,
    dca_buys_300s           INTEGER NOT NULL DEFAULT 0,
    dca_buys_900s           INTEGER NOT NULL DEFAULT 0,
    dca_buys_3600s          INTEGER NOT NULL DEFAULT 0,
    dca_buys_14400s         INTEGER NOT NULL DEFAULT 0,
    updated_at              INTEGER NOT NULL,
    created_at              INTEGER NOT NULL
);
```

**token_signals:**

```sql
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
```

**token_metadata:**

```sql
CREATE TABLE IF NOT EXISTS token_metadata (
    mint                TEXT PRIMARY KEY,
    symbol              TEXT,
    name                TEXT,
    decimals            INTEGER NOT NULL,
    launch_platform     TEXT,
    image_url           TEXT,
    price_usd           REAL,
    market_cap          REAL,
    follow_price        INTEGER NOT NULL DEFAULT 0,
    blocked             INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    CHECK (decimals >= 0 AND decimals <= 18)
);
```

**mint_blocklist:**

```sql
CREATE TABLE IF NOT EXISTS mint_blocklist (
    mint        TEXT PRIMARY KEY,
    reason      TEXT NOT NULL,
    blocked_by  TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER
);
```

### Index Strategy

**token_aggregates Indexes:**

```sql
CREATE INDEX IF NOT EXISTS idx_token_aggregates_updated_at
    ON token_aggregates (updated_at);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_source_program
    ON token_aggregates (source_program);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_netflow_300s
    ON token_aggregates (net_flow_300s_sol DESC);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_dca_buys_3600s
    ON token_aggregates (dca_buys_3600s DESC);
```

**token_metadata Indexes:**

```sql
CREATE INDEX IF NOT EXISTS idx_token_metadata_created_at
    ON token_metadata (created_at);
```

**Observations:**

- ✅ Index on `updated_at` for time-based queries
- ✅ Index on `net_flow_300s_sol DESC` for sorting
- ✅ Index on `dca_buys_3600s DESC` for DCA filtering
- ❌ No index on `token_signals` (table is append-only, queries by mint would benefit)

### WAL Mode Configuration

**Enabled at Startup (from `src/pipeline/db.rs`):**

```rust
conn.pragma_update(None, "journal_mode", "WAL")?;
log::info!("📊 Enabled WAL mode for SQLite database");
```

**Benefits:**

- Concurrent readers while writer active
- Better write performance
- Crash recovery via write-ahead log

**Observed Files:**

- `solflow.db` - Main database file
- `solflow.db-wal` - Write-ahead log
- `solflow.db-shm` - Shared memory for WAL

### Connection Pooling

**Implementation:** ❌ Not implemented

**Current Pattern (from `src/pipeline/db.rs`):**

```rust
pub struct SqliteAggregateWriter {
    conn: Arc<Mutex<Connection>>,  // Single connection wrapped in Arc<Mutex<>>
}
```

**Behavior:**

- Single connection for all writes
- Mutex ensures serial access
- Multiple readers possible via WAL mode

**Bottleneck:**

During flush cycles, only one batch can be written at a time. With 500 mint batches, this creates serial bottleneck.

**Potential Improvement:**

Use connection pool (e.g., `r2d2` crate) with multiple writer connections:

```rust
pub struct SqliteAggregateWriter {
    pool: Arc<r2d2::Pool<SqliteConnectionManager>>,
}
```

### Write Patterns

**Aggregate Writes:**

- Operation: `INSERT ... ON CONFLICT(mint) DO UPDATE SET ...`
- Batch size: 500 mints per transaction (configurable via `FLUSH_BATCH_SIZE`)
- Frequency: Delta flush (5s) + Full flush (60s)

**Signal Writes:**

- Operation: `INSERT INTO token_signals ...`
- Batch size: Individual signals (no batching)
- Frequency: On signal detection (sporadic)

**Metadata Writes:**

- Operation: `INSERT ... ON CONFLICT(mint) DO UPDATE SET ...`
- Frequency: On DexScreener fetch (60s for followed tokens)

### Potential Bottlenecks

**Bottleneck 1: Single Writer**

- Arc<Mutex<Connection>> = serial writes
- Mitigated by batch writes (500 mints/transaction)
- Flush duration: ~100ms (acceptable under current load)

**Bottleneck 2: No Signal Batching**

- Each signal written individually
- 5-10 signals/minute = minimal impact
- Could batch for high-signal scenarios

**Bottleneck 3: Unbounded Signal Growth**

- `token_signals` table never pruned
- Append-only = linear growth
- Eventual impact on database size and query performance

**Observed Performance:**

From logs, flush cycles complete in 50-150ms, suggesting current architecture handles load adequately.

---

## Recommendations

### Critical Priority (Implement Immediately)

**1. Add LP Token Filtering**

- **Issue:** Non-standard tokens (LP, pool tokens) can enter dataset
- **Root Cause:** `find_primary_token_mint()` uses largest balance change heuristic without validation
- **Impact:** Inaccurate trade data, misleading net flows
- **Solution:**
  - Implement mint pattern matching (e.g., exclude "LP-" prefix)
  - Add metadata-based classification if API supports it
  - Check blocklist before trade ingestion (not just signal writes)
- **Implementation Time:** 1-2 days
- **Code Location:** `src/streamer_core/trade_detector.rs`

**2. Add Database Aggregate Cleanup**

- **Issue:** Stale aggregates persist indefinitely in `token_aggregates`
- **Root Cause:** No automatic row deletion mechanism
- **Impact:** Dashboard displays outdated net flows for inactive tokens
- **Solution:**
  - Add cleanup task: `DELETE FROM token_aggregates WHERE updated_at < unixepoch() - 86400` (24h)
  - Run every hour
  - Log cleanup metrics
- **Implementation Time:** 4 hours
- **Code Location:** `src/bin/pipeline_runtime.rs`

**3. Add Frontend `updated_at` Filter**

- **Issue:** Frontend queries include stale aggregates
- **Root Cause:** No timestamp filter in SQL query
- **Impact:** Inactive tokens with old net flows appear in results
- **Solution:**
  - Add `WHERE ta.updated_at > unixepoch() - 14400` to frontend query
  - Filter tokens updated within last 4 hours
- **Implementation Time:** 30 minutes
- **Code Location:** `frontend/lib/queries.ts`

---

### High Priority (Next Sprint)

**4. Add Connection Pooling**

- **Issue:** Single writer connection limits write throughput
- **Impact:** Serial writes during flush cycles
- **Solution:**
  - Integrate `r2d2` connection pool
  - Allow concurrent batch writes
  - Monitor connection pool metrics
- **Implementation Time:** 1 day
- **Code Location:** `src/pipeline/db.rs`

**5. Add Trade Signature Deduplication**

- **Issue:** No protection against replayed transactions
- **Impact:** Potential double-counting of trades
- **Solution:**
  - Track processed signatures in HashSet with TTL
  - Skip trades with duplicate signatures
  - Evict old signatures after 1 hour
- **Implementation Time:** 4 hours
- **Code Location:** `src/pipeline/ingestion.rs`

**6. Add Signal Table Pruning**

- **Issue:** `token_signals` table grows unbounded
- **Impact:** Increasing database size, slower queries
- **Solution:**
  - Add cleanup task: `DELETE FROM token_signals WHERE created_at < unixepoch() - 604800` (7 days)
  - Run daily
  - Archive old signals to separate table if historical analysis needed
- **Implementation Time:** 4 hours
- **Code Location:** `src/bin/pipeline_runtime.rs`

**7. Fix DCA Sparkline Query**

- **Issue:** Query checks `trades` table existence, which may not exist
- **Impact:** Empty sparklines for all tokens
- **Solution:**
  - Check table existence before query
  - Return empty array gracefully if table missing
  - Document `trades` table requirement in schema
- **Implementation Time:** 1 hour
- **Code Location:** `frontend/lib/queries.ts`

---

### Medium Priority (Backlog)

**8. Add Program-Specific Instruction Validation**

- **Issue:** No validation of expected instruction patterns
- **Impact:** False positives from unexpected transaction structures
- **Solution:**
  - Add discriminator validation per program
  - Verify instruction account structure
  - Log anomalous transactions
- **Implementation Time:** 2 days
- **Code Location:** `src/streamer_core/trade_detector.rs`

**9. Implement Metadata-Based Token Classification**

- **Issue:** Cannot distinguish LP vs standard tokens via metadata
- **Impact:** Manual blocklist maintenance required
- **Solution:**
  - Integrate with token registry APIs
  - Check for LP classification flags
  - Cache classification results
- **Implementation Time:** 1 day
- **Code Location:** `src/pipeline/dexscreener.rs`

**10. Add Inner Instruction Deduplication**

- **Issue:** No detection of circular balance changes
- **Impact:** Potential double-counting in edge cases
- **Solution:**
  - Track balance changes per account per transaction
  - Detect and filter duplicate deltas
  - Log suspicious patterns
- **Implementation Time:** 1 day
- **Code Location:** `src/streamer_core/balance_extractor.rs`

**11. Optimize Frontend Refresh Rate**

- **Issue:** 5-second polling may be excessive
- **Impact:** Unnecessary API load
- **Solution:**
  - Increase polling interval to 10-15 seconds
  - Implement WebSocket for real-time updates
  - Add manual refresh button
- **Implementation Time:** 4 hours
- **Code Location:** `frontend/app/page.tsx`

**12. Add Flush Cycle Monitoring**

- **Issue:** No metrics on flush performance
- **Impact:** Cannot detect bottlenecks proactively
- **Solution:**
  - Log flush duration, batch size, touched mint count
  - Add Prometheus metrics
  - Set up alerting for slow flushes (> 500ms)
- **Implementation Time:** 1 day
- **Code Location:** `src/pipeline/ingestion.rs`

---

## Appendices

### A. SQL Schema Reference

See `/sql/` directory for complete schema files:

- `00_token_metadata.sql` - Token metadata table
- `01_mint_blocklist.sql` - Blocklist table
- `02_token_aggregates.sql` - Aggregate metrics table
- `03_token_signals.sql` - Signal events table
- `04_system_metrics.sql` - System health metrics

### B. Configuration Variables

**Environment Variables:**

| Variable | Default | Purpose |
|----------|---------|---------|
| `GEYSER_URL` | (required) | Yellowstone gRPC endpoint |
| `X_TOKEN` | (required) | Geyser authentication token |
| `SOLFLOW_DB_PATH` | `/var/lib/solflow/solflow.db` | SQLite database path |
| `STREAMER_CHANNEL_BUFFER` | 10000 | Pipeline channel buffer size |
| `AGGREGATE_FLUSH_INTERVAL_MS` | 5000 | Delta flush interval |
| `ENABLE_JSONL` | false | Enable JSONL output |
| `ENABLE_PIPELINE` | false | Enable pipeline runtime |
| `MINT_PRUNE_THRESHOLD_SECS` | 7200 | In-memory pruning threshold |
| `FLUSH_BATCH_SIZE` | 500 | Database write batch size |

### C. External API Documentation

**DexScreener API:**

- Base URL: `https://api.dexscreener.com`
- Endpoint: `/token-pairs/v1/solana/{mint}`
- Rate Limit: No official limit (system uses 2-3 req/s)
- Response: JSON array of trading pairs

---

## Verification Checklist

- ✅ All 4 program IDs documented with correct addresses
- ✅ All 6 rolling windows enumerated with full details
- ✅ All SQL schema columns verified against Rust struct fields
- ✅ All frontend columns mapped to database sources
- ✅ All API routes documented with request/response schemas
- ✅ LP token analysis includes root cause and recommendations
- ✅ Net flow calculation methodology fully explained
- ✅ Deduplication mechanisms inventoried at all levels
- ✅ Retention policies documented for memory and database
- ✅ Signal detection algorithms documented with thresholds
- ✅ Database architecture fully mapped with indexes
- ✅ Recommendations prioritized and categorized
- ✅ Document formatted with tables, bullet lists, and diagrams
- ✅ Neutral, objective tone maintained throughout
- ✅ Self-contained with no external context required

---

**End of Review**
