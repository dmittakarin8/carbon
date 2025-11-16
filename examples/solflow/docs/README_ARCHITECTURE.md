# SolFlow Architecture Documentation

**Last Updated:** 2025-11-16

## 📚 Documentation Index

This directory contains comprehensive architectural documentation for the SolFlow project.

### For Frontend Developers

**Start Here:**
- **[FRONTEND_QUICK_START.md](FRONTEND_QUICK_START.md)** - Quick reference guide (5-minute read)
  - Essential queries
  - Schema cheat sheet
  - Common pitfalls
  - Example code (JS/Python)

**Complete Reference:**
- **[FRONTEND_ARCHITECTURE.md](FRONTEND_ARCHITECTURE.md)** - Full architectural guide (1,769 lines)
  - Component survey and classification
  - Complete pipeline architecture
  - Full SQLite schema documentation
  - Streamers → Pipeline → Tables mapping
  - Frontend integration patterns
  - Legacy vs new component analysis

### Key Takeaways

**✅ Current Architecture (Use This):**
```
Streamers → PipelineEngine (in-memory) → SQLite → Your Frontend
```

**Primary Tables:**
- `token_aggregates` - Rolling-window metrics (your main data source)
- `token_signals` - Real-time trading alerts

**❌ Legacy/Separate Components (Avoid):**
- `trades` table - Raw events (not aggregated)
- `aggregator` binary - Standalone tool (not part of primary runtime)
- JSONL files - Backup only

### Quick Decision Matrix

| Question | Answer |
|----------|--------|
| Which tables should I query? | `token_aggregates` + `token_signals` |
| Should I use the `trades` table? | ❌ No (raw data, poor performance) |
| Should I read JSONL files? | ❌ No (legacy backup) |
| What is the `aggregator` binary? | Separate analysis tool (not primary runtime) |
| How often should I poll? | Every 5 seconds |
| Where is the database? | `/var/lib/solflow/solflow.db` |

### Architecture Diagram

```
┌─────────────────────────────────────────┐
│  Solana Blockchain (Yellowstone gRPC)   │
└────────────┬────────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  4 Streamer Binaries                    │
│  • PumpSwap, BonkSwap                   │
│  • Moonshot, Jupiter DCA                │
└────────────┬───────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  PipelineEngine (in-memory)             │
│  • Rolling windows (60s/300s/900s)      │
│  • Signal detection (BREAKOUT, SURGE)   │
└────────────┬───────────────────────────┘
             │ (flush every 5 seconds)
             ▼
┌────────────────────────────────────────┐
│  SQLite Database                        │
│  • token_aggregates (metrics)           │
│  • token_signals (alerts)               │
└────────────┬───────────────────────────┘
             │
             ▼
┌────────────────────────────────────────┐
│  YOUR FRONTEND                          │
└─────────────────────────────────────────┘
```

### File Organization

```
docs/
├── README_ARCHITECTURE.md          ← You are here
├── FRONTEND_QUICK_START.md         ← Start here (quick reference)
├── FRONTEND_ARCHITECTURE.md        ← Complete guide (comprehensive)
│
├── 20251114T18-*.md                ← Historical reviews
├── 20251113T*.md                   ← Design documents
└── ...                             ← Additional timestamped docs
```

### Backend Team Resources

**For backend developers:**
- `../AGENTS.md` - Agent coding guidelines (comprehensive)
- `../ARCHITECTURE.md` - Original architecture notes
- `../sql/readme.md` - Schema documentation and rules

**Design Documents:**
- All timestamped docs in this directory (`YYYYMMDDTHH-*.md`)
- Phase implementation reviews
- Migration guides

### Running the System

**Start Backend:**
```bash
cd /path/to/solflow
ENABLE_PIPELINE=true cargo run --release --bin pipeline_runtime
```

**Verify Data Flow:**
```bash
sqlite3 /var/lib/solflow/solflow.db "SELECT COUNT(*) FROM token_aggregates;"
```

**Check Health:**
```bash
sqlite3 /var/lib/solflow/solflow.db "SELECT MAX(unixepoch() - updated_at) FROM token_aggregates;"
# Should be < 10 seconds if running properly
```

### Common Issues

**Problem:** No data in database
- **Check:** Is `ENABLE_PIPELINE=true` set?
- **Check:** Is `pipeline_runtime` binary running?
- **Check:** Logs for errors (`RUST_LOG=info`)

**Problem:** Data is stale
- **Check:** Backend may have crashed (check logs)
- **Check:** gRPC connection issue (check `GEYSER_URL`)

**Problem:** Queries are slow
- **Check:** Are you querying `trades` table? (use `token_aggregates` instead)

### Migration Notes

**From Old Architecture:**
If you previously built against the `aggregator` binary or `trades` table:

1. **Switch to `token_aggregates`** - Pre-aggregated metrics (much faster)
2. **Use `token_signals`** - Structured alerts (not raw trades)
3. **Remove JSONL parsing** - SQLite only (simpler, faster)

**Schema Changes:**
- Old: Query raw `trades` → aggregate in UI
- New: Query `token_aggregates` → already aggregated

### Known Limitations

⚠️ **Not Yet Implemented:**
- Price enrichment (`price_usd`, `price_sol` columns exist but empty)
- Token metadata (`token_metadata` table not populated)
- DCA correlation in primary pipeline (requires separate `aggregator` binary)
- Historical data beyond 15-minute rolling windows

⚠️ **Frontend Must Handle:**
- Fetching token prices from external APIs
- Fetching token names/symbols
- Aggregating metrics across multiple DEXes (if needed)

### Contributing

**Before making changes:**
1. Review `AGENTS.md` (coding conventions)
2. Check if your feature affects frontend (update these docs if so)
3. Test with both `pipeline_runtime` and standalone tools

**Documentation Updates:**
- Keep `FRONTEND_ARCHITECTURE.md` in sync with schema changes
- Update `FRONTEND_QUICK_START.md` if query patterns change
- Add timestamped design docs for major features

### Questions?

**For Frontend:**
- Read `FRONTEND_QUICK_START.md` first
- Refer to `FRONTEND_ARCHITECTURE.md` for details
- Contact backend team for integration issues

**For Backend:**
- See `AGENTS.md` for coding rules
- See timestamped docs for design history
- See `sql/readme.md` for schema rules

---

**Quick Links:**
- [Quick Start](FRONTEND_QUICK_START.md) - 5-minute read
- [Full Architecture](FRONTEND_ARCHITECTURE.md) - Complete guide
- [Agent Rules](../AGENTS.md) - Backend coding guidelines
- [SQL Schema](../sql/readme.md) - Schema rules

**Version:** 1.0 (2025-11-16)
