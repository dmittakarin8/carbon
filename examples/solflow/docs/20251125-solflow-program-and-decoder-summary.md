# SolFlow Program and Decoder Summary

**Date**: 2025-11-25  
**Purpose**: Factual inventory of program IDs tracked, Carbon decoders used, and TransactionMeta usage in SolFlow

---

## Section 1: Program IDs Currently Tracked

SolFlow explicitly references the following program IDs in its codebase:

### Active Streamers (4 Programs)

1. **PumpSwap**
   - Program ID: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`
   - Referenced in: `src/bin/pumpswap_streamer.rs` (line 16)
   - Referenced in: `src/bin/pipeline_runtime.rs` (line 92)
   - Status: Active streamer binary

2. **BonkSwap** (LetsBonk Launchpad)
   - Program ID: `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`
   - Referenced in: `src/bin/bonkswap_streamer.rs` (line 16)
   - Referenced in: `src/bin/pipeline_runtime.rs` (line 107)
   - Status: Active streamer binary

3. **Moonshot**
   - Program ID: `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG`
   - Referenced in: `src/bin/moonshot_streamer.rs` (line 16)
   - Referenced in: `src/bin/pipeline_runtime.rs` (line 122)
   - Status: Active streamer binary

4. **Jupiter DCA**
   - Program ID: `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M`
   - Referenced in: `src/bin/jupiter_dca_streamer.rs` (line 38)
   - Referenced in: `src/bin/pipeline_runtime.rs` (line 137)
   - Status: Active streamer binary

### Reference-Only Programs (1 Program)

5. **Meteora DLMM**
   - Program ID: `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo`
   - Referenced in: `src/config.rs` (line 50) - in `verified_program_ids()` function
   - Referenced in: `README.md` - listed as available for optional filtering
   - Status: Listed but not actively tracked by any streamer

### Summary

- **Total Programs Referenced**: 5
- **Actively Tracked**: 4 (PumpSwap, BonkSwap, Moonshot, Jupiter DCA)
- **Reference Only**: 1 (Meteora DLMM)

---

## Section 2: Carbon Decoders in Use

### Finding: No Carbon Decoders Are Used

SolFlow **does not use any Carbon instruction decoders**. All transaction processing operates at the metadata level only.

#### Evidence

1. **EmptyDecoderCollection Implementation**
   - File: `src/empty_decoder.rs`
   - Type: Custom implementation of `InstructionDecoderCollection`
   - Behavior: Always returns `None` from `parse_instruction()` (line 41)
   - Purpose: Satisfies Carbon's trait requirements without decoding any instructions

2. **Usage Across All Processors**
   - `src/streamer_core/lib.rs` (line 27, 83, 266): Imports and uses `EmptyDecoderCollection`
   - `src/main.rs` (line 26, 159): Uses `TransactionProcessorInputType<EmptyDecoderCollection>`
   - `src/bin/grpc_verify.rs` (line 16, 328, 529): Uses `EmptyDecoderCollection`
   - `src/meta_analysis/capture_processor.rs` (line 24, 170): Uses `EmptyDecoderCollection`

3. **Pipeline Builder Pattern**
   ```rust
   Pipeline::builder()
       .datasource(client)
       .transaction::<EmptyDecoderCollection, ()>(processor, None)
   ```
   - The `EmptyDecoderCollection` type parameter is passed to `.transaction()`
   - The second parameter is `None`, indicating no instruction decoding
   - This pattern appears in all streamer entry points

#### What SolFlow Does Instead

SolFlow extracts trade information from:
- **SOL Balance Changes**: `metadata.meta.pre_balances` and `metadata.meta.post_balances`
- **Token Balance Changes**: `metadata.meta.pre_token_balances` and `metadata.meta.post_token_balances`
- **Instruction Discriminators**: First 8 bytes of `instruction.data` as hex (for identification only)

#### Decoder Dependencies

From `Cargo.toml`:
- `solana-account-decoder-client-types = "3.0"`: Used for `UiTokenAmount` type only
- No Carbon decoder crates are imported
- No program-specific decoder crates are imported

---

## Section 3: How TransactionMeta Is Used in SolFlow

SolFlow uses Carbon's `TransactionMetadata` abstraction to access Solana transaction metadata without decoding program instructions.

### Core Types Used

#### 1. TransactionMetadata (`carbon_core::transaction::TransactionMetadata`)

**Accessed Fields:**
- `metadata.slot` - Transaction slot number
- `metadata.signature` - Transaction signature
- `metadata.block_time` - Unix timestamp (optional)
- `metadata.fee_payer` - Fee payer public key
- `metadata.message` - Transaction message structure
- `metadata.meta` - `TransactionStatusMeta` from Solana

**References:**
- `src/streamer_core/lib.rs` (line 176)
- `src/streamer_core/balance_extractor.rs` (line 29)
- `src/meta_analysis/capture_processor.rs` (line 128)
- `src/bin/grpc_verify.rs` (line 97)

#### 2. TransactionMessage (`metadata.message`)

**Accessed Methods:**
- `message.static_account_keys()` - Returns static account keys as `Vec<Pubkey>`
- `message.instructions()` - Returns iterator over top-level instructions

**Usage Pattern:**
```rust
let message = &metadata.message;
let mut all_keys = message.static_account_keys().to_vec();
```

**References:**
- `src/streamer_core/balance_extractor.rs` (line 32-33)
- `src/streamer_core/lib.rs` (line 177, 179)
- `src/bin/grpc_verify.rs` (line 100, 365)

#### 3. TransactionStatusMeta (`metadata.meta`)

**Accessed Fields:**
- `meta.pre_balances: Vec<u64>` - SOL balances before transaction
- `meta.post_balances: Vec<u64>` - SOL balances after transaction
- `meta.pre_token_balances: Option<Vec<TransactionTokenBalance>>` - Token balances before
- `meta.post_token_balances: Option<Vec<TransactionTokenBalance>>` - Token balances after
- `meta.loaded_addresses` - Address Lookup Table (ALT) addresses
  - `loaded.writable: Vec<Pubkey>` - Writable ALT accounts
  - `loaded.readonly: Vec<Pubkey>` - Readonly ALT accounts
- `meta.inner_instructions: Option<Vec<InnerInstructions>>` - CPI call stack
- `meta.fee: u64` - Transaction fee in lamports
- `meta.rewards: Option<Vec<Reward>>` - Block rewards

**References:**
- `src/streamer_core/balance_extractor.rs` (lines 46-47, 82-87, 35-36)
- `src/meta_analysis/capture_processor.rs` (lines 133, 223-226)
- `src/bin/grpc_verify.rs` (lines 116-117, 153-158)

### Account Key Resolution

SolFlow builds a complete account key list by combining static keys with ALT-loaded addresses:

```rust
pub fn build_full_account_keys(
    metadata: &TransactionMetadata,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let message = &metadata.message;
    let mut all_keys = message.static_account_keys().to_vec();
    
    let loaded = &meta.loaded_addresses;
    all_keys.extend(loaded.writable.iter().cloned());
    all_keys.extend(loaded.readonly.iter().cloned());
    
    all_keys
}
```

**Location**: `src/streamer_core/balance_extractor.rs` (lines 28-39)

**Used by:**
- All trade extraction logic
- Inner instruction processing
- Balance delta tracking

### Balance Delta Extraction

#### SOL Changes (`extract_sol_changes`)

**Process:**
1. Zip `meta.pre_balances` with `meta.post_balances`
2. Calculate raw change: `(post - pre) as i128`
3. Convert to UI amount: `raw_change / 1_000_000_000.0`
4. Filter changes smaller than 0.0001 SOL
5. Return `Vec<BalanceDelta>` with account indices

**Location**: `src/streamer_core/balance_extractor.rs` (lines 44-77)

#### Token Changes (`extract_token_changes`)

**Process:**
1. Match accounts between `meta.pre_token_balances` and `meta.post_token_balances` by `account_index`
2. Parse raw amounts from `ui_token_amount.amount` string
3. Calculate raw change and UI change
4. Handle new token accounts (exist in post but not pre)
5. Return `Vec<BalanceDelta>` with mint addresses

**Location**: `src/streamer_core/balance_extractor.rs` (lines 80-171)

### Inner Instruction Tracking

SolFlow extracts inner instructions (CPIs) for metadata analysis:

```rust
fn extract_inner_instructions(
    metadata: &TransactionMetadata,
    account_keys: &[Pubkey],
) -> Vec<InnerInstructionRecord> {
    metadata.meta.inner_instructions.as_ref().map(|inner_groups| {
        inner_groups.iter().flat_map(|inner_group| {
            inner_group.instructions.iter().map(|inner| {
                let program_id_index = inner.instruction.program_id_index;
                let program_id = account_keys
                    .get(program_id_index as usize)
                    .map(|pk| pk.to_string())
                    .unwrap_or_else(|| "INVALID_INDEX".to_string());
                // ... extract data prefix
            })
        })
    })
}
```

**Location**: `src/meta_analysis/capture_processor.rs` (lines 128-163)

**Purpose**: Track which programs are called via CPI for analysis (not used in production trade extraction)

### Instruction Data Access

SolFlow extracts discriminators from top-level instructions:

```rust
fn extract_discriminator_hex(metadata: &TransactionMetadata) -> String {
    let message = &metadata.message;
    
    for instruction in message.instructions() {
        if instruction.data.len() >= 8 {
            return hex::encode(&instruction.data[0..8]);
        }
    }
    
    "0000000000000000".to_string()
}
```

**Location**: `src/streamer_core/lib.rs` (lines 176-185)

**Purpose**: Capture instruction discriminator for metadata logging (not decoded)

### Program ID Filtering

Program IDs are used to filter transactions at the gRPC subscription level:

```rust
let transaction_filter = SubscribeRequestFilterTransactions {
    vote: Some(false),
    failed: Some(false),
    account_include: vec![],
    account_exclude: vec![],
    account_required: vec![program_filter.to_string()],
    signature: None,
};
```

**Location**: `src/streamer_core/grpc_client.rs` (lines 34-41)

**Behavior**: Only transactions involving the specified program ID are received from gRPC stream

### Pipeline Integration

Carbon Pipeline processes transactions through the `Processor` trait:

```rust
#[async_trait]
impl Processor for TradeProcessor {
    type InputType = TransactionProcessorInputType<EmptyDecoderCollection>;

    async fn process(
        &mut self,
        (metadata, _instructions, _): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let account_keys = build_full_account_keys(&metadata, &metadata.meta);
        let sol_deltas = extract_sol_changes(&metadata.meta, &account_keys);
        let token_deltas = extract_token_changes(&metadata.meta, &account_keys);
        // ... extract trade info and write events
    }
}
```

**Location**: `src/streamer_core/lib.rs` (lines 83-120)

**Key Points:**
- `metadata` is `Arc<TransactionMetadata>`
- `_instructions` is ignored (empty due to `EmptyDecoderCollection`)
- All processing uses `metadata.meta` directly

### Summary of TransactionMeta Usage

| Component | Source | Extraction Method | Purpose |
|-----------|--------|-------------------|---------|
| Account Keys | `message.static_account_keys()` + `meta.loaded_addresses` | Build full key list | Resolve account indices to public keys |
| SOL Deltas | `meta.pre_balances`, `meta.post_balances` | Zip and diff | Calculate SOL flow for trade direction |
| Token Deltas | `meta.pre_token_balances`, `meta.post_token_balances` | Match by index and diff | Calculate token flow and mint |
| Inner Instructions | `meta.inner_instructions` | Extract program IDs | Metadata analysis only |
| Discriminators | `message.instructions()[0].data[0..8]` | Hex encode first 8 bytes | Logging and identification |
| Transaction Identity | `metadata.signature`, `metadata.slot`, `metadata.block_time` | Direct access | Event metadata |

---

## Verification Notes

All statements in this document are derived from:
- Source code inspection of `examples/solflow/src/` directory
- Cargo.toml dependency analysis
- Program ID string searches across the codebase
- Carbon type usage patterns

No assumptions were made about:
- Programs that "should" be tracked
- Decoders that "could" be used
- Features that "might" be added

This document reflects **only what SolFlow currently does** as of commit `59c65482` (2025-11-25).
