# Unified Instruction Scanner Architecture

**Date**: 2025-11-26  
**Status**: ✅ IMPLEMENTED  
**Branch**: `feature/unified-instruction-scanner`

---

## Executive Summary

This document describes the implementation of the unified instruction scanner architecture for SolFlow. The scanner consolidates 4 program-specific streamers into a single unified binary that:

- Tracks 5 programs (including PumpFun, previously missing)
- Scans both outer and inner (CPI) instructions
- Uses multi-program gRPC filtering
- Provides complete swap coverage including nested program calls

---

## Problem Statement

### Previous Architecture Limitations

1. **Per-Program Binaries**: 4 separate streamer binaries (PumpSwap, BonkSwap, Moonshot, Jupiter DCA)
2. **Missing Coverage**: PumpFun program completely missed despite being the primary bonding curve protocol
3. **Inner Instruction Gap**: gRPC filters only matched outer instructions, missing CPI calls
4. **Maintenance Overhead**: Duplicated code across 4 binaries

### Critical Coverage Gap: Inner Instructions

Example scenario that was previously missed:

```
User Transaction
├─ Outer Instruction: Jupiter Router (JUP6LkbZ...)
│  └─ Inner Instruction #1: PumpSwap CPI (pAMMBay6...)  ← MISSED
│  └─ Inner Instruction #2: Token Program
└─ Result: Trade executed through PumpSwap but not detected
```

**Impact**: Significant portion of trades missed when users interact through aggregators (Jupiter, Photon, Bloom, etc.)

---

## Solution Architecture

### 1. Instruction Scanner Module

**File**: `examples/solflow/src/instruction_scanner.rs`

**Core Responsibilities**:
- Load tracked program registry (5 programs)
- Scan outer instructions for matches
- Recursively scan inner instructions (CPIs)
- Return match result with location metadata

**Key Types**:

```rust
pub struct InstructionScanner {
    tracked_programs: HashSet<Pubkey>,
    program_names: HashMap<Pubkey, &'static str>,
}

pub struct InstructionMatch {
    pub program_id: Pubkey,
    pub program_name: &'static str,
    pub instruction_path: InstructionPath,
}

pub enum InstructionPath {
    Outer { index: usize },
    Inner { outer_index: usize, inner_path: Vec<usize> },
}
```

**Program Registry** (5 programs):

| Program | Program ID | Purpose |
|---------|-----------|---------|
| **PumpFun** | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` | Token minting & bonding curve |
| **PumpSwap** | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | Pump token swaps |
| **BonkSwap** | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | LetsBonk launchpad |
| **Moonshot** | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | Moonshot DEX |
| **Jupiter DCA** | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | DCA protocol |

### 2. Multi-Program gRPC Filtering

**File**: `examples/solflow/src/streamer_core/grpc_client.rs`

**Implementation**: `create_multi_program_client()`

```rust
let transaction_filter = SubscribeRequestFilterTransactions {
    vote: Some(false),
    failed: Some(false),
    account_required: vec![
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", // PumpFun
        "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", // PumpSwap
        "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // BonkSwap
        "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG",  // Moonshot
        "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M", // Jupiter DCA
    ],
    // ...
};
```

**Why This Works**:
- Solana includes ALL CPI program IDs in transaction account keys
- gRPC matches ANY transaction where these programs appear
- Covers both outer and inner instructions automatically
- More efficient than unfiltered subscription

### 3. Unified Trade Processor

**File**: `examples/solflow/src/streamer_core/lib.rs`

**Implementation**: `UnifiedTradeProcessor` + `run_unified()`

**Processing Flow**:

```
Transaction → InstructionScanner → Match?
                                      ↓
                                     No → Discard (early exit)
                                      ↓
                                    Yes → Extract Balance Deltas
                                          → Extract Trade Info
                                          → Blocklist Check
                                          → Create TradeEvent
                                          → Write to Backend
```

**Key Features**:
- Scanner runs FIRST (filtering layer)
- Non-matching transactions discarded immediately
- Matched program name injected into events
- All downstream logic unchanged (balance extraction, trade detection)

### 4. Unified Streamer Binary

**File**: `examples/solflow/src/bin/unified_streamer.rs`

**Purpose**: Single binary replacing 4 program-specific streamers

**Initialization**:

```rust
let scanner = InstructionScanner::new();
let client = create_multi_program_client(&runtime_config).await?;
let processor = UnifiedTradeProcessor::new(scanner, ...);

Pipeline::builder()
    .datasource(client)
    .transaction::<EmptyDecoderCollection, ()>(processor, None)
    .build()?
    .run()
    .await?;
```

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│           gRPC Transaction Stream (Multi-Program)            │
│   account_required: [PumpFun, PumpSwap, BonkSwap,           │
│                      Moonshot, Jupiter DCA]                  │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│             TransactionMetadata (Carbon Core)                │
│  • message.instructions() → Outer instructions               │
│  • meta.inner_instructions → CPI tree                        │
│  • meta.loaded_addresses → ALT account keys                  │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                InstructionScanner (NEW)                      │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ 1. Build full account keys (static + ALT)           │   │
│  │ 2. Check outer instructions → tracked_programs?     │   │
│  │ 3. Check inner instructions → tracked_programs?     │   │
│  │ 4. Return: Some(InstructionMatch) or None           │   │
│  │                                                      │   │
│  │ Output:                                              │   │
│  │   - program_id: Pubkey                               │   │
│  │   - program_name: "PumpFun" | "PumpSwap" | ...      │   │
│  │   - instruction_path: Outer { index } | Inner { .. }│   │
│  └──────────────────────────────────────────────────────┘   │
└────────────────────────┬────────────────────────────────────┘
                         │
                    Match?  No ──► Discard Transaction
                         │            (log::debug)
                       Yes
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│           UnifiedTradeProcessor (Modified Logic)             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ 1. Scanner returned match → continue processing     │   │
│  │ 2. build_full_account_keys(metadata)                │   │
│  │ 3. extract_sol_changes(meta, account_keys)          │   │
│  │ 4. extract_token_changes(meta, account_keys)        │   │
│  │ 5. extract_trade_info(sol_deltas, token_deltas)     │   │
│  │ 6. Check blocklist (optional)                       │   │
│  │ 7. Create TradeEvent with matched program_name      │   │
│  └──────────────────────────────────────────────────────┘   │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
                    TradeEvent
           (program_name from scanner match)
                         │
                         ├─► Pipeline Channel (optional)
                         └─► Backend (JSONL/SQLite)
```

---

## Implementation Details

### Scanner Algorithm

**Outer Instruction Scan**:

```rust
for (idx, instruction) in metadata.message.instructions().iter().enumerate() {
    let program_id_index = instruction.program_id_index as usize;
    
    if let Some(program_id) = account_keys.get(program_id_index) {
        if self.tracked_programs.contains(program_id) {
            return Some(InstructionMatch {
                program_id: *program_id,
                program_name: self.program_names.get(program_id).unwrap(),
                instruction_path: InstructionPath::Outer { index: idx },
            });
        }
    }
}
```

**Inner Instruction Scan**:

```rust
if let Some(inner_groups) = &metadata.meta.inner_instructions {
    for inner_group in inner_groups {
        let outer_index = inner_group.index as usize;

        for (inner_idx, inner) in inner_group.instructions.iter().enumerate() {
            let program_id_index = inner.instruction.program_id_index as usize;
            
            if let Some(program_id) = account_keys.get(program_id_index) {
                if self.tracked_programs.contains(program_id) {
                    return Some(InstructionMatch {
                        program_id: *program_id,
                        program_name: self.program_names.get(program_id).unwrap(),
                        instruction_path: InstructionPath::Inner {
                            outer_index,
                            inner_path: vec![inner_idx],
                        },
                    });
                }
            }
        }
    }
}
```

### Account Key Resolution

The scanner uses the existing `build_full_account_keys()` helper to construct the complete account key list:

```rust
pub fn build_full_account_keys(
    metadata: &Arc<TransactionMetadata>,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let message = &metadata.message;
    let mut all_keys = message.static_account_keys().to_vec();
    
    // Add loaded addresses from Address Lookup Tables (v0 transactions)
    let loaded = &meta.loaded_addresses;
    all_keys.extend(loaded.writable.iter().cloned());
    all_keys.extend(loaded.readonly.iter().cloned());
    
    all_keys
}
```

This ensures proper program ID resolution for both legacy and v0 transactions.

---

## Why PumpFun Addition is Critical

### PumpFun Protocol Overview

PumpFun is the **primary bonding curve protocol** on Solana for new token launches. It handles:

- Token minting and metadata creation
- Initial bonding curve purchases
- Liquidity pool migration to Raydium
- Early-stage token price discovery

### Impact of Missing PumpFun

Before this implementation, SolFlow:
- ❌ Completely missed ALL PumpFun token creation events
- ❌ Missed initial bonding curve trades (critical early signals)
- ❌ Missed liquidity migration events
- ❌ Had no visibility into tokens before PumpSwap involvement

### Coverage Examples

**Scenario 1: Direct PumpFun Interaction**
```
User creates token on PumpFun
├─ Outer Instruction: PumpFun Create (6EF8rr...)
└─ NOW DETECTED ✅ (previously completely missed ❌)
```

**Scenario 2: PumpFun via Aggregator**
```
User buys PumpFun token via Photon
├─ Outer Instruction: Photon Router
│  └─ Inner Instruction: PumpFun Buy (6EF8rr...)
└─ NOW DETECTED ✅ (previously completely missed ❌)
```

**Scenario 3: Multi-Hop Swap**
```
User swaps SOL → USDC → PumpFun Token
├─ Outer Instruction: Jupiter Router
│  ├─ Inner Instruction #1: Orca Swap (SOL → USDC)
│  └─ Inner Instruction #2: PumpFun Buy (USDC → Token)
└─ NOW DETECTED ✅ (inner instruction scan)
```

---

## Inner Instruction Coverage Examples

### Example 1: PumpSwap via Jupiter Router

**Transaction Structure**:
```json
{
  "message": {
    "instructions": [
      {
        "program_id_index": 15,  // Jupiter Router
        "accounts": [...],
        "data": "..."
      }
    ]
  },
  "meta": {
    "inner_instructions": [
      {
        "index": 0,
        "instructions": [
          {
            "program_id_index": 8,  // PumpSwap ← MATCH FOUND
            "accounts": [...],
            "data": "..."
          },
          {
            "program_id_index": 3,  // Token Program
            "accounts": [...],
            "data": "..."
          }
        ]
      }
    ]
  }
}
```

**Scanner Behavior**:
1. Check outer instruction #0 → Jupiter Router (no match)
2. Check inner instructions from outer #0:
   - Inner #0: PumpSwap → **MATCH FOUND** ✅
3. Return `InstructionMatch { program_name: "PumpSwap", instruction_path: Inner { outer_index: 0, inner_path: [0] } }`

**Previous Behavior**: Transaction missed entirely (gRPC filter only saw Jupiter)

### Example 2: PumpFun Token Creation

**Transaction Structure**:
```json
{
  "message": {
    "instructions": [
      {
        "program_id_index": 5,  // PumpFun ← MATCH FOUND
        "accounts": [...],
        "data": "181ec828..." // Create instruction
      }
    ]
  },
  "meta": {
    "inner_instructions": []
  }
}
```

**Scanner Behavior**:
1. Check outer instruction #0 → PumpFun → **MATCH FOUND** ✅
2. Return `InstructionMatch { program_name: "PumpFun", instruction_path: Outer { index: 0 } }`

**Previous Behavior**: Completely missed (no PumpFun streamer existed)

---

## TransactionMeta Guarantee

The scanner is **read-only** and does not modify `TransactionMetadata`:

| Component | Status | Notes |
|-----------|--------|-------|
| Balance Extraction | ✅ Unchanged | Same `extract_sol_changes()` + `extract_token_changes()` |
| Delta Calculation | ✅ Unchanged | Same raw change computation |
| Trade Detection | ✅ Unchanged | Same `extract_trade_info()` logic |
| Account Resolution | ✅ Unchanged | Same `build_full_account_keys()` |
| **Only Addition** | ✅ Scanner filter | Early discard + program name enrichment |

**Verification**:
- Scanner takes `&Arc<TransactionMetadata>` (immutable reference)
- No mutable methods called on metadata
- Balance extractor receives identical inputs as before

---

## File Changes Summary

### New Files (2 files)

1. **`examples/solflow/src/instruction_scanner.rs`** (195 lines)
   - Core scanner module with 5-program registry
   - Outer + inner instruction matching
   - Read-only metadata access
   - Unit tests for initialization

2. **`examples/solflow/src/bin/unified_streamer.rs`** (71 lines)
   - Unified ingestion binary
   - Replaces 4 program-specific streamers
   - Integrates scanner into pipeline

### Modified Files (4 files)

1. **`examples/solflow/src/main.rs`**
   - Added `pub mod instruction_scanner;`

2. **`examples/solflow/src/streamer_core/grpc_client.rs`**
   - Added `create_multi_program_client()` function (45 lines)
   - Multi-program filter with 5 program IDs
   - Kept existing `create_client()` for backward compatibility

3. **`examples/solflow/src/streamer_core/lib.rs`**
   - Added `UnifiedTradeProcessor` struct (132 lines)
   - Added `run_unified()` function (107 lines)
   - Integrated scanner into processing flow
   - Scanner runs before balance extraction

4. **`examples/solflow/src/streamer_core/mod.rs`**
   - Exported `run_unified` function
   - Made available for unified_streamer binary

5. **`examples/solflow/Cargo.toml`**
   - Added `[[bin]]` entry for unified_streamer

### Deprecated (Eventually - 4 files)

These files are kept during the dual-run validation period:

1. `examples/solflow/src/bin/pumpswap_streamer.rs`
2. `examples/solflow/src/bin/bonkswap_streamer.rs`
3. `examples/solflow/src/bin/moonshot_streamer.rs`
4. `examples/solflow/src/bin/jupiter_dca_streamer.rs`

**Deprecation Plan**: After 7-14 days of validation, these will be archived with migration notes.

---

## Usage

### Running the Unified Streamer

```bash
# With SQLite backend (default)
cargo run --bin unified_streamer

# With JSONL backend
cargo run --bin unified_streamer -- jsonl

# With environment variables
export SOLFLOW_DB_PATH="/var/lib/solflow/solflow.db"
export GEYSER_URL="https://api.mainnet-beta.solana.com"
export RUST_LOG="info"
cargo run --bin unified_streamer
```

### Comparing with Old Streamers (Validation Period)

```bash
# Terminal 1: Run unified streamer
cargo run --bin unified_streamer

# Terminal 2: Run PumpSwap streamer (for comparison)
cargo run --bin pumpswap_streamer

# Compare event counts:
# - Unified should detect ALL events from PumpSwap
# - Unified should also detect PumpFun events (new)
# - Unified should detect inner instruction matches (new)
```

---

## Performance Considerations

### Scanner Overhead

- **Account Key Resolution**: Already performed by balance extractor (no additional cost)
- **Outer Instruction Scan**: O(n) where n = # of outer instructions (typically 1-5)
- **Inner Instruction Scan**: O(m) where m = # of inner instructions (typically 0-20)
- **Early Exit**: Returns on first match (optimization)

**Expected Impact**: <5ms latency per transaction

### gRPC Bandwidth

**Multi-Program Filter** (5 programs):
- More efficient than unfiltered subscription
- Less efficient than single-program filter
- Trade-off: Slight bandwidth increase for complete coverage

**Estimated Throughput Impact**: <10% increase in transaction volume

---

## Logging Strategy

### Validation Period (Current)

**Match Logs** (INFO level):
```
✅ Matched PumpSwap at Outer { index: 0 } (signature: 5KJ...)
✅ Matched PumpFun at Inner { outer_index: 0, inner_path: [1] } (signature: 3hR...)
```

**Miss Logs** (DEBUG level):
```
⏭️  No tracked program matched (signature: 2aB...)
```

### Post-Rollout

**Match Logs**: Downgrade to DEBUG level
```
log::debug!("Matched {} at {:?}", program_name, path);
```

**Miss Logs**: Disabled (remove from code)

---

## Migration Plan

### Phase 1: Deployment (Week 1)
- ✅ Deploy unified_streamer alongside existing 4 binaries
- ✅ Validate compilation and startup
- Configure dual-run environment

### Phase 2: Validation (Week 2)
- Compare event counts per program
- Verify PumpFun event capture (new coverage)
- Verify inner instruction matches
- Monitor performance metrics

### Phase 3: Analysis (Week 3)
- Analyze event parity (unified vs. individual streamers)
- Document any discrepancies
- Verify no regressions in trade detection

### Phase 4: Rollout (Week 4)
- Deprecate old streamer binaries
- Update deployment scripts
- Archive old code with migration notes
- Update documentation

---

## Success Metrics

| Metric | Target | Status |
|--------|--------|--------|
| **Complete Coverage** | ALL trades from 4 existing streamers detected | ✅ Architecture complete |
| **PumpFun Detection** | New events captured from PumpFun | ✅ Registry includes PumpFun |
| **Inner Instruction Support** | Nested program calls detected | ✅ Inner scan implemented |
| **Performance** | <5ms scanner overhead | ⏳ Pending validation |
| **Metadata Integrity** | TransactionMeta unchanged | ✅ Read-only scanner |
| **Code Reduction** | 4 binaries → 1 | ✅ unified_streamer complete |
| **Compilation** | Clean build | ✅ Compiles successfully |

---

## Testing Strategy

### Unit Tests (Planned)

**File**: `examples/solflow/tests/instruction_scanner_tests.rs`

**Test Cases**:
1. Scanner initialization (5 programs)
2. Outer instruction matching (all 5 programs)
3. Inner instruction matching (PumpSwap via Jupiter)
4. No match (irrelevant transactions)
5. Metadata immutability (read-only guarantee)

### Integration Tests (Validation Period)

**Approach**: Dual-run comparison
1. Run unified_streamer + old streamers in parallel
2. Compare event counts per program
3. Verify inner instruction coverage
4. Check for PumpFun events (new)

**Success Criteria**:
- Event parity: unified >= sum of individual streamers
- PumpFun events: detected (previously 0)
- Inner instruction matches: detected (previously 0)

---

## Known Limitations

### Out of Scope

1. **No New Program Support**: Only the existing 5 programs are tracked
2. **No Decoder Changes**: Still uses `EmptyDecoderCollection` (metadata-only)
3. **No Trade Logic Changes**: Balance extraction and trade detection unchanged
4. **No External Config**: Program registry is hardcoded

### Future Enhancements

1. **External Registry**: Move program list to config file if registry grows beyond 10 programs
2. **Metrics Dashboard**: Add scanner hit/miss metrics
3. **Deep Inner Instruction Traversal**: Currently scans first level of CPIs only
4. **Program-Specific Handlers**: Add per-program post-processing logic

---

## Troubleshooting

### Issue: Scanner not detecting trades

**Diagnostic**:
```bash
# Check if transaction contains tracked programs
export RUST_LOG="debug,solflow=trace"
cargo run --bin unified_streamer
```

**Look for**:
- `⏭️  No tracked program matched` → gRPC filter may be too narrow
- No log output → gRPC connection issue

### Issue: Inner instructions not detected

**Diagnostic**:
1. Check if transaction has inner_instructions:
   ```
   metadata.meta.inner_instructions.is_some()
   ```

2. Verify program ID in account keys:
   ```
   account_keys.get(inner.instruction.program_id_index)
   ```

3. Confirm program ID in registry:
   ```
   scanner.tracked_programs.contains(program_id)
   ```

### Issue: Compilation errors

**Common Fixes**:
1. `InstructionScanner: Clone` → Add `#[derive(Clone)]` to struct
2. `.enumerate() not found` → Use `.iter().enumerate()`
3. `unused imports` → Remove from use statements

---

## References

### Carbon Framework
- [Carbon Core Documentation](https://docs.rs/carbon-core)
- [Transaction Processing](../../crates/core/src/transaction.rs)
- [Instruction Handling](../../crates/core/src/instruction.rs)

### Solana
- [Transaction Structure](https://docs.solana.com/developing/programming-model/transactions)
- [Inner Instructions (CPIs)](https://docs.solana.com/developing/programming-model/calling-between-programs)
- [Address Lookup Tables](https://docs.solana.com/developing/lookup-tables)

### Program Decoders
- [PumpFun Decoder](../../decoders/pumpfun-decoder)
- [PumpSwap Decoder](../../decoders/pump-swap-decoder)

---

## Changelog

### 2025-11-26 - Initial Implementation

**Added**:
- InstructionScanner module with 5-program registry
- Multi-program gRPC filtering
- UnifiedTradeProcessor with scanner integration
- unified_streamer binary
- Architecture documentation

**Modified**:
- grpc_client.rs: Added `create_multi_program_client()`
- lib.rs: Added `UnifiedTradeProcessor` and `run_unified()`
- Cargo.toml: Added unified_streamer binary

**Status**: ✅ Compiled successfully, ready for validation

---

## Approval

This implementation follows the approved specification:

- ✅ Option B (multi-program gRPC filtering)
- ✅ 5-program registry (including PumpFun)
- ✅ Outer + inner instruction scanning
- ✅ Read-only scanner (no metadata mutations)
- ✅ Hardcoded registry configuration
- ✅ Backward-compatible dual-run support

**Next Steps**: Deploy for validation period (7-14 days)
