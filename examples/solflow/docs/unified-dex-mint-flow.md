# Unified DEX Mint Flow Architecture

**Branch:** `feature/unified-dex-mint-flow`  
**Status:** Implemented  
**Date:** 2025-11-26

---

## Overview

The Unified DEX Mint Flow is a comprehensive transaction processing pipeline that extracts **ALL** token mint movements from pump-ecosystem transactions. It supports multi-mint swaps, Jupiter routing, and fragmented DEX aggregations while maintaining zero instruction parsing overhead.

---

## Key Features

### ✅ Unified Pump-Relevant Detection

**Single Entry Point:** `InstructionScanner::is_pump_relevant()`

Detects transactions involving ANY of the 5 tracked programs:
- **PumpFun** - Token launches and bonding curves
- **PumpSwap** - Pump token swap protocol
- **BonkSwap** - LetsBonk launchpad swaps
- **Moonshot** - Moonshot DEX
- **Jupiter DCA** - Jupiter DCA protocol

**Coverage:** Both outer (top-level) and inner (CPI) instructions

### ✅ Multi-Mint Swap Support

**Capability:** Extract multiple trades from a single transaction

**Use Cases:**
- Jupiter-routed swaps through multiple pools
- Multi-leg DEX aggregations
- Router-wrapped CPIs
- Simultaneous buy/sell events

**Example:**
```
Transaction contains:
  - MintA: -500 tokens (SELL)
  - MintB: +2,000 tokens (BUY)

Result: 2 trade events emitted
```

### ✅ DEX Origin Attribution

**Mechanism:** First-match program detection via `InstructionScanner`

Each trade event includes:
```rust
{
  source_program: "PumpSwap" | "PumpFun" | "BonkSwap" | "Moonshot" | "JupiterDCA"
}
```

### ✅ Zero Instruction Parsing

**Data Source:** ONLY TransactionMetadata balance deltas

Uses:
- `pre_balances` / `post_balances` → SOL changes
- `pre_token_balances` / `post_token_balances` → Token changes

Does NOT use:
- Instruction data parsing
- Discriminator decoding
- Fallback heuristics
- Custom decoders

---

## Architecture

### Pipeline Flow

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. TRANSACTION INGESTION (via Yellowstone gRPC)                 │
│    - Multi-program filter (5 program IDs)                       │
│    - Commitment level: confirmed                                │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ 2. UNIFIED PUMP-RELEVANT DETECTION                              │
│    InstructionScanner::is_pump_relevant()                       │
│                                                                  │
│    Scans: Outer + Inner instructions                            │
│    Returns: true if ANY tracked program found                   │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼ (if pump-relevant)
┌─────────────────────────────────────────────────────────────────┐
│ 3. BALANCE EXTRACTION (Fallback Path)                           │
│    extract_sol_changes() + extract_token_changes()              │
│                                                                  │
│    SOL Deltas:   pre_balances → post_balances                   │
│    Token Deltas: pre_token_balances → post_token_balances       │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ 4. MULTI-MINT TRADE EXTRACTION                                  │
│    extract_all_trades()                                         │
│                                                                  │
│    Logic:                                                        │
│    1. Find user account (largest SOL delta)                     │
│    2. Group token deltas by mint address                        │
│    3. For each mint:                                             │
│       - Determine direction (SOL flow)                           │
│       - Extract token amount                                     │
│       - Create TradeInfo struct                                  │
│                                                                  │
│    Output: Vec<TradeInfo> (one per mint)                        │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼ (for each trade)
┌─────────────────────────────────────────────────────────────────┐
│ 5. BLOCKLIST FILTERING                                           │
│    BlocklistChecker::is_blocked()                               │
│                                                                  │
│    If blocked: skip this mint, continue with others             │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ 6. TRADE EVENT CREATION                                          │
│    TradeEvent {                                                  │
│      mint, sol_amount, token_amount, direction,                 │
│      source_program: from InstructionScanner,                   │
│      user_account, timestamp, signature                          │
│    }                                                             │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ 7. DUAL-CHANNEL EMISSION                                         │
│    - Pipeline channel (mpsc::Sender)                            │
│    - JSONL file (optional, if ENABLE_JSONL=true)                │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Details

### File Structure

| File | Purpose | Key Functions |
|------|---------|---------------|
| `instruction_scanner.rs` | Program detection | `scan()`, `is_pump_relevant()` |
| `balance_extractor.rs` | Balance delta extraction | `extract_sol_changes()`, `extract_token_changes()` |
| `trade_detector.rs` | Trade info extraction | `extract_all_trades()`, `extract_trade_info()` |
| `lib.rs` (streamer_core) | Processing pipeline | `UnifiedTradeProcessor::process()` |
| `pipeline_runtime.rs` | Orchestration | `run()` |

### Core Components

#### 1. InstructionScanner

**Responsibility:** Unified program detection

**Registry:**
```rust
PumpFun:     6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
PumpSwap:    pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
BonkSwap:    LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj
Moonshot:    MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG
JupiterDCA:  DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M
```

**Methods:**
- `scan(metadata)` → `Option<InstructionMatch>` - First match with program name
- `is_pump_relevant(metadata)` → `bool` - Simple boolean check

**Scan Coverage:**
- Outer instructions: `message.instructions()`
- Inner instructions: `meta.inner_instructions`
- Account keys: static + ALT (Address Lookup Table) loaded addresses

#### 2. Balance Extractor

**Responsibility:** Extract balance deltas from metadata

**SOL Changes:**
```rust
pub fn extract_sol_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta>
```

**Logic:**
- Compare `pre_balances[i]` vs `post_balances[i]`
- Filter: `abs(delta) >= 0.0001 SOL`
- Returns: `BalanceDelta` with `raw_change`, `ui_change`, `account_index`

**Token Changes:**
```rust
pub fn extract_token_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta>
```

**Logic:**
- Compare `pre_token_balances` vs `post_token_balances`
- Handle missing entries (accounts created/closed mid-transaction)
- Returns: `BalanceDelta` with `mint`, `raw_change`, `ui_change`, `decimals`

#### 3. Trade Detector

**Responsibility:** Convert balance deltas into trade info

**Multi-Mint Extraction:**
```rust
pub fn extract_all_trades(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
    account_keys: &[Pubkey],
) -> Vec<TradeInfo>
```

**Algorithm:**
1. **Find User Account:**
   - Largest SOL delta by absolute value
   - Assumption: User is the primary SOL mover

2. **Determine Trade Direction:**
   - SOL outflow → BUY (user spent SOL)
   - SOL inflow → SELL (user received SOL)

3. **Group by Mint:**
   - Create HashMap of mint → Vec<BalanceDelta>
   - Skip wrapped SOL (So11111...)

4. **Extract Per-Mint Trade:**
   - For each mint: find largest delta
   - Create `TradeInfo` with:
     - `mint`, `sol_amount`, `token_amount`, `decimals`
     - `direction`, `user_account`

**Backwards Compatibility:**
```rust
pub fn extract_trade_info(...) -> Option<TradeInfo>
```
- Wrapper around `extract_all_trades()`
- Returns first trade only
- Preserves existing single-mint behavior

#### 4. UnifiedTradeProcessor

**Responsibility:** Process transactions end-to-end

**Process Flow:**
```rust
async fn process(&mut self, metadata, ...) {
    // 1. Scan for tracked programs
    let program_match = self.scanner.scan(&metadata)?;
    
    // 2. Extract balance deltas
    let sol_deltas = extract_sol_changes(...);
    let token_deltas = extract_token_changes(...);
    
    // 3. Extract all trades (multi-mint)
    let all_trades = extract_all_trades(&sol_deltas, &token_deltas, &account_keys);
    
    // 4-6. For each trade:
    for trade_info in all_trades {
        // 4. Blocklist check
        if is_blocked(&trade_info.mint) { continue; }
        
        // 5. Create event
        let event = TradeEvent {
            source_program: program_match.program_name,
            mint: trade_info.mint,
            sol_amount: trade_info.sol_amount,
            token_amount: trade_info.token_amount,
            direction: trade_info.direction,
            ...
        };
        
        // 6. Emit to pipeline + JSONL
        pipeline_tx.try_send(event);
        jsonl_writer.write(&event);
    }
}
```

---

## Multi-Mint Scenarios

### Scenario 1: Jupiter-Routed PumpSwap

**Transaction:**
```
Outer Instruction: Jupiter Aggregator
  Inner Instruction 0: PumpSwap (MintA → SOL)
  Inner Instruction 1: Raydium (SOL → MintB)
```

**Detection:**
- `InstructionScanner` finds `PumpSwap` in inner instruction 0
- `source_program: "PumpSwap"`

**Balance Deltas:**
```
SOL:   -0.5 SOL (user spent)
MintA: -1000 tokens (user sold)
MintB: +5000 tokens (user bought)
```

**Output:**
```
Event 1:
  mint: MintA
  direction: SELL
  sol_amount: 0.5
  token_amount: 1000
  source_program: PumpSwap

Event 2:
  mint: MintB
  direction: BUY
  sol_amount: 0.5
  token_amount: 5000
  source_program: PumpSwap
```

### Scenario 2: BonkSwap via Router

**Transaction:**
```
Outer Instruction: BonkSwap Router
  Inner Instruction 0: BonkSwap Core (MintC swap)
  Inner Instruction 1: Token Program (transfer)
```

**Detection:**
- `InstructionScanner` finds `BonkSwap` in inner instruction 0
- `source_program: "BonkSwap"`

**Balance Deltas:**
```
SOL:   +1.2 SOL (user received)
MintC: -10000 tokens (user sold)
```

**Output:**
```
Event 1:
  mint: MintC
  direction: SELL
  sol_amount: 1.2
  token_amount: 10000
  source_program: BonkSwap
```

### Scenario 3: Multi-Program Pipeline (Jupiter + Moonshot)

**Transaction:**
```
Outer Instruction: Jupiter Aggregator
  Inner Instruction 0: Moonshot (MintD swap)
  Inner Instruction 1: PumpSwap (MintE swap)
```

**Detection:**
- `InstructionScanner` finds `Moonshot` FIRST (early exit)
- `source_program: "Moonshot"`

**Balance Deltas:**
```
SOL:   -2.0 SOL (user spent)
MintD: +20000 tokens (user bought)
MintE: +5000 tokens (user bought)
```

**Output:**
```
Event 1:
  mint: MintD
  direction: BUY
  sol_amount: 2.0
  token_amount: 20000
  source_program: Moonshot  ← First match wins

Event 2:
  mint: MintE
  direction: BUY
  sol_amount: 2.0
  token_amount: 5000
  source_program: Moonshot  ← Same for all trades in txn
```

**Note:** All trades in a single transaction share the same `source_program` (first match from scanner).

---

## Guarantees

### From Verified Analysis

1. **InstructionScanner** handles all 5 programs ✅
2. **Balance extraction** uses ONLY metadata deltas ✅
3. **DEX attribution** uses program_name from scanner ✅
4. **Inner instruction detection** uses Carbon's `meta.inner_instructions` ✅

### New Guarantees (Post-Implementation)

1. **Every mint in a pump-relevant transaction is extracted** ✅
2. **Multi-mint swaps emit multiple events** ✅
3. **DEX origin is preserved per event** ✅
4. **No instruction parsing or fallback logic needed** ✅
5. **Zero breaking changes to existing code** ✅

---

## Regression Prevention

### Preserved Behaviors

1. **Single-mint transactions** → Emit 1 event (unchanged)
2. **Jupiter-routed swaps** → Detected via scanner (unchanged)
3. **Nested CPI detection** → Works via `meta.inner_instructions` (unchanged)
4. **Blocklist filtering** → Applied per-mint (unchanged)
5. **Pipeline ingestion** → Non-blocking `try_send()` (unchanged)

### New Behaviors

1. **Multi-mint transactions** → Emit N events (NEW)
2. **Blocked mint skipping** → Uses `continue` not `return` (NEW)
3. **Multi-mint logging** → Debug log when N > 1 (NEW)

---

## Testing Strategy

### Unit Tests

**File:** `trade_detector.rs`

```rust
#[test]
fn test_extract_all_trades_single_mint() {
    // Verify backwards compatibility
}

#[test]
fn test_extract_all_trades_multi_mint() {
    // Verify multi-mint extraction
}

#[test]
fn test_extract_all_trades_wrapped_sol_skipped() {
    // Verify wrapped SOL is filtered
}
```

### Integration Tests

**File:** `tests/test_pipeline_integration.rs`

```rust
#[test]
fn test_multi_mint_transaction_processing() {
    // Mock transaction with 2 mints
    // Verify 2 events emitted
    // Verify correct DEX attribution
}
```

### Manual Verification

**Tool:** `mint_trace` binary

```bash
# Find real multi-mint transaction
cargo run --bin mint_trace -- --mint <MULTI_MINT_TX>

# Verify output shows multiple token mints
```

---

## Configuration

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `USE_UNIFIED_STREAMER` | Enable unified mode | `true` |
| `ENABLE_JSONL` | Enable JSONL writes | `false` |
| `ENABLE_PIPELINE` | Enable pipeline | `true` |
| `SOLFLOW_DB_PATH` | Database path | `/var/lib/solflow/solflow.db` |

### Runtime Toggles

**Unified Streamer (Recommended):**
```bash
USE_UNIFIED_STREAMER=true cargo run --bin pipeline_runtime
```

**Legacy Mode (Deprecated):**
```bash
USE_UNIFIED_STREAMER=false cargo run --bin pipeline_runtime
```

---

## Performance Characteristics

### Computational Complexity

- **Instruction Scanning:** O(N + M) where N = outer, M = inner instructions
- **Balance Extraction:** O(B) where B = balance entries
- **Trade Detection:** O(T) where T = token deltas
- **Multi-Mint Grouping:** O(T) via HashMap

**Total:** O(N + M + B + T) - Linear in transaction complexity

### Memory Usage

- **Scanner:** 5 program IDs (constant)
- **Balance Deltas:** Temporary vectors (dropped after processing)
- **Trade Extraction:** Temporary HashMap (dropped after processing)
- **No persistent state** per transaction

### Throughput

- **Single-mint transactions:** No performance change
- **Multi-mint transactions:** Additional event emissions (non-blocking)
- **Bottleneck:** Pipeline channel capacity (configurable via `STREAMER_CHANNEL_BUFFER`)

---

## Troubleshooting

### No Events Emitted

**Check:**
1. Is transaction pump-relevant? (Scanner found a program?)
2. Are there SOL changes? (Swaps require SOL flow)
3. Are there token changes? (Swaps require token balance deltas)
4. Is mint blocklisted? (Check `token_metadata` table)

### Duplicate Events

**Cause:** Same mint appears multiple times in transaction

**Expected:** One event per unique mint (HashMap deduplicates)

**If seeing duplicates:** Check signature - might be different transactions

### Missing Multi-Mint Events

**Check:**
1. Is `extract_all_trades()` being called? (Not `extract_trade_info()`)
2. Are non-SOL mints being filtered correctly?
3. Check logs for "Multi-mint transaction: N trades extracted"

---

## Future Enhancements

### Potential Additions

1. **Per-Mint DEX Attribution:**
   - Track which program touched each mint
   - Requires deeper instruction tree analysis

2. **Swap Path Reconstruction:**
   - Identify full swap route (MintA → MintB → MintC)
   - Requires instruction ordering analysis

3. **Multi-User Detection:**
   - Handle transactions with multiple wallet participants
   - Requires sophisticated SOL flow analysis

4. **Historical Backfill:**
   - Reprocess old transactions with new logic
   - Requires RPC historical query integration

---

## References

### Related Documentation

- [InstructionScanner Architecture](./20251126-unified-instruction-scanner-architecture.md)
- [Mint Trace Usage](./mint-trace-usage.md)
- [Pipeline Runtime Architecture](./UNIFIED_STREAMER_USAGE.md)

### Code References

- `examples/solflow/src/instruction_scanner.rs`
- `examples/solflow/src/streamer_core/balance_extractor.rs`
- `examples/solflow/src/streamer_core/trade_detector.rs`
- `examples/solflow/src/streamer_core/lib.rs`

---

## Changelog

### 2025-11-26 - Initial Implementation

**Added:**
- `extract_all_trades()` function for multi-mint support
- `is_pump_relevant()` helper method for unified detection
- Multi-event emission in `UnifiedTradeProcessor`
- Comprehensive documentation

**Changed:**
- `extract_trade_info()` now wraps `extract_all_trades()`
- Blocklist filtering uses `continue` instead of `return`

**Preserved:**
- All existing single-mint behavior
- All existing tests pass
- Zero breaking changes

---

**Author:** Droid (Factory AI)  
**Branch:** `feature/unified-dex-mint-flow`  
**Status:** ✅ Complete
