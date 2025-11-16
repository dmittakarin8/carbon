# Frontend Architecture Analysis

**Date:** 2025-01-15  
**Purpose:** Comprehensive review of frontend data flow, column derivation, and data sources

---

## Executive Summary

This document provides a complete architectural review of the SolFlow frontend, analyzing:
- All displayed columns and their data sources
- Data flow from database to UI
- How each metric is computed
- Identified gaps and potential issues

**Key Finding:** There is a **critical data mismatch** in the DCA column - the frontend queries for `net_flow_sol` in `details_json`, but the backend signal generation does not include this field.

---

## Frontend Architecture Overview

### Technology Stack
- **Framework:** Next.js 14 (App Router)
- **UI Library:** React with Tailwind CSS
- **Charts:** Recharts (for sparklines)
- **Database:** SQLite (better-sqlite3) - Read-only connection
- **Refresh Rate:** 5 seconds (auto-refresh)

### Data Flow Pipeline

```
SQLite Database (/var/lib/solflow/solflow.db)
    ↓
Next.js API Routes (/api/tokens, /api/sparkline/[mint], /api/tokens/[mint]/signal)
    ↓
Query Functions (frontend/lib/queries.ts)
    ↓
React Components (TokenDashboard.tsx, NetFlowSparkline.tsx)
    ↓
Browser UI (Table with 13 columns)
```

---

## Column-by-Column Analysis

### 1. Mint Column

**Display:** Truncated mint address (first 4 + last 4 chars) with Solscan link and copy button

**Data Source:**
- Table: `token_aggregates.mint` (PRIMARY KEY)
- Query: `frontend/lib/queries.ts::getTokens()` → `SELECT ta.mint FROM token_aggregates ta`

**Derivation:**
- Direct database column (no computation)
- Formatting: `frontend/app/components/TokenDashboard.tsx::formatMint()` → `${mint.slice(0, 4)}...${mint.slice(-4)}`

**Status:** ✅ Working correctly

---

### 2. Net Flow 1m (netFlow60s)

**Display:** Colored number (green if positive, red if negative) in SOL

**Data Source:**
- Table: `token_aggregates.net_flow_60s_sol` (REAL)
- Query: `SUM(ta.net_flow_60s_sol) as net_flow_60s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:**
  1. Filter trades in `trades_60s` Vec (trades within last 60 seconds)
  2. For each trade:
     - BUY: `net_flow += trade.sol_amount`
     - SELL: `net_flow -= trade.sol_amount`
  3. Result: `net_flow_60s_sol = sum(buy_volume) - sum(sell_volume)`

**Derivation:**
- Direct aggregation from rolling window trades
- Written to DB by: `src/pipeline/db.rs::write_aggregates()` (UPSERT)

**Status:** ✅ Working correctly

---

### 3. Net Flow 5m (netFlow300s)

**Display:** Colored number (green if positive, red if negative) in SOL  
**Default Sort:** DESC (primary sort column)

**Data Source:**
- Table: `token_aggregates.net_flow_300s_sol` (REAL)
- Query: `SUM(ta.net_flow_300s_sol) as net_flow_300s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:** Same as 1m, but uses `trades_300s` Vec (300-second window)

**Status:** ✅ Working correctly

---

### 4. Net Flow 15m (netFlow900s)

**Display:** Colored number (green if positive, red if negative) in SOL

**Data Source:**
- Table: `token_aggregates.net_flow_900s_sol` (REAL)
- Query: `SUM(ta.net_flow_900s_sol) as net_flow_900s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:** Same as 1m, but uses `trades_900s` Vec (900-second window)

**Status:** ✅ Working correctly

---

### 5. Net Flow 1h (netFlow3600s)

**Display:** Colored number (green if positive, red if negative) in SOL

**Data Source:**
- Table: `token_aggregates.net_flow_3600s_sol` (REAL)
- Query: `SUM(ta.net_flow_3600s_sol) as net_flow_3600s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:** Same as 1m, but uses `trades_3600s` Vec (3600-second window)

**Status:** ✅ Working correctly

---

### 6. Net Flow 2h (netFlow7200s)

**Display:** Colored number (green if positive, red if negative) in SOL

**Data Source:**
- Table: `token_aggregates.net_flow_7200s_sol` (REAL)
- Query: `SUM(ta.net_flow_7200s_sol) as net_flow_7200s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:** Same as 1m, but uses `trades_7200s` Vec (7200-second window)

**Status:** ✅ Working correctly

---

### 7. Net Flow 4h (netFlow14400s)

**Display:** Colored number (green if positive, red if negative) in SOL

**Data Source:**
- Table: `token_aggregates.net_flow_14400s_sol` (REAL)
- Query: `SUM(ta.net_flow_14400s_sol) as net_flow_14400s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:** Same as 1m, but uses `trades_14400s` Vec (14400-second window)

**Status:** ✅ Working correctly

---

### 8. DCA Activity (1h) Column ⚠️ **CRITICAL ISSUE**

**Display:** `{dca_buys_300s} buys, {dca_net_flow_300s} SOL` or "—" if no activity

**Data Source:**
- **Primary:** `token_signals` table (CTE in query)
- **Query Logic:**
```sql
WITH dca AS (
  SELECT 
    mint,
    COUNT(*) AS dca_count,
    MAX(created_at) AS last_dca_ts,
    SUM(CAST(json_extract(details_json, '$.net_flow_sol') AS REAL)) AS dca_net_flow
  FROM token_signals
  WHERE signal_type = 'DCA_CONVICTION'
    AND created_at > unixepoch() - 3600
  GROUP BY mint
)
```

**Backend Signal Generation:**
- **Location:** `src/pipeline/state.rs::detect_dca_conviction_signals()`
- **Signal Details JSON Structure:**
```json
{
  "overlap_ratio": 0.30,
  "dca_buys": 5,
  "spot_buys": 20,
  "matched_dca": 3
}
```

**⚠️ CRITICAL GAP IDENTIFIED:**

The frontend query attempts to extract `net_flow_sol` from `details_json`:
```sql
SUM(CAST(json_extract(details_json, '$.net_flow_sol') AS REAL)) AS dca_net_flow
```

However, the backend signal generation (`src/pipeline/state.rs:484-487`) does **NOT** include `net_flow_sol` in the JSON. The actual fields are:
- `overlap_ratio` (float)
- `dca_buys` (count)
- `spot_buys` (count)
- `matched_dca` (count)

**Impact:**
- `dca_net_flow_300s` will always be `NULL` or `0` because the field doesn't exist
- The DCA column will show "X buys, 0 SOL" instead of actual net flow
- Users cannot see DCA volume contribution

**Root Cause:**
- Frontend query was written assuming `net_flow_sol` exists in signal details
- Backend signal generation doesn't compute or store net flow for DCA signals
- Mismatch between frontend expectations and backend implementation

**Status:** ❌ **BROKEN** - Data mismatch between frontend query and backend signal structure

---

### 9. Sparkline Column

**Display:** Mini line chart (100px × 20px) showing net flow trend over time

**Data Source:**
- **Table:** `token_signals` (historical signal events)
- **API Route:** `/api/sparkline/[mint]` → `frontend/app/api/sparkline/[mint]/route.ts`
- **Query Function:** `frontend/lib/queries.ts::getSparklineData()`

**Query Logic:**
```sql
SELECT 
  created_at as timestamp,
  CAST(json_extract(details_json, '$.net_flow_sol') AS REAL) as net_flow_sol,
  CAST(json_extract(details_json, '$.net_flow_300s') AS REAL) as net_flow_300s
FROM token_signals
WHERE mint = ?
  AND created_at > unixepoch() - 3600
  AND (
    json_extract(details_json, '$.net_flow_sol') IS NOT NULL
    OR json_extract(details_json, '$.net_flow_300s') IS NOT NULL
  )
ORDER BY created_at DESC
LIMIT 30
```

**Backend Signal Details:**
- **Signal Types:** BREAKOUT, SURGE, FOCUSED, BOT_DROPOFF, DCA_CONVICTION
- **Details JSON Structure:** Varies by signal type
  - Most signals: May or may not include `net_flow_sol` or `net_flow_300s`
  - DCA_CONVICTION: Does NOT include net flow fields (see issue above)

**⚠️ POTENTIAL GAP:**

The sparkline relies on `details_json` containing `net_flow_sol` or `net_flow_300s`, but:
1. Not all signal types include these fields
2. DCA_CONVICTION signals don't include net flow
3. If no signals exist with net flow data, sparkline shows "—"

**Alternative Data Source (Not Currently Used):**
- Could query `token_aggregates.updated_at` changes over time
- However, `token_aggregates` is UPSERT (PRIMARY KEY on mint), so historical values are overwritten
- No time-series history available from aggregates table

**Status:** ⚠️ **PARTIALLY WORKING** - Depends on signal types that include net flow in details_json

---

### 10. Signal Column

**Display:** Badge showing latest signal type (BREAKOUT, SURGE, FOCUSED, BOT_DROPOFF, DCA_CONVICTION) or "—"

**Data Source:**
- **Table:** `token_signals`
- **API Route:** `/api/tokens/[mint]/signal` → `frontend/app/api/tokens/[mint]/signal/route.ts`
- **Query Function:** `frontend/lib/queries.ts::getLatestSignal()`

**Query Logic:**
```sql
SELECT signal_type, created_at
FROM token_signals
WHERE mint = ?
ORDER BY created_at DESC
LIMIT 1
```

**Backend Signal Generation:**
- **Location:** `src/pipeline/state.rs::detect_signals()`
- **Signal Types:**
  - `BREAKOUT`: Large buying pressure (`net_flow_300s > 50 SOL` AND `buy_count_300s > 10`)
  - `SURGE`: Rapid accumulation (`buy_count_60s > 5` AND `net_flow_60s > 10 SOL`)
  - `FOCUSED`: Concentrated buying (`unique_wallets_300s < 5` AND `volume_300s > 100 SOL`)
  - `BOT_DROPOFF`: Bot activity ceased (previous bot count > 5 AND current ≤ 2)
  - `DCA_CONVICTION`: DCA overlap ≥ 25% (see DCA column analysis)

**Derivation:**
- Direct query of latest signal per mint
- No computation in frontend

**Status:** ✅ Working correctly

---

### 11. Wallets Column

**Display:** Number of unique wallets or "—"

**Data Source:**
- Table: `token_aggregates.unique_wallets_300s` (INTEGER)
- Query: `MAX(ta.unique_wallets_300s) as max_unique_wallets`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::TokenRollingState`
- **Algorithm:**
  1. Maintain `HashSet<String>` of unique wallet addresses in 300s window
  2. For each trade in `trades_300s`:
     - Extract `user_account` from trade
     - Insert into HashSet
  3. Result: `unique_wallets_300s = HashSet.len()`

**Derivation:**
- Count of unique wallet addresses that traded in 5-minute window
- Written to DB by: `src/pipeline/db.rs::write_aggregates()`

**Status:** ✅ Working correctly

---

### 12. Volume Column

**Display:** Total volume in SOL (formatted number)

**Data Source:**
- Table: `token_aggregates.volume_300s_sol` (REAL)
- Query: `SUM(ta.volume_300s_sol) as total_volume_300s`

**Backend Computation:**
- **Location:** `src/pipeline/state.rs::compute_rolling_metrics()`
- **Algorithm:**
  - `volume_300s_sol = sum(buy_volume) + sum(sell_volume)` (total volume, not net)

**Derivation:**
- Sum of all trade volumes (buys + sells) in 5-minute window
- Written to DB by: `src/pipeline/db.rs::write_aggregates()`

**Status:** ✅ Working correctly

---

### 13. Block Column

**Display:** Button to block/unblock token

**Data Source:**
- **Table:** `mint_blocklist`
- **Component:** `frontend/app/components/BlockButton.tsx`
- **API:** Write operations via `frontend/lib/queries.ts::blockToken()` / `unblockToken()`

**Query Logic:**
```sql
-- Block
INSERT OR REPLACE INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at)
VALUES (?, ?, 'web-ui', ?, NULL)

-- Unblock
DELETE FROM mint_blocklist WHERE mint = ?
```

**Derivation:**
- Direct database write operations
- No computation

**Status:** ✅ Working correctly

---

## Data Flow Deep Dive

### Main Query: `getTokens()`

**File:** `frontend/lib/queries.ts:4-83`

**Query Structure:**
1. **CTE (DCA):** Aggregates DCA_CONVICTION signals from last hour
2. **Main SELECT:** Joins `token_aggregates` with DCA CTE
3. **Filtering:**
   - `updated_at > unixepoch() - 60` (only recently updated tokens)
   - Excludes blocked mints (`mint_blocklist`)
4. **Aggregation:** `GROUP BY ta.mint` (handles multiple source_program rows per mint)
5. **Sorting:** `ORDER BY SUM(ta.net_flow_300s_sol) DESC`
6. **Limit:** 100 tokens

**Key Observations:**
- Uses `SUM()` for net flow columns (handles multiple `source_program` rows)
- Uses `MAX()` for `unique_wallets_300s` (takes highest value across programs)
- DCA data comes from separate `token_signals` table (not `token_aggregates`)

---

### Sparkline Query: `getSparklineData()`

**File:** `frontend/lib/queries.ts:85-123`

**Query Structure:**
1. Selects from `token_signals` (not `token_aggregates`)
2. Extracts `net_flow_sol` or `net_flow_300s` from `details_json`
3. Filters: Last hour only (`created_at > unixepoch() - 3600`)
4. Orders: `DESC` (newest first)
5. Limits: 30 data points
6. Reverses: In JavaScript to get chronological order

**Key Observations:**
- Relies on signal `details_json` containing net flow data
- Not all signal types include this data
- Falls back to `net_flow_300s` if `net_flow_sol` is NULL

---

### Signal Query: `getLatestSignal()`

**File:** `frontend/lib/queries.ts:154-176`

**Query Structure:**
1. Simple SELECT from `token_signals`
2. Filters by mint
3. Orders by `created_at DESC`
4. Limits to 1 row

**Key Observations:**
- Fetched separately for each token (N+1 query pattern)
- Called in `useEffect` hook in `TokenDashboard.tsx`
- Could be optimized with a single JOIN query

---

## Identified Gaps and Issues

### 1. ❌ CRITICAL: DCA Net Flow Missing

**Issue:** Frontend queries for `net_flow_sol` in DCA signal `details_json`, but backend doesn't include it.

**Impact:**
- DCA column always shows "X buys, 0 SOL"
- Users cannot see DCA volume contribution
- Misleading data presentation

**Root Cause:**
- Backend signal generation (`src/pipeline/state.rs:484-487`) only stores:
  - `overlap_ratio`
  - `dca_buys` (count)
  - `spot_buys` (count)
  - `matched_dca` (count)
- Frontend query (`frontend/lib/queries.ts:16`) expects `net_flow_sol`

**Fix Options:**
1. **Option A:** Add `net_flow_sol` to DCA signal details_json in backend
2. **Option B:** Query DCA net flow from `token_aggregates` filtered by `source_program = 'JupiterDCA'`
3. **Option C:** Compute DCA net flow in frontend query by joining with `token_aggregates` where `source_program = 'JupiterDCA'`

**Recommendation:** Option B or C (query from aggregates table) - more reliable than storing in signal JSON

---

### 2. ⚠️ Sparkline Data Availability

**Issue:** Sparkline relies on `token_signals.details_json` containing net flow, but:
- Not all signal types include net flow
- DCA_CONVICTION signals don't include net flow
- If no signals exist, sparkline shows "—"

**Impact:**
- Sparklines may be empty for tokens with no signals
- Historical trend data unavailable for tokens without signal events

**Root Cause:**
- `token_aggregates` is UPSERT (no historical values)
- Sparkline needs time-series data, but aggregates table only stores current state

**Fix Options:**
1. **Option A:** Ensure all signal types include `net_flow_sol` in `details_json`
2. **Option B:** Create a separate `token_metrics_history` table for time-series data
3. **Option C:** Query `token_aggregates.updated_at` changes (limited - only shows when aggregates updated)

**Recommendation:** Option B (dedicated history table) - most reliable for sparklines

---

### 3. ⚠️ N+1 Query Pattern for Signals

**Issue:** Signal data is fetched separately for each token (N queries for N tokens).

**Impact:**
- Performance degradation with many tokens
- Increased database load

**Current Implementation:**
```typescript
// TokenDashboard.tsx:74-101
useEffect(() => {
  const signalPromises = tokens.map(async (token) => {
    const response = await fetch(`/api/tokens/${token.mint}/signal`);
    // ...
  });
}, [tokens]);
```

**Fix Options:**
1. **Option A:** Modify `getTokens()` to LEFT JOIN with latest signal per mint
2. **Option B:** Create `/api/tokens/signals` endpoint that returns all signals in one query
3. **Option C:** Keep current approach but add caching

**Recommendation:** Option A (single JOIN query) - most efficient

---

### 4. ⚠️ Multiple Source Programs Aggregation

**Issue:** Query uses `SUM()` for net flow columns, which may double-count if same mint appears in multiple `source_program` rows.

**Current Query:**
```sql
SUM(ta.net_flow_60s_sol) as net_flow_60s
-- ...
GROUP BY ta.mint
```

**Impact:**
- If same mint has rows for both "PumpSwap" and "BonkSwap", net flow is summed
- May be intentional (aggregate across all DEXs) or bug (should be MAX/AVG)

**Question:** Is this intentional aggregation across DEXs, or should we show per-DEX breakdown?

**Recommendation:** Clarify requirement - if aggregating across DEXs is desired, current approach is correct. If not, use `MAX()` or separate columns per DEX.

---

### 5. ⚠️ Time Window Mismatch

**Issue:** DCA column header says "DCA Activity (1h)" but query filters signals from last hour (`created_at > unixepoch() - 3600`), while DCA data is aggregated from 300s window signals.

**Impact:**
- Label may be misleading
- DCA count may include signals older than 1 hour if signal was created recently but represents older data

**Recommendation:** Clarify label or adjust query window to match label

---

### 6. ℹ️ Missing Columns (Not Currently Displayed)

**Available in Database but Not Shown:**
- `buy_count_60s`, `sell_count_60s` (trade counts for 1m window)
- `buy_count_300s`, `sell_count_300s` (trade counts for 5m window)
- `buy_count_900s`, `sell_count_900s` (trade counts for 15m window)
- `bot_trades_300s`, `bot_wallets_300s` (bot detection metrics)
- `avg_trade_size_300s_sol` (average trade size)
- `price_usd`, `price_sol`, `market_cap_usd` (price data - may be NULL)

**Recommendation:** Consider adding buy/sell counts and bot metrics to UI for better insights

---

## Data Source Summary

### Tables Used

| Table | Read/Write | Purpose | Columns Used |
|-------|-----------|---------|--------------|
| `token_aggregates` | Read | Main metrics | `mint`, `net_flow_*_sol`, `unique_wallets_300s`, `volume_300s_sol`, `updated_at` |
| `token_signals` | Read | Signal events | `mint`, `signal_type`, `created_at`, `details_json` |
| `mint_blocklist` | Read/Write | Blocked tokens | `mint`, `expires_at` |

### Backend Components Writing Data

| Component | Writes To | Frequency |
|-----------|-----------|-----------|
| `src/pipeline/db.rs::write_aggregates()` | `token_aggregates` | Every trade batch (UPSERT) |
| `src/pipeline/db.rs::write_signal()` | `token_signals` | When signal detected (INSERT) |
| `frontend/lib/queries.ts::blockToken()` | `mint_blocklist` | User action (INSERT/UPDATE) |

---

## Recommendations

### High Priority

1. **Fix DCA Net Flow:** Add `net_flow_sol` to DCA signal details OR query from `token_aggregates` filtered by `source_program = 'JupiterDCA'`
2. **Optimize Signal Queries:** Use JOIN instead of N+1 pattern
3. **Clarify Multi-DEX Aggregation:** Confirm if `SUM()` across `source_program` is intentional

### Medium Priority

4. **Improve Sparkline Data:** Ensure all signal types include net flow OR create history table
5. **Add Missing Metrics:** Display buy/sell counts and bot metrics
6. **Fix Time Window Labels:** Ensure labels match query windows

### Low Priority

7. **Add Price Data:** Display price/market cap if available
8. **Add Filtering:** Allow filtering by signal type, DEX, etc.
9. **Add Export:** CSV/JSON export functionality

---

## Conclusion

The frontend architecture is **mostly functional** with one **critical data mismatch** (DCA net flow) and several **optimization opportunities**. The core data flow is sound, but the DCA column issue should be addressed immediately as it provides misleading information to users.

**Overall Status:**
- ✅ 11/13 columns working correctly
- ❌ 1/13 columns broken (DCA net flow)
- ⚠️ 1/13 columns partially working (Sparkline - depends on signal types)

