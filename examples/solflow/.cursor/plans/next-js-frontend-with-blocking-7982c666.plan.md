<!-- 7982c666-87a9-46ee-bf83-89fa40a8f72a 73e4a68d-c739-4bdd-b896-830a431c6a10 -->
# Next.js Dashboard - Single Page with Available Metrics

## Architecture Overview

**Stack:**

- **Frontend:** Next.js 14+ (App Router) with TypeScript
- **API:** Next.js API Routes (direct SQLite access)
- **Database:** SQLite at `/var/lib/solflow/solflow.db`
- **SQLite Library:** `better-sqlite3` (synchronous, fast for local use)
- **Charts:** Recharts (for sparklines)
- **UI:** Tailwind CSS + shadcn/ui components

**Location:** `/home/dgem8/projects/carbon/examples/solflow/frontend/`

**Branch:** Create feature branch `feature/nextjs-frontend` before starting

## Database Schema Reference

**Primary Data Source: `token_aggregates` table**

**Available Windows (from primary pipeline):**

- `net_flow_60s_sol` - 1-minute net flow
- `net_flow_300s_sol` - 5-minute net flow (primary metric)
- `net_flow_900s_sol` - 15-minute net flow (closest to requested 15m)

**Available Metrics:**

- `buy_count_60s`, `sell_count_60s` - 1-minute counts
- `buy_count_300s`, `sell_count_300s` - 5-minute counts
- `buy_count_900s`, `sell_count_900s` - 15-minute counts
- `unique_wallets_300s` - Unique traders (5 minutes)
- `volume_300s_sol` - Total volume (5 minutes)
- `source_program` - "PumpSwap", "BonkSwap", "Moonshot", "JupiterDCA"
- `updated_at` - Last update timestamp (should be <10s old)

**DCA Information:**

- DCA trades appear in `token_aggregates` with `source_program = 'JupiterDCA'`
- Can show DCA activity: `buy_count_300s`, `net_flow_300s_sol` where `source_program = 'JupiterDCA'`
- **Note:** DCA overlap % (correlation with PumpSwap) is NOT available in primary pipeline
- Requires separate `aggregator` binary to compute overlap (outputs to JSONL files)

**Signals: `token_signals` table**

- `signal_type`: "BREAKOUT", "SURGE", "FOCUSED", "BOT_DROPOFF"
- `severity`: 1-5 (currently all signals are severity=1)
- `created_at`: Timestamp

**Blocklist: `mint_blocklist` table**

- Filter blocked tokens: `WHERE mint NOT IN (SELECT mint FROM mint_blocklist WHERE expires_at IS NULL OR expires_at > unixepoch())`

## Window Mapping Strategy

**User Requested:** 15m, 1h, 2h, 4h windows

**Available:** 60s, 300s, 900s windows

**Mapping:**

- **15m** → Use `net_flow_900s_sol` (900s = 15 minutes) ✅ Exact match
- **1h** → Approximate from `net_flow_900s_sol` trend OR aggregate multiple 900s windows (if historical data available)
- **2h** → Approximate from trend OR aggregate
- **4h** → Approximate from trend OR aggregate

**Alternative:** If user needs exact 1h/2h/4h windows, must run separate `aggregator` binary which outputs to JSONL files (`streams/aggregates/*.jsonl`). Frontend would need to read JSONL files instead of SQLite.

**Recommendation:** Start with available windows (60s/300s/900s) and show 900s as "15m". For 1h/2h/4h, either:

1. Show "N/A - requires aggregator binary" message
2. Read from JSONL files if aggregator is running
3. Approximate from available data

## Iteration 1: Single Dashboard + Blocking

### 1. Project Setup

**Structure:**

```
frontend/
├── package.json
├── next.config.js
├── tsconfig.json
├── tailwind.config.js
├── .env.local (DB_PATH=/var/lib/solflow/solflow.db)
├── app/
│   ├── layout.tsx
│   ├── page.tsx (single dashboard page)
│   ├── api/
│   │   ├── tokens/route.ts (GET tokens with metrics)
│   │   ├── sparkline/[mint]/route.ts (GET historical net flow for sparkline)
│   │   ├── tokens/[mint]/block/route.ts (POST block token)
│   │   └── tokens/[mint]/unblock/route.ts (POST unblock token)
│   └── components/
│       ├── TokenDashboard.tsx (main table component)
│       ├── NetFlowSparkline.tsx (sparkline chart component)
│       └── BlockButton.tsx (inline block/unblock button)
├── lib/
│   ├── db.ts (SQLite connection singleton)
│   ├── queries.ts (SQL query functions)
│   └── types.ts (TypeScript types)
└── public/
```

### 2. API Routes

**GET `/api/tokens`** - List all tokens with metrics

**Query Strategy:**

- Query `token_aggregates` table (NOT `trades` table)
- Aggregate across `source_program` if token trades on multiple DEXes
- Filter blocked tokens
- Get latest metrics (WHERE `updated_at > unixepoch() - 60`)

**SQL Query:**

```sql
SELECT 
    mint,
    SUM(net_flow_60s_sol) as net_flow_60s,
    SUM(net_flow_300s_sol) as net_flow_300s,
    SUM(net_flow_900s_sol) as net_flow_900s,
    SUM(buy_count_300s) as total_buys_300s,
    SUM(sell_count_300s) as total_sells_300s,
    SUM(CASE WHEN source_program = 'JupiterDCA' THEN buy_count_300s ELSE 0 END) as dca_buys_300s,
    SUM(CASE WHEN source_program = 'JupiterDCA' THEN net_flow_300s_sol ELSE 0 END) as dca_net_flow_300s,
    MAX(unique_wallets_300s) as max_unique_wallets,
    SUM(volume_300s_sol) as total_volume_300s,
    MAX(updated_at) as last_update
FROM token_aggregates
WHERE updated_at > unixepoch() - 60
    AND mint NOT IN (
        SELECT mint FROM mint_blocklist 
        WHERE expires_at IS NULL OR expires_at > unixepoch()
    )
GROUP BY mint
ORDER BY net_flow_300s DESC
LIMIT 100;
```

**Return Format:**

```typescript
{
  tokens: Array<{
    mint: string;
    netFlow60s: number;      // 1-minute net flow
    netFlow300s: number;     // 5-minute net flow (primary sort)
    netFlow900s: number;     // 15-minute net flow
    totalBuys300s: number;
    totalSells300s: number;
    dcaBuys300s: number;     // DCA buy count (JupiterDCA only)
    dcaNetFlow300s: number; // DCA net flow (JupiterDCA only)
    maxUniqueWallets: number;
    totalVolume300s: number;
    lastUpdate: number;
  }>
}
```

**GET `/api/sparkline/[mint]`** - Historical net flow data for sparkline

**Strategy:** Query `token_signals` table for historical events (append-only log)

- Get signals for this mint ordered by `created_at`
- Extract net flow from `details_json` if available
- OR query `token_aggregates` and track `updated_at` changes (limited history)

**Alternative:** If aggregator binary is running, read from JSONL files:

- `streams/aggregates/1h.jsonl` (if available)
- Parse JSONL and filter by mint

**Return Format:**

```typescript
{
  dataPoints: Array<{
    timestamp: number;
    netFlowSol: number;
  }>
}
```

**POST `/api/tokens/[mint]/block`** - Block a token

- Insert: `INSERT OR REPLACE INTO mint_blocklist (mint, reason, blocked_by, created_at, expires_at) VALUES (?, ?, 'web-ui', ?, ?)`
- Return: `{ success: true }`

**POST `/api/tokens/[mint]/unblock`** - Unblock a token

- Delete: `DELETE FROM mint_blocklist WHERE mint = ?`
- Return: `{ success: true }`

### 3. Frontend Components

**Single Dashboard (`app/page.tsx`):**

- Main token table with all metrics
- Auto-refresh every 5-10 seconds (matches pipeline flush interval)
- Sortable columns for each timeframe
- Inline block button on each row
- Focus on signals and volume narrative

**Table Columns:**

1. **Mint** (truncated, copyable, link to Solscan)
2. **Net Flow 1m** (`netFlow60s`, sortable, colored: green positive, red negative)
3. **Net Flow 5m** (`netFlow300s`, sortable, colored, default sort column)
4. **Net Flow 15m** (`netFlow900s`, sortable, colored)
5. **DCA Activity** (show `dcaBuys300s` count and `dcaNetFlow300s` SOL - note: not overlap %)
6. **Sparkline** (mini chart showing net flow trend over time)
7. **Signal** (badge from `token_signals` - show most recent signal for this mint)
8. **Wallets** (`maxUniqueWallets` - unique trader count)
9. **Volume** (`totalVolume300s` - total volume in SOL)
10. **Block** (button to block/unblock)

**Components:**

- `TokenDashboard.tsx` - Main table component with sortable columns, row rendering, block buttons
- `NetFlowSparkline.tsx` - Recharts sparkline component (small line chart, ~100px wide, 20px tall)
- `BlockButton.tsx` - Inline button with confirmation modal, calls block/unblock API

### 4. Key Implementation Details

**Window Display:**

- Show available windows: "1m", "5m", "15m" (map to 60s, 300s, 900s)
- For requested "1h/2h/4h": Show "N/A" or read from aggregator JSONL if available

**DCA Display:**

- Show DCA activity: "DCA: X buys, Y SOL" (from JupiterDCA source_program)
- Note: Cannot show DCA overlap % (requires aggregator binary correlation)

**Sparkline Data:**

- Query `token_signals` for this mint (last 20-30 signals)
- Extract net flow from `details_json` if available
- OR track `token_aggregates.updated_at` changes (limited to recent updates)
- Fallback: Show placeholder if no historical data

**Sorting:**

- Default: Sort by `netFlow300s` DESC (5-minute net flow)
- User can click column headers to sort by any timeframe
- Maintain sort state in URL query params or local state

## Technical Considerations

### SQLite WAL Mode

- Database uses WAL (Write-Ahead Logging) mode
- `better-sqlite3` handles WAL automatically
- Reads see consistent snapshot even during writes
- No locking issues for read-only queries

### Performance

- Index on `token_aggregates(updated_at DESC)` for fast freshness checks
- Index on `token_aggregates(mint, updated_at DESC)` for token queries
- Index on `mint_blocklist(mint)` for fast blocklist checks
- Use prepared statements for all queries
- Limit result sets (100 tokens max per page)

### Error Handling

- Database connection errors → 500 response
- Missing token → 404 response
- Blocklist write failures → 500 response with error message
- Missing aggregator data → Show "N/A" gracefully

### Security (Local Use)

- No authentication needed (local only)
- Input validation on mint addresses (base58 check)
- SQL injection prevention (parameterized queries)

## File Changes Summary

**New Files:**

- `frontend/` directory (entire Next.js app)
- `frontend/lib/db.ts` (SQLite connection singleton)
- `frontend/lib/queries.ts` (SQL query functions)
- `frontend/lib/types.ts` (TypeScript types)
- `frontend/app/api/tokens/route.ts` (GET tokens with metrics)
- `frontend/app/api/sparkline/[mint]/route.ts` (GET historical data for sparkline)
- `frontend/app/api/tokens/[mint]/block/route.ts` (POST block)
- `frontend/app/api/tokens/[mint]/unblock/route.ts` (POST unblock)
- `frontend/app/page.tsx` (single dashboard page)
- `frontend/app/components/TokenDashboard.tsx` (main table)
- `frontend/app/components/NetFlowSparkline.tsx` (sparkline chart)
- `frontend/app/components/BlockButton.tsx` (block button)

**No Changes to:**

- Rust codebase (streamers, aggregator, pipeline_runtime)
- SQL schema files (read-only reference)
- Existing binaries

## Dependencies

**package.json:**

```json
{
  "dependencies": {
    "next": "^14.0.0",
    "react": "^18.0.0",
    "react-dom": "^18.0.0",
    "better-sqlite3": "^9.0.0",
    "recharts": "^2.10.0",
    "tailwindcss": "^3.4.0",
    "@radix-ui/react-dialog": "^1.0.0",
    "@radix-ui/react-toast": "^1.0.0"
  },
  "devDependencies": {
    "@types/better-sqlite3": "^7.6.0",
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0"
  }
}
```

## Testing Strategy

**Manual Testing:**

1. Create feature branch: `git checkout -b feature/nextjs-frontend`
2. Start Next.js dev server: `cd frontend && npm run dev`
3. Verify dashboard loads token table with columns (1m, 5m, 15m net flow)
4. Verify DCA activity displays (buy count and net flow from JupiterDCA)
5. Verify sparklines render for each token (or show placeholder)
6. Click column header → verify sorting works for each timeframe
7. Click "Block" button → verify token disappears from list
8. Check database → verify `mint_blocklist` row inserted
9. Verify auto-refresh updates data every 5-10 seconds
10. Verify blocklist filter excludes blocked tokens

**Database Verification:**

- Query token_aggregates: `SELECT mint, net_flow_300s_sol, source_program FROM token_aggregates WHERE updated_at > unixepoch() - 60 LIMIT 10`
- Verify DCA data: `SELECT mint, buy_count_300s, net_flow_300s_sol FROM token_aggregates WHERE source_program = 'JupiterDCA' LIMIT 10`
- Verify blocklist filtering works correctly

## Known Limitations

**Missing Features (Require Backend Changes):**

1. **1h/2h/4h Windows:** Not available in primary pipeline. Options:

   - Run separate `aggregator` binary and read from JSONL files
   - Approximate from available 900s data
   - Show "N/A" message

2. **DCA Overlap %:** Not available in primary pipeline. Options:

   - Run separate `aggregator` binary and read from JSONL files
   - Show DCA activity counts/volume only (not correlation %)

3. **Historical Sparklines:** Limited by `token_signals` table. Options:

   - Query `token_signals.details_json` for historical net flow
   - Track `token_aggregates.updated_at` changes (limited history)
   - Read from aggregator JSONL files if available

## Next Steps After Iteration 1

1. **Iteration 1.5:** Add aggregator JSONL reader (if aggregator binary is running)
2. **Iteration 2:** Metadata enrichment (on-demand button for token details)
3. **Iteration 3:** Advanced filtering (signal type, volume thresholds)
4. **Iteration 4:** Export functionality (CSV, JSON)