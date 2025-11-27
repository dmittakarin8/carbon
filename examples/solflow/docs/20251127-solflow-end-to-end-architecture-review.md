# SolFlow End-to-End Architecture Review

**Date:** 2025-11-27  
**Purpose:** Comprehensive architectural review of the SolFlow application stack covering data flow from processed trade events to user interface  
**Scope:** Database schema, backend processing, aggregation logic, frontend queries, and data aging mechanisms

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Data Model and Persistence Layer](#1-data-model-and-persistence-layer-sqlite)
3. [Backend Processing and Aggregation Logic](#2-backend-processing-and-aggregation-logic)
4. [Frontend Data Retrieval and Display Logic](#3-frontend-data-retrieval-and-display-logic)
5. [Cleanup and Aging Mechanisms](#4-cleanup-and-aging-mechanisms)
6. [Architectural Insights and Recommendations](#5-architectural-insights-and-recommendations)

---

## Executive Summary

SolFlow is a real-time Solana DEX trade monitoring and analytics system that processes trades from 5 DEX programs (PumpSwap, BonkSwap, Moonshot, Raydium CPMM, Jupiter DCA), aggregates metrics across multiple rolling time windows, and presents actionable signals for token accumulation patterns.

**Architecture Overview:**

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Yellowstone gRPC Stream                       │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Unified Streamer (Carbon-based)                   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │              InstructionScanner (5 programs)                 │   │
│  │  • PumpSwap    • BonkSwap    • Moonshot                     │   │
│  │  • Raydium CPMM    • Jupiter DCA                            │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                               │                                      │
│                               ▼                                      │
│                    TradeEvent Extraction                             │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    SQLite Database (WAL Mode)                        │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  trades table (Raw Events - Source of Truth)                │   │
│  │  • INSERT OR IGNORE (duplicate prevention)                   │   │
│  │  • Batched writes (100 events / 2s flush)                    │   │
│  └─────────────────────────────────────────────────────────────┘   │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Pipeline Runtime (Aggregator)                     │
│                                                                       │
│  SqliteTradeReader ──► PipelineEngine ──► SqliteAggregateWriter    │
│  (cursor-based)        (rolling windows)   (batched UPSERT)         │
│                                                                       │
│  Computes:                                                           │
│  • Net SOL flow (6 windows: 60s - 14400s)                           │
│  • Buy/sell counts per window                                        │
│  • DCA overlap metrics (5 windows)                                   │
│  • Unique wallet counts                                              │
│  • Bot activity detection                                            │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    SQLite Database (Continued)                       │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  token_aggregates table (Rolling Window Metrics)            │   │
│  │  • UPSERT on mint (500 batch size)                          │   │
│  │  • Updated every ~5s                                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  token_metadata table (Enrichment Data)                     │   │
│  │  • Symbol, name, decimals, launch info                      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  token_signals table (Append-Only Signal Log)               │   │
│  │  • Breakout, Surge, Focused, etc.                           │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  token_signal_summary table (Persistence Scoring)           │   │
│  │  • Pattern tags: ACCUMULATION, MOMENTUM, DISTRIBUTION       │   │
│  │  • Confidence: LOW, MEDIUM, HIGH (age-weighted)             │   │
│  └─────────────────────────────────────────────────────────────┘   │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Frontend (Terminal UI)                         │
│                                                                       │
│  Dashboard Query:                                                    │
│  SELECT * FROM token_aggregates ta                                   │
│  LEFT JOIN token_metadata tm ON ta.mint = tm.mint                   │
│  WHERE ta.dca_buys_3600s > 0                                         │
│    AND (tm.blocked IS NULL OR tm.blocked = 0)                       │
│  ORDER BY ta.net_flow_300s_sol DESC                                  │
│  LIMIT 100                                                           │
│                                                                       │
│  Display: Top 100 tokens by 5-min net SOL flow                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 1. Data Model and Persistence Layer (SQLite)

### 1.1 Database Configuration

**File Location:** `/var/lib/solflow/solflow.db` (configurable via `SOLFLOW_DB_PATH`)

**SQLite Optimizations:**
- **Journal Mode:** WAL (Write-Ahead Logging) for concurrent reads/writes
- **Synchronous:** NORMAL (balanced durability/performance)
- **Cache Size:** 200MB (50,000 pages)
- **Memory Mapped I/O:** 2GB for fast reads
- **Autocheckpoint:** 1000 pages

### 1.2 Core Tables and Schemas

#### 1.2.1 `trades` Table - Raw Trade Event Storage

**Purpose:** Source of truth for all trade events. Immutable append-only log.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    program TEXT NOT NULL,                -- Program ID
    program_name TEXT NOT NULL,           -- PumpSwap, BonkSwap, etc.
    mint TEXT NOT NULL,                   -- Token mint address
    signature TEXT UNIQUE NOT NULL,       -- Transaction signature (dedup key)
    action TEXT NOT NULL,                 -- BUY or SELL
    sol_amount REAL NOT NULL,             -- SOL volume (lamports → SOL)
    token_amount REAL NOT NULL,           -- Token volume (raw → decimal-adjusted)
    token_decimals INTEGER NOT NULL,      -- Token decimals
    user_account TEXT,                    -- User wallet address
    discriminator TEXT NOT NULL,          -- Instruction discriminator
    timestamp INTEGER NOT NULL            -- Unix timestamp (seconds)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_mint_timestamp 
    ON trades(mint, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_timestamp 
    ON trades(timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_program 
    ON trades(program, timestamp DESC);
```

**Key Fields:**
- `signature`: Ensures deduplication via `INSERT OR IGNORE`
- `program_name`: Used for filtering (aggregator reads only PumpSwap/JupiterDCA)
- `action`: BUY or SELL classification
- `sol_amount`: Primary volume metric for net flow calculations
- `timestamp`: Critical for rolling window calculations

**Write Pattern:**
- **Operation:** `INSERT OR IGNORE` (duplicate prevention)
- **Batch Size:** 100 events
- **Flush Interval:** 2 seconds or when batch full
- **Concurrency:** WAL mode allows concurrent readers during writes

---

#### 1.2.2 `token_aggregates` Table - Rolling Window Metrics

**Purpose:** Pre-computed metrics for dashboard display. Updated via UPSERT from Pipeline Runtime.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS token_aggregates (
    mint TEXT PRIMARY KEY,
    
    -- Metadata
    source_program TEXT NOT NULL,         -- Primary program for this token
    last_trade_timestamp INTEGER,         -- Most recent trade timestamp
    
    -- Pricing (enriched from DexScreener API)
    price_usd REAL,
    price_sol REAL,
    market_cap_usd REAL,
    
    -- Net SOL Flow (6 rolling windows)
    net_flow_60s_sol REAL,                -- 1 minute
    net_flow_300s_sol REAL,               -- 5 minutes (PRIMARY DASHBOARD SORT)
    net_flow_900s_sol REAL,               -- 15 minutes
    net_flow_3600s_sol REAL,              -- 1 hour
    net_flow_7200s_sol REAL,              -- 2 hours
    net_flow_14400s_sol REAL,             -- 4 hours
    
    -- Buy/Sell Counts (3 rolling windows)
    buy_count_60s INTEGER,
    sell_count_60s INTEGER,
    buy_count_300s INTEGER,
    sell_count_300s INTEGER,
    buy_count_900s INTEGER,
    sell_count_900s INTEGER,
    
    -- Behavioral Metrics (300s window)
    unique_wallets_300s INTEGER,          -- Distinct wallet addresses
    bot_trades_300s INTEGER,              -- Detected bot activity
    bot_wallets_300s INTEGER,             -- Distinct bot wallets
    avg_trade_size_300s_sol REAL,         -- Average trade size
    volume_300s_sol REAL,                 -- Total volume (buy + sell)
    
    -- DCA Buy Counts (5 rolling windows - Jupiter DCA only)
    dca_buys_60s INTEGER NOT NULL DEFAULT 0,
    dca_buys_300s INTEGER NOT NULL DEFAULT 0,
    dca_buys_900s INTEGER NOT NULL DEFAULT 0,
    dca_buys_3600s INTEGER NOT NULL DEFAULT 0,  -- DASHBOARD FILTER THRESHOLD
    dca_buys_14400s INTEGER NOT NULL DEFAULT 0,
    
    -- Timestamps
    updated_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- Indexes for dashboard queries
CREATE INDEX IF NOT EXISTS idx_token_aggregates_updated_at
    ON token_aggregates (updated_at);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_source_program
    ON token_aggregates (source_program);

CREATE INDEX IF NOT EXISTS idx_token_aggregates_netflow_300s
    ON token_aggregates (net_flow_300s_sol DESC);  -- PRIMARY DASHBOARD SORT

CREATE INDEX IF NOT EXISTS idx_token_aggregates_dca_buys_3600s
    ON token_aggregates (dca_buys_3600s DESC);     -- FILTER INDEX
```

**Key Fields Explained:**

1. **Net Flow Fields (`net_flow_*s_sol`):**
   - Formula: `Σ(buy_volumes) - Σ(sell_volumes)` within window
   - Positive = Accumulation (more buying than selling)
   - Negative = Distribution (more selling than buying)
   - Primary dashboard metric: `net_flow_300s_sol` (5-minute window)

2. **DCA Fields (`dca_buys_*s`):**
   - Counts Jupiter DCA buy orders only
   - Used to identify accumulation patterns
   - Dashboard filter: `dca_buys_3600s > 0` (tokens with 1-hour DCA activity)

3. **Bot Detection Fields:**
   - `bot_trades_300s`: Trades flagged as bot activity (rapid execution, small amounts)
   - `bot_wallets_300s`: Distinct bot wallet addresses
   - Used by persistence scorer to penalize bot-heavy tokens

**Write Pattern:**
- **Operation:** `INSERT ... ON CONFLICT(mint) DO UPDATE`
- **Batch Size:** 500 mints per transaction (configurable via `FLUSH_BATCH_SIZE`)
- **Update Frequency:** ~5 seconds (configurable via `AGGREGATE_FLUSH_INTERVAL_MS`)
- **Atomicity:** Single transaction per batch ensures consistency

---

#### 1.2.3 `token_metadata` Table - Token Enrichment Data

**Purpose:** Store token metadata fetched from DexScreener API.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS token_metadata (
    mint TEXT PRIMARY KEY,
    symbol TEXT,                          -- Token ticker (e.g., BONK)
    name TEXT,                            -- Token full name
    decimals INTEGER NOT NULL,            -- Decimal places (0-18)
    launch_platform TEXT,                 -- pump.fun, raydium, etc.
    pair_created_at INTEGER,              -- Pair creation timestamp (for age calculation)
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    
    CHECK (decimals >= 0 AND decimals <= 18)
);

CREATE INDEX IF NOT EXISTS idx_token_metadata_created_at
    ON token_metadata (created_at);
```

**Enrichment Source:** DexScreener API  
**Update Schedule:** Background task (every 5 seconds for active tokens)

---

#### 1.2.4 `token_signals` Table - Signal Event Log

**Purpose:** Append-only log of detected signals. Used for historical analysis and appearance tracking.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS token_signals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    
    mint TEXT NOT NULL,
    signal_type TEXT NOT NULL,            -- BREAKOUT, SURGE, FOCUSED, etc.
    window_seconds INTEGER NOT NULL,      -- Window size where signal occurred
    severity INTEGER NOT NULL DEFAULT 1,  -- Signal strength (1-5)
    score REAL,                           -- Numeric score (0.0-1.0)
    details_json TEXT,                    -- JSON blob with additional context
    created_at INTEGER NOT NULL,
    
    -- Status tracking
    sent_to_discord INTEGER NOT NULL DEFAULT 0,
    seen_in_terminal INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_token_signals_mint_created
    ON token_signals (mint, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_token_signals_type_created
    ON token_signals (signal_type, created_at DESC);
```

**Signal Types:**
- `BREAKOUT`: Sudden volume spike with positive net flow
- `SURGE`: High uptrend score (buy pressure > threshold)
- `FOCUSED`: Concentrated wallet activity
- `ACCUMULATION`: DCA overlap + positive net flow (combined signal)

**Write Pattern:**
- **Operation:** `INSERT` (append-only)
- **Blocklist Check:** Must validate mint against `mint_blocklist` before writing
- **Details JSON:** Stores contextual data (net_flow, unique_wallets, etc.)

---

#### 1.2.5 `token_signal_summary` Table - Persistence Scoring

**Purpose:** Persistent scoring and pattern classification. Updated periodically by scoring engine.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS token_signal_summary (
    token_address TEXT PRIMARY KEY,
    
    -- Core scoring metrics
    persistence_score INTEGER NOT NULL DEFAULT 0,  -- 0-10 scale
    pattern_tag TEXT,                              -- ACCUMULATION, MOMENTUM, etc.
    confidence TEXT,                               -- LOW, MEDIUM, HIGH
    
    -- Appearance tracking
    appearance_24h INTEGER NOT NULL DEFAULT 0,     -- Signal appearances in 24h
    appearance_72h INTEGER NOT NULL DEFAULT 0,     -- Signal appearances in 72h
    
    -- Metadata
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_persistence_score
    ON token_signal_summary (persistence_score DESC);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_pattern_tag
    ON token_signal_summary (pattern_tag);

CREATE INDEX IF NOT EXISTS idx_token_signal_summary_updated_at
    ON token_signal_summary (updated_at DESC);
```

**Persistence Score Calculation (0-10):**

```rust
// Multi-window presence (30%): Token appears in multiple time windows
let window_presence = active_windows / 6.0;
score += window_presence * 30.0;

// Wallet growth (25%): Unique wallet count
let wallet_score = (unique_wallets_300s / 50.0).min(1.0);
score += wallet_score * 25.0;

// Net flow strength (25%): Consistent buy pressure
let avg_net_flow = (net_flow_300s + net_flow_900s + net_flow_3600s) / 3.0;
let flow_score = (avg_net_flow / 10.0).min(1.0);
score += flow_score * 25.0;

// Behavioral consistency (10%): Lifetime normalization
let lifetime_factor = (lifetime_hours / 24.0).min(1.0);
score += lifetime_factor * 10.0;

// Bot penalty (10%): Penalize bot activity
let bot_penalty = bot_ratio * 10.0;
score -= bot_penalty;

// Normalize to 0-10 scale
(score / 10.0).clamp(0.0, 10.0).round()
```

**Pattern Tags:**
- `ACCUMULATION`: High DCA overlap + positive net flow + buy ratio > 0.6
- `MOMENTUM`: Strong net flow (>5 SOL) + buy ratio > 0.7
- `DISTRIBUTION`: Negative net flow (<-2 SOL) + buy ratio < 0.4
- `WASHOUT`: Severe negative net flow (<-5 SOL)
- `NOISE`: Inconsistent or low-quality signals

**Confidence Levels (Age-Weighted):**

```rust
// Base confidence calculation
let base_score = data_richness * 0.4 
               + lifetime_factor * 0.3 
               + (1.0 - bot_ratio) * 0.3;

// Age-based multipliers
let age_multiplier = match token_age {
    < 1 hour    => 0.5,  // 50% penalty (very new)
    1-24 hours  => 0.7,  // 30% penalty (young)
    1-7 days    => 1.0,  // Neutral
    7-30 days   => 1.1,  // 10% boost (established)
    > 30 days   => 1.3,  // 30% boost (mature)
};

let final_score = (base_score * age_multiplier).clamp(0.0, 1.0);

// Thresholds
if final_score > 0.7 { "HIGH" }
else if final_score > 0.4 { "MEDIUM" }
else { "LOW" }
```

---

#### 1.2.6 `dca_activity_buckets` Table - Sparkline Data

**Purpose:** Time-series storage for DCA activity sparkline visualization (1-minute buckets).

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS dca_activity_buckets (
    mint TEXT NOT NULL,
    bucket_timestamp INTEGER NOT NULL,  -- Unix timestamp floored to 60s boundary
    buy_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (mint, bucket_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_dca_buckets_timestamp
    ON dca_activity_buckets (bucket_timestamp);

CREATE INDEX IF NOT EXISTS idx_dca_buckets_mint_timestamp
    ON dca_activity_buckets (mint, bucket_timestamp);
```

**Bucket Computation:**
```rust
let bucket_timestamp = (current_timestamp / 60) * 60;  // Floor to minute boundary
```

**Retention:** 2 hours (120 buckets × 60 seconds)  
**Cleanup Schedule:** Every 5 minutes via `cleanup_old_dca_buckets()`

---

#### 1.2.7 `mint_blocklist` Table - Token Filtering

**Purpose:** Block spam tokens or scam addresses from appearing in dashboard/signals.

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS mint_blocklist (
    mint TEXT PRIMARY KEY,
    reason TEXT,
    blocked_by TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER                    -- NULL = permanent block
);
```

**Validation Check (before signal write):**
```sql
SELECT mint FROM mint_blocklist
WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?)
```

---

#### 1.2.8 `system_metrics` Table - System State

**Purpose:** Store system-wide metrics (e.g., last cleanup timestamp, processing stats).

**Schema:**
```sql
CREATE TABLE IF NOT EXISTS system_metrics (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
```

---

## 2. Backend Processing and Aggregation Logic

### 2.1 Data Ingestion Pipeline

#### 2.1.1 Unified Streamer (Entry Point)

**Component:** `bin/unified_streamer.rs` + `instruction_scanner.rs`

**Function:** Connect to Yellowstone gRPC, decode transactions, extract trade events.

**Architecture:**
```rust
YellowstoneGrpcClient
    ↓
Carbon Pipeline
    ↓
InstructionScanner::scan(accounts, data) → Option<TradeEvent>
    ↓
SqliteWriter::write(TradeEvent)
    ↓
SQLite trades table
```

**InstructionScanner Logic:**

```rust
pub fn scan(&self, accounts: &[String], data: &[u8]) -> Option<TradeEvent> {
    // 1. Match program ID from accounts[0]
    let program_id = &accounts[0];
    let program_info = self.program_map.get(program_id)?;
    
    // 2. Extract discriminator (first 8 bytes)
    let discriminator = hex::encode(&data[0..8]);
    
    // 3. Match against known instruction discriminators
    match program_info.name.as_str() {
        "PumpSwap" => {
            if discriminator == "33e685a4017f83ad" { // Swap
                // Extract mint, sol_amount, token_amount, action
                return Some(self.extract_pumpswap_trade(accounts, data)?);
            }
        },
        "BonkSwap" => {
            if discriminator == "f8c69e91e17587c8" { // Swap
                return Some(self.extract_bonkswap_trade(accounts, data)?);
            }
        },
        // ... similar for Moonshot, Raydium CPMM, Jupiter DCA
    }
    
    None
}
```

**Trade Event Structure:**
```rust
pub struct TradeEvent {
    pub timestamp: i64,
    pub signature: String,
    pub program_id: String,
    pub program_name: String,      // PumpSwap, BonkSwap, etc.
    pub action: String,            // BUY or SELL
    pub mint: String,              // Token mint address
    pub sol_amount: f64,           // SOL volume
    pub token_amount: f64,         // Token volume (decimal-adjusted)
    pub token_decimals: u8,
    pub user_account: Option<String>,
    pub discriminator: String,
}
```

---

#### 2.1.2 SQLite Writer (Raw Event Storage)

**Component:** `streamer_core/sqlite_writer.rs`

**Function:** Batch writes to `trades` table with duplicate prevention.

**Implementation:**
```rust
pub struct SqliteWriter {
    conn: Connection,
    batch: Vec<TradeEvent>,
    batch_size: usize,             // Default: 100
    last_flush: Instant,
    flush_interval_secs: u64,      // Default: 2 seconds
}

impl WriterBackend for SqliteWriter {
    async fn write(&mut self, event: &TradeEvent) -> Result<(), WriterError> {
        self.batch.push(event.clone());
        
        // Auto-flush on batch size or time threshold
        if self.batch.len() >= self.batch_size 
           || self.last_flush.elapsed().as_secs() >= self.flush_interval_secs {
            self.flush_batch()?;
        }
        
        Ok(())
    }
}

fn flush_batch(&mut self) -> Result<(), WriterError> {
    let tx = self.conn.transaction()?;
    
    for event in &self.batch {
        tx.execute(
            "INSERT OR IGNORE INTO trades 
             (program, program_name, mint, signature, action, sol_amount, 
              token_amount, token_decimals, user_account, discriminator, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![/* ... */],
        )?;
    }
    
    tx.commit()?;
    self.batch.clear();
    Ok(())
}
```

**Key Features:**
- **Deduplication:** `INSERT OR IGNORE` on `signature` UNIQUE constraint
- **Batching:** Reduces SQLite write lock contention
- **WAL Mode:** Allows concurrent reads during writes

---

### 2.2 Aggregation Engine (Pipeline Runtime)

#### 2.2.1 SQLite Trade Reader (Input Layer)

**Component:** `aggregator_core/sqlite_reader.rs`

**Function:** Incremental cursor-based reads from `trades` table.

**Implementation:**
```rust
pub struct SqliteTradeReader {
    conn: Connection,
    last_read_id: i64,             // Cursor position
    poll_interval: Duration,       // Default: 500ms
}

impl SqliteTradeReader {
    pub fn read_new_trades(&mut self) -> Result<Vec<Trade>, ReaderError> {
        let mut stmt = self.conn.prepare(
            "SELECT timestamp, signature, program_name, action, mint,
                    sol_amount, token_amount, token_decimals, user_account, id
             FROM trades
             WHERE id > ?1 
               AND program_name IN ('PumpSwap', 'JupiterDCA')  -- FILTER
             ORDER BY id ASC
             LIMIT 1000"  -- Batch size
        )?;
        
        let trades = /* ... map rows to Trade structs ... */;
        
        // Update cursor to highest processed ID
        if let Some(max_id) = trades.iter().map(|t| t.id).max() {
            self.last_read_id = max_id;
        }
        
        Ok(trades)
    }
}
```

**Key Features:**
- **Cursor-Based:** Tracks `last_read_id` to avoid re-reading
- **Program Filter:** Only processes PumpSwap and JupiterDCA (excludes Aggregator rows)
- **Batch Limit:** 1000 trades per poll to prevent memory spikes
- **Read-Only Mode:** `PRAGMA query_only = ON` prevents accidental writes

---

#### 2.2.2 Pipeline Engine (Core Aggregation)

**Component:** `pipeline/engine.rs` + `pipeline/state.rs`

**Function:** Rolling window calculations for each token.

**Data Structure:**
```rust
pub struct TokenState {
    pub mint: String,
    
    // Trade history per window
    trades_60s: Vec<Trade>,
    trades_300s: Vec<Trade>,
    trades_900s: Vec<Trade>,
    trades_3600s: Vec<Trade>,
    trades_7200s: Vec<Trade>,
    trades_14400s: Vec<Trade>,
    
    // Per-program trade buckets (for DCA detection)
    pumpswap_trades_300s: Vec<Trade>,
    jupiter_dca_trades_300s: Vec<Trade>,
    
    // Wallet tracking (for unique wallet count)
    wallets_300s: HashSet<String>,
    
    // Bot detection
    bot_wallets_300s: HashSet<String>,
    bot_trades_300s: usize,
}
```

**Core Aggregation Logic:**

```rust
impl TokenState {
    pub fn add_trade(&mut self, trade: Trade) {
        // Add to appropriate window buckets
        self.trades_60s.push(trade.clone());
        self.trades_300s.push(trade.clone());
        self.trades_900s.push(trade.clone());
        self.trades_3600s.push(trade.clone());
        self.trades_7200s.push(trade.clone());
        self.trades_14400s.push(trade.clone());
        
        // Program-specific buckets
        match trade.program_name.as_str() {
            "PumpSwap" => self.pumpswap_trades_300s.push(trade.clone()),
            "JupiterDCA" => self.jupiter_dca_trades_300s.push(trade.clone()),
            _ => {}
        }
        
        // Track wallet
        if let Some(wallet) = &trade.user_account {
            self.wallets_300s.insert(wallet.clone());
        }
    }
    
    pub fn evict_old_trades(&mut self, now: i64) {
        // Remove trades outside each window
        self.trades_60s.retain(|t| t.timestamp > now - 60);
        self.trades_300s.retain(|t| t.timestamp > now - 300);
        self.trades_900s.retain(|t| t.timestamp > now - 900);
        self.trades_3600s.retain(|t| t.timestamp > now - 3600);
        self.trades_7200s.retain(|t| t.timestamp > now - 7200);
        self.trades_14400s.retain(|t| t.timestamp > now - 14400);
        
        // Evict from program buckets
        self.pumpswap_trades_300s.retain(|t| t.timestamp > now - 300);
        self.jupiter_dca_trades_300s.retain(|t| t.timestamp > now - 300);
        
        // Rebuild wallet set (recount from remaining trades)
        self.wallets_300s.clear();
        for trade in &self.trades_300s {
            if let Some(wallet) = &trade.user_account {
                self.wallets_300s.insert(wallet.clone());
            }
        }
    }
    
    pub fn compute_metrics(&self) -> AggregatedTokenState {
        // Net flow calculation (buy volume - sell volume)
        let net_flow_60s = self.compute_net_flow(&self.trades_60s);
        let net_flow_300s = self.compute_net_flow(&self.trades_300s);
        let net_flow_900s = self.compute_net_flow(&self.trades_900s);
        let net_flow_3600s = self.compute_net_flow(&self.trades_3600s);
        let net_flow_7200s = self.compute_net_flow(&self.trades_7200s);
        let net_flow_14400s = self.compute_net_flow(&self.trades_14400s);
        
        // Buy/sell counts
        let buy_count_60s = self.count_buys(&self.trades_60s);
        let sell_count_60s = self.count_sells(&self.trades_60s);
        // ... repeat for other windows
        
        // DCA buy counts (Jupiter DCA only)
        let dca_buys_60s = self.count_dca_buys(&self.trades_60s);
        let dca_buys_300s = self.count_dca_buys(&self.trades_300s);
        let dca_buys_900s = self.count_dca_buys(&self.trades_900s);
        let dca_buys_3600s = self.count_dca_buys(&self.trades_3600s);
        let dca_buys_14400s = self.count_dca_buys(&self.trades_14400s);
        
        // Behavioral metrics
        let unique_wallets_300s = self.wallets_300s.len() as i64;
        let bot_trades_300s = self.bot_trades_300s as i64;
        let bot_wallets_300s = self.bot_wallets_300s.len() as i64;
        
        AggregatedTokenState {
            mint: self.mint.clone(),
            net_flow_60s_sol: Some(net_flow_60s),
            net_flow_300s_sol: Some(net_flow_300s),
            net_flow_900s_sol: Some(net_flow_900s),
            net_flow_3600s_sol: Some(net_flow_3600s),
            net_flow_7200s_sol: Some(net_flow_7200s),
            net_flow_14400s_sol: Some(net_flow_14400s),
            buy_count_60s: Some(buy_count_60s),
            sell_count_60s: Some(sell_count_60s),
            // ... more fields
            dca_buys_60s: Some(dca_buys_60s),
            dca_buys_300s: Some(dca_buys_300s),
            dca_buys_900s: Some(dca_buys_900s),
            dca_buys_3600s: Some(dca_buys_3600s),
            dca_buys_14400s: Some(dca_buys_14400s),
            unique_wallets_300s: Some(unique_wallets_300s),
            bot_trades_300s: Some(bot_trades_300s),
            bot_wallets_300s: Some(bot_wallets_300s),
            // ...
        }
    }
    
    fn compute_net_flow(&self, trades: &[Trade]) -> f64 {
        trades.iter().map(|t| {
            match t.action {
                TradeAction::Buy => t.sol_amount,
                TradeAction::Sell => -t.sol_amount,
            }
        }).sum()
    }
    
    fn count_dca_buys(&self, trades: &[Trade]) -> i32 {
        trades.iter()
            .filter(|t| t.program_name == "JupiterDCA" && t.action == TradeAction::Buy)
            .count() as i32
    }
}
```

**Pipeline Flow:**
```rust
// In pipeline_runtime.rs
loop {
    // 1. Read new trades from database
    let new_trades = sqlite_reader.read_new_trades().await?;
    
    for trade in new_trades {
        // 2. Add to rolling windows
        engine.process_trade(trade, now);
    }
    
    // 3. Compute aggregates (periodic flush)
    if should_flush(last_flush, flush_interval) {
        let aggregates = engine.compute_all_aggregates();
        
        // 4. Write to database
        db_writer.write_aggregates(aggregates).await?;
        
        last_flush = Instant::now();
    }
    
    tokio::time::sleep(poll_interval).await;
}
```

---

#### 2.2.3 SQLite Aggregate Writer (Output Layer)

**Component:** `pipeline/db.rs` → `SqliteAggregateWriter`

**Function:** Batched UPSERT to `token_aggregates` table.

**Implementation:**
```rust
pub struct SqliteAggregateWriter {
    conn: Arc<Mutex<Connection>>,
}

impl AggregateDbWriter for SqliteAggregateWriter {
    async fn write_aggregates(
        &self,
        aggregates: Vec<AggregatedTokenState>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Load batch size from environment (default: 500)
        let batch_size = std::env::var("FLUSH_BATCH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(500);
        
        let mut conn = self.conn.lock().unwrap();
        
        // Process in batches to avoid long-running transactions
        for chunk in aggregates.chunks(batch_size) {
            let tx = conn.transaction()?;
            
            for agg in chunk {
                tx.execute(
                    r#"
                    INSERT INTO token_aggregates (
                        mint, source_program, last_trade_timestamp,
                        net_flow_60s_sol, net_flow_300s_sol, net_flow_900s_sol,
                        net_flow_3600s_sol, net_flow_7200s_sol, net_flow_14400s_sol,
                        buy_count_60s, sell_count_60s,
                        buy_count_300s, sell_count_300s,
                        buy_count_900s, sell_count_900s,
                        unique_wallets_300s, bot_trades_300s, bot_wallets_300s,
                        avg_trade_size_300s_sol, volume_300s_sol,
                        dca_buys_60s, dca_buys_300s, dca_buys_900s, 
                        dca_buys_3600s, dca_buys_14400s,
                        price_usd, price_sol, market_cap_usd,
                        updated_at, created_at
                    ) VALUES (?, ?, ?, /* ... 30 params ... */)
                    ON CONFLICT(mint) DO UPDATE SET
                        source_program = excluded.source_program,
                        last_trade_timestamp = excluded.last_trade_timestamp,
                        net_flow_60s_sol = excluded.net_flow_60s_sol,
                        /* ... update all fields except created_at ... */
                        updated_at = excluded.updated_at
                    "#,
                    rusqlite::params![/* ... */],
                )?;
                
                // Write DCA activity buckets for sparkline (if DCA activity exists)
                if let Some(dca_3600s) = agg.dca_buys_3600s {
                    if dca_3600s > 0 {
                        Self::write_dca_buckets(&tx, &agg.mint, agg.updated_at, dca_3600s)?;
                    }
                }
            }
            
            tx.commit()?;
        }
        
        Ok(())
    }
}
```

**Write DCA Buckets (Sparkline Data):**
```rust
fn write_dca_buckets(
    tx: &rusqlite::Transaction,
    mint: &str,
    timestamp: i64,
    buy_count: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Floor timestamp to 60-second boundary
    let bucket_timestamp = (timestamp / 60) * 60;
    
    tx.execute(
        r#"
        INSERT OR REPLACE INTO dca_activity_buckets (
            mint, bucket_timestamp, buy_count
        ) VALUES (?, ?, ?)
        "#,
        rusqlite::params![mint, bucket_timestamp, buy_count],
    )?;
    
    Ok(())
}
```

---

## 3. Frontend Data Retrieval and Display Logic

### 3.1 Dashboard Query (Persistence Scorer Context)

**Location:** `pipeline/persistence_scorer.rs` → `fetch_active_tokens()`

**Query:**
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
    ta.bot_trades_300s,
    ta.buy_count_300s,
    ta.sell_count_300s,
    ta.dca_buys_3600s,
    ta.volume_300s_sol,
    ta.updated_at,
    ta.created_at,
    tm.pair_created_at
FROM token_aggregates ta
LEFT JOIN token_metadata tm ON ta.mint = tm.mint
WHERE ta.dca_buys_3600s > 0                         -- Active DCA filter
  AND (tm.blocked IS NULL OR tm.blocked = 0)       -- Exclude blocked tokens
ORDER BY ta.net_flow_300s_sol DESC                  -- Sort by 5-min net flow
LIMIT 100;                                          -- Top 100 tokens
```

**Query Breakdown:**

1. **Primary Filter:** `dca_buys_3600s > 0`
   - Only show tokens with Jupiter DCA activity in the last hour
   - Rationale: DCA orders indicate serious accumulation intent

2. **Blocklist Filter:** `(tm.blocked IS NULL OR tm.blocked = 0)`
   - Exclude spam tokens and scam addresses
   - Allows manual curation via `mint_blocklist` table

3. **Sort Order:** `ORDER BY net_flow_300s_sol DESC`
   - Tokens with highest 5-minute net SOL flow appear first
   - Positive values = Accumulation (buying > selling)
   - Negative values = Distribution (selling > buying)

4. **Result Limit:** `LIMIT 100`
   - Dashboard displays top 100 tokens
   - Reduces query time and UI clutter

**Index Usage:**
- `idx_token_aggregates_dca_buys_3600s` for WHERE clause
- `idx_token_aggregates_netflow_300s` for ORDER BY clause

---

### 3.2 Terminal UI Display (Main Application)

**Location:** `src/main.rs` + `ui/layout.rs`

**Architecture:**
```
main.rs
    ↓
YellowstoneGrpcClient → TradeProcessor → State (shared)
    ↓
ui::run_ui(state) → Ratatui rendering
```

**Display Logic (Terminal):**

```rust
// ui/layout.rs
fn render_trades_table(f: &mut Frame, area: Rect, state: &State) {
    let trades = state.get_recent_trades();
    
    let rows: Vec<Row> = trades
        .iter()
        .rev()  // Show newest first
        .take(50)  // Limit to 50 rows
        .map(|trade| {
            let direction_str = match trade.direction {
                TradeKind::Buy => "BUY",
                TradeKind::Sell => "SELL",
                _ => "UNK",
            };
            
            let direction_color = match trade.direction {
                TradeKind::Buy => Color::Green,
                TradeKind::Sell => Color::Red,
                _ => Color::Gray,
            };
            
            // Get net volume for this token
            let net_vol = state
                .get_token_metrics(&trade.mint)
                .map(|m| m.buy_volume_sol - m.sell_volume_sol)
                .unwrap_or(0.0);
            
            Row::new(vec![
                format_timestamp(trade.timestamp),
                trade.mint[..8].to_string(),  // First 8 chars
                direction_str.to_string(),
                format!("{:.6}", trade.sol_amount),
                format!("{:.2}", trade.token_amount),
                format!("{:.6}", net_vol),
            ])
            .style(Style::default().fg(direction_color))
        })
        .collect();
    
    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            "Time", "Mint", "Direction", "SOL Amount", "Token Amount", "Net Vol"
        ]))
        .block(Block::default().borders(Borders::ALL).title("Recent Trades"));
    
    f.render_widget(table, area);
}
```

**Display Fields:**
- **Time:** HH:MM:SS format (converted from Unix timestamp)
- **Mint:** First 8 characters of token address
- **Direction:** BUY (green), SELL (red), UNK (gray)
- **SOL Amount:** Trade size in SOL (6 decimal places)
- **Token Amount:** Token quantity (2 decimal places)
- **Net Vol:** Cumulative net volume for this token (buy - sell)

**Data Flow:**
```
Yellowstone gRPC → TradeProcessor → State.add_trade() → UI reads State
                                           ↓
                                    Shared RwLock<State>
```

**State Struct:**
```rust
pub struct State {
    trades: VecDeque<Trade>,           // Last 1000 trades
    token_metrics: HashMap<String, TokenMetrics>,
    aggregator: VolumeAggregator,      // Rolling window calculator
}

pub struct TokenMetrics {
    pub mint: String,
    pub buy_volume_sol: f64,
    pub sell_volume_sol: f64,
    pub trade_count: usize,
}
```

---

### 3.3 Real Dashboard Query (Web Frontend - Future)

**Note:** Current implementation uses Terminal UI. Web dashboard query would be:

```sql
-- Dashboard endpoint: /api/tokens/top
SELECT 
    ta.mint,
    ta.net_flow_300s_sol,
    ta.net_flow_3600s_sol,
    ta.dca_buys_3600s,
    ta.unique_wallets_300s,
    ta.volume_300s_sol,
    ta.buy_count_300s,
    ta.sell_count_300s,
    ta.price_usd,
    ta.market_cap_usd,
    tm.symbol,
    tm.name,
    tss.persistence_score,
    tss.pattern_tag,
    tss.confidence
FROM token_aggregates ta
LEFT JOIN token_metadata tm ON ta.mint = tm.mint
LEFT JOIN token_signal_summary tss ON ta.mint = tss.token_address
WHERE ta.dca_buys_3600s > 0
  AND (tm.blocked IS NULL OR tm.blocked = 0)
ORDER BY ta.net_flow_300s_sol DESC
LIMIT 100;
```

**Sparkline Data Query:**
```sql
-- Endpoint: /api/tokens/{mint}/sparkline
SELECT bucket_timestamp, buy_count
FROM dca_activity_buckets
WHERE mint = ?
  AND bucket_timestamp > (unixepoch() - 7200)  -- Last 2 hours
ORDER BY bucket_timestamp ASC;
```

---

## 4. Cleanup and Aging Mechanisms

### 4.1 DCA Bucket Cleanup (Scheduled)

**Location:** `pipeline/db.rs` → `SqliteAggregateWriter::cleanup_old_dca_buckets()`

**Schedule:** Every 5 minutes (configured in `pipeline_runtime.rs`)

**Implementation:**
```rust
pub fn cleanup_old_dca_buckets(&self) -> Result<usize, Box<dyn std::error::Error>> {
    let conn = self.conn.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    
    let cutoff = now - 7200;  // 2 hours
    
    let deleted = conn.execute(
        "DELETE FROM dca_activity_buckets WHERE bucket_timestamp < ?",
        rusqlite::params![cutoff],
    )?;
    
    if deleted > 0 {
        log::debug!("🧹 Cleaned up {} old DCA buckets (older than {})", deleted, cutoff);
    }
    
    Ok(deleted)
}
```

**Retention Policy:**
- **Keep:** Last 2 hours (120 buckets)
- **Delete:** Buckets older than 2 hours
- **Rationale:** Sparkline only displays 1-hour window, 2-hour buffer for safety

**Trigger Location:**
```rust
// pipeline_runtime.rs
let mut cleanup_ticker = interval(Duration::from_secs(300));  // 5 minutes

loop {
    tokio::select! {
        _ = cleanup_ticker.tick() => {
            if let Some(sqlite_writer) = db_writer.as_any()
                .downcast_ref::<SqliteAggregateWriter>() {
                match sqlite_writer.cleanup_old_dca_buckets() {
                    Ok(deleted) if deleted > 0 => {
                        info!("🧹 DCA bucket cleanup: removed {} old buckets", deleted);
                    }
                    Err(e) => {
                        error!("❌ DCA bucket cleanup failed: {}", e);
                    }
                    _ => {}
                }
            }
        }
    }
}
```

---

### 4.2 Rolling Window Trade Eviction (In-Memory)

**Location:** `pipeline/state.rs` → `TokenState::evict_old_trades()`

**Trigger:** On every trade ingestion (before adding new trade)

**Implementation:**
```rust
pub fn evict_old_trades(&mut self, now: i64) {
    // Calculate cutoffs for each window
    let cutoff_60s = now - 60;
    let cutoff_300s = now - 300;
    let cutoff_900s = now - 900;
    let cutoff_3600s = now - 3600;
    let cutoff_7200s = now - 7200;
    let cutoff_14400s = now - 14400;
    
    // Remove trades older than each window
    self.trades_60s.retain(|t| t.timestamp > cutoff_60s);
    self.trades_300s.retain(|t| t.timestamp > cutoff_300s);
    self.trades_900s.retain(|t| t.timestamp > cutoff_900s);
    self.trades_3600s.retain(|t| t.timestamp > cutoff_3600s);
    self.trades_7200s.retain(|t| t.timestamp > cutoff_7200s);
    self.trades_14400s.retain(|t| t.timestamp > cutoff_14400s);
    
    // Evict from program-specific buckets
    self.pumpswap_trades_300s.retain(|t| t.timestamp > cutoff_300s);
    self.jupiter_dca_trades_300s.retain(|t| t.timestamp > cutoff_300s);
    
    // Rebuild wallet set (recount unique wallets from remaining trades)
    self.wallets_300s.clear();
    for trade in &self.trades_300s {
        if let Some(wallet) = &trade.user_account {
            self.wallets_300s.insert(wallet.clone());
        }
    }
    
    // Rebuild bot wallet set
    self.bot_wallets_300s.clear();
    for trade in &self.trades_300s {
        if self.is_bot_trade(trade) {
            if let Some(wallet) = &trade.user_account {
                self.bot_wallets_300s.insert(wallet.clone());
            }
        }
    }
}
```

**Scope:** In-memory only (database rows persist)

**Rationale:**
- Keeps memory usage bounded
- Ensures accurate rolling window calculations
- No impact on historical database records

---

### 4.3 No Explicit Token Removal (Database Persistence)

**Observation:** There is **no automated cleanup** of stale tokens from `token_aggregates` table.

**Current Behavior:**
- Inactive tokens remain in database indefinitely
- Dashboard filters by `dca_buys_3600s > 0` to hide inactive tokens
- Tokens without recent DCA activity disappear from display but persist in database

**Implications:**

**Positive:**
- Historical data preserved for backtesting
- Tokens can reappear if activity resumes
- No data loss from premature deletion

**Negative:**
- Database grows unbounded
- Query performance may degrade over time (millions of rows)
- Disk space consumption increases

**Recommendation:** Consider implementing a soft-delete or archival strategy:

```sql
-- Option 1: Add active flag (soft delete)
ALTER TABLE token_aggregates ADD COLUMN active INTEGER DEFAULT 1;

UPDATE token_aggregates 
SET active = 0 
WHERE updated_at < (unixepoch() - 86400)  -- Inactive for 24 hours
  AND dca_buys_3600s = 0;

-- Dashboard query adds: AND active = 1

-- Option 2: Archive old tokens to separate table
CREATE TABLE token_aggregates_archive AS 
SELECT * FROM token_aggregates 
WHERE updated_at < (unixepoch() - 604800);  -- 1 week old

DELETE FROM token_aggregates 
WHERE updated_at < (unixepoch() - 604800);
```

---

### 4.4 Signal Age-Based Confidence Adjustment

**Location:** `pipeline/persistence_scorer.rs` → `compute_age_multiplier()`

**Purpose:** Reduce confidence for very new tokens, boost confidence for mature tokens.

**Implementation:**
```rust
fn compute_age_multiplier(&self, pair_created_at: Option<i64>, now: i64) -> f64 {
    let Some(created_at) = pair_created_at else {
        return 0.8;  // Unknown age: modest penalty
    };
    
    let age_seconds = now - created_at;
    let age_hours = age_seconds as f64 / 3600.0;
    let age_days = age_hours / 24.0;
    
    if age_hours < 1.0 {
        0.5  // <1h: strongest penalty (50%)
    } else if age_hours < 24.0 {
        0.7  // 1-24h: moderate penalty (30%)
    } else if age_days < 7.0 {
        1.0  // 1-7d: neutral (no change)
    } else if age_days < 30.0 {
        1.1  // 7-30d: small boost (10%)
    } else {
        1.3  // >30d: stronger boost (30%)
    }
}
```

**Rationale:**
- New tokens (<1h) are high-risk (pump & dump potential)
- Young tokens (1-24h) lack sufficient data
- Mature tokens (>30d) have proven track record

**Impact on Dashboard:**
```rust
// Confidence calculation with age adjustment
let base_confidence = data_richness * 0.4 
                    + lifetime_factor * 0.3 
                    + (1.0 - bot_ratio) * 0.3;

let age_multiplier = self.compute_age_multiplier(token.pair_created_at, now);
let adjusted_confidence = base_confidence * age_multiplier;

let confidence_level = if adjusted_confidence > 0.7 { "HIGH" }
    else if adjusted_confidence > 0.4 { "MEDIUM" }
    else { "LOW" };
```

---

### 4.5 Trades Table Growth (Unbounded)

**Observation:** `trades` table has **no retention policy** and grows indefinitely.

**Current State:**
- All trades are stored permanently
- No cleanup or archival mechanism
- Oldest trades remain in database forever

**Growth Estimates:**
- Assume 1000 trades/minute (moderate activity)
- 1,440,000 trades/day
- 525,600,000 trades/year (~525 million rows)
- ~100 bytes/row → ~50GB/year

**Recommendation:** Implement periodic archival or pruning:

```sql
-- Option 1: Archive old trades (>30 days)
CREATE TABLE trades_archive AS 
SELECT * FROM trades 
WHERE timestamp < (unixepoch() - 2592000);

DELETE FROM trades 
WHERE timestamp < (unixepoch() - 2592000);

-- Option 2: Drop old trades (if historical data not needed)
DELETE FROM trades 
WHERE timestamp < (unixepoch() - 2592000);

-- Schedule via cron or background task (monthly)
```

---

## 5. Architectural Insights and Recommendations

### 5.1 Key Architectural Patterns

#### 5.1.1 Dual-Purpose Database
**Pattern:** SQLite serves as both operational store (real-time writes) and analytics engine (aggregations)

**Advantages:**
- Simple architecture (no separate OLAP database)
- Low operational complexity
- Efficient for moderate scale (< 1M tokens)

**Limitations:**
- Write contention during high load
- Query performance degrades with large datasets
- Not horizontally scalable

**Recommendation:** Consider migration to PostgreSQL or ClickHouse for production scale.

---

#### 5.1.2 Activity-Based Filtering (No Time-Based Expiration)

**Current Strategy:**
- Dashboard filters by `dca_buys_3600s > 0` (activity threshold)
- No explicit deletion of inactive tokens
- Inactive tokens remain in database but hidden

**Alternative Strategy (Time-Based):**
```sql
-- Option 1: Filter by last update time
WHERE updated_at > (unixepoch() - 3600)  -- Active in last hour

-- Option 2: Filter by last trade time
WHERE last_trade_timestamp > (unixepoch() - 3600)
```

**Trade-offs:**
| Strategy | Pros | Cons |
|----------|------|------|
| **Activity-Based** (current) | Preserves all data, flexible thresholds | Database grows unbounded |
| **Time-Based** | Bounded dataset, predictable performance | May miss slow-accumulating tokens |
| **Hybrid** | Best of both worlds | More complex query logic |

**Recommendation:** Implement hybrid approach:
```sql
WHERE (dca_buys_3600s > 0 OR persistence_score > 7)
  AND updated_at > (unixepoch() - 86400)  -- At least updated in last 24h
```

---

#### 5.1.3 Pipeline Parallelism

**Architecture:**
```
Unified Streamer (writes to trades)
    ↓
Pipeline Runtime (reads from trades, writes to token_aggregates)
    ↓
Persistence Scorer (reads from token_aggregates, writes to token_signal_summary)
```

**Advantages:**
- Independent failure isolation (streamer crash doesn't affect aggregator)
- Horizontal scaling potential (run multiple pipeline workers)
- Clear separation of concerns

**Considerations:**
- Cursor-based incremental reads prevent duplicate processing
- SQLite WAL mode enables concurrent reads during writes
- Potential lag between streaming and aggregation (acceptable for analytics)

---

#### 5.1.4 Batched Writes for Performance

**Pattern:** Accumulate writes in memory, flush periodically or on size threshold.

**Examples:**
- **Streamer:** 100 trades / 2 seconds → `trades` table
- **Pipeline:** 500 mints / 5 seconds → `token_aggregates` table

**Benefits:**
- Reduces SQLite lock contention
- Amortizes transaction overhead
- Improves write throughput

**Trade-off:** Slight delay in data visibility (acceptable for analytics)

---

### 5.2 Critical Observations

#### 5.2.1 No Raw Trade Retention Policy
**Issue:** `trades` table grows indefinitely (estimated 50GB/year)

**Impact:**
- Disk space exhaustion risk
- Query performance degradation
- Backup/restore complexity

**Solution:** Implement monthly archival:
```bash
# Cron job (monthly)
sqlite3 /var/lib/solflow/solflow.db <<EOF
CREATE TABLE IF NOT EXISTS trades_archive_$(date +%Y%m) AS 
SELECT * FROM trades 
WHERE timestamp < (unixepoch() - 2592000);

DELETE FROM trades 
WHERE timestamp < (unixepoch() - 2592000);
EOF
```

---

#### 5.2.2 Dashboard Relies on Single Sort Column
**Current:** `ORDER BY net_flow_300s_sol DESC`

**Limitation:** Only one sorting dimension visible at a time

**Enhancement:** Multi-factor ranking:
```sql
-- Composite score (example)
SELECT 
    *,
    (net_flow_300s_sol * 0.4 +            -- Net flow weight
     dca_buys_3600s * 0.3 +               -- DCA activity weight
     unique_wallets_300s * 0.2 +          -- Wallet diversity weight
     persistence_score * 0.1              -- Historical persistence weight
    ) AS composite_score
FROM token_aggregates
ORDER BY composite_score DESC;
```

---

#### 5.2.3 Signal Scoring Decoupled from Real-Time Pipeline
**Current:** Persistence scorer runs as separate periodic job

**Implication:**
- `token_signal_summary` may be stale (out of sync with `token_aggregates`)
- Dashboard shows latest aggregates but outdated persistence scores

**Recommendation:** Trigger scoring on aggregate write:
```rust
// After writing aggregates
if needs_scoring_update(aggregate) {
    persistence_scorer.score_token(&aggregate.mint).await?;
}
```

---

#### 5.2.4 No Health Monitoring or Alerting
**Gap:** No built-in mechanism to detect:
- Pipeline lag (aggregator falling behind streamer)
- Database lock contention
- Disk space exhaustion
- Stale data (no recent updates)

**Recommendation:** Add health checks:
```rust
// Health endpoint/log
pub struct PipelineHealth {
    pub streamer_lag_seconds: i64,      // Time since last trade write
    pub aggregator_lag_seconds: i64,    // Time since last aggregate update
    pub pending_trades: usize,          // Backlog size
    pub db_size_mb: u64,
    pub oldest_trade_age_hours: i64,
}
```

---

### 5.3 Scalability Considerations

#### 5.3.1 Current Limits (SQLite)

**Theoretical:**
- Max database size: 281 TB (SQLite limit)
- Max rows per table: 2^64 (effectively unlimited)

**Practical:**
- Write throughput: ~10,000 INSERTs/sec (with WAL)
- Query performance: Degrades after ~100M rows without partitioning
- Concurrent writers: Limited (single writer in WAL mode)

**Estimated Capacity:**
- **Trades:** ~1M trades/day → SQLite sufficient for 1-2 years
- **Aggregates:** ~100K tokens → SQLite sufficient for 5+ years

---

#### 5.3.2 Migration Path to PostgreSQL

**When to migrate:**
- Trades table > 500M rows
- Write throughput > 5,000 trades/sec
- Need for horizontal read scaling

**Migration steps:**
1. Export SQLite to CSV: `sqlite3 solflow.db ".mode csv" ".output trades.csv" "SELECT * FROM trades;"`
2. Create PostgreSQL schema (same structure)
3. Import CSV: `COPY trades FROM 'trades.csv' CSV HEADER;`
4. Update connection strings in code
5. Test query performance (add indexes as needed)

**PostgreSQL advantages:**
- Better write concurrency (MVCC)
- Horizontal read replicas
- Advanced indexing (BRIN, GIN, GIST)
- Partitioning for time-series data

---

### 5.4 Security and Data Integrity

#### 5.4.1 Blocklist Validation
**Current:** Checked only on signal writes

**Gap:** No validation on aggregate writes or dashboard queries

**Recommendation:** Add blocklist check to dashboard query:
```sql
WHERE (tm.blocked IS NULL OR tm.blocked = 0)  -- Already present
```

---

#### 5.4.2 SQL Injection Prevention
**Current:** All queries use parameterized statements (`rusqlite::params!`)

**Status:** ✅ Secure (no raw string interpolation)

---

#### 5.4.3 Database Backup Strategy
**Gap:** No documented backup/restore procedure

**Recommendation:**
```bash
# Backup (daily cron)
sqlite3 /var/lib/solflow/solflow.db ".backup /backups/solflow_$(date +%Y%m%d).db"

# Restore
sqlite3 /var/lib/solflow/solflow.db ".restore /backups/solflow_20251127.db"
```

---

## 6. Conclusion

SolFlow implements a robust, pipeline-based architecture for real-time DEX trade analytics with the following strengths:

✅ **Strengths:**
- Clean separation of concerns (streamer → aggregator → scorer)
- Efficient rolling window calculations
- Duplicate prevention via unique constraints
- Batched writes for performance
- WAL mode for concurrency
- Age-weighted confidence scoring

⚠️ **Areas for Improvement:**
- Implement retention policies for `trades` table (50GB/year growth)
- Add stale token cleanup for `token_aggregates`
- Multi-factor dashboard ranking (beyond single sort column)
- Real-time health monitoring and alerting
- Database backup automation

📊 **Scalability:**
- Current architecture sufficient for 1-2 years at moderate load
- PostgreSQL migration recommended beyond 500M trades
- Horizontal scaling possible with read replicas

This architecture review provides a comprehensive foundation for future development, optimization, and troubleshooting of the SolFlow application stack.

---

## Appendix A: Quick Reference

### Database Tables Summary
| Table | Purpose | Row Count (Est.) | Primary Key | Retention |
|-------|---------|------------------|-------------|-----------|
| `trades` | Raw events | ~1M/day | `id` (autoincrement) | **Unlimited** ⚠️ |
| `token_aggregates` | Metrics | ~100K | `mint` | Unlimited (filtered display) |
| `token_metadata` | Enrichment | ~100K | `mint` | Unlimited |
| `token_signals` | Signal log | ~10K/day | `id` (autoincrement) | Unlimited |
| `token_signal_summary` | Scoring | ~10K | `token_address` | Unlimited |
| `dca_activity_buckets` | Sparkline | ~7K per mint | `(mint, bucket_timestamp)` | **2 hours** ✅ |
| `mint_blocklist` | Filtering | ~100 | `mint` | Per `expires_at` |

### Key Metrics
| Metric | Field | Window | Formula |
|--------|-------|--------|---------|
| Net SOL Flow | `net_flow_300s_sol` | 5 min | Σ(buy_volumes) - Σ(sell_volumes) |
| DCA Activity | `dca_buys_3600s` | 1 hour | count(JupiterDCA + BUY) |
| Unique Wallets | `unique_wallets_300s` | 5 min | DISTINCT(user_accounts) |
| Bot Ratio | `bot_trades_300s / total_trades` | 5 min | Pattern detection |

### Critical Queries
```sql
-- Dashboard top tokens
SELECT * FROM token_aggregates 
WHERE dca_buys_3600s > 0 
ORDER BY net_flow_300s_sol DESC 
LIMIT 100;

-- Sparkline data
SELECT bucket_timestamp, buy_count 
FROM dca_activity_buckets 
WHERE mint = ? AND bucket_timestamp > (unixepoch() - 7200)
ORDER BY bucket_timestamp ASC;

-- Blocklist check
SELECT mint FROM mint_blocklist 
WHERE mint = ? AND (expires_at IS NULL OR expires_at > ?);
```

### File Locations
| Component | Path |
|-----------|------|
| SQL Schemas | `examples/solflow/sql/*.sql` |
| Pipeline Runtime | `examples/solflow/src/bin/pipeline_runtime.rs` |
| Unified Streamer | `examples/solflow/src/bin/unified_streamer.rs` |
| InstructionScanner | `examples/solflow/src/instruction_scanner.rs` |
| Database Writer | `examples/solflow/src/pipeline/db.rs` |
| Persistence Scorer | `examples/solflow/src/pipeline/persistence_scorer.rs` |
| Terminal UI | `examples/solflow/src/main.rs` + `src/ui/layout.rs` |

---

**End of Document**
