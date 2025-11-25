# Unified Streamer Usage Guide

## Quick Start

The unified streamer replaces the 4 individual program streamers (PumpSwap, BonkSwap, Moonshot, Jupiter DCA) with a single binary that tracks all 5 programs including PumpFun.

### Basic Usage

```bash
# Run with default settings (JSONL backend)
cargo run --release --bin unified_streamer

# Run with SQLite backend
export SOLFLOW_DB_PATH="/var/lib/solflow/solflow.db"
cargo run --release --bin unified_streamer
```

### Required Environment Variables

```bash
# Geyser gRPC endpoint (REQUIRED)
export GEYSER_URL="https://your-geyser-endpoint.com"

# Optional authentication token
export X_TOKEN="your-auth-token"
```

### Optional Environment Variables

```bash
# Database path (enables SQLite backend)
export SOLFLOW_DB_PATH="/var/lib/solflow/solflow.db"

# Commitment level (default: Confirmed)
export COMMITMENT_LEVEL="Confirmed"  # Options: Finalized, Confirmed, Processed

# Logging level
export RUST_LOG="info"
# Or for detailed scanner logs:
export RUST_LOG="solflow::instruction_scanner=debug,solflow::streamer_core::lib=info"

# JSONL settings
export ENABLE_JSONL="true"           # Enable JSONL writes (default: false)
export OUTPUT_MAX_SIZE_MB="100"      # Max file size before rotation
export OUTPUT_MAX_ROTATIONS="10"     # Number of rotated files to keep
export UNIFIED_OUTPUT_PATH="streams/unified/events.jsonl"
```

### Enable Pipeline Integration

```bash
# Run as part of pipeline runtime (Phase 4.2)
export ENABLE_PIPELINE="true"
cargo run --release --bin unified_streamer
```

## Example Commands

### Development Mode (Debug Logging)

```bash
SOLFLOW_DB_PATH=/var/lib/solflow/solflow.db \
GEYSER_URL="https://api.mainnet-beta.solana.com" \
RUST_LOG="solflow::instruction_scanner=debug,solflow::streamer_core::lib=info" \
cargo run --release --bin unified_streamer
```

### Production Mode (Minimal Logging)

```bash
SOLFLOW_DB_PATH=/var/lib/solflow/solflow.db \
GEYSER_URL="https://api.mainnet-beta.solana.com" \
RUST_LOG="error" \
cargo run --release --bin unified_streamer
```

### JSONL Backend with Rotation

```bash
GEYSER_URL="https://api.mainnet-beta.solana.com" \
ENABLE_JSONL="true" \
UNIFIED_OUTPUT_PATH="streams/unified/events.jsonl" \
OUTPUT_MAX_SIZE_MB="100" \
OUTPUT_MAX_ROTATIONS="10" \
RUST_LOG="info" \
cargo run --release --bin unified_streamer
```

### Validation Mode (Compare with Old Streamers)

Terminal 1 - Unified Streamer:
```bash
SOLFLOW_DB_PATH=/var/lib/solflow/unified.db \
GEYSER_URL="https://api.mainnet-beta.solana.com" \
RUST_LOG="info" \
cargo run --release --bin unified_streamer
```

Terminal 2 - PumpSwap Streamer (for comparison):
```bash
SOLFLOW_DB_PATH=/var/lib/solflow/pumpswap.db \
GEYSER_URL="https://api.mainnet-beta.solana.com" \
RUST_LOG="info" \
cargo run --release --bin pumpswap_streamer
```

## What to Expect

### Startup Logs

```
🚀 Starting Unified SolFlow Streamer
   Tracked Programs: 5 (PumpFun, PumpSwap, BonkSwap, Moonshot, Jupiter DCA)
   gRPC Filter: Multi-program subscription
   Coverage: Outer + Inner (CPI) instructions
   Geyser URL: https://api.mainnet-beta.solana.com
   Commitment: Confirmed
📋 InstructionScanner initialized with 5 programs
   ├─ PumpFun: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
   ├─ PumpSwap: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
   ├─ BonkSwap: LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj
   ├─ Moonshot: MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG
   └─ JupiterDCA: DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M
💾 SQLite backend: /var/lib/solflow/solflow.db
🔗 Creating multi-program gRPC client
   Filtering: 5 tracked programs (outer + inner instructions)
✅ Connected to gRPC server (multi-program filter)
```

### Match Logs (Validation Period)

```
✅ Matched PumpSwap at Outer { index: 0 } (signature: 5KJ4d...)
✅ Matched PumpFun at Inner { outer_index: 0, inner_path: [1] } (signature: 3hR2b...)
✅ Matched Moonshot at Outer { index: 0 } (signature: 2aB9c...)
```

### Performance Logs

```
📊 Pipeline ingestion: 10000 trades sent
📊 Pipeline ingestion: 20000 trades sent
```

## Tracked Programs

| Program | Program ID | Detection |
|---------|-----------|-----------|
| **PumpFun** | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` | ✅ Outer + Inner |
| **PumpSwap** | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | ✅ Outer + Inner |
| **BonkSwap** | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | ✅ Outer + Inner |
| **Moonshot** | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | ✅ Outer + Inner |
| **Jupiter DCA** | `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M` | ✅ Outer + Inner |

## Coverage Improvements

### Before Unified Streamer

- ❌ **PumpFun**: Completely missed (no streamer existed)
- ⚠️ **Inner Instructions**: Missed when programs called via aggregators
- ⚠️ **Example**: PumpSwap trade through Jupiter Router → **MISSED**

### After Unified Streamer

- ✅ **PumpFun**: Full coverage (outer + inner)
- ✅ **Inner Instructions**: All CPI calls detected
- ✅ **Example**: PumpSwap trade through Jupiter Router → **DETECTED**

## Troubleshooting

### Error: "Missing environment variable: GEYSER_URL"

**Solution**: Set the GEYSER_URL environment variable:
```bash
export GEYSER_URL="https://api.mainnet-beta.solana.com"
```

### Error: "InvalidValue: program_id must be 32-44 characters"

**Solution**: This error should not occur with the unified_streamer (fixed in commit 277ce5c6). If you see it, make sure you're running the latest version:
```bash
git pull
cargo build --release --bin unified_streamer
```

### No matches detected

**Diagnostic**:
1. Check if transactions contain tracked programs:
   ```bash
   export RUST_LOG="solflow::instruction_scanner=debug"
   cargo run --release --bin unified_streamer
   ```

2. Look for `⏭️  No tracked program matched` logs

3. Verify gRPC connection:
   ```bash
   # Should see "✅ Connected to gRPC server"
   ```

### High memory usage

**Solution**: The unified streamer processes more transactions than individual streamers. Consider:
- Disabling JSONL writes: `ENABLE_JSONL="false"`
- Increasing rotation frequency: `OUTPUT_MAX_SIZE_MB="50"`
- Using SQLite backend for better memory efficiency

## Validation Checklist

During the validation period (7-14 days), verify:

- [ ] Unified streamer detects all events from old streamers
- [ ] PumpFun events are captured (new coverage)
- [ ] Inner instruction matches work (e.g., PumpSwap via Jupiter)
- [ ] No regressions in trade detection accuracy
- [ ] Performance is acceptable (<10% throughput impact)
- [ ] Memory usage is stable

## Migration from Old Streamers

### Phase 1: Parallel Run (Week 1)

Run both systems simultaneously:
```bash
# Terminal 1: Unified streamer
SOLFLOW_DB_PATH=/var/lib/solflow/unified.db cargo run --release --bin unified_streamer

# Terminal 2-5: Old streamers
SOLFLOW_DB_PATH=/var/lib/solflow/pumpswap.db cargo run --release --bin pumpswap_streamer
SOLFLOW_DB_PATH=/var/lib/solflow/bonkswap.db cargo run --release --bin bonkswap_streamer
SOLFLOW_DB_PATH=/var/lib/solflow/moonshot.db cargo run --release --bin moonshot_streamer
SOLFLOW_DB_PATH=/var/lib/solflow/jupiter_dca.db cargo run --release --bin jupiter_dca_streamer
```

### Phase 2: Compare Results (Week 2)

Query databases and compare event counts:
```sql
-- Unified database
SELECT program_name, COUNT(*) FROM trades GROUP BY program_name;

-- Individual databases
SELECT 'PumpSwap' as program, COUNT(*) FROM trades; -- pumpswap.db
SELECT 'BonkSwap' as program, COUNT(*) FROM trades; -- bonkswap.db
-- etc.
```

### Phase 3: Switch Over (Week 3-4)

Once validated, stop old streamers and use only unified_streamer.

## Performance Tips

1. **Use SQLite for production**: More memory efficient than JSONL
2. **Adjust logging**: Set `RUST_LOG="error"` for minimal overhead
3. **Disable JSONL**: Set `ENABLE_JSONL="false"` unless needed
4. **Monitor metrics**: Watch for `📊 Pipeline ingestion` logs

## Support

For issues or questions:
- Check logs with `RUST_LOG="debug"`
- Review architecture docs: `docs/20251126-unified-instruction-scanner-architecture.md`
- File issues on the repository
