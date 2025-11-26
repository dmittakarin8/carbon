# Mint Trace - Usage Guide

**Tool:** Comprehensive transaction monitoring for specific token mints  
**Location:** `examples/solflow/src/bin/mint_trace.rs`  
**Branch:** feature/mint-trace-bin

---

## Overview

`mint_trace` is a standalone development tool that monitors ALL transactions involving a specific token mint address. It uses Carbon's TransactionMetadata abstraction to provide complete visibility into:

- Transaction metadata (slot, signature, fee payer, block time)
- All token mints involved in each transaction
- Complete instruction tree (outer + inner/CPI instructions)
- All program IDs invoked
- SOL and token balance changes
- Transaction status and fees

**Use Cases:**
- Debugging token behavior on-chain
- Monitoring new token launches
- Tracking specific wallet interactions with a mint
- Validating DEX integrations
- Investigating suspicious transactions

---

## Quick Start

### 1. Build the Binary

```bash
cd examples/solflow
cargo build --release --bin mint_trace
```

### 2. Set Environment Variables

```bash
# gRPC endpoint (Yellowstone-compatible)
export GEYSER_URL="http://127.0.0.1:10000"

# Optional: Authentication token
export GEYSER_TOKEN="your-token-here"

# Optional: Commitment level (default: confirmed)
export COMMITMENT_LEVEL="confirmed"  # processed | confirmed | finalized

# Optional: Rust log level
export RUST_LOG="info"  # debug | info | warn | error
```

### 3. Run Against a Mint

```bash
# Example: Monitor USDC transactions (console only)
cargo run --release --bin mint_trace -- --mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v

# Example: Monitor Wrapped SOL (console only)
cargo run --release --bin mint_trace -- --mint So11111111111111111111111111111111111111112

# Example: Monitor with log file (console + file)
cargo run --release --bin mint_trace -- --mint <MINT> --log-file mint_trace.log

# Example: Monitor trending token with log file
cargo run --release --bin mint_trace -- --mint <TRENDING_TOKEN_MINT> --log-file /var/log/mint_trace.log
```

### 4. Stop Monitoring

Press `CTRL+C` to gracefully shutdown.

---

## Log File Mode

When `--log-file` is specified, the tool writes ALL transaction details to both the console and the specified file.

### What Gets Logged

The log file contains the complete, structured output for each matching transaction:

✅ **Transaction Metadata**
- Slot number
- Transaction signature
- Fee payer address
- Block time (if available)

✅ **Token Mints**
- All mints involved in the transaction
- TARGET marker for the monitored mint

✅ **Complete Instruction Tree**
- Outer (top-level) instructions with:
  - Program ID
  - Data length
  - Account count
  - Discriminator (first 8 bytes if available)
- Inner (CPI) instructions with:
  - Nested program IDs
  - Data lengths
  - Full nesting structure preserved

✅ **Balance Changes**
- SOL balance deltas (pre/post comparison)
- Token balance deltas with:
  - Mint addresses
  - Decimals
  - UI amounts
  - Account addresses
  - TARGET markers

✅ **Transaction Status**
- Success/failure status
- Transaction fee
- Error details (if failed)

### File Format

The log file uses the same formatted output as console display, preserving:
- Box-drawing characters for visual structure
- Indentation for instruction hierarchy
- Markers (→ TARGET, ← TARGET) for mint identification
- All numerical precision (SOL amounts, token decimals, etc.)

### Example Usage Scenarios

**Scenario 1: Audit Trail**
```bash
# Monitor high-value mint with persistent log
cargo run --release --bin mint_trace -- \
  --mint <HIGH_VALUE_MINT> \
  --log-file /audit/mint_$(date +%Y%m%d).log
```

**Scenario 2: Background Monitoring with Real-Time Tailing**
```bash
# Terminal 1: Run mint_trace with log file
cargo run --release --bin mint_trace -- \
  --mint <MINT> \
  --log-file mint_trace.log

# Terminal 2: Tail the log file
tail -f mint_trace.log
```

**Scenario 3: Long-Running Capture**
```bash
# Capture all activity for 24 hours
timeout 86400 cargo run --release --bin mint_trace -- \
  --mint <MINT> \
  --log-file mint_capture_$(date +%Y%m%d_%H%M%S).log
```

**Scenario 4: Post-Processing Analysis**
```bash
# Run with log file, then analyze
cargo run --release --bin mint_trace -- \
  --mint <MINT> \
  --log-file raw_data.log

# Extract all signatures
grep "Signature:" raw_data.log | awk '{print $2}'

# Count transactions
grep -c "MINT MATCH" raw_data.log

# Find failed transactions
grep -B 5 "❌ FAILED" raw_data.log
```

### Performance Considerations

- **BufWriter**: The logger uses `BufWriter` for efficient file I/O
- **Immediate Flush**: Each transaction block is flushed immediately after writing
- **Append Mode**: File is opened in append mode (safe for restarts)
- **Console Impact**: Writing to console is NOT affected by file logging (independent streams)

### Safety Features

- **Error Handling**: If file cannot be opened, tool exits with clear error message
- **Append Mode**: Existing log files are preserved, new entries appended
- **Flush After Each Transaction**: Data is safely written even if tool crashes
- **No Buffering Issues**: Each complete transaction block is guaranteed to be written

---

## Command-Line Options

### Required Arguments

- `--mint <ADDRESS>` - Token mint address to monitor (base58-encoded Pubkey)

### Optional Arguments

- `--log-file <PATH>` - Log file path for detailed transaction auditing (optional)
  - When provided, all transaction details are written to BOTH console and file
  - File is opened in append mode (creates if doesn't exist)
  - Each transaction block is flushed immediately for safety
  - Includes complete instruction tree (outer + inner + nested CPIs)

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GEYSER_URL` | `http://127.0.0.1:10000` | Yellowstone gRPC endpoint |
| `GEYSER_TOKEN` | (none) | Optional authentication token |
| `COMMITMENT_LEVEL` | `confirmed` | Transaction commitment level |
| `RUST_LOG` | `info` | Log verbosity level |

---

## Output Format

### Header
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                          MINT TRACE - Transaction Monitor                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ Target Mint:  EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v                    ║
║ Geyser URL:   http://127.0.0.1:10000                                          ║
║ Commitment:   Confirmed                                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ This tool monitors ALL transactions involving the target mint address.       ║
║ Press CTRL+C to stop.                                                         ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Transaction Match
For each transaction involving the target mint:

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║ MINT MATCH #1                                                                 ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ Target Mint: EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ 📊 TRANSACTION METADATA                                                       ║
║ Slot:        12345678                                                         ║
║ Signature:   5Xm...abc                                                        ║
║ Fee Payer:   9Jk...xyz                                                        ║
║ Block Time:  1732651234                                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ 🪙 TOKEN MINTS (2)                                                            ║
║   1. EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v → TARGET                   ║
║   2. So11111111111111111111111111111111111111112                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ 📋 INSTRUCTION TREE                                                           ║
║   Total Instructions: 3                                                       ║
║   [0] Outer Instruction                                                       ║
║       Program:  JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB                  ║
║       Data Len: 128 bytes                                                     ║
║       Accounts: 15                                                            ║
║       Discriminator: 0xe445a52e51cb9a1d                                      ║
║   [1] Inner Group (from outer instruction 0)                                  ║
║       [0.0] Program:  TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA            ║
║             Data Len: 36 bytes                                                ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ 💰 BALANCE CHANGES                                                            ║
║   SOL Changes: 2                                                              ║
║     - 0.05 SOL | 9Jk...xyz                                                    ║
║     + 0.04 SOL | 5Ab...def                                                    ║
║                                                                               ║
║   Token Changes: 2                                                            ║
║     - 100.00 tokens (decimals: 6)                                             ║
║       Mint:    EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v ← TARGET         ║
║       Account: 3Cd...ghi                                                      ║
║     + 95.00 tokens (decimals: 6)                                              ║
║       Mint:    EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v ← TARGET         ║
║       Account: 7Ef...jkl                                                      ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║ 📈 TRANSACTION STATUS                                                         ║
║   Status: ✅ SUCCESS                                                          ║
║   Fee:    5000 lamports                                                       ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Real-World Examples

### Example 1: Monitor New Token Launch

```bash
# Scenario: Pump.fun token just launched
# Goal: See every transaction in real-time

export GEYSER_URL="https://your-rpc-provider.com"
export COMMITMENT_LEVEL="confirmed"

cargo run --release --bin mint_trace -- --mint <NEW_TOKEN_MINT>
```

**Expected Output:**
- Initial liquidity add transactions
- First buys from early traders
- Subsequent buy/sell activity
- Full visibility into program interactions (PumpSwap, Jupiter, etc.)

### Example 2: Debug Failing Swaps

```bash
# Scenario: Users report failed swaps for your token
# Goal: Capture failed transactions to analyze error reasons

export RUST_LOG="debug"  # More verbose logging

cargo run --release --bin mint_trace -- --mint <YOUR_TOKEN_MINT> 2>&1 | tee mint_debug.log
```

**What to Look For:**
- Transaction status: ❌ FAILED
- Error field showing specific failure reason
- Instruction tree to identify which program call failed
- Balance changes (or lack thereof) indicating where transfer failed

### Example 3: Track Whale Activity

```bash
# Scenario: Monitor large transactions for a specific mint
# Goal: See when large holders buy/sell

cargo run --release --bin mint_trace -- --mint <WHALE_WATCHED_TOKEN> | \
  grep -A 50 "MINT MATCH" | \
  grep -E "(Token Changes|[+-] [0-9]{3,}\.)"
```

This pipes output through grep to filter for matches and large token amounts.

### Example 4: Validate DEX Integration

```bash
# Scenario: You integrated your token on a new DEX
# Goal: Confirm transactions route through expected programs

cargo run --release --bin mint_trace -- --mint <YOUR_TOKEN_MINT> | \
  grep "Program:"
```

**Expected Programs:**
- Jupiter Aggregator: `JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB`
- Raydium: `675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8`
- Orca: `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc`
- Token Program: `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`

---

## Performance Tips

### High-Volume Tokens

For popular mints with high transaction volume:

1. **Redirect to File:**
   ```bash
   cargo run --release --bin mint_trace -- --mint <POPULAR_MINT> > trace.log 2>&1
   ```

2. **Filter Output in Real-Time:**
   ```bash
   cargo run --release --bin mint_trace -- --mint <POPULAR_MINT> | \
     grep -E "(MINT MATCH|Signature|SOL Changes|Token Changes)"
   ```

3. **Limit Output Duration:**
   ```bash
   timeout 60 cargo run --release --bin mint_trace -- --mint <POPULAR_MINT>
   ```
   This stops after 60 seconds.

### Low-Volume Development Tokens

For newly launched or low-activity tokens:
- Use `COMMITMENT_LEVEL=processed` for fastest confirmation
- Run in foreground with full output for debugging

---

## Troubleshooting

### Error: "Missing --mint argument"

**Cause:** Forgot to specify the mint address

**Solution:**
```bash
cargo run --release --bin mint_trace -- --mint <MINT_ADDRESS>
```
Note the `--` before `--mint` (separates cargo args from binary args).

### Error: "Invalid mint address"

**Cause:** Provided string is not a valid base58-encoded Solana address

**Solution:** Verify the mint address from:
- Solana Explorer: `https://explorer.solana.com/address/<MINT>`
- DexScreener: `https://dexscreener.com/solana/<MINT>`

### Error: "Connection failed"

**Cause:** Cannot reach gRPC endpoint

**Solutions:**
1. Check `GEYSER_URL` is correct
2. Verify gRPC server is running
3. Test with curl:
   ```bash
   curl -v $GEYSER_URL
   ```
4. Check firewall rules if using remote server

### No Transactions Appearing

**Possible Causes:**

1. **Mint Not Active:**
   - Verify token has recent activity on Solana Explorer
   - Try a known-active mint (e.g., USDC) to test setup

2. **Wrong Commitment Level:**
   - Switch to `processed` for faster visibility:
     ```bash
     export COMMITMENT_LEVEL=processed
     ```

3. **gRPC Filter Issue:**
   - Check logs for "Connected successfully" message
   - Verify mint address is correct (case-sensitive)

4. **No Balance Changes:**
   - Tool only captures transactions with token balance changes
   - If mint is only queried (no transfers), it won't appear

### Tool Crashes After Few Transactions

**Cause:** Memory issue or gRPC stream interruption

**Solutions:**
1. Restart with lower log level:
   ```bash
   export RUST_LOG=warn
   ```

2. Check available memory:
   ```bash
   free -h
   ```

3. Monitor for gRPC errors in logs

---

## Architecture Notes

### Why Account-Based Filtering?

`mint_trace` uses `account_required` filtering at the gRPC level, which matches transactions where the mint address appears in the account keys. This works because:

1. Solana transactions list ALL accounts involved
2. Token accounts contain their mint address
3. When balances change, token accounts are in `pre_token_balances` / `post_token_balances`
4. Those token accounts are also in the transaction's account key list

**Trade-off:** This won't capture transactions that READ mint data without changing balances (e.g., metadata queries). This is intentional - we focus on balance-affecting transactions.

### How It Differs from Instruction Scanning

Unlike the unified instruction scanner (which filters by program IDs), mint_trace:
- Subscribes to ALL transactions involving a specific mint address
- Processes transaction-level balance changes (not instruction decoding)
- Works across ANY program that touches the mint
- No program-specific logic required

### Token Balance Change Detection

The tool extracts mints from:
```rust
// Pre-transaction token balances
metadata.meta.pre_token_balances

// Post-transaction token balances  
metadata.meta.post_token_balances
```

This provides transaction-wide visibility regardless of which instruction caused the change.

---

## Advanced Usage

### JSON Output Mode (Future Enhancement)

Currently, output is human-readable only. For machine parsing, redirect to file and parse with tools like `jq`:

```bash
cargo run --release --bin mint_trace -- --mint <MINT> 2>&1 | \
  grep -oP 'Signature:\s+\K\S+' > signatures.txt
```

### Integration with Other Tools

**Export Signatures for Block Explorer:**
```bash
cargo run --release --bin mint_trace -- --mint <MINT> 2>&1 | \
  grep "Signature:" | \
  awk '{print "https://explorer.solana.com/tx/" $2}' > explorer_links.txt
```

**Monitor Multiple Mints (via tmux):**
```bash
# Terminal 1
tmux new-session -s mint1
cargo run --release --bin mint_trace -- --mint <MINT_1>

# Terminal 2  
tmux new-session -s mint2
cargo run --release --bin mint_trace -- --mint <MINT_2>
```

**Compare Activity Across Mints:**
```bash
# Count transactions per minute
cargo run --release --bin mint_trace -- --mint <MINT> 2>&1 | \
  grep "MINT MATCH" | \
  while read line; do date +%Y-%m-%d\ %H:%M; done | \
  uniq -c
```

---

## Known Limitations

1. **No Historical Data:** Only monitors NEW transactions after start
   - Use Solana RPC `getSignaturesForAddress` for historical queries

2. **Read-Only Transactions Not Captured:** Only balance-changing transactions appear
   - Metadata queries without transfers won't match

3. **High Memory on Popular Mints:** Full transaction details stored in memory briefly
   - Redirect to file for long-running sessions on high-volume mints

4. **No Deduplication:** Reconnections may cause duplicate logs during overlap
   - This is a dev tool, duplicates are acceptable

5. **Binary Size:** ~170MB due to full Carbon dependencies
   - Use `--release` for optimized builds

---

## Future Enhancements

Potential additions (not currently implemented):

1. **Program Logs:** Add `log_messages` from `TransactionStatusMeta`
2. **JSON Output:** `--format json` flag for machine parsing
3. **Filtering:** `--action buy|sell`, `--min-amount <SOL>`
4. **Historical Query:** `--from-slot <SLOT>` for past transactions
5. **Multi-Mint:** `--mints mint1,mint2,mint3` for portfolio tracking
6. **Database Output:** Write to SQLite for querying
7. **WebSocket Push:** Stream matches to connected clients

---

## Testing Checklist

Before using in critical scenarios:

- [ ] Test with known-active mint (e.g., USDC)
- [ ] Verify CTRL+C shutdown works
- [ ] Test with invalid mint (error handling)
- [ ] Test with missing --mint arg (error handling)
- [ ] Monitor trending token for real-time validation
- [ ] Check memory usage on high-volume mint
- [ ] Verify reconnection works (kill gRPC server mid-run)

---

## Support & Debugging

### Enable Debug Logging

```bash
export RUST_LOG=debug
cargo run --release --bin mint_trace -- --mint <MINT>
```

This shows:
- gRPC connection attempts
- Filter configuration
- Transaction processing steps
- Reconnection logic

### Capture Full Output

```bash
cargo run --release --bin mint_trace -- --mint <MINT> 2>&1 | tee full_trace.log
```

This logs to file while displaying in terminal.

### Report Issues

When reporting issues, include:
1. Full command used
2. Mint address tested
3. Error messages from logs
4. gRPC endpoint type (local / remote)
5. Expected vs. actual behavior

---

## Summary

`mint_trace` is a powerful development tool for monitoring token activity at the transaction level. It leverages Carbon's TransactionMetadata abstraction to provide complete visibility into all aspects of transactions involving a specific mint, without requiring custom instruction decoders.

**Best For:**
- Debugging token integrations
- Monitoring new launches
- Validating DEX behavior
- Tracking wallet activity

**Not For:**
- Production monitoring (use pipeline_runtime)
- Historical analysis (use RPC queries)
- High-frequency trading signals (too verbose)

---

*For implementation details, see `mint-trace-self-review.md`*
