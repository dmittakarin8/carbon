# SolFlow Frontend Quick Start

**For:** Frontend developers who need to get started quickly  
**Full Documentation:** See `FRONTEND_ARCHITECTURE.md` for complete details

---

## TL;DR - What You Need to Know

### ✅ Use These Tables (Primary Data Sources)

```sql
-- 1. token_aggregates - Rolling-window metrics (your main data source)
SELECT 
    mint,                      -- Token address
    net_flow_300s_sol,         -- 5-min net buying pressure
    buy_count_300s,            -- Buy trades in 5 minutes
    sell_count_300s,           -- Sell trades in 5 minutes
    unique_wallets_300s,       -- Unique traders
    volume_300s_sol,           -- Total volume
    updated_at                 -- Last update (should be <10s old)
FROM token_aggregates
WHERE updated_at > unixepoch() - 60  -- Fresh data only
ORDER BY net_flow_300s_sol DESC
LIMIT 50;

-- 2. token_signals - Real-time alerts
SELECT 
    mint,
    signal_type,               -- "BREAKOUT", "SURGE", "FOCUSED", "BOT_DROPOFF"
    severity,                  -- 1-5 (higher = more important)
    created_at
FROM token_signals
WHERE created_at > unixepoch() - 3600  -- Last hour
ORDER BY created_at DESC, severity DESC;
```

### ❌ Don't Use These

- **`trades` table** - Raw data, not aggregated, poor performance
- **JSONL files** (`streams/*/events.jsonl`) - Legacy backup, not for frontend
- **`aggregator` binary outputs** - Separate system, not part of main runtime

---

## Database Connection

**Location:** `/var/lib/solflow/solflow.db` (or `SOLFLOW_DB_PATH` env var)

**Connection String:**
```javascript
// Node.js example (better-sqlite3)
const Database = require('better-sqlite3');
const db = new Database('/var/lib/solflow/solflow.db', { readonly: true });

// Python example (sqlite3)
import sqlite3
conn = sqlite3.connect('/var/lib/solflow/solflow.db')
```

**Important:** Open as **read-only** from frontend (backend writes to it).

---

## Top 5 Essential Queries

### 1. Dashboard - Hot Tokens List

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
WHERE updated_at > unixepoch() - 60
  AND mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())
ORDER BY net_flow_300s_sol DESC
LIMIT 50;
```

**Refresh:** Every 5 seconds  
**Returns:** Top 50 tokens by buying pressure

---

### 2. Token Detail Page

```sql
-- Get all metrics for a token
SELECT * FROM token_aggregates WHERE mint = ?;

-- Get recent signals for this token
SELECT * FROM token_signals 
WHERE mint = ? 
ORDER BY created_at DESC 
LIMIT 20;
```

**Refresh:** Every 5 seconds  
**Note:** May return multiple rows from `token_aggregates` if token trades on multiple DEXes (PumpSwap, BonkSwap, etc.). Aggregate them:

```sql
SELECT 
    mint,
    SUM(net_flow_300s_sol) as total_net_flow,
    SUM(buy_count_300s) as total_buys,
    SUM(sell_count_300s) as total_sells,
    SUM(volume_300s_sol) as total_volume
FROM token_aggregates
WHERE mint = ?
GROUP BY mint;
```

---

### 3. Recent Alerts Feed

```sql
SELECT 
    id,
    mint,
    signal_type,
    severity,
    score,
    created_at
FROM token_signals
WHERE created_at > unixepoch() - 3600
  AND mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())
ORDER BY created_at DESC, severity DESC
LIMIT 100;
```

**Refresh:** Every 3-5 seconds  
**Returns:** Recent signals sorted by time and importance

---

### 4. Mark Signal as Seen

```sql
UPDATE token_signals
SET seen_in_terminal = 1
WHERE id IN (?, ?, ?, ...);
```

**Use Case:** Track which alerts user has already viewed

---

### 5. Check System Health

```sql
-- Find stale data (backend may be down)
SELECT 
    mint, 
    updated_at,
    (unixepoch() - updated_at) as seconds_ago
FROM token_aggregates
WHERE (unixepoch() - updated_at) > 60
ORDER BY seconds_ago DESC;
```

**Expected:** All tokens updated within last 10 seconds  
**If stale:** Check if `pipeline_runtime` binary is running

---

## Schema Cheat Sheet

### token_aggregates (Primary Metrics)

| Column | Type | Description |
|--------|------|-------------|
| `mint` | TEXT | Token address (PRIMARY KEY) |
| `source_program` | TEXT | "PumpSwap", "BonkSwap", "Moonshot", "JupiterDCA" |
| `net_flow_60s_sol` | REAL | 1-minute net buying pressure |
| `net_flow_300s_sol` | REAL | 5-minute net buying pressure ⭐ |
| `net_flow_900s_sol` | REAL | 15-minute net buying pressure |
| `buy_count_60s` | INTEGER | Buy trades in 1 minute |
| `buy_count_300s` | INTEGER | Buy trades in 5 minutes ⭐ |
| `sell_count_300s` | INTEGER | Sell trades in 5 minutes ⭐ |
| `unique_wallets_300s` | INTEGER | Unique traders (5 minutes) ⭐ |
| `bot_trades_300s` | INTEGER | Suspected bot trades |
| `volume_300s_sol` | REAL | Total volume (5 minutes) ⭐ |
| `updated_at` | INTEGER | Last update timestamp ⭐ |

⭐ = Most important columns for frontend

---

### token_signals (Alerts)

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Auto-incrementing ID |
| `mint` | TEXT | Token address |
| `signal_type` | TEXT | "BREAKOUT", "SURGE", "FOCUSED", "BOT_DROPOFF" |
| `severity` | INTEGER | 1-5 (higher = more urgent) |
| `score` | REAL | Signal strength |
| `created_at` | INTEGER | Unix timestamp |
| `seen_in_terminal` | INTEGER | 0=unseen, 1=seen |

---

## Signal Types

| Type | Meaning | Condition |
|------|---------|-----------|
| `BREAKOUT` | Large buying pressure | `net_flow_300s > 50 SOL` AND `buy_count_300s > 10` |
| `SURGE` | Rapid accumulation | `buy_count_60s > 5` AND `net_flow_60s > 10 SOL` |
| `FOCUSED` | Concentrated buying (possible insider) | `unique_wallets_300s < 5` AND `volume_300s > 100 SOL` |
| `BOT_DROPOFF` | Bot activity ceased (organic interest?) | Previous bot count > 5, now ≤ 2 |

---

## Data Refresh Rates

**Backend Updates:**
- `token_aggregates`: Every 5 seconds
- `token_signals`: When conditions are met (varies)

**Recommended Frontend Polling:**
- Dashboard: **5-10 seconds**
- Token detail page: **5 seconds**
- Alert feed: **3-5 seconds**
- Historical charts: **On-demand (no polling)**

---

## Common Pitfalls

### ❌ Don't Query `trades` Table

```sql
-- BAD: Raw trades query (slow, unbounded)
SELECT * FROM trades WHERE mint = ?;  -- DON'T DO THIS
```

**Why:** Raw data, millions of rows, not indexed for frontend queries.

**Instead:**
```sql
-- GOOD: Use aggregated metrics
SELECT * FROM token_aggregates WHERE mint = ?;
```

---

### ❌ Don't Forget Blocklist Filter

```sql
-- BAD: Includes blocked tokens
SELECT * FROM token_aggregates ORDER BY net_flow_300s_sol DESC;
```

**Instead:**
```sql
-- GOOD: Filter out blocked tokens
SELECT * FROM token_aggregates
WHERE mint NOT IN (
    SELECT mint FROM mint_blocklist 
    WHERE expires_at IS NULL OR expires_at > unixepoch()
)
ORDER BY net_flow_300s_sol DESC;
```

---

### ❌ Don't Assume Single Row per Token

```sql
-- BAD: Assumes one row per mint
SELECT * FROM token_aggregates WHERE mint = ?;  -- May return 2-3 rows!
```

**Why:** Each DEX (PumpSwap, BonkSwap, etc.) gets a separate row.

**Instead:**
```sql
-- GOOD: Aggregate across DEXes
SELECT 
    mint,
    SUM(net_flow_300s_sol) as total_net_flow
FROM token_aggregates
WHERE mint = ?
GROUP BY mint;
```

---

## Architecture Overview (Simplified)

```
┌─────────────────────────────────────────┐
│  Solana Blockchain (Yellowstone gRPC)   │
└────────────┬────────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  4 Streamer Binaries                    │
│  - PumpSwap, BonkSwap                   │
│  - Moonshot, Jupiter DCA                │
└────────────┬───────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  PipelineEngine (in-memory)             │
│  - Rolling windows (60s/300s/900s)      │
│  - Signal detection                     │
└────────────┬───────────────────────────┘
             │ (flush every 5 seconds)
             ▼
┌────────────────────────────────────────┐
│  SQLite Database                        │
│  - token_aggregates (metrics)           │
│  - token_signals (alerts)               │
└────────────┬───────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  YOUR FRONTEND                          │
│  - Query every 5 seconds                │
│  - Display metrics & alerts             │
└─────────────────────────────────────────┘
```

---

## Running the Backend

**Start the pipeline runtime:**
```bash
ENABLE_PIPELINE=true cargo run --release --bin pipeline_runtime
```

**Verify it's working:**
```bash
# Check if database exists
ls -lh /var/lib/solflow/solflow.db

# Query token count
sqlite3 /var/lib/solflow/solflow.db "SELECT COUNT(*) FROM token_aggregates;"

# Check recent signals
sqlite3 /var/lib/solflow/solflow.db "SELECT * FROM token_signals ORDER BY created_at DESC LIMIT 5;"
```

**Logs:**
```bash
RUST_LOG=info cargo run --release --bin pipeline_runtime 2>&1 | tee pipeline.log
```

---

## Example Frontend Code

### JavaScript/Node.js (better-sqlite3)

```javascript
const Database = require('better-sqlite3');
const db = new Database('/var/lib/solflow/solflow.db', { readonly: true });

// Get top tokens
function getTopTokens() {
  const now = Math.floor(Date.now() / 1000);
  return db.prepare(`
    SELECT 
      mint,
      net_flow_300s_sol,
      buy_count_300s,
      sell_count_300s,
      unique_wallets_300s,
      volume_300s_sol,
      updated_at
    FROM token_aggregates
    WHERE updated_at > ?
      AND mint NOT IN (
        SELECT mint FROM mint_blocklist 
        WHERE expires_at IS NULL OR expires_at > ?
      )
    ORDER BY net_flow_300s_sol DESC
    LIMIT 50
  `).all(now - 60, now);
}

// Get recent signals
function getRecentSignals(limit = 100) {
  const now = Math.floor(Date.now() / 1000);
  return db.prepare(`
    SELECT 
      id, mint, signal_type, severity, score, created_at
    FROM token_signals
    WHERE created_at > ?
      AND mint NOT IN (
        SELECT mint FROM mint_blocklist 
        WHERE expires_at IS NULL OR expires_at > ?
      )
    ORDER BY created_at DESC, severity DESC
    LIMIT ?
  `).all(now - 3600, now, limit);
}

// Poll every 5 seconds
setInterval(() => {
  const tokens = getTopTokens();
  const signals = getRecentSignals();
  console.log(`Found ${tokens.length} tokens, ${signals.length} signals`);
  // Update UI here
}, 5000);
```

---

### Python (sqlite3)

```python
import sqlite3
import time

def get_top_tokens(db_path='/var/lib/solflow/solflow.db'):
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row  # Return rows as dicts
    cursor = conn.cursor()
    
    now = int(time.time())
    cursor.execute("""
        SELECT 
            mint,
            net_flow_300s_sol,
            buy_count_300s,
            sell_count_300s,
            unique_wallets_300s,
            volume_300s_sol,
            updated_at
        FROM token_aggregates
        WHERE updated_at > ?
          AND mint NOT IN (
            SELECT mint FROM mint_blocklist 
            WHERE expires_at IS NULL OR expires_at > ?
          )
        ORDER BY net_flow_300s_sol DESC
        LIMIT 50
    """, (now - 60, now))
    
    return [dict(row) for row in cursor.fetchall()]

# Poll every 5 seconds
while True:
    tokens = get_top_tokens()
    print(f"Found {len(tokens)} tokens")
    for token in tokens[:5]:  # Top 5
        print(f"  {token['mint'][:8]}... net_flow={token['net_flow_300s_sol']:.2f} SOL")
    time.sleep(5)
```

---

## Troubleshooting

### "No data in token_aggregates"

**Check:**
1. Is `pipeline_runtime` running?
   ```bash
   ps aux | grep pipeline_runtime
   ```

2. Check logs for errors:
   ```bash
   tail -f pipeline.log
   ```

3. Verify environment variable:
   ```bash
   echo $ENABLE_PIPELINE  # Should be "true"
   ```

---

### "Data is stale (updated_at > 60 seconds ago)"

**Possible causes:**
1. Backend crashed (check logs)
2. No trading activity on monitored DEXes
3. gRPC connection issue (check `GEYSER_URL`)

**Debug:**
```bash
# Check ingestion logs
grep "Ingestion rate" pipeline.log

# Check flush logs
grep "Flush complete" pipeline.log
```

---

### "Getting data from `trades` table by mistake"

**Symptom:** Queries are slow, data looks like raw events

**Fix:** Change table name from `trades` to `token_aggregates`:
```sql
-- WRONG
SELECT * FROM trades WHERE mint = ?;

-- CORRECT
SELECT * FROM token_aggregates WHERE mint = ?;
```

---

## What's NOT Implemented Yet

⚠️ **Price Data:**
- `price_usd`, `price_sol`, `market_cap_usd` columns exist but are NOT populated
- Frontend must fetch prices from external APIs (Jupiter, Birdeye)

⚠️ **Token Metadata:**
- `token_metadata` table exists but is NOT populated
- Frontend must fetch token names/symbols separately

⚠️ **DCA Correlation:**
- DCA overlap % is NOT in `token_aggregates` (separate `aggregator` binary required)
- `ACCUMULATION` signals are NOT emitted by primary pipeline

⚠️ **Historical Data:**
- Rolling windows are in-memory only (no history beyond 15 minutes)
- Use `token_signals` table for event timeline

---

## Next Steps

1. **Read this quick start** ✅ (you're here!)
2. **Test basic queries** against your local database
3. **Build UI components** using `token_aggregates` and `token_signals`
4. **Set up polling** (every 5 seconds)
5. **Refer to full docs** (`FRONTEND_ARCHITECTURE.md`) for advanced topics

---

## Getting Help

**Full Documentation:** `/docs/FRONTEND_ARCHITECTURE.md` (1,769 lines, comprehensive)

**Key Sections:**
- Component survey and binary classification
- Complete pipeline architecture
- Full SQLite schema with all columns
- Advanced query patterns
- Legacy vs new component analysis

**Contact:** Backend team for:
- DCA correlation integration questions
- Price enrichment timeline
- Schema migration issues

---

**Document Version:** 1.0 (2025-11-16)
