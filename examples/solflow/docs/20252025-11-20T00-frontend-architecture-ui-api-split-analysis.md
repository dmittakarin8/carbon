# SolFlow Frontend Architecture Review
## Complete Analysis for UI/API Port Splitting

**Document Version:** 1.0  
**Generated:** 2025-11-20  
**Author:** Droid (Factory AI Agent)  
**Review Status:** Ready for Implementation

---

## 1. EXECUTIVE SUMMARY

**System Type:** Next.js 16 App Router with Server-Side SQLite Integration  
**Architecture:** Client-side React UI + Server-side API Routes + Shared SQLite Database  
**Current Port:** 3000 (unified UI + API)  
**Proposed Split:** Port 3000 (UI only) + Port 3001 (API only)  
**Deployment Target:** ngrok tunnel for UI-only (reducing bandwidth)

**Key Finding:** The architecture is **SAFE TO SPLIT** with proper preparation, but requires careful handling of database access patterns and API client routing logic.

---

## 2. FILESYSTEM STRUCTURE

```
frontend/
├── app/
│   ├── layout.tsx                 # Root layout (server component, static)
│   ├── page.tsx                   # Homepage (client component, fetches /api/tokens)
│   ├── globals.css                # Tailwind styles
│   ├── api/                       # ALL API routes (Next.js Route Handlers)
│   │   ├── tokens/
│   │   │   ├── route.ts          # GET /api/tokens (main data endpoint)
│   │   │   └── [mint]/
│   │   │       ├── signal/route.ts      # GET /api/tokens/[mint]/signal
│   │   │       ├── block/route.ts       # POST /api/tokens/[mint]/block
│   │   │       └── unblock/route.ts     # POST /api/tokens/[mint]/unblock
│   │   ├── sparkline/
│   │   │   └── [mint]/route.ts   # GET /api/sparkline/[mint]
│   │   ├── dca-sparkline/
│   │   │   └── [mint]/route.ts   # GET /api/dca-sparkline/[mint]
│   │   └── metadata/
│   │       ├── get/route.ts      # GET /api/metadata/get?mint=...
│   │       ├── update/route.ts   # POST /api/metadata/update (DexScreener fetch)
│   │       ├── counts/route.ts   # GET /api/metadata/counts
│   │       ├── followed/route.ts # GET /api/metadata/followed
│   │       ├── blocked/route.ts  # GET /api/metadata/blocked
│   │       ├── follow/route.ts   # POST /api/metadata/follow
│   │       ├── block/route.ts    # POST /api/metadata/block
│   │       └── unblock/route.ts  # POST /api/metadata/unblock
│   └── components/
│       ├── TokenDashboard.tsx    # Main table (client component)
│       ├── NetFlowSparkline.tsx  # Net flow chart (client component)
│       ├── DcaSparkline.tsx      # DCA activity chart (client component)
│       ├── BlockedTokensModal.tsx # Modal (client component)
│       ├── FollowedTokensModal.tsx # Modal (client component)
│       └── BlockButton.tsx       # Block button (client component)
├── lib/
│   ├── api-client.ts             # **CRITICAL** - API routing logic
│   ├── queries.ts                # Database query layer (server-side only)
│   ├── db.ts                     # SQLite connection manager (server-side only)
│   └── types.ts                  # Shared TypeScript types
├── next.config.ts                # Next.js configuration (EMPTY - no rewrites yet)
├── package.json                  # Dependencies (better-sqlite3, recharts, radix-ui)
├── .env.local                    # Environment variables (DB_PATH)
└── tsconfig.json                 # TypeScript configuration
```

### Component Classification

| File | Type | Runtime | Database Access | API Calls |
|------|------|---------|----------------|-----------|
| `app/layout.tsx` | Server Component | Server | ❌ | ❌ |
| `app/page.tsx` | Client Component | Browser | ❌ | ✅ `/api/tokens`, `/api/metadata/counts` |
| `app/api/**/*.ts` | Route Handler | Server | ✅ (via lib/queries) | ⚠️ External: DexScreener |
| `lib/queries.ts` | Query Module | Server | ✅ Direct SQLite | ❌ |
| `lib/db.ts` | DB Module | Server | ✅ Direct SQLite | ❌ |
| `lib/api-client.ts` | API Router | Browser | ❌ | ✅ Wraps fetch() |
| `components/**/*.tsx` | Client Components | Browser | ❌ | ✅ Various API routes |

---

## 3. DATA FLOW ARCHITECTURE

### 3.1 Complete Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                   BACKEND: Rust Pipeline Runtime                     │
│  (Separate process - writes to SQLite via pipeline_runtime.rs)      │
│                                                                       │
│  Yellowstone gRPC → Streamers (4) → PipelineEngine → SQLite DB      │
│    (PumpSwap, BonkSwap, Moonshot, JupiterDCA)                       │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ Writes to:
                                │ /var/lib/solflow/solflow.db
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        SQLite Database (Shared)                      │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Tables:                                                       │   │
│  │  • token_aggregates (net flows, DCA counts, wallets)        │   │
│  │  • token_signals (BREAKOUT, FOCUSED, SURGE)                 │   │
│  │  • token_metadata (name, symbol, image, price, market cap)  │   │
│  │  • dca_activity_buckets (time-series DCA buys)              │   │
│  │  • mint_blocklist (blocked tokens)                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ Read by:
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│              FRONTEND: Next.js App (Port 3000 currently)            │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ Server-Side API Routes (app/api/**/route.ts)                 │  │
│  │  • GET /api/tokens                                            │  │
│  │  • GET /api/sparkline/[mint]                                  │  │
│  │  • GET /api/dca-sparkline/[mint]                              │  │
│  │  • GET /api/metadata/get?mint=...                             │  │
│  │  • POST /api/metadata/update (external DexScreener API)       │  │
│  │  • GET /api/metadata/counts                                   │  │
│  │  • GET /api/metadata/followed                                 │  │
│  │  • GET /api/metadata/blocked                                  │  │
│  │  • POST /api/metadata/follow                                  │  │
│  │  • POST /api/metadata/block                                   │  │
│  │  • POST /api/metadata/unblock                                 │  │
│  │  • POST /api/tokens/[mint]/block                              │  │
│  │  • POST /api/tokens/[mint]/unblock                            │  │
│  │  • GET /api/tokens/[mint]/signal                              │  │
│  │  │                                                              │  │
│  │  └─► All routes call lib/queries.ts → lib/db.ts → SQLite     │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                 │                                    │
│                                 │ JSON responses                     │
│                                 ▼                                    │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │ Client-Side Components (Browser)                              │  │
│  │  • app/page.tsx (Homepage - auto-refreshes every 5s)         │  │
│  │  • TokenDashboard.tsx (Main table with sorting/actions)      │  │
│  │  • NetFlowSparkline.tsx (Historical net flow chart)          │  │
│  │  • DcaSparkline.tsx (DCA activity bar chart)                 │  │
│  │  • BlockedTokensModal.tsx (Manage blocked tokens)            │  │
│  │  • FollowedTokensModal.tsx (Manage followed tokens)          │  │
│  │  │                                                              │  │
│  │  └─► All use lib/api-client.ts apiFetch() wrapper            │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘

External API Integrations:
- DexScreener API (api.dexscreener.com) - Triggered by POST /api/metadata/update
  Used for fetching token name, symbol, image, price, market cap
```

### 3.2 API Call Patterns

#### 3.2.1 Initial Page Load (app/page.tsx)

```typescript
useEffect(() => {
  fetchTokens();      // → GET /api/tokens
  refreshCounts();    // → GET /api/metadata/counts
  
  // Auto-refresh every 5 seconds
  const interval = setInterval(fetchTokens, 5000);
}, []);
```

#### 3.2.2 Token Dashboard (components/TokenDashboard.tsx)

```typescript
useEffect(() => {
  // Fetch signals for ALL tokens (parallel)
  tokens.forEach(token => {
    fetch(`/api/tokens/${token.mint}/signal`)  // → GET (per token)
  });
  
  // Fetch metadata for ALL tokens (parallel)
  tokens.forEach(token => {
    fetch(`/api/metadata/get?mint=${token.mint}`)  // → GET (per token)
  });
}, [tokens]);
```

**⚠️ Performance Note:** Dashboard makes **N+M API calls** where N = tokens, M = tokens needing metadata. For 40 tokens, this is ~80 parallel requests on every 5-second refresh cycle.

#### 3.2.3 DCA Sparkline (components/DcaSparkline.tsx)

```typescript
useEffect(() => {
  fetch(`/api/dca-sparkline/${mint}`)  // → GET (per sparkline)
  
  // Refresh every 60 seconds
  const interval = setInterval(fetchData, 60000);
}, [mint]);
```

#### 3.2.4 User Actions (Interactive)

```typescript
// Block token
handleBlockFixed(mint) → POST /api/metadata/block {mint}

// Unblock token
handleUnblock(mint) → POST /api/metadata/unblock {mint}

// Follow/Unfollow token
handleFollowPrice(mint, value) → POST /api/metadata/follow {mint, value}

// Refresh metadata from DexScreener
handleGetMetadata(mint) → POST /api/metadata/update {mint}
  └─► Internally calls: https://api.dexscreener.com/token-pairs/v1/solana/${mint}
```

### 3.3 Caching Strategy

**Current State:** ❌ **NO CACHING LAYER**

- No SWR, react-query, or similar caching library
- Every 5-second refresh fetches full dataset from SQLite
- Sparklines re-fetch every 60 seconds
- Metadata fetches are per-token, per-render (no deduplication)

**Implications for Port Split:**
- API server will receive HIGH request volume (especially with multiple users)
- No client-side cache to reduce redundant API calls
- Consider adding caching layer (SWR/react-query) BEFORE splitting ports

---

## 4. INTERNAL API ARCHITECTURE

### 4.1 Complete API Route Inventory

| Route | Method | Handler File | Purpose | DB Access | External APIs |
|-------|--------|-------------|---------|-----------|---------------|
| `/api/tokens` | GET | `app/api/tokens/route.ts` | Get top 40 tokens (sorted by net flow) | ✅ `token_aggregates` (JOIN `token_metadata`) | ❌ |
| `/api/sparkline/[mint]` | GET | `app/api/sparkline/[mint]/route.ts` | Get historical net flow for sparkline | ✅ `token_signals` (extract from `details_json`) | ❌ |
| `/api/dca-sparkline/[mint]` | GET | `app/api/dca-sparkline/[mint]/route.ts` | Get 60-minute DCA activity buckets | ✅ `dca_activity_buckets` (time-series) | ❌ |
| `/api/tokens/[mint]/signal` | GET | `app/api/tokens/[mint]/signal/route.ts` | Get latest signal for token | ✅ `token_signals` | ❌ |
| `/api/tokens/[mint]/block` | POST | `app/api/tokens/[mint]/block/route.ts` | Block token (add to blocklist) | ✅ `mint_blocklist` (INSERT) | ❌ |
| `/api/tokens/[mint]/unblock` | POST | `app/api/tokens/[mint]/unblock/route.ts` | Unblock token | ✅ `mint_blocklist` (DELETE) | ❌ |
| `/api/metadata/get` | GET | `app/api/metadata/get/route.ts` | Get token metadata | ✅ `token_metadata` (SELECT) | ❌ |
| `/api/metadata/update` | POST | `app/api/metadata/update/route.ts` | Fetch metadata from DexScreener | ✅ `token_metadata` (UPSERT) | ✅ DexScreener API |
| `/api/metadata/counts` | GET | `app/api/metadata/counts/route.ts` | Get counts of followed/blocked tokens | ✅ `token_metadata` (COUNT) | ❌ |
| `/api/metadata/followed` | GET | `app/api/metadata/followed/route.ts` | Get followed tokens list | ✅ `token_aggregates` JOIN `token_metadata` | ❌ |
| `/api/metadata/blocked` | GET | `app/api/metadata/blocked/route.ts` | Get blocked tokens list | ✅ `token_aggregates` JOIN `token_metadata` | ❌ |
| `/api/metadata/follow` | POST | `app/api/metadata/follow/route.ts` | Follow/unfollow token | ✅ `token_metadata` (UPSERT) | ❌ |
| `/api/metadata/block` | POST | `app/api/metadata/block/route.ts` | Block token (via metadata table) | ✅ `token_metadata` (UPDATE) | ❌ |
| `/api/metadata/unblock` | POST | `app/api/metadata/unblock/route.ts` | Unblock token (via metadata table) | ✅ `token_metadata` (UPDATE) | ❌ |

### 4.2 Database Access Patterns

**All API routes use this pattern:**

```typescript
// app/api/tokens/route.ts
import { getTokens } from '@/lib/queries';

export async function GET() {
  const tokens = getTokens(100);  // ← Calls lib/queries.ts
  return NextResponse.json({ tokens });
}
```

**Query layer (lib/queries.ts):**

```typescript
import { getDb, getWriteDb } from './db';

export function getTokens(limit: number = 100): TokenMetrics[] {
  const db = getDb();  // ← Opens SQLite connection (read-only)
  const stmt = db.prepare(`SELECT ... FROM token_aggregates ...`);
  const rows = stmt.all();
  return rows.map(...);  // Transform to TypeScript objects
}
```

**Database connection manager (lib/db.ts):**

```typescript
let db: Database.Database | null = null;

export function getDb(): Database.Database {
  if (db) return db;
  
  const resolvedPath = process.env.DB_PATH || '/var/lib/solflow/solflow.db';
  db = new Database(resolvedPath, { readonly: true });
  db.pragma('journal_mode = WAL');  // ← WAL mode for concurrent reads
  return db;
}
```

**Key Observations:**

1. **Connection Pooling:** Single read-only connection cached globally (`db` singleton)
2. **Write Operations:** Use `getWriteDb()` which opens **new connection per write**
3. **WAL Mode:** Enabled for concurrent reads while Rust pipeline writes
4. **Absolute Path:** Database path from environment variable (`DB_PATH`)
5. **Server-Side Only:** `better-sqlite3` is Node.js native addon (cannot run in browser)

---

## 5. APPLICATION RUNTIME STRUCTURE

### 5.1 Server-Side Rendering (SSR) Analysis

**Root Layout (app/layout.tsx):**
- **Type:** Server Component (default in App Router)
- **Rendering:** Static HTML at build time
- **No API Calls:** Only loads fonts and applies CSS
- **Hydration:** Basic HTML structure, no dynamic data

**Homepage (app/page.tsx):**
- **Type:** Client Component (`'use client'` directive)
- **Rendering:** SSR + Hydration (Next.js default)
  1. **Server:** Renders empty shell with "Loading tokens..."
  2. **Browser:** Hydrates React, triggers `useEffect()` → fetches `/api/tokens`
- **No Server-Side Data Fetching:** Uses client-side `fetch()` instead of Next.js data fetching

**API Routes:**
- **Type:** Server-only (Next.js Route Handlers)
- **Rendering:** N/A (pure API endpoints)
- **Execution:** Node.js runtime on every request

### 5.2 Hydration Boundaries

```
┌────────────────────────────────────────────────────────────────┐
│ Server (Build Time + Request Time)                             │
│                                                                  │
│  app/layout.tsx (Server Component)                              │
│    └─► Renders: <html><body>{children}</body></html>           │
│                                                                  │
│  app/page.tsx (Client Component - SSR)                          │
│    └─► Server renders: <div>Loading tokens...</div>            │
│    └─► Serialized as HTML                                      │
│                                                                  │
│  app/api/**/route.ts (API Handlers)                             │
│    └─► Always server-side (never runs in browser)              │
└────────────────────────────────────────────────────────────────┘
                                 │
                                 │ HTTP Response: HTML + JS bundle
                                 ▼
┌────────────────────────────────────────────────────────────────┐
│ Browser (Runtime)                                               │
│                                                                  │
│  1. Parse HTML → Display "Loading tokens..."                   │
│  2. Load JS bundle → React hydration                            │
│  3. useEffect() triggers → fetch('/api/tokens')                 │
│  4. Render TokenDashboard with data                             │
│  5. Auto-refresh every 5s                                       │
└────────────────────────────────────────────────────────────────┘
```

**Critical Insight:** There are **NO server-side data fetching patterns** (no `async` components, no `getServerSideProps`). All data fetching happens **client-side via fetch()**.

### 5.3 Middleware & Edge Runtime

**Status:** ❌ **NO MIDDLEWARE CONFIGURED**

- No `middleware.ts` file exists
- No Edge Runtime usage
- No request interception layer
- All API routes run in Node.js runtime (required for `better-sqlite3`)

### 5.4 Environment Variables

**Configuration (.env.local):**

```bash
DB_PATH=/var/lib/solflow/solflow.db
```

**Usage:**
- Read by `lib/db.ts` at runtime
- Must be accessible from API server process
- Will need to be set on API server when split

---

## 6. UI/SERVER RUNTIME BOUNDARIES

### 6.1 Component Classification Matrix

| Component | Directive | Runs In | Can Access DB? | Makes API Calls? | State Management |
|-----------|-----------|---------|----------------|------------------|------------------|
| `app/layout.tsx` | None (default server) | Server | ✅ (but doesn't) | ❌ | None |
| `app/page.tsx` | `'use client'` | Browser | ❌ | ✅ | React useState/useEffect |
| `TokenDashboard.tsx` | `'use client'` | Browser | ❌ | ✅ (per-token metadata/signals) | React useState/useEffect |
| `NetFlowSparkline.tsx` | `'use client'` | Browser | ❌ | ✅ (`/api/sparkline/[mint]`) | React useState/useEffect |
| `DcaSparkline.tsx` | `'use client'` | Browser | ❌ | ✅ (`/api/dca-sparkline/[mint]`) | React useState/useEffect |
| `BlockedTokensModal.tsx` | `'use client'` | Browser | ❌ | ✅ (modal actions) | React useState/useEffect |
| `FollowedTokensModal.tsx` | `'use client'` | Browser | ❌ | ✅ (modal actions) | React useState/useEffect |
| `BlockButton.tsx` | `'use client'` | Browser | ❌ | ✅ (POST block) | React useState |
| `lib/api-client.ts` | N/A (module) | Browser | ❌ | ✅ (wraps fetch) | None |
| `lib/queries.ts` | N/A (module) | **Server ONLY** | ✅ Direct SQLite | ❌ | None |
| `lib/db.ts` | N/A (module) | **Server ONLY** | ✅ Direct SQLite | ❌ | None |
| `app/api/**/route.ts` | N/A (handlers) | **Server ONLY** | ✅ (via queries) | ⚠️ DexScreener (one route) | None |

### 6.2 Critical Dependencies

**Server-Side ONLY (Cannot move to browser):**
- `better-sqlite3` - Native Node.js addon (C++ bindings)
- `lib/db.ts` - Manages SQLite connections
- `lib/queries.ts` - All SQL query logic
- All API route handlers in `app/api/`

**Browser-Side ONLY (Cannot run server-side):**
- `recharts` - Requires browser DOM APIs
- `@radix-ui/*` - React component library (requires DOM)
- All `'use client'` components

**Shared (Safe for both):**
- `lib/types.ts` - Pure TypeScript interfaces
- `lib/api-client.ts` - Uses browser `fetch()` (only runs client-side)

### 6.3 Data Flow Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                 BROWSER RUNTIME (Port 3000 - UI)                │
│                                                                   │
│  React Components                                                │
│    └─► lib/api-client.ts (apiFetch wrapper)                     │
│          └─► fetch(API_BASE_URL + path)                         │
│                                                                   │
│                ↓ HTTP (relative or absolute paths)               │
└─────────────────────────────────────────────────────────────────┘
                                 │
                                 │ Proxy Layer (to be added)
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                SERVER RUNTIME (Port 3001 - API)                 │
│                                                                   │
│  Next.js Route Handlers (app/api/**/route.ts)                   │
│    └─► lib/queries.ts                                           │
│          └─► lib/db.ts (better-sqlite3)                         │
│                └─► /var/lib/solflow/solflow.db                  │
│                                                                   │
│  External API: POST /api/metadata/update                         │
│    └─► fetch('https://api.dexscreener.com/...')                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 7. CONSTRAINTS FOR UI/API PORT SPLITTING

### 7.1 Critical Constraint: API Client Routing Logic

**File:** `lib/api-client.ts`

```typescript
const API_BASE_URL = typeof window !== 'undefined' && window.location.hostname.includes('ngrok')
  ? 'http://localhost:3000'
  : '';

export async function apiFetch(path: string, options?: RequestInit): Promise<Response> {
  const url = `${API_BASE_URL}${path}`;
  return fetch(url, options);
}
```

**Current Logic:**
1. If running via ngrok → API calls go to `http://localhost:3000`
2. Otherwise → API calls are relative paths (e.g., `/api/tokens`)

**Problem for Port Split:**
- After split, UI runs on port 3000, API on port 3001
- Current logic still routes to `http://localhost:3000` (incorrect)
- **Fix Required:** Update logic to detect split mode

**Proposed Fix:**

```typescript
// Option 1: Environment variable (build-time)
const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || '';

// Option 2: Dynamic detection (runtime)
const API_BASE_URL = typeof window !== 'undefined' && window.location.hostname.includes('ngrok')
  ? 'http://localhost:3001'  // ← Change to port 3001
  : '';
```

### 7.2 Critical Constraint: Database Access Path

**Current:** API routes expect database at `/var/lib/solflow/solflow.db`

**Requirements for Split:**
1. API server (port 3001) MUST have read access to SQLite file
2. Filesystem path must be mounted/accessible from API server process
3. Environment variable `DB_PATH` must be set correctly

**Options:**
- **Local Development:** Both processes run on same machine (shared filesystem)
- **Docker/Containers:** Mount volume to both UI and API containers
- **Remote API Server:** Consider moving to client-server database (PostgreSQL)

### 7.3 Component Dependencies on API Routes

**Current:** All components use relative paths (e.g., `fetch('/api/tokens')`)

**Impact of Split:**
- If API moves to port 3001, components cannot use relative paths
- Must use `lib/api-client.ts` wrapper (already partially implemented)

**Current Coverage:**

| Component | Uses apiFetch? | Uses Raw fetch? | Risk Level |
|-----------|----------------|-----------------|------------|
| `app/page.tsx` | ❌ | ✅ `/api/tokens`, `/api/metadata/counts` | 🔴 HIGH |
| `TokenDashboard.tsx` | ❌ | ✅ All metadata/signal/action routes | 🔴 HIGH |
| `NetFlowSparkline.tsx` | ❌ | ✅ `/api/sparkline/[mint]` | 🔴 HIGH |
| `DcaSparkline.tsx` | ❌ | ✅ `/api/dca-sparkline/[mint]` | 🔴 HIGH |
| `BlockedTokensModal.tsx` | ❌ | ✅ `/api/metadata/blocked` | 🔴 HIGH |
| `FollowedTokensModal.tsx` | ❌ | ✅ `/api/metadata/followed` | 🔴 HIGH |
| `BlockButton.tsx` | ❌ | ✅ `/api/tokens/[mint]/block` | 🔴 HIGH |

**⚠️ CRITICAL:** `lib/api-client.ts` exists but is **NOT USED** by any component! All components use raw `fetch()` with relative paths.

**Fix Required:** Refactor all components to use `apiFetch()` wrapper.

### 7.4 Server Component Implications

**Current State:**
- `app/layout.tsx` is server component (default)
- Does NOT fetch data server-side
- Only renders static HTML shell

**Impact of Split:**
- If layout remains server component, it will render on UI server (port 3000)
- No data fetching means no impact from API split
- **Safe to keep as-is**

**If Future Changes Add Server-Side Data Fetching:**
- Server components on UI server CANNOT directly import `lib/queries.ts` (database access)
- Must use `fetch()` to call API server (port 3001)
- Example:
  ```typescript
  // WRONG (after split):
  import { getTokens } from '@/lib/queries';  // ❌ SQLite only on API server
  
  // CORRECT:
  const res = await fetch('http://localhost:3001/api/tokens');  // ✅ HTTP call
  const data = await res.json();
  ```

### 7.5 Build-Time Constraints

**Next.js Build Process:**
- Runs `next build` to generate production bundle
- Server components may execute during build (static generation)
- **Current:** No static generation of data (all client-side fetching)

**After Split:**
- UI server build: Should complete successfully (no DB dependencies in UI code)
- API server build: May not need separate build (can use same Next.js app but different port)

**Deployment Strategy:**
- **Option 1:** Single Next.js app, two processes (UI port 3000, API port 3001)
- **Option 2:** Separate UI and API builds (UI = static export, API = server)

---

## 8. PROPOSED SPLIT ARCHITECTURE

### 8.1 Port Configuration

```
┌───────────────────────────────────────────────────────────────────┐
│                     SPLIT ARCHITECTURE                            │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │ UI Server (Port 3000)                                     │    │
│  │  • Next.js App Router                                     │    │
│  │  • React components (client-side rendering)               │    │
│  │  • Static assets (CSS, JS bundles)                        │    │
│  │  • API routes REMOVED or proxied                          │    │
│  │  • Environment: NEXT_PUBLIC_API_URL=http://localhost:3001 │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                 │                                  │
│                                 │ Rewrite rules:                   │
│                                 │ /api/* → http://localhost:3001   │
│                                 ▼                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │ API Server (Port 3001)                                    │    │
│  │  • Next.js API routes ONLY (app/api/**/route.ts)         │    │
│  │  • lib/queries.ts + lib/db.ts                            │    │
│  │  • SQLite database access (/var/lib/solflow/solflow.db) │    │
│  │  • Environment: DB_PATH=/var/lib/solflow/solflow.db      │    │
│  └──────────────────────────────────────────────────────────┘    │
└───────────────────────────────────────────────────────────────────┘
```

### 8.2 Rewrite Configuration (next.config.ts)

**UI Server (Port 3000):**

```typescript
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'http://localhost:3001/api/:path*'
      }
    ];
  }
};

export default nextConfig;
```

**API Server (Port 3001):**

```typescript
import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // No rewrites needed (API server handles all /api/* routes natively)
};

export default nextConfig;
```

### 8.3 Component Refactoring Plan

**Phase 1: Centralize API Calls (CRITICAL)**

Update all components to use `lib/api-client.ts`:

```typescript
// BEFORE:
const response = await fetch('/api/tokens');

// AFTER:
import { apiFetch } from '@/lib/api-client';
const response = await apiFetch('/api/tokens');
```

**Files to Update (7 components):**
1. `app/page.tsx` - 2 fetch calls
2. `components/TokenDashboard.tsx` - 6 fetch calls
3. `components/NetFlowSparkline.tsx` - 1 fetch call
4. `components/DcaSparkline.tsx` - 1 fetch call
5. `components/BlockedTokensModal.tsx` - 2 fetch calls
6. `components/FollowedTokensModal.tsx` - 2 fetch calls
7. `components/BlockButton.tsx` - 1 fetch call

**Phase 2: Update API Client Logic**

```typescript
// lib/api-client.ts (UPDATED)
const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_URL ||
  (typeof window !== 'undefined' && window.location.hostname.includes('ngrok')
    ? 'http://localhost:3001'  // ← Changed from 3000 to 3001
    : '');

export async function apiFetch(path: string, options?: RequestInit): Promise<Response> {
  const url = `${API_BASE_URL}${path}`;
  console.log(`[API Client] Fetching: ${url}`);  // Debug logging
  return fetch(url, options);
}
```

**Phase 3: Test Proxy Routing**

```bash
# Terminal 1: Start API server
cd frontend
PORT=3001 npm start

# Terminal 2: Start UI server with rewrite
cd frontend
PORT=3000 npm start

# Terminal 3: Test API routes
curl http://localhost:3000/api/tokens
curl http://localhost:3001/api/tokens  # Should return same data
```

### 8.4 Deployment Checklist

**Pre-Split Validation:**
- [ ] All components use `apiFetch()` wrapper
- [ ] `lib/api-client.ts` points to port 3001
- [ ] `next.config.ts` has rewrite rules on UI server
- [ ] Environment variables configured (`DB_PATH` on API server)
- [ ] SQLite database accessible from API server filesystem
- [ ] DexScreener API calls still work (external API in `/api/metadata/update`)

**Post-Split Testing:**
- [ ] UI server loads at `http://localhost:3000`
- [ ] API requests proxy to port 3001 successfully
- [ ] Token dashboard displays data correctly
- [ ] Sparklines render (NetFlow + DCA)
- [ ] Block/unblock actions work
- [ ] Follow/unfollow actions work
- [ ] Metadata refresh from DexScreener works
- [ ] Auto-refresh every 5 seconds continues working
- [ ] Modals display blocked/followed tokens
- [ ] No CORS errors in browser console
- [ ] No 404 errors for API routes

---

## 9. RISKS & MITIGATION STRATEGIES

### 9.1 High-Risk Areas

| Risk | Severity | Impact | Mitigation |
|------|----------|--------|------------|
| **Components use raw fetch()** | 🔴 CRITICAL | API calls fail after split | Refactor to use `apiFetch()` wrapper |
| **Database path inaccessible** | 🔴 CRITICAL | API server cannot read SQLite | Mount shared volume or migrate to PostgreSQL |
| **CORS issues** | 🟡 MEDIUM | Browser blocks cross-origin requests | Add CORS headers to API server responses |
| **Rewrite rules misconfigured** | 🟡 MEDIUM | 404 errors on API routes | Test all 14 API routes after split |
| **Environment variables missing** | 🟡 MEDIUM | API server crashes or uses wrong DB | Document required env vars in deployment guide |
| **High API request volume** | 🟡 MEDIUM | API server overload (80 req/5s) | Add caching layer (SWR/react-query) |
| **External API rate limits** | 🟢 LOW | DexScreener blocks requests | Already rate-limited (only on manual refresh) |

### 9.2 Testing Strategy

**Unit Tests (Before Split):**
```bash
# Test API client wrapper
npm test lib/api-client.test.ts  # (TO BE CREATED)

# Test API routes with mock DB
npm test app/api/**/*.test.ts  # (TO BE CREATED)
```

**Integration Tests (After Split):**
```bash
# Test UI → API communication
curl http://localhost:3000/api/tokens
# Should return: {"tokens": [...]}

# Test direct API access
curl http://localhost:3001/api/tokens
# Should return: {"tokens": [...]}

# Test proxy routing
curl -H "Host: localhost:3000" http://localhost:3001/api/tokens
# Should fail (no rewrite on API server)
```

**End-to-End Tests (Post-Deployment):**
1. Open `http://localhost:3000` in browser
2. Open DevTools → Network tab
3. Verify all API requests show status 200
4. Check request URLs point to port 3001 (via proxy)
5. Verify sparklines render after data loads
6. Test block/unblock actions
7. Test follow/unfollow actions
8. Verify auto-refresh continues every 5 seconds

### 9.3 Rollback Plan

**If Split Fails:**

1. **Immediate Rollback:**
   ```bash
   # Stop split deployment
   killall node
   
   # Revert next.config.ts (remove rewrites)
   git checkout next.config.ts
   
   # Restart unified server on port 3000
   PORT=3000 npm start
   ```

2. **Revert Code Changes:**
   ```bash
   # Revert API client changes
   git checkout lib/api-client.ts
   
   # Optionally revert component changes (if using apiFetch breaks)
   git checkout app/page.tsx components/
   ```

3. **Verify Rollback:**
   - UI loads at `http://localhost:3000`
   - All API routes work (relative paths)
   - No errors in browser console

---

## 10. RECOMMENDED IMPLEMENTATION PLAN

### Phase 1: Pre-Split Preparation (2-3 hours)

**Step 1.1: Create API Client Wrapper Tests**
- File: `lib/__tests__/api-client.test.ts`
- Verify routing logic for ngrok vs localhost

**Step 1.2: Refactor All Components**
- Replace all `fetch()` calls with `apiFetch()`
- Files: 7 components (see Section 8.3)
- Test manually: UI still works on port 3000

**Step 1.3: Update API Client Logic**
- Change `http://localhost:3000` → `http://localhost:3001`
- Add `NEXT_PUBLIC_API_URL` environment variable support

**Step 1.4: Add Rewrite Rules**
- Update `next.config.ts` with API proxy rewrites

### Phase 2: Split Execution (1-2 hours)

**Step 2.1: Start API Server (Port 3001)**
```bash
cd frontend
DB_PATH=/var/lib/solflow/solflow.db PORT=3001 npm start
```

**Step 2.2: Start UI Server (Port 3000)**
```bash
cd frontend
NEXT_PUBLIC_API_URL=http://localhost:3001 PORT=3000 npm start
```

**Step 2.3: Verify Proxy Routing**
```bash
# Test from UI server
curl http://localhost:3000/api/tokens

# Test from API server directly
curl http://localhost:3001/api/tokens

# Should return identical JSON
```

### Phase 3: Testing & Validation (1-2 hours)

**Step 3.1: Browser Testing**
- Open `http://localhost:3000` in Chrome
- Check DevTools → Network tab for 200 responses
- Verify sparklines render correctly
- Test all interactive actions (block/unblock/follow)

**Step 3.2: Load Testing**
```bash
# Simulate 5-second refresh cycle
for i in {1..100}; do
  curl -s http://localhost:3000/api/tokens > /dev/null
  sleep 5
done

# Monitor API server logs for errors
```

**Step 3.3: Edge Case Testing**
- Test with empty database
- Test with 100+ tokens
- Test with slow DexScreener API responses
- Test with database locked (write in progress)

### Phase 4: Production Deployment (1 hour)

**Step 4.1: Deploy API Server**
```bash
# Systemd service (API server)
[Unit]
Description=SolFlow API Server
After=network.target

[Service]
Type=simple
User=solflow
WorkingDirectory=/opt/solflow/frontend
Environment="DB_PATH=/var/lib/solflow/solflow.db"
Environment="PORT=3001"
ExecStart=/usr/bin/npm start
Restart=always

[Install]
WantedBy=multi-user.target
```

**Step 4.2: Deploy UI Server**
```bash
# Systemd service (UI server)
[Unit]
Description=SolFlow UI Server
After=network.target

[Service]
Type=simple
User=solflow
WorkingDirectory=/opt/solflow/frontend
Environment="NEXT_PUBLIC_API_URL=http://localhost:3001"
Environment="PORT=3000"
ExecStart=/usr/bin/npm start
Restart=always

[Install]
WantedBy=multi-user.target
```

**Step 4.3: Configure ngrok**
```bash
# ngrok tunnel points to UI server ONLY
ngrok http 3000
```

---

## 11. CONCLUSION

### 11.1 Architectural Assessment

**Summary:** The SolFlow frontend is **SAFE TO SPLIT** with proper preparation. The architecture is well-structured with clear separation between UI components and API logic, but requires refactoring to use centralized API client before splitting.

**Key Strengths:**
- ✅ Clear separation: UI components in `app/components`, API in `app/api`
- ✅ Database access isolated to `lib/queries.ts` (server-side only)
- ✅ No complex SSR patterns (all data fetching is client-side)
- ✅ WAL mode enabled on SQLite (concurrent reads safe)

**Key Weaknesses:**
- ❌ All components use raw `fetch()` (not centralized API client)
- ❌ No caching layer (high API request volume)
- ⚠️ Single database connection model (may need connection pooling)

### 11.2 Effort Estimate

| Phase | Task | Estimated Time | Risk |
|-------|------|---------------|------|
| **Phase 1** | Refactor components to use apiFetch() | 2-3 hours | LOW |
| **Phase 2** | Configure and test split locally | 1-2 hours | MEDIUM |
| **Phase 3** | End-to-end testing and validation | 1-2 hours | MEDIUM |
| **Phase 4** | Production deployment and monitoring | 1 hour | HIGH |
| **Total** | - | **5-8 hours** | **MEDIUM** |

### 11.3 Success Criteria

**Split is considered successful when:**
- [ ] UI server runs on port 3000 (via ngrok tunnel)
- [ ] API server runs on port 3001 (localhost only)
- [ ] All 14 API routes return correct data
- [ ] UI displays tokens, sparklines, and metadata correctly
- [ ] Block/unblock/follow actions work without errors
- [ ] Auto-refresh continues every 5 seconds
- [ ] No CORS errors in browser console
- [ ] No 404 errors on API routes
- [ ] Ngrok bandwidth reduced (only HTML/CSS/JS served via tunnel)

### 11.4 Future Optimizations

**After successful split, consider:**

1. **Add Caching Layer (SWR or react-query)**
   - Reduce API request volume by 80-90%
   - Implement stale-while-revalidate pattern
   - Share cache across components

2. **Connection Pooling for SQLite**
   - Replace singleton connection with pool
   - Handle high concurrency better
   - Consider migrating to PostgreSQL

3. **API Rate Limiting**
   - Add rate limiter middleware (e.g., `express-rate-limit`)
   - Prevent abuse from multiple concurrent users
   - Implement per-IP rate limits

4. **WebSocket Integration**
   - Push updates from backend to UI (no polling)
   - Reduce API request overhead
   - Real-time updates for signal detection

---

## 12. APPENDICES

### Appendix A: Environment Variables Reference

| Variable | Purpose | Default | Required On |
|----------|---------|---------|-------------|
| `DB_PATH` | SQLite database path | `/var/lib/solflow/solflow.db` | API Server |
| `NEXT_PUBLIC_API_URL` | API server URL for UI | (empty = relative paths) | UI Server (optional) |
| `PORT` | Server port | 3000 | Both servers |

### Appendix B: API Route Response Schemas

**(See Section 4.1 for complete route inventory)**

Example: `GET /api/tokens`
```json
{
  "tokens": [
    {
      "mint": "So11111111111111111111111111111111111111112",
      "netFlow60s": 12.34,
      "netFlow300s": 56.78,
      "netFlow900s": 90.12,
      "netFlow3600s": 123.45,
      "netFlow7200s": 234.56,
      "netFlow14400s": 345.67,
      "dcaBuys60s": 5,
      "dcaBuys300sWindow": 15,
      "dcaBuys900s": 30,
      "dcaBuys3600s": 100,
      "dcaBuys14400s": 250,
      "maxUniqueWallets": 42,
      "totalVolume300s": 789.01,
      "lastUpdate": 1731491200
    }
  ]
}
```

### Appendix C: Database Schema Summary

**(Referenced from `/sql` directory in backend)**

- `token_aggregates` - Rolling-window metrics (net flows, DCA counts, wallets)
- `token_signals` - Signal events (BREAKOUT, FOCUSED, SURGE, BOT_DROPOFF)
- `token_metadata` - Token name, symbol, image, price, market cap, follow/block status
- `dca_activity_buckets` - Time-series DCA buy counts (1-minute buckets)
- `mint_blocklist` - Blocked token addresses with expiration

---

**Document Version:** 1.0  
**Generated:** 2025-11-20  
**Author:** Droid (Factory AI Agent)  
**Review Status:** Ready for Implementation
