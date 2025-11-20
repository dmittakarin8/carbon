# Ngrok Proxy Analysis - UI-Only Configuration

**Date:** 2025-11-19  
**Status:** Analysis Complete - Technical Constraints Identified

## Problem Statement

When friends access the SolFlow dashboard via ngrok URL (`https://xyz.ngrok.io`), the browser makes API calls that traverse the ngrok tunnel, consuming bandwidth unnecessarily.

## Current Architecture

```
User Browser (via ngrok)
   ↓ https://xyz.ngrok.io/api/tokens
Ngrok Tunnel
   ↓
Next.js Server (localhost:3000)
   ↓ (already local!)
SQLite Database (filesystem)
```

## Key Insight: API Routes Are Already Server-Side!

**CRITICAL REALIZATION:** Next.js API routes in `/app/api/**` are **server-side handlers**, not client-side code. They already execute locally on the Next.js server and access the local SQLite database directly.

### What Actually Happens

1. **Browser Request:** `GET https://xyz.ngrok.io/api/tokens`
2. **Ngrok Tunnel:** HTTP request/response (small JSON payload)
3. **Next.js Handler:** Executes `getTokens()` function **locally**
4. **Database Query:** Reads from local SQLite file (`/var/lib/solflow/solflow.db`)
5. **Response:** JSON data sent back through tunnel

### Bandwidth Analysis

- **Through Ngrok:** HTTP headers + JSON response (~10-50 KB per request)
- **NOT Through Ngrok:** Database I/O (stays on server filesystem)
- **Reality:** Only HTTP protocol overhead goes through ngrok, not database traffic

## Attempted Solutions

### Attempt 1: Server-Side Rewrites (FAILED)

```typescript
// This creates a proxy loop!
async rewrites() {
  return [
    {
      source: '/api/:path*',
      destination: 'http://localhost:3000/api/:path*',
    },
  ];
}
```

**Why it fails:** Next.js tries to proxy to itself, causing `ECONNRESET` errors.

### Attempt 2: Client-Side API Base URL (REQUIRES CODE CHANGES)

```typescript
// This would work but violates "don't modify fetch code" constraint
const API_BASE_URL = window.location.hostname.includes('ngrok')
  ? 'http://localhost:3000'
  : '';

fetch(`${API_BASE_URL}/api/tokens`);
```

**Why not used:** Requires modifying all fetch calls in components.

## Technical Constraints

Given the requirements:
1. ✅ Can modify `next.config.js`
2. ❌ Cannot modify fetch code
3. ❌ Cannot modify query files  
4. ❌ Cannot modify API handlers

**Conclusion:** It is **technically impossible** to change browser fetch behavior without modifying application code.

## Browser Same-Origin Policy

When a page is loaded from `https://xyz.ngrok.io`, the browser enforces Same-Origin Policy:

- Relative fetch(`/api/tokens`) → Resolves to `https://xyz.ngrok.io/api/tokens`
- This is **hardcoded browser behavior** and cannot be overridden server-side
- Only the client (browser JavaScript) can decide where to send requests

## Alternative: Accept Current Behavior

### Why Current Setup is Actually Fine

1. **Database access is already local** - This is the expensive operation
2. **HTTP overhead is minimal** - JSON payloads are small (10-50 KB)
3. **Ngrok free tier** - 40 MB/month might be sufficient for text-only responses
4. **No modification needed** - System works as designed

### Bandwidth Calculation

- 50 tokens × 1 KB each = 50 KB per dashboard load
- Metadata requests: ~20 requests × 2 KB = 40 KB
- Sparklines: ~20 requests × 1 KB = 20 KB
- **Total per page load:** ~110 KB
- **Free tier limit:** 40 MB/month = ~364 page loads
- **With friends:** 10 users × 10 loads/day × 30 days = 3,000 loads (needs paid tier)

## Recommended Solutions (If Bandwidth IS a Problem)

### Option A: Modify Fetch Calls (Recommended)

Create `/lib/api-client.ts`:

```typescript
const API_BASE = typeof window !== 'undefined' && window.location.hostname.includes('ngrok')
  ? 'http://localhost:3000'
  : '';

export const apiFetch = (path: string, options?: RequestInit) => 
  fetch(`${API_BASE}${path}`, options);
```

Then replace all `fetch('/api/...')` with `apiFetch('/api/...')` in components.

**Impact:** ~15 files, ~25 lines changed

### Option B: Use Environment Variable

Add to `.env.local`:
```bash
NEXT_PUBLIC_API_URL=http://localhost:3000
```

Update fetch calls:
```typescript
fetch(`${process.env.NEXT_PUBLIC_API_URL}/api/tokens`)
```

**Impact:** ~15 files, ~25 lines changed

### Option C: Service Worker Proxy

Install service worker that intercepts `/api/*` requests and redirects to localhost.

**Impact:** New file + registration code, more complex

### Option D: Upgrade Ngrok Plan

Ngrok paid plans have higher bandwidth limits:
- **Personal:** $8/month, 1 GB/month
- **Production:** $20/month, 100 GB/month

**Impact:** $0 code changes, $8-20/month cost

## Conclusion

**Technical Reality:** Without modifying application code, you cannot change where browser fetch requests go. Server-side Next.js config cannot override browser Same-Origin Policy.

**Recommendation:**

1. **If bandwidth is NOT a concern:** Keep current setup, it's already optimal for database access
2. **If bandwidth IS a concern:** Implement Option A (api-client wrapper) with minimal code changes
3. **If zero code changes required:** Upgrade ngrok plan or use Cloudflare Tunnel (unlimited bandwidth)

## Final Answer to Original Request

**The feature as originally described cannot be implemented** due to browser security policies and the "don't modify fetch code" constraint.

**What CAN be done:**
- Accept current behavior (database already local)
- Modify fetch calls to use localhost (breaks constraint)
- Upgrade ngrok plan (costs money)
- Switch to Cloudflare Tunnel (free, unlimited bandwidth)

