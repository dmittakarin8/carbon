# Carbon Terminal - Architecture Documentation

**Created:** 2025-01-XX  
**Purpose:** Complete architecture documentation for the `carbon_terminal` project before migration to fresh Carbon clone

---

## Table of Contents

1. [Overview](#overview)
2. [Project Structure](#project-structure)
3. [Key Components](#key-components)
4. [Architecture Decisions](#architecture-decisions)
5. [Configuration](#configuration)
6. [Dependencies](#dependencies)
7. [Data Flow](#data-flow)
8. [Implementation Details](#implementation-details)
9. [Migration Checklist](#migration-checklist)

---

## Overview

### Purpose

The `carbon_terminal` project is a real-time Solana transaction monitoring tool built on the Carbon framework. It streams transactions from Yellowstone gRPC, extracts trade information using metadata-based detection, and displays BUY/SELL trades from multiple DEX programs.

### Key Features

- **Multi-Program Support**: Monitors multiple Solana program IDs simultaneously (PumpSwap, LetsBonk, Moonshot)
- **Metadata-Based Detection**: Uses Carbon's `TransactionStatusMeta` to extract trade information without requiring instruction decoders
- **Real-Time Streaming**: Connects to Yellowstone gRPC for live transaction data
- **Trade Identification**: Automatically identifies BUY/SELL transactions based on SOL flow direction
- **OR-Based Filtering**: Uses separate filters per program ID to achieve OR logic (any program can match)

---

## Project Structure

```
carbon_terminal/
├── Cargo.toml                 # Package configuration and dependencies
├── README.md                  # User-facing documentation
├── AGENTS.md                  # Agent guide (workspace rules)
├── ARCHITECTURE.md            # This document
├── src/
│   ├── bin/
│   │   └── grpc_verify.rs     # Main binary: gRPC verification and trade monitoring
│   ├── empty_decoder.rs       # Empty decoder collection (metadata-only processing)
│   ├── main.rs                # Terminal UI binary (if exists)
│   ├── config.rs              # Configuration management
│   ├── trade_extractor.rs    # Trade extraction utilities
│   ├── aggregator.rs          # Volume aggregation logic
│   ├── persistence.rs          # State persistence
│   ├── state.rs                # State management
│   └── ui/                     # Terminal UI components
│       ├── mod.rs
│       ├── layout.rs
│       ├── renderer.rs
│       └── terminal.rs
└── trades.json                # Persisted trade data (if exists)
```

---

## Key Components

### 1. `grpc_verify.rs` - Main Binary

**Purpose:** Streams transactions from Yellowstone gRPC and displays trade information

**Key Structures:**

- `Config`: Loads configuration from environment variables
  - `geyser_url`: Yellowstone gRPC endpoint
  - `x_token`: Optional authentication token
  - `program_filters`: Vector of program IDs to monitor

- `BalanceDelta`: Represents balance changes for accounts
  - Tracks SOL and token balance changes
  - Provides `is_inflow()` and `is_outflow()` helpers
  - Stores raw change (i128) and UI-friendly change (f64)

- `DiscriminatorProcessor`: Main processor implementing Carbon's `Processor` trait
  - Processes transactions from the pipeline
  - Extracts trade information from metadata
  - Filters by program IDs
  - Displays formatted output

**Key Functions:**

- `build_full_account_keys()`: Builds complete account list including ALT-loaded addresses
- `extract_sol_changes()`: Extracts SOL balance changes from transaction metadata
- `extract_token_changes()`: Extracts token balance changes
- `find_user_account()`: Finds user account based on largest absolute SOL change (works for both BUY and SELL)
- `find_primary_token_mint()`: Identifies the primary token mint in a transaction
- `extract_trade_info()`: Combines SOL and token changes to determine trade direction and amounts
- `extract_discriminator()`: Extracts first 8 bytes of instruction data (for reference)
- `format_discriminator()`: Formats discriminator as hex string

### 2. `empty_decoder.rs` - Empty Decoder Collection

**Purpose:** Provides a minimal `InstructionDecoderCollection` implementation that never decodes instructions

**Why:** We use metadata-based detection, so we don't need instruction decoding. This decoder always returns `None` from `parse_instruction()`, allowing the pipeline to process transactions at the metadata level only.

**Implementation:**
- `EmptyInstruction`: Placeholder enum (never actually created)
- `EmptyDecoderCollection`: Implements `InstructionDecoderCollection` trait
- Always returns `None` from `parse_instruction()`

### 3. Configuration System

**Environment Variables:**

```bash
# Required
GEYSER_URL=https://basic.grpc.solanavibestation.com
PROGRAM_FILTERS=pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj,MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG

# Optional
X_TOKEN=your_token_here
RUST_LOG=info
```

**Program IDs Configured:**
- PumpSwap: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`
- LetsBonk Launchpad: `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`
- Moonshot: `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG`

---

## Architecture Decisions

### 1. Metadata-Based Detection (Not Instruction Decoding)

**Decision:** Use Carbon's `TransactionStatusMeta` to extract trade information instead of decoding instructions.

**Rationale:**
- Works universally across all Solana DEX programs
- No need for program-specific decoders
- More reliable (based on actual balance changes, not instruction parsing)
- Simpler implementation

**Trade-off:** Less detailed than decoded instructions (no instruction-specific data), but sufficient for trade monitoring.

### 2. OR-Based Filtering via Multiple Filters

**Decision:** Create separate transaction filters for each program ID instead of a single filter with all program IDs.

**Rationale:**
- Yellowstone gRPC's `account_required` uses AND logic (all accounts must be present)
- Multiple filters in the HashMap are treated as OR (any filter can match)
- This allows monitoring multiple programs simultaneously

**Implementation:**
```rust
// Create separate filter for each program ID
for (idx, program_id) in config.program_filters.iter().enumerate() {
    let filter = SubscribeRequestFilterTransactions {
        account_required: vec![program_id.clone()],
        // ... other fields
    };
    transaction_filters.insert(format!("program_filter_{}", idx), filter);
}
```

### 3. User Account Detection (Fixed SELL Bug)

**Decision:** Find user account based on largest absolute SOL change, regardless of direction.

**Previous Bug:** Only looked for negative SOL changes (`is_outflow()`), missing SELL transactions.

**Fix:** Removed the filter, now finds the account with the largest absolute change:
```rust
fn find_user_account(sol_deltas: &[BalanceDelta]) -> Option<usize> {
    sol_deltas
        .iter()
        .max_by_key(|d| d.raw_change.abs())  // No filter - works for both BUY and SELL
        .map(|d| d.account_index)
}
```

### 4. Trade Direction Detection

**Decision:** Determine BUY/SELL based on SOL flow direction from metadata.

**Logic:**
- **BUY**: User spends SOL (negative SOL change) → `is_outflow()` → "BUY"
- **SELL**: User receives SOL (positive SOL change) → `is_inflow()` → "SELL"
- **Mint**: The mint shown is the token being bought (BUY) or sold (SELL)

---

## Configuration

### Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `GEYSER_URL` | Yes | Yellowstone gRPC endpoint | `https://basic.grpc.solanavibestation.com` |
| `PROGRAM_FILTERS` | Yes | Comma-separated program IDs | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` |
| `X_TOKEN` | No | Authentication token | `your_token_here` |
| `RUST_LOG` | No | Logging level | `info`, `debug`, `warn`, `error` |

### Program IDs Reference

The following program IDs are configured in `Config::verified_program_ids()`:

1. **PumpSwap**: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`
2. **LetsBonk Launchpad**: `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`
3. **Moonshot**: `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG`

---

## Dependencies

### Carbon Framework Dependencies

```toml
carbon-core = { workspace = true }
carbon-log-metrics = { workspace = true }
carbon-yellowstone-grpc-datasource = { workspace = true }
```

### External Dependencies

```toml
async-trait = { workspace = true }
dotenv = { workspace = true }
env_logger = { workspace = true }
hex = "0.4"
log = { workspace = true }
chrono = { workspace = true, features = ["serde"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
solana-account-decoder-client-types = "2.2"
solana-instruction = "2.2"
solana-pubkey = { workspace = true }
solana-signature = { workspace = true }
solana-transaction-status = "2.1"
tokio = { workspace = true, features = ["full"] }
yellowstone-grpc-proto = { workspace = true }
rustls = { version = "0.23.0", default-features = false, features = ["std", "aws_lc_rs"] }
```

### Special Notes

- **rustls**: Requires `aws_lc_rs` provider (workaround for compatibility)
- **chrono**: Used for timestamp formatting
- **hex**: Used for discriminator formatting

---

## Data Flow

```
Yellowstone gRPC Stream
    ↓
YellowstoneGrpcGeyserClient
    ↓ (TransactionMetadata + TransactionStatusMeta)
Carbon Pipeline
    ↓
DiscriminatorProcessor.process()
    ↓
1. Extract account keys (static + ALT-loaded)
2. Extract SOL balance changes (extract_sol_changes)
3. Extract token balance changes (extract_token_changes)
4. Find user account (largest absolute SOL change)
5. Determine trade direction (BUY/SELL from SOL flow)
6. Find primary token mint
7. Calculate trade amounts
    ↓
Console Output
    [timestamp] sig=... program=... discriminator=... action=BUY/SELL mint=... sol=... token=...
```

### Filter Flow

```
PROGRAM_FILTERS (comma-separated)
    ↓
Split into Vec<String>
    ↓
Create separate filter per program ID
    ↓
HashMap<String, SubscribeRequestFilterTransactions>
    ↓
Yellowstone gRPC (OR logic - any filter matches)
    ↓
Transactions matching ANY program ID
```

---

## Implementation Details

### 1. Account Keys Building

Handles both v0 (with ALTs) and legacy transactions:

```rust
fn build_full_account_keys(
    metadata: &TransactionMetadata,
    meta: &TransactionStatusMeta,
) -> Vec<Pubkey> {
    let mut all_keys = message.static_account_keys().to_vec();
    
    // Add ALT-loaded addresses if present
    if let Some(loaded_addresses) = &meta.loaded_addresses {
        for alt in loaded_addresses {
            all_keys.extend_from_slice(&alt.account_keys);
        }
    }
    
    all_keys
}
```

### 2. SOL Change Extraction

Extracts SOL balance changes from `pre_balances` and `post_balances`:

```rust
fn extract_sol_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    // Compare pre_balances and post_balances
    // Filter for SOL accounts (no mint or native SOL)
    // Calculate deltas
}
```

### 3. Token Change Extraction

Extracts token balance changes from `pre_token_balances` and `post_token_balances`:

```rust
fn extract_token_changes(
    meta: &TransactionStatusMeta,
    account_keys: &[Pubkey],
) -> Vec<BalanceDelta> {
    // Compare pre_token_balances and post_token_balances
    // Extract mint, decimals, UI amount
    // Calculate deltas
}
```

### 4. Trade Info Extraction

Combines SOL and token changes to determine trade:

```rust
fn extract_trade_info(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
) -> Option<(String, f64, f64, &'static str)> {
    // 1. Find user account (largest absolute SOL change)
    // 2. Determine direction (BUY = outflow, SELL = inflow)
    // 3. Find primary token mint
    // 4. Calculate amounts
    // Returns: (mint, sol_amount, token_amount, direction)
}
```

### 5. Output Format

Each trade is displayed as:

```
[YYYY-MM-DD HH:MM:SS UTC] sig=<signature> program=<program_id> discriminator=<hex> action=<BUY|SELL> mint=<mint_address> sol=<amount> token=<amount>
```

Example:
```
[2025-01-15 14:30:45 UTC] sig=5aB3cD... program=pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA discriminator=66063d12 action=BUY mint=8xY9zA... sol=1.500000 token=1000000.000000
```

### 6. "(no matching instruction)" Message

This appears when:
- Trade information is detected from metadata (balance changes present)
- BUT no instruction in the transaction matches the target program ID
- This can happen when the program is involved via CPI (Cross-Program Invocation) or indirectly

The trade is still displayed because it's detected from metadata, but the instruction-level match failed.

---

## Migration Checklist

When setting up in a fresh Carbon clone:

### 1. Copy Files

- [ ] Copy entire `carbon_terminal/` directory to new Carbon clone's `examples/` directory
- [ ] Verify all source files are present
- [ ] Check that `Cargo.toml` is intact

### 2. Verify Dependencies

- [ ] Ensure Carbon workspace includes required crates:
  - `carbon-core`
  - `carbon-log-metrics`
  - `carbon-yellowstone-grpc-datasource`
- [ ] Check that workspace dependencies are available
- [ ] Verify Solana dependencies versions match

### 3. Update Dependencies (If Needed)

- [ ] Check if newer Carbon versions require dependency updates
- [ ] Update `Cargo.toml` if workspace structure changed
- [ ] Verify `rustls` configuration still works

### 4. Test Build

- [ ] Run `cargo build --release --bin grpc_verify`
- [ ] Fix any compilation errors
- [ ] Verify all imports resolve correctly

### 5. Configuration

- [ ] Create `.env` file with required variables
- [ ] Set `GEYSER_URL`
- [ ] Set `PROGRAM_FILTERS` with all three program IDs
- [ ] Optionally set `X_TOKEN` and `RUST_LOG`

### 6. Integration with New Decoders (Future)

When Bonk Swap Decoder and other decoders are available:

- [ ] Add decoder dependencies to `Cargo.toml`:
  ```toml
  carbon-bonk-swap-decoder = { workspace = true }
  carbon-moonshot-decoder = { workspace = true }
  carbon-pump-swap-decoder = { workspace = true }
  ```

- [ ] Create decoder collection using `instruction_decoder_collection!` macro:
  ```rust
  use carbon_proc_macros::instruction_decoder_collection;
  
  instruction_decoder_collection!(
      AllInstructions, AllInstructionTypes, AllPrograms,
      BonkSwap => BonkSwapDecoder => BonkSwapInstruction,
      Moonshot => MoonshotDecoder => MoonshotInstruction,
      PumpSwap => PumpSwapDecoder => PumpSwapInstruction
  );
  ```

- [ ] Update processor to use decoded instructions when available, fallback to metadata

### 7. Verify Functionality

- [ ] Run `cargo run --bin grpc_verify`
- [ ] Verify connection to Yellowstone gRPC
- [ ] Check that transactions are being received
- [ ] Verify BUY/SELL detection works correctly
- [ ] Confirm all three programs are being monitored

---

## Known Issues and Limitations

### 1. Empty Decoder Collection

Currently uses `EmptyDecoderCollection` which never decodes instructions. This is intentional for metadata-based detection, but means we don't get instruction-specific data.

**Future Enhancement:** When decoders are available, integrate them using `instruction_decoder_collection!` macro.

### 2. Commitment Level

Currently set to `CommitmentLevel::Confirmed`. Consider if `Finalized` is needed for production use.

### 3. Error Handling

Some error cases may not be fully handled (e.g., missing account keys, invalid data). Consider adding more robust error handling.

### 4. Performance

No rate limiting or backpressure handling in the current implementation. For high-volume scenarios, consider adding buffering or rate limiting.

---

## Future Enhancements

1. **Decoder Integration**: Use `instruction_decoder_collection!` to combine Bonk Swap, Moonshot, and PumpSwap decoders
2. **Enhanced Output**: Add more trade details when decoders are available
3. **Filtering**: Add ability to filter by token mint or trade size
4. **Persistence**: Save trades to database or file for analysis
5. **Metrics**: Add metrics collection for trade volume, frequency, etc.
6. **UI**: Enhance terminal output with colors, formatting, or TUI

---

## Notes

- This project uses metadata-based detection, which is more universal but less detailed than instruction decoding
- The OR-based filtering approach allows monitoring multiple programs simultaneously
- The SELL detection bug has been fixed (was only detecting BUY transactions)
- All trade information is extracted from Carbon's `TransactionStatusMeta`, not from instruction decoding

---

**End of Architecture Documentation**

