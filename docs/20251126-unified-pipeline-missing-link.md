# Unified Pipeline Missing Link Analysis

**Date:** 2025-11-26  
**Author:** Droid Analysis  
**Issue:** Matched transactions not reaching database

---

## Section 1 — Root Cause Summary

**The unified streamer (`unified_streamer.rs`) does NOT have pipeline ingestion enabled.**

The critical missing piece:

1. ✅ `unified_streamer.rs` correctly initializes `InstructionScanner`
2. ✅ `run_unified()` creates `UnifiedTradeProcessor` with the scanner
3. ❌ **`StreamerConfig` is created with `pipeline_tx: None`** (line 62)
4. ❌ **No pipeline channel exists to receive matched trades**

The unified streamer is completely isolated — it logs matches but has **no database writer or pipeline channel** to persist events.

**Contrast with `pipeline_runtime.rs`:**
- Creates mpsc channel with `mpsc::channel::<TradeEvent>(config.channel_buffer)` (line 89)
- Passes `pipeline_tx: Some(tx)` to each streamer config (lines 98, 110, 122, 134)
- Spawns `start_pipeline_ingestion()` task to consume from channel (line 168)
- All 4 legacy streamers (PumpSwap, BonkSwap, Moonshot, JupiterDCA) have working ingestion

**The unified streamer was designed to work standalone but never integrated into the pipeline architecture.**

---

## Section 2 — Call Flow Trace

### Expected Flow (Working in pipeline_runtime.rs)
```
Streamer (PumpSwap/BonkSwap/etc)
  ↓
TradeProcessor::process()
  ↓
Extract balance deltas
  ↓
Create TradeEvent
  ↓
tx.try_send(pipeline_event)  ← Sends to channel
  ↓
start_pipeline_ingestion() (pipeline_runtime.rs:168)
  ↓
PipelineEngine::process_trade()
  ↓
Periodic flush timer
  ↓
SqliteAggregateWriter::write_aggregates()
  ↓
DATABASE (solflow.db)
```

### Actual Flow (Broken in unified_streamer)
```
unified_streamer.rs:main()
  ↓
run_unified() with InstructionScanner
  ↓
UnifiedTradeProcessor::process()
  ↓
scanner.scan(&metadata) → Some(program_match) ✅
  ↓
log::info!("✅ Matched {} ...", program_match.program_name) ✅
  ↓
Extract balance deltas ✅
  ↓
extract_trade_info() → Some(trade_info) ✅
  ↓
Create TradeEvent ✅
  ↓
if let Some(tx) = &self.pipeline_tx {  ← **ALWAYS None**
  tx.try_send(...)                      ← NEVER EXECUTED
}
  ↓
if self.enable_jsonl {                  ← **false by default**
  writer.write(&event).await            ← NEVER EXECUTED
}
  ↓
return Ok(())                           ← **EVENT DISCARDED**
  ↓
END (no database write, no pipeline send)
```

**The flow stops at line 359 in `lib.rs` — events are created but never written anywhere.**

---

## Section 3 — Code Locations

### 1. Unified Streamer Entry Point
**File:** `examples/solflow/src/bin/unified_streamer.rs`

**Line 61-68:** StreamerConfig creation with **no pipeline channel**
```rust
let config = StreamerConfig {
    program_id: "11111111111111111111111111111111".to_string(),
    program_name: "Unified".to_string(),
    output_path,
    backend,
    pipeline_tx: None,  // ← ROOT CAUSE: No channel
};
```

### 2. UnifiedTradeProcessor Event Handling
**File:** `examples/solflow/src/streamer_core/lib.rs`

**Line 329:** Scanner successfully matches transaction ✅
```rust
let program_match = match self.scanner.scan(&metadata) {
    Some(m) => m,
    None => return Ok(()), // Not a tracked program
};
```

**Line 335:** Validation log appears ✅
```rust
log::info!(
    "✅ Matched {} at {:?} (signature: {})",
    program_match.program_name,
    program_match.instruction_path,
    metadata.signature
);
```

**Line 377-387:** Pipeline send attempt (SKIPPED)
```rust
if let Some(tx) = &self.pipeline_tx {  // ← ALWAYS ENTERS ELSE BRANCH
    let pipeline_event = convert_to_pipeline_event(&event);
    if tx.try_send(pipeline_event).is_ok() {
        // Count successful sends
    }
}  // ← THIS ENTIRE BLOCK NEVER EXECUTES
```

**Line 389-400:** JSONL write attempt (SKIPPED)
```rust
if self.enable_jsonl {  // ← ALWAYS FALSE (ENABLE_JSONL=true not set)
    let mut writer = self.writer.lock().await;
    writer.write(&event).await?;  // ← NEVER EXECUTES
}
```

**Line 403:** Function returns without writing event
```rust
Ok(())  // ← Event discarded
```

### 3. Working Pipeline Integration (Legacy Streamers)
**File:** `examples/solflow/src/bin/pipeline_runtime.rs`

**Line 89:** Channel creation ✅
```rust
let (tx, rx) = mpsc::channel::<TradeEvent>(config.channel_buffer);
```

**Line 98, 110, 122, 134:** Channel passed to streamers ✅
```rust
let streamer_config = StreamerConfig {
    program_id: "...",
    program_name: "...",
    output_path: "...",
    backend: BackendType::Jsonl,
    pipeline_tx: Some(tx_pump),  // ← PRESENT
};
```

**Line 168:** Ingestion task spawned ✅
```rust
tokio::spawn(async move {
    start_pipeline_ingestion(rx, engine_ingestion, db_writer_ingestion, flush_interval).await;
});
```

### 4. Database Writer Selection
**File:** `examples/solflow/src/streamer_core/lib.rs`

**Line 432-444:** Writer backend initialization in `run_unified()`
```rust
let writer: Box<dyn WriterBackend> = match streamer_config.backend {
    BackendType::Jsonl => {
        Box::new(JsonlWriter::new(...))
    }
    BackendType::Sqlite => {
        Box::new(SqliteWriter::new(&streamer_config.output_path)?)
    }
};
```

**Problem:** Even with `BackendType::Sqlite`, the writer is only used if `self.enable_jsonl == true`, which it never is in unified mode.

The SQLite writer path exists but is unreachable:
1. `backend = BackendType::Sqlite` sets the backend type ✅
2. Writer is created: `SqliteWriter::new(db_path)` ✅
3. Writer is passed to `UnifiedTradeProcessor` ✅
4. BUT: `enable_jsonl = runtime_config.enable_jsonl` is `false` by default
5. SO: Line 391 check `if self.enable_jsonl` fails, writer never used

### 5. Blocklist Filtering (NOT the issue)
**Line 346-359:** Blocklist check (event passes through)
```rust
if let Some(ref checker) = self.blocklist_checker {
    match checker.is_blocked(&trade_info.mint) {
        Ok(true) => {
            log::debug!("🚫 Blocked token: {}", trade_info.mint);
            return Ok(());  // Only returns on blocked tokens
        }
        Ok(false) => {}  // Continue processing
        Err(e) => {
            log::warn!("⚠️  Blocklist check failed: {}", e);
        }
    }
}
```

**Not the issue:** Blocklist would log "🚫 Blocked token" if filtering. User sees "✅ Matched" logs, meaning events pass blocklist check.

---

## Section 4 — Validation Steps

### Test 1: Verify `pipeline_tx` is None
**Command:**
```bash
RUST_LOG=debug cargo run --release --bin unified_streamer
```

**Expected output:**
```
✅ Matched PumpSwap at Outer(0) (signature: ...)
✅ Matched BonkSwap at Inner(1, 0) (signature: ...)
...
```

**Validation:**
- Search logs for "📊 Pipeline ingestion active:" → **NOT FOUND** (proves channel is unused)
- Search logs for "✅ JSONL:" → **NOT FOUND** (proves JSONL writes disabled)
- Check DB file size: `stat /var/lib/solflow/solflow.db` → **NO GROWTH**

### Test 2: Verify JSONL path is also disabled
**Command:**
```bash
ENABLE_JSONL=true cargo run --release --bin unified_streamer
```

**Expected output:**
```
📝 JSONL writes: ENABLED
✅ Matched PumpSwap at Outer(0) (signature: ...)
✅ JSONL: BUY abc123... 0.500000 SOL → 1000.00 tokens (mint_xyz...)
```

**Validation:**
- Search logs for "✅ JSONL:" → **SHOULD APPEAR** (proves writer works if enabled)
- Check JSONL file: `cat streams/unified/events.jsonl` → **SHOULD HAVE ENTRIES**
- BUT: JSONL writes to file, NOT to SQLite database used by frontend

### Test 3: Confirm pipeline ingestion never starts
**Command:**
```bash
grep -r "start_pipeline_ingestion" examples/solflow/src/bin/unified_streamer.rs
```

**Result:** **No matches** (proves unified_streamer never spawns ingestion task)

**Contrast with pipeline_runtime.rs:**
```bash
grep -r "start_pipeline_ingestion" examples/solflow/src/bin/pipeline_runtime.rs
```

**Result:**
```
use solflow::pipeline::ingestion::start_pipeline_ingestion;
start_pipeline_ingestion(rx, engine_ingestion, db_writer_ingestion, flush_interval).await;
```

### Test 4: Verify `TradeProcessor::process()` is called
**Add logging in `lib.rs:329`:**
```rust
let program_match = match self.scanner.scan(&metadata) {
    Some(m) => {
        log::debug!("🔍 Scanner matched: {:?}", m);
        m
    }
    None => {
        log::debug!("⏭️  No match for signature: {}", metadata.signature);
        return Ok(());
    }
};
```

**Expected:** Logs appear for every transaction received from gRPC.

**If not:** Problem is upstream (gRPC client not receiving transactions).

### Test 5: Check if backend is SQLite
**Command:**
```bash
cargo run --release --bin unified_streamer --backend sqlite
```

**Expected log:**
```
💾 SQLite backend: /var/lib/solflow/solflow.db
```

**Validation:**
- Backend is correctly set ✅
- Writer is initialized ✅
- BUT: Writer never called because `enable_jsonl == false`

---

## Section 5 — Next Steps (High-Level)

### Option 1: Integrate Unified Streamer into pipeline_runtime.rs
**Approach:** Add unified streamer as 5th streamer in `pipeline_runtime.rs`

**Changes needed:**
1. Modify `pipeline_runtime.rs:140-145` to spawn `run_unified()` instead of `run_streamer()`
2. Pass `pipeline_tx: Some(tx_unified)` to unified streamer config
3. Remove standalone `unified_streamer.rs` binary (or mark deprecated)

**Pros:**
- Reuses existing pipeline infrastructure
- All streamers use same ingestion path
- No code duplication

**Cons:**
- Unified streamer tied to pipeline_runtime (can't run standalone)
- Requires ENABLE_PIPELINE=true to work

### Option 2: Add Pipeline Mode to Unified Streamer
**Approach:** Make `unified_streamer.rs` optionally spawn its own pipeline

**Changes needed:**
1. Check `ENABLE_PIPELINE` env var in `unified_streamer.rs`
2. If true:
   - Create mpsc channel
   - Spawn `start_pipeline_ingestion()` task
   - Pass channel to `run_unified()`
3. If false:
   - Use existing JSONL-only mode

**Pros:**
- Unified streamer can run standalone OR with pipeline
- Backwards compatible with JSONL mode

**Cons:**
- Duplicates pipeline setup code from `pipeline_runtime.rs`
- Two separate ingestion paths to maintain

### Option 3: Minimal Fix — Enable JSONL Writer
**Approach:** Quick workaround for immediate testing

**Changes needed:**
1. Set `enable_jsonl = true` in `UnifiedTradeProcessor::new()` (line 450)
2. Use SQLite backend: `--backend sqlite`
3. Events write to `SqliteWriter` directly (no pipeline)

**Pros:**
- Minimal code change (1 line)
- Can validate scanner + trade extraction immediately

**Cons:**
- Writes raw trades, not aggregates (frontend expects aggregates)
- Bypasses entire pipeline engine (signals, metrics, scoring)
- Not production-ready

### Option 4: Deprecate Unified Streamer (Recommended)
**Approach:** Unified streamer was experimental; consolidate into pipeline_runtime

**Changes needed:**
1. Add InstructionScanner to `pipeline_runtime.rs`
2. Replace 4 individual `run_streamer()` calls with single `run_unified()`
3. Delete `unified_streamer.rs` binary
4. Update documentation

**Pros:**
- Single production binary (`pipeline_runtime.rs`)
- No confusion about which binary to run
- Unified architecture matches unified scanner

**Cons:**
- Requires refactoring pipeline_runtime startup
- Removes standalone unified binary

---

## Architectural Summary

```
CURRENT STATE (BROKEN):
┌─────────────────────────────────────────────────────────────┐
│ unified_streamer.rs (ISOLATED)                              │
│   ├─ InstructionScanner ✅                                   │
│   ├─ UnifiedTradeProcessor ✅                                │
│   ├─ Scanner matches transactions ✅                         │
│   ├─ Logs appear ✅                                          │
│   └─ pipeline_tx: None ❌ → EVENTS DISCARDED                │
└─────────────────────────────────────────────────────────────┘

WORKING STATE (pipeline_runtime.rs):
┌─────────────────────────────────────────────────────────────┐
│ pipeline_runtime.rs (4 LEGACY STREAMERS)                    │
│   ├─ Creates mpsc channel ✅                                 │
│   ├─ Spawns PumpSwap with pipeline_tx ✅                     │
│   ├─ Spawns BonkSwap with pipeline_tx ✅                     │
│   ├─ Spawns Moonshot with pipeline_tx ✅                     │
│   ├─ Spawns JupiterDCA with pipeline_tx ✅                   │
│   └─ Spawns start_pipeline_ingestion() ✅                    │
│       └─ Writes to solflow.db ✅                             │
└─────────────────────────────────────────────────────────────┘

MISSING INTEGRATION:
unified_streamer.rs needs to either:
  A) Run INSIDE pipeline_runtime.rs (replaces 4 legacy streamers)
  B) Spawn its OWN pipeline_ingestion task
  C) Enable JSONL writer (bypasses pipeline, writes raw trades)
```

---

## Conclusion

**The unified streamer is architecturally complete but operationally isolated.**

- ✅ Instruction scanning works
- ✅ Trade extraction works  
- ✅ Logging works
- ❌ **No output channel configured**
- ❌ **No pipeline ingestion running**
- ❌ **Events are created and immediately discarded**

**The fix is NOT in the processing logic — it's in the binary's initialization.**

Either integrate unified_streamer into pipeline_runtime.rs (Option 1/4), or add pipeline setup to unified_streamer.rs (Option 2).

**Recommended:** Option 4 (deprecate standalone binary, integrate into pipeline_runtime).
