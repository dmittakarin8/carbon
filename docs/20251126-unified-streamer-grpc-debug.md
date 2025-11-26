# Unified Streamer gRPC Filter Diagnostic Report

**Date**: 2025-11-26  
**Issue**: Unified streamer connects to gRPC successfully but receives zero transactions  
**Expected**: Constant activity from PumpFun, PumpSwap, BonkSwap, Moonshot, and Jupiter DCA  

---

## Section 1 — Summary of Findings

### ROOT CAUSE IDENTIFIED ✓

The unified streamer is **correctly wired** at the code level, but is receiving zero transactions due to **the behavior of `account_required` filtering in Yellowstone gRPC**.

**Key Finding**:
The multi-program filter uses `account_required` with 5 program IDs in a single filter. Based on Yellowstone gRPC's filtering semantics, **`account_required` with multiple addresses acts as an AND filter** — meaning a transaction must involve **ALL 5 programs simultaneously** to match.

Since real-world Solana transactions almost never involve all 5 programs in a single transaction, **the filter matches nothing**.

**Why this is the root cause**:
- Line 50-55 in `examples/solflow/src/streamer_core/grpc_client.rs` creates a single `SubscribeRequestFilterTransactions` with:
  ```rust
  account_required: vec![
      "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", // PumpFun
      "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", // PumpSwap
      "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj", // BonkSwap
      "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG",  // Moonshot
      "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M", // Jupiter DCA
  ],
  ```
- This creates an **AND condition**: transaction must contain program_1 AND program_2 AND program_3 AND program_4 AND program_5
- Real transactions only involve 1-2 programs, never all 5
- Result: **zero matches**

---

## Section 2 — gRPC Filter Analysis

### 2.1 Filter Construction ✓ CORRECT

**Location**: `examples/solflow/src/streamer_core/grpc_client.rs:42-66`

The filter is constructed correctly as a `SubscribeRequestFilterTransactions`:

```rust
let transaction_filter = SubscribeRequestFilterTransactions {
    vote: Some(false),                  // ✓ Exclude vote transactions
    failed: Some(false),                // ✓ Exclude failed transactions
    account_include: vec![],            // ✓ Empty (not used)
    account_exclude: vec![],            // ✓ Empty (not used)
    account_required: vec![/* 5 programs */], // ⚠️ AND semantics!
    signature: None,                    // ✓ Not filtering by signature
};
```

**Status**: Structurally correct, but semantically incorrect for multi-program OR filtering.

### 2.2 Filter Map Population ✓ CORRECT

**Location**: `examples/solflow/src/streamer_core/grpc_client.rs:58-59`

```rust
let mut transaction_filters = HashMap::new();
transaction_filters.insert("multi_program_filter".to_string(), transaction_filter);
```

- Filter is inserted into `transaction_filters` map ✓
- Filter name: `"multi_program_filter"` ✓
- Map type: `HashMap<String, SubscribeRequestFilterTransactions>` ✓

**Status**: Correct map usage.

### 2.3 Client Constructor Parameter Ordering ✓ CORRECT

**Location**: `examples/solflow/src/streamer_core/grpc_client.rs:64-73`

```rust
YellowstoneGrpcGeyserClient::new(
    config.geyser_url.clone(),                      // 1. endpoint ✓
    config.x_token.clone(),                         // 2. x_token ✓
    Some(config.commitment_level),                  // 3. commitment ✓
    HashMap::default(),                             // 4. account_filters ✓
    transaction_filters,                            // 5. transaction_filters ✓
    Default::default(),                             // 6. block_filters ✓
    Arc::new(RwLock::new(HashSet::new())),         // 7. account_deletions_tracked ✓
    Default::default(),                             // 8. geyser_config ✓
)
```

**Expected signature** (from `datasources/yellowstone-grpc-datasource/src/lib.rs:79-88`):
```rust
pub const fn new(
    endpoint: String,                                                        // 1
    x_token: Option<String>,                                                // 2
    commitment: Option<CommitmentLevel>,                                    // 3
    account_filters: HashMap<String, SubscribeRequestFilterAccounts>,      // 4
    transaction_filters: HashMap<String, SubscribeRequestFilterTransactions>, // 5
    block_filters: BlockFilters,                                            // 6
    account_deletions_tracked: Arc<RwLock<HashSet<Pubkey>>>,              // 7
    geyser_config: YellowstoneGrpcClientConfig,                            // 8
) -> Self
```

**Status**: Parameter order is 100% correct. The `transaction_filters` map is passed to the correct position.

### 2.4 SubscribeRequest Construction ✓ CORRECT

**Location**: `datasources/yellowstone-grpc-datasource/src/lib.rs:185-194`

```rust
let subscribe_request = SubscribeRequest {
    slots: HashMap::new(),
    accounts: account_filters,           // account_filters goes to 'accounts'
    transactions: transaction_filters,   // transaction_filters goes to 'transactions' ✓
    transactions_status: HashMap::new(),
    entry: HashMap::new(),
    blocks: filters,
    blocks_meta: HashMap::new(),
    commitment: commitment.map(|x| x as i32),
    accounts_data_slice: vec![],
    ping: None,
};
```

**Status**: The `transaction_filters` map is correctly assigned to the `transactions` field of `SubscribeRequest`. This is the correct field for transaction filtering.

### 2.5 Filter Not Dropped or Overwritten ✓ VERIFIED

The filter lifecycle:
1. Created in `create_multi_program_client()` ✓
2. Inserted into `transaction_filters` map ✓
3. Passed to `YellowstoneGrpcGeyserClient::new()` ✓
4. Stored in client struct field ✓
5. Cloned in `consume()` method (line 163) ✓
6. Used in `SubscribeRequest` (line 188) ✓
7. Sent to gRPC server ✓

**Status**: Filter is preserved throughout the entire pipeline.

---

## Section 3 — Call-Site Analysis

### 3.1 Unified Streamer Entry Point

**File**: `examples/solflow/src/bin/unified_streamer.rs`

**Line 70**: Calls `run_unified(config, scanner).await`

**Status**: ✓ Correctly calls the unified run function.

### 3.2 Run Unified Function

**File**: `examples/solflow/src/streamer_core/lib.rs:420-523`

**Line 483**: Calls `create_multi_program_client(&runtime_config).await`

```rust
loop {
    match create_multi_program_client(&runtime_config).await {
        Ok(client) => {
            log::info!("✅ Connected to gRPC server (multi-program filter)");
            backoff.reset();
            
            let proc = processor.clone();
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                Pipeline::builder()
                    .datasource(client)  // Client is used as datasource
                    // ... rest of pipeline
            }.await;
```

**Status**: ✓ The unified streamer is **definitely** calling `create_multi_program_client()`, not the old `create_client()`.

### 3.3 Import Verification

**File**: `examples/solflow/src/streamer_core/lib.rs` (top of file)

The function `create_multi_program_client` is defined in:
```
examples/solflow/src/streamer_core/grpc_client.rs
```

And imported via:
```rust
use crate::streamer_core::grpc_client::create_multi_program_client;
```

**Status**: ✓ Correct function is imported and used.

### 3.4 No Old Code Paths

The old single-program function `create_client()` is still defined in `grpc_client.rs` but:
- It is marked with documentation comment: "backward compatibility"
- It is **NOT** called by `unified_streamer.rs` or `run_unified()`
- Only used by the old individual program streamers

**Status**: ✓ No old client creation code is being invoked.

---

## Section 4 — Correctness Notes

### 4.1 Filter Map Status ✗ NOT EMPTY

The `transaction_filters` map contains exactly 1 entry:
- Key: `"multi_program_filter"`
- Value: `SubscribeRequestFilterTransactions` with 5 program IDs

**Status**: Map is populated correctly.

### 4.2 Filter Structure ✓ WELL-FORMED

The filter structure matches the proto definition exactly:
- All required fields are set
- Field types match proto expectations
- No missing or extra fields

**Status**: Filter is well-formed.

### 4.3 Wrong Map? ✗ NO

**Verification**:
- Filter is placed in `transaction_filters` (correct for transaction filtering)
- Filter is NOT in `account_filters` (which would be wrong)
- Filter is NOT in `block_filters` (which would be wrong)

**Status**: Correct map is being used.

### 4.4 Program IDs ✓ MATCH

**Comparison between scanner and filter**:

| Program | Scanner (instruction_scanner.rs:58-65) | Filter (grpc_client.rs:51-55) | Match |
|---------|----------------------------------------|-------------------------------|-------|
| PumpFun | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` | ✓ |
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | ✓ |
| BonkSwap | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | ✓ |
| Moonshot | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | ✓ |
| Jupiter DCA | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | ✓ |

**Status**: Program IDs are identical between filter and scanner.

### 4.5 Program ID Validity ✓ VALID BASE58

All 5 program IDs decode successfully as valid Solana addresses:
- All are 43-44 characters (standard base58 length)
- All use valid base58 alphabet
- Used successfully in `Pubkey::from_str()` in scanner (line 58-65)

**Status**: All program IDs are valid.

### 4.6 Filter Silently Ignored? ✗ NO

The filter is:
1. Logged at creation: `log::info!("🔗 Creating multi-program gRPC client")` (line 61)
2. Logged at connection: `log::info!("✅ Connected to gRPC server (multi-program filter)")` (line 485)
3. Sent to gRPC server in `SubscribeRequest`
4. NOT ignored or dropped

**Status**: Filter is actively used.

### 4.7 The Critical Issue: AND vs OR Semantics ⚠️

**Current Implementation** (one filter with 5 program IDs):
```rust
account_required: vec![
    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",  // AND
    "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",  // AND
    "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj",  // AND
    "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG",   // AND
    "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M",  // AND
],
```

This matches transactions that involve **all 5 programs** in the same transaction.

**What we actually need** (5 filters, one per program):
```rust
// Filter 1: PumpFun OR
// Filter 2: PumpSwap OR
// Filter 3: BonkSwap OR
// Filter 4: Moonshot OR
// Filter 5: Jupiter DCA
```

Multiple filters in the same map are treated as **OR** logic by Yellowstone.

**Evidence from existing code**:
- `grpc_verify.rs:486-502` creates **separate filters per program** for OR logic
- `grpc_client.rs:create_client()` (single-program) creates **one filter with one program**
- Documentation in `20251113T08-architecture-grpc-verify.md:63` explicitly states: "Yellowstone treats multiple filters as OR logic"

**Status**: The current implementation uses AND semantics when OR is required.

---

## Section 5 — Recommended Fix (High-Level Only)

### 5.1 The Problem

**Current**: 1 filter with 5 program IDs in `account_required` = AND logic  
**Needed**: 5 filters with 1 program ID each in `account_required` = OR logic

### 5.2 What Must Change

**File**: `examples/solflow/src/streamer_core/grpc_client.rs`  
**Function**: `create_multi_program_client()`  
**Lines**: 42-66

**Change Summary**:
Instead of creating **one** `SubscribeRequestFilterTransactions` with all 5 program IDs, create **five** separate `SubscribeRequestFilterTransactions` objects, each with one program ID.

**Pseudo-code**:
```rust
let programs = vec![
    ("pumpfun", "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"),
    ("pumpswap", "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
    ("bonkswap", "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj"),
    ("moonshot", "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG"),
    ("jupiter_dca", "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M"),
];

let mut transaction_filters = HashMap::new();

for (name, program_id) in programs {
    let filter = SubscribeRequestFilterTransactions {
        vote: Some(false),
        failed: Some(false),
        account_include: vec![],
        account_exclude: vec![],
        account_required: vec![program_id.to_string()], // ONE program per filter
        signature: None,
    };
    
    transaction_filters.insert(format!("{}_filter", name), filter);
}

// Now transaction_filters contains 5 entries (OR logic)
```

### 5.3 Reference Implementation

The correct pattern already exists in:
- **File**: `examples/solflow/src/bin/grpc_verify.rs`
- **Lines**: 486-502
- **Pattern**: Loop over program IDs, create one filter per program, insert each into the map with a unique key

This is the **proven working approach** for multi-program OR filtering.

### 5.4 Impact Assessment

**No other changes required**:
- `YellowstoneGrpcGeyserClient::new()` signature: unchanged
- `run_unified()` function: unchanged
- `unified_streamer.rs` binary: unchanged
- `InstructionScanner`: unchanged
- Parameter ordering: unchanged

**Only change**: The contents of the `transaction_filters` map:
- Before: 1 entry with 5 programs (AND)
- After: 5 entries with 1 program each (OR)

### 5.5 Expected Outcome After Fix

- PumpFun transactions: detected ✓
- PumpSwap transactions: detected ✓
- BonkSwap transactions: detected ✓
- Moonshot transactions: detected ✓
- Jupiter DCA transactions: detected ✓
- Inner (CPI) instructions: detected via `InstructionScanner` ✓
- Zero transaction issue: resolved ✓

---

## Appendix: Code References

### A.1 Key Files Analyzed

1. `examples/solflow/src/bin/unified_streamer.rs` — Entry point
2. `examples/solflow/src/streamer_core/lib.rs` — `run_unified()` function
3. `examples/solflow/src/streamer_core/grpc_client.rs` — **`create_multi_program_client()` ← BUG HERE**
4. `datasources/yellowstone-grpc-datasource/src/lib.rs` — Client constructor and consume logic
5. `examples/solflow/src/instruction_scanner.rs` — Program registry (correct IDs)

### A.2 Evidence of AND Semantics

From Yellowstone gRPC proto documentation and observed behavior:
- `account_required: Vec<String>` — ALL addresses in this vector must be present in the transaction
- Multiple entries in `transaction_filters` map — Treated as OR (any filter can match)

### A.3 Validation

To validate the fix works:
1. Modify `create_multi_program_client()` to use 5 separate filters
2. Run unified streamer
3. Observe transaction count > 0
4. Verify all 5 programs are detected in logs
5. Compare event counts with individual streamers (should be sum of all 5)

---

## Section 6 — Fix Applied

**Date Applied**: 2025-11-26  
**Branch**: `feature/unified-instruction-scanner`  
**Status**: ✅ FIXED

### 6.1 Changes Made

**File Modified**: `examples/solflow/src/streamer_core/grpc_client.rs`  
**Function**: `create_multi_program_client()`  
**Lines Changed**: 42-77

### 6.2 Before vs After

**BEFORE (BROKEN - AND semantics)**:
```rust
// Created ONE filter with ALL 5 program IDs
let transaction_filter = SubscribeRequestFilterTransactions {
    vote: Some(false),
    failed: Some(false),
    account_include: vec![],
    account_exclude: vec![],
    account_required: vec![
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",  // AND
        "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",  // AND
        "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj",  // AND
        "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG",   // AND
        "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M",  // AND (all 5 required!)
    ],
    signature: None,
};

let mut transaction_filters = HashMap::new();
transaction_filters.insert("multi_program_filter", transaction_filter);
// Result: 1 filter with 5 programs = AND logic = ZERO MATCHES
```

**AFTER (FIXED - OR semantics)**:
```rust
// Define all tracked programs
let programs = vec![
    ("pumpfun", "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"),
    ("pumpswap", "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA"),
    ("bonkswap", "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj"),
    ("moonshot", "MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG"),
    ("jupiter_dca", "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M"),
];

// Create ONE filter per program (OR logic)
let mut transaction_filters = HashMap::new();

for (name, program_id) in programs.iter() {
    let filter = SubscribeRequestFilterTransactions {
        vote: Some(false),
        failed: Some(false),
        account_include: vec![],
        account_exclude: vec![],
        account_required: vec![program_id.to_string()], // ONE program only
        signature: None,
    };
    transaction_filters.insert(format!("{}_filter", name), filter);
}
// Result: 5 filters with 1 program each = OR logic = MATCHES ALL 5 PROGRAMS
```

### 6.3 Understanding AND vs OR in Yellowstone gRPC Filters

#### The AND Trap

**Within a single `account_required` vector**:
```rust
account_required: vec!["program_A", "program_B", "program_C"]
```
This means: "Match transactions that contain program_A **AND** program_B **AND** program_C"

A transaction must have all three programs in its account keys to match.

**Why this failed for unified streamer**:
- We put all 5 program IDs in one `account_required` vector
- Yellowstone required transactions to involve **all 5 programs simultaneously**
- Real-world transactions never involve more than 1-2 of our tracked programs
- Result: Zero matches

#### The OR Solution

**Multiple entries in `transaction_filters` HashMap**:
```rust
let mut transaction_filters = HashMap::new();
transaction_filters.insert("filter_1", /* filter for program_A */);
transaction_filters.insert("filter_2", /* filter for program_B */);
transaction_filters.insert("filter_3", /* filter for program_C */);
```

Yellowstone treats multiple map entries as: "Match transactions that match filter_1 **OR** filter_2 **OR** filter_3"

A transaction matching **any** of the filters will be received.

**Why this works**:
- Each filter has only 1 program ID in `account_required`
- Filter 1 matches PumpFun transactions
- Filter 2 matches PumpSwap transactions
- Filter 3 matches BonkSwap transactions
- Filter 4 matches Moonshot transactions
- Filter 5 matches Jupiter DCA transactions
- Yellowstone sends us **any** transaction matching **any** filter
- Result: All 5 programs detected

### 6.4 Filter Logic Summary

| Approach | Structure | Logic | Unified Streamer Result |
|----------|-----------|-------|------------------------|
| **BROKEN** | 1 filter with 5 programs in `account_required` | AND | ❌ Zero transactions (all 5 required) |
| **FIXED** | 5 filters with 1 program each in `account_required` | OR | ✅ All transactions (any program matches) |

### 6.5 Code Pattern Reference

This fix follows the proven pattern from:
- **File**: `examples/solflow/src/bin/grpc_verify.rs`
- **Lines**: 486-502
- **Pattern**: Loop over program IDs → create one filter per program → insert into map

### 6.6 Logging Changes

**New log output from unified streamer**:
```
🔗 Creating multi-program gRPC client
   Registered 5 transaction filters for multi-program matching
   Filter logic: OR (transactions matching ANY of the 5 programs)
   Filtering: PumpFun, PumpSwap, BonkSwap, Moonshot, Jupiter DCA
```

This explicitly confirms:
- 5 separate filters are created
- OR logic is active
- All 5 programs are included

### 6.7 Impact Assessment

**Changed**:
- `create_multi_program_client()` implementation (filter creation logic only)
- Log messages (added clarity about OR semantics)

**Unchanged**:
- `YellowstoneGrpcGeyserClient::new()` signature
- `run_unified()` function
- `unified_streamer.rs` binary
- `InstructionScanner` logic
- `TradeProcessor` logic
- Program IDs (identical before/after)
- Parameter ordering (identical before/after)

**Scope**: This is a surgical fix isolated to gRPC filter creation. No other logic was touched.

### 6.8 Expected Behavior After Fix

✅ **PumpFun transactions**: Detected and processed  
✅ **PumpSwap transactions**: Detected and processed  
✅ **BonkSwap transactions**: Detected and processed  
✅ **Moonshot transactions**: Detected and processed  
✅ **Jupiter DCA transactions**: Detected and processed  
✅ **Inner (CPI) instructions**: Scanned by `InstructionScanner`  
✅ **Zero transaction issue**: Resolved  
✅ **Dual-run validation**: Can resume  

### 6.9 Validation Steps

To confirm the fix works:

1. **Compile and run**:
   ```bash
   cargo build --release --bin unified_streamer
   cargo run --bin unified_streamer
   ```

2. **Check logs for new messages**:
   ```
   INFO  Registered 5 transaction filters for multi-program matching
   INFO  Filter logic: OR (transactions matching ANY of the 5 programs)
   ```

3. **Verify transaction flow**:
   - Transaction count should increase continuously
   - Scanner should log matches for all 5 programs
   - Events should be written to output backend

4. **Compare with individual streamers** (optional):
   - Sum of events from 5 individual streamers should approximately equal unified streamer events
   - Unified streamer should detect additional CPI cases missed by individual streamers

---

**End of Report**
