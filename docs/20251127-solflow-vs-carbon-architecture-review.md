# SolFlow vs Carbon Framework: Architectural Review

**Date:** November 27, 2025  
**Purpose:** Neutral comparison of SolFlow's custom approach vs Carbon's native framework patterns  
**Scope:** Instruction processing, CPI handling, and discriminator matching

---

## Executive Summary

This document compares how **reference Carbon examples** and **Carbon decoders** handle Solana transactions vs. how **SolFlow** currently implements instruction detection. The analysis reveals fundamental differences in:

1. **Pipeline Integration**: Carbon uses declarative pipeline builders; SolFlow uses manual gRPC streaming
2. **Instruction Decoding**: Carbon uses trait-based decoders with automatic discriminator matching; SolFlow uses manual program ID filtering
3. **CPI Handling**: Carbon automatically processes nested instructions recursively; SolFlow ignores inner instructions
4. **Type Safety**: Carbon provides strongly-typed instruction enums; SolFlow works with raw transaction metadata

---

## Section 1: Reference Carbon Examples

### 1.1 Example: `pumpfun-alerts`

**Location:** `examples/pumpfun-alerts/src/main.rs`

#### Initialization Pattern

```rust
carbon_core::pipeline::Pipeline::builder()
    .datasource(helius_websocket)
    .instruction(PumpfunDecoder, PumpfunInstructionProcessor)
    .build()?
    .run()
    .await?;
```

**Key Observations:**
- Uses **declarative pipeline builder** pattern
- Pairs decoder (`PumpfunDecoder`) with processor (`PumpfunInstructionProcessor`)
- Framework handles all transaction routing, filtering, and CPI recursion
- No manual transaction parsing required

#### Instruction Detection

```rust
impl Processor for PumpfunInstructionProcessor {
    type InputType = InstructionProcessorInputType<PumpfunInstruction>;

    async fn process(&mut self, data: Self::InputType, _metrics: Arc<MetricsCollection>) 
        -> CarbonResult<()> 
    {
        let pumpfun_instruction: PumpfunInstruction = data.1.data;

        match pumpfun_instruction {
            PumpfunInstruction::CreateEvent(create_event) => {
                log::info!("New token created: {:#?}", create_event);
            }
            PumpfunInstruction::TradeEvent(trade_event) => {
                if trade_event.sol_amount > 10 * LAMPORTS_PER_SOL {
                    log::info!("Big trade occured: {:#?}", trade_event);
                }
            }
            PumpfunInstruction::CompleteEvent(complete_event) => {
                log::info!("Bonded: {:#?}", complete_event);
            }
            _ => {}
        };

        Ok(())
    }
}
```

**Key Observations:**
- Uses **enum-based pattern matching** (type-safe)
- No manual discriminator parsing
- No manual iteration through instructions
- Automatically receives both outer AND inner (CPI) instructions
- `data.1.data` contains the **already-decoded** instruction

#### CPI Handling

The framework automatically handles CPIs:
- `InstructionProcessorInputType` tuple includes `NestedInstructions` at position 2
- The pipeline recursively processes all nested instructions
- Each nested instruction triggers the processor if it matches the decoder's program ID

**Proof:**
```rust
pub type InstructionProcessorInputType<T> = (
    InstructionMetadata,
    DecodedInstruction<T>,
    NestedInstructions,  // <-- Contains all inner instructions
    solana_instruction::Instruction,
);
```

---

### 1.2 Example: `pumpswap-alerts`

**Location:** `examples/pumpswap-alerts/src/main.rs`

#### Initialization Pattern

```rust
carbon_core::pipeline::Pipeline::builder()
    .datasource(laserstream_datasource)
    .metrics(Arc::new(LogMetrics::new()))
    .metrics_flush_interval(3)
    .instruction(PumpSwapDecoder, PumpSwapInstructionProcessor)
    .shutdown_strategy(carbon_core::pipeline::ShutdownStrategy::Immediate)
    .build()?
    .run()
    .await?;
```

**Key Observations:**
- Same declarative pattern as `pumpfun-alerts`
- Uses `LaserStream` datasource instead of `Helius WebSocket`
- Includes metrics configuration
- Framework handles gRPC subscription filters internally

#### Instruction Detection

```rust
match pumpswap_instruction {
    PumpSwapInstruction::Buy(buy) => {
        log::info!("Buy: signature: {signature}, buy: {buy:?}");
    }
    PumpSwapInstruction::Sell(sell) => {
        log::info!("Sell: signature: {signature}, sell: {sell:?}");
    }
    PumpSwapInstruction::BuyEvent(buy_event) => {
        let sol_amount = buy_event.quote_amount_in as f64 / LAMPORTS_PER_SOL as f64;
        log::info!(
            "BuyEvent: signature: {signature}, SOL: {:.4}, pool: {}, user: {}",
            sol_amount,
            buy_event.pool,
            buy_event.user,
        );
    }
    PumpSwapInstruction::SellEvent(sell_event) => {
        let sol_amount = sell_event.quote_amount_out as f64 / LAMPORTS_PER_SOL as f64;
        log::info!(
            "SellEvent: signature: {signature}, SOL: {:.4}, pool: {}, user: {}",
            sol_amount,
            sell_event.pool,
            sell_event.user
        );
    }
    // ... more variants
}
```

**Key Observations:**
- Uses **enum matching** (no manual byte inspection)
- Receives **both instructions AND events** (events are CPI-emitted)
- `BuyEvent` and `SellEvent` are automatically captured from inner instructions
- No manual parsing of `meta.inner_instructions` required

---

### 1.3 Example: `jupiter-swap-alerts`

**Location:** `examples/jupiter-swap-alerts/src/main.rs`

#### Nested Instructions Evidence

```rust
match instruction.data {
    JupiterSwapInstruction::Route(route) => {
        assert!(
            !nested_instructions.is_empty(),
            "nested instructions empty: {} ",
            signature
        );
        log::info!("route: signature: {signature}, route: {route:?}");
    }
    JupiterSwapInstruction::SharedAccountsRoute(shared_accounts_route) => {
        assert!(
            !nested_instructions.is_empty(),
            "nested instructions empty: {} ",
            signature
        );
        log::info!("shared_accounts_route: signature: {signature}, ...");
    }
    // ... all route variants have assertions checking nested_instructions
}
```

**Key Observations:**
- **Asserts that nested instructions are NOT empty** for routing instructions
- Jupiter routes **always** involve CPIs to underlying DEXs (Raydium, Orca, etc.)
- The framework **guarantees** that `nested_instructions` contains the inner calls
- This is **proof** that Carbon automatically handles CPIs

---

### 1.4 Example: `raydium-alerts`

**Location:** `examples/raydium-alerts/src/main.rs`

#### Account Processing

```rust
carbon_core::pipeline::Pipeline::builder()
    .datasource(yellowstone_grpc)
    .instruction(RaydiumAmmV4Decoder, RaydiumAmmV4InstructionProcessor)
    .account(RaydiumAmmV4Decoder, RaydiumAmmV4AccountProcessor)  // <-- Account updates
    .build()?
```

**Key Observations:**
- Processes **both** instructions AND account updates
- Uses `.account()` builder method for account state changes
- Same decoder can handle both instruction and account data
- Framework routes data to appropriate processors

---

## Section 2: Carbon Decoder Capabilities

### 2.1 Decoder: `pump-swap-decoder`

**Location:** `decoders/pump-swap-decoder/src/instructions/mod.rs`

#### Instruction Enum

```rust
#[derive(
    carbon_core::InstructionType,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Hash,
)]
pub enum PumpSwapInstruction {
    Buy(buy::Buy),
    Sell(sell::Sell),
    CreatePool(create_pool::CreatePool),
    // ... 30+ instruction types
    BuyEvent(buy_event::BuyEvent),      // Events from inner instructions
    SellEvent(sell_event::SellEvent),   // Events from inner instructions
    CreatePoolEvent(create_pool_event::CreatePoolEvent),
    // ... all event variants
}
```

**Key Observations:**
- Single enum contains **both instructions AND events**
- Events are emitted via CPI logs (anchor events)
- Decoder automatically parses all variants

#### Discriminator Handling

**File:** `decoders/pump-swap-decoder/src/instructions/buy.rs`

```rust
#[derive(CarbonDeserialize, Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
#[carbon(discriminator = "0x66063d1201daebea")]
pub struct Buy {
    pub base_amount_out: u64,
    pub max_quote_amount_in: u64,
    pub track_volume: OptionBool,
}
```

**File:** `decoders/pump-swap-decoder/src/instructions/sell.rs`

```rust
#[derive(CarbonDeserialize, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone, Hash)]
#[carbon(discriminator = "0x33e685a4017f83ad")]
pub struct Sell {
    pub base_amount_in: u64,
    pub min_quote_amount_out: u64,
}
```

**Key Observations:**
- Discriminators are **embedded in the type definition** via `#[carbon(discriminator = "...")]`
- No manual hex string matching required
- Framework uses `CarbonDeserialize` trait to match discriminators automatically
- `try_decode_instructions!` macro in `mod.rs` handles the matching logic

#### Decoder Implementation

**File:** `decoders/pump-swap-decoder/src/instructions/mod.rs`

```rust
impl carbon_core::instruction::InstructionDecoder<'_> for PumpSwapDecoder {
    type InstructionType = PumpSwapInstruction;

    fn decode_instruction(
        &self,
        instruction: &solana_instruction::Instruction,
    ) -> Option<DecodedInstruction<Self::InstructionType>> {
        if !instruction.program_id.eq(&PROGRAM_ID) {
            return None;
        }
        
        // Special case: handle legacy Buy instruction format
        let instruction = if !instruction.data.is_empty()
            && instruction.data[..8] == *buy::Buy::DISCRIMINATOR
            && instruction.data.len() == 24
        {
            let mut data = instruction.data.clone();
            data.push(0);
            &Instruction { ... }
        } else {
            instruction
        };
        
        carbon_core::try_decode_instructions!(instruction,
            PumpSwapInstruction::Buy => buy::Buy,
            PumpSwapInstruction::Sell => sell::Sell,
            PumpSwapInstruction::BuyEvent => buy_event::BuyEvent,
            PumpSwapInstruction::SellEvent => sell_event::SellEvent,
            // ... all variants
        )
    }
}
```

**Key Observations:**
- `try_decode_instructions!` macro handles discriminator matching
- Checks program ID first (early exit optimization)
- Returns `Option<DecodedInstruction<T>>` (None if no match)
- The macro internally compares `instruction.data[..8]` against each discriminator
- **No manual hex string comparison in user code**

---

### 2.2 Decoder: `pumpfun-decoder`

**Location:** `decoders/pumpfun-decoder/src/instructions/mod.rs`

#### Instruction Enum

```rust
pub enum PumpfunInstruction {
    Buy(buy::Buy),
    Sell(sell::Sell),
    Create(create::Create),
    CreateV2(create_v2::CreateV2),
    // ... instructions
    TradeEvent(trade_event::TradeEvent),
    CompleteEvent(complete_event::CompleteEvent),
    CreateEvent(create_event::CreateEvent),
    // ... events
}
```

**Key Observations:**
- Same pattern: instructions + events in single enum
- Events come from anchor program logs (emitted during CPIs)
- Framework automatically captures and decodes events

---

### 2.3 Decoder: `moonshot-decoder`

**Location:** `decoders/moonshot-decoder/src/instructions/mod.rs`

#### Instruction Enum

```rust
pub enum MoonshotInstruction {
    TokenMint(token_mint::TokenMint),
    Buy(buy::Buy),
    Sell(sell::Sell),
    MigrateFunds(migrate_funds::MigrateFunds),
    ConfigInit(config_init::ConfigInit),
    ConfigUpdate(config_update::ConfigUpdate),
    TradeEvent(trade_event::TradeEvent),      // Event
    MigrationEvent(migration_event::MigrationEvent),  // Event
}
```

**Key Observations:**
- Includes `TradeEvent` and `MigrationEvent` (CPI-emitted events)
- Same automatic event capture pattern

---

### 2.4 Decoder: `bonkswap-decoder`

**Location:** `decoders/bonkswap-decoder/src/lib.rs`

```rust
pub struct BonkswapDecoder;
pub const PROGRAM_ID: Pubkey = 
    solana_pubkey::Pubkey::from_str_const("BSwp6bEBihVLdqJRKGgzjcGLHkcTuzmSo1TQkHepzH8p");

pub mod accounts;
pub mod instructions;
pub mod types;
#[cfg(feature = "graphql")]
pub mod graphql;
```

**Key Observations:**
- Auto-generated from IDL using Codama
- Follows same decoder pattern as other decoders

---

### 2.5 Decoder: `jupiter-dca-decoder`

**Location:** `decoders/jupiter-dca-decoder/src/lib.rs`

```rust
pub struct JupiterDcaDecoder;
pub const PROGRAM_ID: Pubkey = 
    Pubkey::from_str_const("DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M");
```

**Key Observations:**
- Same pattern across all decoders
- Program ID embedded as constant
- Framework uses this for filtering

---

## Section 3: Carbon Framework Core

### 3.1 How Instructions Are Processed

**File:** `crates/core/src/instruction.rs`

#### InstructionProcessorInputType

```rust
pub type InstructionProcessorInputType<T> = (
    InstructionMetadata,              // [0] Transaction context
    DecodedInstruction<T>,            // [1] Decoded instruction data
    NestedInstructions,               // [2] All inner (CPI) instructions
    solana_instruction::Instruction,  // [3] Raw instruction
);
```

**Key Point:** Position [2] `NestedInstructions` contains ALL nested instructions (CPIs) for this transaction.

#### NestedInstruction Structure

```rust
#[derive(Debug, Clone)]
pub struct NestedInstruction {
    pub metadata: InstructionMetadata,
    pub instruction: solana_instruction::Instruction,
    pub inner_instructions: NestedInstructions,  // <-- Recursive!
}

#[derive(Debug, Default)]
pub struct NestedInstructions(pub Vec<NestedInstruction>);
```

**Key Observations:**
- **Recursive structure**: Each instruction can contain nested instructions
- Framework builds this tree automatically from `meta.inner_instructions`
- Processors receive the full nested tree

#### How the Pipeline Builds Nested Instructions

**File:** `crates/core/src/transformers.rs`

```rust
pub fn extract_instructions_with_metadata(
    transaction_metadata: &Arc<TransactionMetadata>,
    transaction_update: &TransactionUpdate,
) -> CarbonResult<Vec<(InstructionMetadata, solana_instruction::Instruction)>> {
    let message = &transaction_update.transaction.message;
    let meta = &transaction_update.meta;
    let mut instructions_with_metadata = Vec::with_capacity(32);

    match message {
        VersionedMessage::Legacy(legacy) => {
            process_instructions(
                &legacy.account_keys,
                &legacy.instructions,
                &meta.inner_instructions,  // <-- Passed to processor
                transaction_metadata,
                &mut instructions_with_metadata,
                |_, idx| legacy.is_maybe_writable(idx, None),
                |_, idx| legacy.is_signer(idx),
            );
        }
        VersionedMessage::V0(v0) => {
            // ... handles Address Lookup Tables (ALTs)
            process_instructions(
                &account_keys,
                &v0.instructions,
                &meta.inner_instructions,  // <-- Also passed here
                transaction_metadata,
                &mut instructions_with_metadata,
                // ...
            );
        }
    }

    Ok(instructions_with_metadata)
}
```

**Key Observations:**
- Framework extracts `meta.inner_instructions` automatically
- `process_instructions` iterates through BOTH outer and inner instructions
- Each inner instruction gets its own `InstructionMetadata` with `stack_height`

#### Inner Instruction Processing

```rust
fn process_instructions<F1, F2>(
    account_keys: &[Pubkey],
    instructions: &[CompiledInstruction],
    inner: &Option<Vec<InnerInstructions>>,
    transaction_metadata: &Arc<TransactionMetadata>,
    result: &mut Vec<(InstructionMetadata, solana_instruction::Instruction)>,
    is_writable: F1,
    is_signer: F2,
) {
    // STEP 1: Process outer (top-level) instructions
    for (i, compiled_instruction) in instructions.iter().enumerate() {
        result.push((
            InstructionMetadata {
                transaction_metadata: transaction_metadata.clone(),
                stack_height: 1,
                index: i as u32,
                absolute_path: vec![i as u8],
            },
            build_instruction(account_keys, compiled_instruction, &is_writable, &is_signer),
        ));

        // STEP 2: Process inner (CPI) instructions for this outer instruction
        if let Some(inner_instructions) = inner {
            for inner_tx in inner_instructions {
                if inner_tx.index as usize == i {
                    let mut path_stack = [0; MAX_INSTRUCTION_STACK_DEPTH];
                    path_stack[0] = inner_tx.index;
                    let mut prev_height = 0;

                    for inner_inst in &inner_tx.instructions {
                        let stack_height = inner_inst.stack_height.unwrap_or(1) as usize;
                        // ... tracks nested depth
                        
                        result.push((
                            InstructionMetadata {
                                transaction_metadata: transaction_metadata.clone(),
                                stack_height: stack_height as u32,
                                index: inner_tx.index as u32,
                                absolute_path: path_stack[..stack_height].into(),
                            },
                            build_instruction(
                                account_keys,
                                &inner_inst.instruction,
                                &is_writable,
                                &is_signer,
                            ),
                        ));
                    }
                }
            }
        }
    }
}
```

**Key Observations:**
- **Iterates through ALL inner instructions**
- Tracks `stack_height` to represent nesting depth
- Each inner instruction gets full `InstructionMetadata`
- Framework handles **all CPIs automatically**

---

### 3.2 How Decoders Match Instructions

**File:** `crates/core/src/instruction.rs` (trait definition)

```rust
pub trait InstructionDecoder<'a> {
    type InstructionType;

    fn decode_instruction(
        &self,
        instruction: &'a solana_instruction::Instruction,
    ) -> Option<DecodedInstruction<Self::InstructionType>>;
}
```

**File:** `crates/core/src/pipeline.rs` (how pipeline uses decoders)

```rust
async fn run(&mut self) -> CarbonResult<()> {
    loop {
        match update {
            Update::Transaction(transaction) => {
                // Extract ALL instructions (outer + inner)
                let instructions = extract_instructions_with_metadata(&metadata, &transaction)?;
                
                // Convert to nested structure
                let nested_instructions: NestedInstructions = instructions.into();
                
                // Pass to each instruction pipe
                for pipe in &mut self.instruction_pipes {
                    pipe.run(&nested_instruction, metrics.clone()).await?;
                }
            }
        }
    }
}
```

#### InstructionPipe Implementation

```rust
impl<T: Send + 'static> InstructionPipes<'_> for InstructionPipe<T> {
    async fn run(
        &mut self,
        nested_instruction: &NestedInstruction,
        metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        // Try to decode this instruction
        if let Some(decoded_instruction) = self
            .decoder
            .decode_instruction(&nested_instruction.instruction)
        {
            // FOUND A MATCH! Process it.
            self.processor
                .process(
                    (
                        nested_instruction.metadata.clone(),
                        decoded_instruction,
                        nested_instruction.inner_instructions.clone(),
                        nested_instruction.instruction.clone(),
                    ),
                    metrics.clone(),
                )
                .await?;
        }

        // Recursively process ALL inner instructions
        for inner in &nested_instruction.inner_instructions.0 {
            self.run(inner, metrics.clone()).await?;
        }

        Ok(())
    }
}
```

**Key Observations:**
- **Recursive processing**: Processes current instruction, then all inner instructions
- Each decoder attempts to decode each instruction
- If match found (program ID + discriminator), processor is called
- **Automatic CPI recursion** - no manual iteration required in user code

---

## Section 4: SolFlow Current Implementation

### 4.1 Unified Streamer Entry Point

**Location:** `examples/solflow/src/bin/unified_streamer.rs`

#### Initialization

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let runtime_config = RuntimeConfig::from_env()?;
    
    // Initialize the instruction scanner
    let scanner = InstructionScanner::new();

    let config = StreamerConfig {
        program_id: "11111111111111111111111111111111".to_string(), // Placeholder
        program_name: "Unified".to_string(),
        output_path,
        backend,
        pipeline_tx: None,
    };

    run_unified(config, scanner).await
}
```

**Key Observations:**
- **Does NOT use Carbon pipeline builder**
- Uses custom `run_unified()` function
- Uses `InstructionScanner` for manual program filtering
- No decoder/processor pair registration

---

### 4.2 Instruction Scanner

**Location:** `examples/solflow/src/instruction_scanner.rs`

#### Program Registry

```rust
pub struct InstructionScanner {
    tracked_programs: HashSet<Pubkey>,
    program_names: HashMap<Pubkey, &'static str>,
}

impl InstructionScanner {
    pub fn new() -> Self {
        let mut program_names = HashMap::new();

        let pumpfun = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap();
        let pumpswap = Pubkey::from_str("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA").unwrap();
        let bonkswap = Pubkey::from_str("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj").unwrap();
        let moonshot = Pubkey::from_str("MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG").unwrap();
        let jupiter_dca = Pubkey::from_str("DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M").unwrap();

        program_names.insert(pumpfun, "PumpFun");
        program_names.insert(pumpswap, "PumpSwap");
        program_names.insert(bonkswap, "BonkSwap");
        program_names.insert(moonshot, "Moonshot");
        program_names.insert(jupiter_dca, "JupiterDCA");

        let tracked_programs = program_names.keys().copied().collect();

        Self { tracked_programs, program_names }
    }
}
```

**Key Observations:**
- Maintains manual registry of program IDs
- Hardcoded program addresses
- No connection to Carbon decoders
- Doesn't use discriminator information

#### Scanning Logic

```rust
pub fn scan(&self, metadata: &Arc<TransactionMetadata>) -> Option<InstructionMatch> {
    let account_keys = build_full_account_keys(metadata, &metadata.meta);

    // STEP 1: Check outer (top-level) instructions
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

    // STEP 2: Check inner (CPI) instructions
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

    None
}
```

**Key Observations:**
- **Manually iterates** through `metadata.message.instructions()`
- **Manually iterates** through `metadata.meta.inner_instructions`
- Only checks **program ID** (no discriminator matching)
- Returns on **first match** (early exit)
- **Does NOT decode** instruction data
- **Does NOT determine instruction type** (Buy vs Sell vs CreateEvent, etc.)

#### Boolean Helper

```rust
pub fn is_pump_relevant(&self, metadata: &Arc<TransactionMetadata>) -> bool {
    self.scan(metadata).is_some()
}
```

**Key Observations:**
- Simple wrapper for boolean check
- Used in trade extraction pipeline
- Only answers "contains tracked program?" not "what instruction types?"

---

### 4.3 Trade Extraction (Balance-Based)

**Location:** `examples/solflow/src/trade_extractor.rs`

#### Approach

SolFlow uses **balance delta analysis** instead of instruction decoding:

```rust
pub fn extract_user_volumes(
    sol_deltas: &[BalanceDelta],
    token_deltas: &[BalanceDelta],
) -> Option<(f64, f64, String, u8, TradeKind)> {
    // Find user account (largest negative SOL change)
    let user_idx = find_user_account(sol_deltas)?;
    let user_sol_delta = sol_deltas.iter().find(|d| d.account_index == user_idx)?;

    let sol_volume = user_sol_delta.abs_ui_change();
    let direction = determine_trade_direction(user_sol_delta);

    let token_mint = find_primary_token_mint(token_deltas)?;
    let user_token_delta = token_deltas
        .iter()
        .filter(|d| d.mint == token_mint)
        .max_by_key(|d| d.raw_change.abs())?;

    Some((sol_volume, token_volume, token_mint, decimals, direction))
}
```

**Key Observations:**
- Infers trade from **pre/post balance changes**
- Direction determined by SOL delta (negative = buy, positive = sell)
- **Ignores instruction discriminators** entirely
- Cannot distinguish between:
  - Direct swaps vs CPI swaps
  - Failed instructions vs successful ones
  - Different trade types (limit orders, market orders, etc.)
- **Does NOT capture events** (CreateEvent, CompleteEvent, etc.)

---

### 4.4 How SolFlow Uses Transactions

**Flow:**
1. gRPC receives `TransactionUpdate`
2. `InstructionScanner.is_pump_relevant()` checks if transaction contains tracked program
3. If relevant: extract balance deltas from `meta.pre_balances` / `meta.post_balances`
4. Infer trade direction from balance changes
5. Write to database/JSONL

**What's Missing:**
- No use of Carbon decoders
- No discriminator matching
- No instruction type identification
- No event capture (CreateEvent, TradeEvent, etc.)
- No structured data from instruction payloads

---

## Section 5: Comparative Summary

### 5.1 Initialization

| Aspect | Carbon Examples | SolFlow |
|--------|----------------|---------|
| **Pattern** | Declarative pipeline builder | Manual gRPC streaming |
| **Datasource** | `.datasource(helius_websocket)` | Custom `run_unified()` |
| **Decoder** | `.instruction(Decoder, Processor)` | None - uses manual scanner |
| **CPI Handling** | Automatic (framework) | Manual iteration |
| **Type Safety** | Strongly-typed enums | Raw program ID checks |

---

### 5.2 Instruction Detection

| Aspect | Carbon Examples | SolFlow |
|--------|----------------|---------|
| **Method** | Enum pattern matching | Program ID filtering |
| **Discriminator** | Automatic via `#[carbon(discriminator)]` | Not used |
| **Data Extraction** | Parsed into structs | Balance delta analysis |
| **Instruction Type** | Known (Buy, Sell, CreateEvent, etc.) | Unknown (only program match) |
| **Events** | Captured automatically | Not captured |

**Example:**

**Carbon:**
```rust
match pumpswap_instruction {
    PumpSwapInstruction::Buy(buy) => { /* strongly typed */ }
    PumpSwapInstruction::Sell(sell) => { /* strongly typed */ }
    PumpSwapInstruction::BuyEvent(buy_event) => { /* event from CPI */ }
}
```

**SolFlow:**
```rust
if scanner.scan(metadata).is_some() {
    // Only know: "transaction contains tracked program"
    // Don't know: which instruction type, what data, etc.
}
```

---

### 5.3 CPI (Inner Instruction) Handling

| Aspect | Carbon Framework | SolFlow |
|--------|------------------|---------|
| **Mechanism** | Automatic recursion via `InstructionPipes` | Manual iteration in `scan()` |
| **Coverage** | All nested depths | Single level check |
| **Data Access** | Full `NestedInstructions` tree | Only program ID |
| **Event Capture** | Yes (anchor events) | No |
| **Recursion** | Framework handles | User must implement |

**Evidence from Jupiter Example:**
```rust
JupiterSwapInstruction::Route(route) => {
    assert!(!nested_instructions.is_empty()); // Proves CPIs are captured
}
```

**Evidence from SolFlow:**
```rust
// STEP 2: Check inner (CPI) instructions
if let Some(inner_groups) = &metadata.meta.inner_instructions {
    for inner_group in inner_groups {
        // Manual iteration - no framework support
    }
}
```

---

### 5.4 Discriminator Matching

| Aspect | Carbon Decoders | SolFlow |
|--------|----------------|---------|
| **Storage** | `#[carbon(discriminator = "0x...")]` attribute | Not stored |
| **Matching** | `try_decode_instructions!` macro | Not performed |
| **Validation** | Checks discriminator + program ID | Only program ID |
| **Fallback** | Returns `None` if no match | N/A (doesn't decode) |

**Example from `pump-swap-decoder`:**
```rust
#[carbon(discriminator = "0x66063d1201daebea")]
pub struct Buy { ... }

#[carbon(discriminator = "0x33e685a4017f83ad")]
pub struct Sell { ... }
```

**SolFlow Approach:**
```rust
// No discriminator checking - only program ID
if self.tracked_programs.contains(program_id) {
    return Some(InstructionMatch { ... });
}
```

---

### 5.5 Data Extraction Methods

| Method | Carbon | SolFlow |
|--------|--------|---------|
| **Instruction Payloads** | Parsed via `CarbonDeserialize` | Not parsed |
| **Balance Changes** | Available via `TransactionMetadata` | Primary data source |
| **Events** | Captured from CPI logs | Not captured |
| **Account Data** | Available via account processors | Not used |
| **Type Safety** | Strong (Rust enums) | Weak (inferred from balances) |

---

### 5.6 What Each Approach Can Detect

#### Carbon Framework Can Detect:
✅ Outer instruction types (Buy, Sell, CreatePool, etc.)  
✅ Inner instruction types (via CPI recursion)  
✅ Events (CreateEvent, TradeEvent, CompleteEvent, etc.)  
✅ Instruction parameters (amounts, slippage, flags, etc.)  
✅ Failed vs successful instructions  
✅ Account state changes  
✅ Multi-program interactions (Jupiter routing through Raydium, etc.)  

#### SolFlow Can Detect:
✅ Presence of tracked program (any of 5 programs)  
✅ SOL and token balance changes  
✅ Inferred trade direction (buy vs sell)  
✅ User account (largest negative SOL delta)  
✅ Primary token mint  

#### SolFlow CANNOT Detect:
❌ Specific instruction type (only knows program ID)  
❌ Instruction parameters  
❌ Events (CreateEvent, CompleteEvent, etc.)  
❌ Failed instructions (relies on balance changes)  
❌ Complex multi-program flows  
❌ Limit orders vs market orders  
❌ CPI-only transactions (no outer instruction match)  

---

## Section 6: Blind Spots Analysis

### 6.1 Potential Blind Spots in SolFlow

#### **1. CPI-Only Transactions**

**Scenario:** A transaction where the tracked program is ONLY called via CPI (no outer instruction).

**Example:** Jupiter routing that calls PumpSwap internally:
- Outer: `JupiterSwapInstruction::Route`
- Inner (CPI): `PumpSwapInstruction::Buy`

**SolFlow Behavior:**
- `scan()` would detect the inner PumpSwap instruction
- Returns `InstructionPath::Inner { ... }`
- But: no instruction type information extracted
- Balance extraction would work (since balance changed)

**Risk:** Medium - Transaction captured, but metadata incomplete.

---

#### **2. Failed Instructions**

**Scenario:** A swap instruction that fails (reverted transaction).

**Carbon Behavior:**
- Still decodes the instruction
- Can check `meta.err` field
- Can distinguish failed vs successful

**SolFlow Behavior:**
- Balance extraction shows no changes
- Trade not recorded (filtered out by balance delta check)
- Transaction ignored

**Risk:** Low - Failed trades shouldn't be recorded anyway.

---

#### **3. Event-Only Data**

**Scenario:** Anchor program emits `CreateEvent` with token metadata.

**Carbon Behavior:**
```rust
PumpfunInstruction::CreateEvent(create_event) => {
    log::info!("New token created: {:#?}", create_event);
    // Contains: name, symbol, uri, creator, etc.
}
```

**SolFlow Behavior:**
- Not captured
- Token creation not detected
- Metadata (name, symbol) not extracted

**Risk:** High for token launch detection - SolFlow doesn't track token creates.

---

#### **4. Multi-Program Interactions**

**Scenario:** Jupiter swap routing through PumpSwap + Raydium.

**Carbon Behavior:**
```rust
JupiterSwapInstruction::Route(route) => {
    // nested_instructions contains:
    // - PumpSwapInstruction::Sell
    // - RaydiumAmmV4Instruction::SwapBaseIn
}
```

**SolFlow Behavior:**
- `scan()` returns on first match (Jupiter OR PumpSwap)
- Doesn't track multi-program hops
- Cannot reconstruct full routing path

**Risk:** Medium - Can still extract final balance changes, but loses routing information.

---

#### **5. Discriminator Variants**

**Scenario:** Program has multiple instruction types with similar balance effects.

**Example:** PumpSwap `Buy` vs `BuyWithReferral`:
- Both result in: SOL out, Token in
- Different discriminators
- Different instruction parameters

**Carbon Behavior:**
- Distinguishes via discriminator
- Extracts referral info from `BuyWithReferral`

**SolFlow Behavior:**
- Both look like "Buy" (negative SOL, positive token)
- Cannot distinguish instruction variants
- Referral data lost

**Risk:** Medium - Loses granularity on instruction types.

---

### 6.2 Advantages of SolFlow Approach

Despite the blind spots, SolFlow's balance-based approach has benefits:

1. **Simplicity:** No need to maintain decoders for each program
2. **Resilience:** Works even if program IDL changes (balance deltas remain stable)
3. **Universal:** Can handle any swap protocol (even unknown ones)
4. **Performance:** No instruction deserialization overhead

However, these come at the cost of **semantic information loss**.

---

## Section 7: Recommendations

### 7.1 Hybrid Approach

**Proposal:** Use Carbon decoders for semantic richness, keep balance extraction as fallback.

```rust
// Use Carbon pipeline with decoders
carbon_core::pipeline::Pipeline::builder()
    .datasource(yellowstone_grpc)
    .instruction(PumpSwapDecoder, PumpSwapProcessor)
    .instruction(PumpfunDecoder, PumpfunProcessor)
    .instruction(BonkswapDecoder, BonkswapProcessor)
    .instruction(MoonshotDecoder, MoonshotProcessor)
    .instruction(JupiterDcaDecoder, JupiterDcaProcessor)
    .build()?
    .run()
    .await?;

// Processor implementation
impl Processor for PumpSwapProcessor {
    async fn process(&mut self, data: Self::InputType, ...) -> CarbonResult<()> {
        let (metadata, decoded_instruction, nested_instructions, _) = data;
        
        match decoded_instruction.data {
            PumpSwapInstruction::Buy(buy) => {
                // PRIMARY: Extract from instruction payload
                let base_amount = buy.base_amount_out;
                let max_quote = buy.max_quote_amount_in;
                
                // FALLBACK: Verify with balance deltas
                let balance_deltas = extract_balance_deltas(&metadata);
                
                // Store both sources
                store_trade(TradeRecord {
                    instruction_type: "Buy",
                    base_amount_out: base_amount,
                    balance_verified: balance_deltas.matches(base_amount),
                    // ...
                });
            }
            // ... other variants
        }
    }
}
```

**Benefits:**
- Semantic richness from decoders
- Balance verification as sanity check
- Fallback for unknown programs
- Full CPI coverage

---

### 7.2 Migration Path

#### Phase 1: Add Carbon Pipeline (Non-Breaking)
- Create new `pipeline_runtime` binary (already exists)
- Keep existing `unified_streamer` running
- Run both in parallel (dual ingestion)

#### Phase 2: Integrate Decoders
- Register all 5 decoders in pipeline
- Implement processors for each program
- Compare output against balance-based extraction

#### Phase 3: Sunset Manual Scanning
- Once validated, deprecate `unified_streamer`
- Keep `InstructionScanner` as utility for filtering
- Use Carbon as primary source

---

### 7.3 What to Keep from SolFlow

**Keep:**
- Balance delta extraction (useful for verification)
- `MIN_SOL_DELTA` filtering (noise reduction)
- Trade direction inference (useful heuristic)
- Account key building (`build_full_account_keys`)

**Replace:**
- Manual instruction iteration → Carbon's automatic recursion
- Program ID filtering → Decoder-based detection
- Balance-only trade extraction → Instruction payload parsing

---

## Section 8: Conclusion

### Key Findings

1. **Carbon Framework provides automatic CPI handling** via recursive `InstructionPipes`
2. **Carbon Decoders use discriminator matching** embedded in type definitions
3. **Reference examples prove that nested instructions are automatically captured**
4. **SolFlow currently uses manual iteration** and program ID filtering only
5. **SolFlow's balance-based approach loses semantic information** (instruction types, events, parameters)

### Answer to Original Questions

#### Q1: How do reference examples initialize the Carbon pipeline?
**A:** Via `.instruction(Decoder, Processor)` builder pattern - declarative, no manual parsing.

#### Q2: How do they identify "Swap" events?
**A:** Via enum pattern matching on decoded instructions - no manual byte matching.

#### Q3: How do they handle Inner Instructions (CPIs)?
**A:** Framework handles automatically via `NestedInstructions` recursion - no manual iteration needed.

#### Q4: What instruction Enums are exposed?
**A:** Each decoder exposes an enum (e.g., `PumpSwapInstruction`) with all instruction and event variants.

#### Q5: Do decoders parse discriminators automatically?
**A:** Yes - via `#[carbon(discriminator = "0x...")]` attribute and `try_decode_instructions!` macro.

#### Q6: Is SolFlow using Carbon's InstructionProcessor trait?
**A:** No - SolFlow uses custom streaming with manual instruction iteration.

#### Q7: Is SolFlow iterating through `meta.inner_instructions` manually?
**A:** Yes - in `InstructionScanner.scan()` method.

#### Q8: Is SolFlow manually matching hex discriminators?
**A:** No - SolFlow doesn't match discriminators at all, only program IDs.

---

### Architectural Gap Summary

| Component | Carbon Framework | SolFlow Current |
|-----------|------------------|-----------------|
| Pipeline | Declarative builder | Manual gRPC loop |
| Decoders | Trait-based, automatic | Not used |
| Discriminators | Embedded in types | Not checked |
| CPIs | Automatic recursion | Manual iteration |
| Events | Captured automatically | Not captured |
| Type Safety | Strong (enums) | Weak (program ID only) |
| Instruction Data | Parsed into structs | Not parsed |

**Verdict:** SolFlow is **functionally working** but **architecturally divergent** from Carbon patterns. It successfully detects transactions but loses semantic richness by not using Carbon's decoder infrastructure.

---

**Document Status:** COMPLETE  
**Next Steps:** Review findings, decide on migration strategy, and align SolFlow with Carbon framework patterns.
