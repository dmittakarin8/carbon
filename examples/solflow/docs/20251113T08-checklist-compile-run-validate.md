# Compile, Run, and Validate Checklist: grpc_verify.rs

**Created:** 2025-11-13T08:00  
**Purpose:** Step-by-step guide to compile, run, and validate grpc_verify.rs (no code changes)  
**Scope:** Prerequisites → Compile → Configure → Run → Validate → Acceptance Criteria  
**Quality Bar:** PumpSwap Terminal operational excellence

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Compile Steps](#compile-steps)
3. [Environment Configuration](#environment-configuration)
4. [Run Commands](#run-commands)
5. [Golden-Path Validation](#golden-path-validation)
6. [Acceptance Criteria](#acceptance-criteria)
7. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### System Requirements

| Requirement | Minimum | Recommended | Check Command |
|-------------|---------|-------------|---------------|
| Rust | 1.75.0 | 1.76+ | `rustc --version` |
| Cargo | 1.75.0 | 1.76+ | `cargo --version` |
| RAM | 2 GB | 4 GB | `free -h` |
| Disk Space | 500 MB | 2 GB | `df -h .` |
| Network | Internet access | Low latency to gRPC endpoint | `ping -c 3 solanavibestation.com` |

### Dependency Check

```bash
# Verify Rust installation
rustc --version
# Expected: rustc 1.75.0 or higher

# Verify Cargo installation
cargo --version
# Expected: cargo 1.75.0 or higher

# Verify protoc (Protocol Buffers compiler)
protoc --version
# Expected: libprotoc 3.x or higher (usually installed automatically)
```

**If missing:**
```bash
# Install Rust (includes Cargo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Reload environment
source $HOME/.cargo/env

# Verify installation
rustc --version && cargo --version
```

### Access Requirements

- [ ] Yellowstone gRPC endpoint URL (e.g., `https://basic.grpc.solanavibestation.com`)
- [ ] Authentication token (if required by endpoint)
- [ ] At least one Solana program ID to monitor (e.g., PumpSwap, LetsBonk, Moonshot)
- [ ] Network firewall allows HTTPS/gRPC connections (port 443)

---

## Compile Steps

### Step 1: Navigate to Project Directory

```bash
cd /home/dgem8/projects/carbon/examples/solflow
```

**Verify Location:**
```bash
pwd
# Expected: /home/dgem8/projects/carbon/examples/solflow

ls -1
# Expected output:
# Cargo.toml
# README.md
# src/
# ...
```

### Step 2: Clean Build (Optional but Recommended)

```bash
cargo clean
```

**Expected Output:**
```
   Removing target directory
```

**Purpose:** Ensures fresh build; removes cached artifacts

### Step 3: Build Release Binary

```bash
cargo build --release --bin grpc_verify
```

**Expected Output:**
```
   Compiling proc-macro2 v1.x.x
   Compiling quote v1.x.x
   ...
   Compiling carbon-core v0.x.x
   Compiling grpc_verify v0.1.0 (/home/dgem8/projects/carbon/examples/solflow)
    Finished release [optimized] target(s) in 2m 15s
```

**Compilation Time:** ~2-5 minutes (first build); ~30s (incremental)

**Disk Usage:** ~1.5 GB in `target/` directory

### Step 4: Verify Binary Exists

```bash
ls -lh target/release/grpc_verify
```

**Expected Output:**
```
-rwxr-xr-x 1 user user 8.5M Nov 13 08:00 target/release/grpc_verify
```

**Binary Size:** ~8-10 MB (release mode with optimizations)

### Acceptance Criteria: Compile Phase

- [x] **Exit code 0** (no errors)
- [x] **0 compiler warnings** (clean build)
- [x] **Binary exists:** `target/release/grpc_verify`
- [x] **Compilation time:** < 10 minutes (reasonable)

---

## Environment Configuration

### ENV Variables Matrix

| Variable | Purpose | Required | Default | Example | Validation |
|----------|---------|----------|---------|---------|------------|
| `GEYSER_URL` | Yellowstone gRPC endpoint | ✅ Yes | (none) | `https://basic.grpc.solanavibestation.com` | Must start with `https://` or `http://` |
| `X_TOKEN` | Authentication token | ❌ No | None | `abc123def456...` | Any string |
| `PROGRAM_FILTERS` | Comma-separated Solana program IDs | ✅ Yes | (none) | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | Must be valid base58 strings (44 chars each) |
| `RUST_LOG` | Logging level | ❌ No | `info` | `debug`, `warn`, `error` | One of: trace, debug, info, warn, error |

### Step 1: Create .env File

```bash
# Navigate to project directory
cd /home/dgem8/projects/carbon/examples/solflow

# Create .env file (if doesn't exist)
touch .env
```

### Step 2: Configure Required Variables

**Edit `.env` file:**

```bash
# Yellowstone gRPC Configuration
GEYSER_URL=https://basic.grpc.solanavibestation.com

# Authentication (if required by your endpoint)
X_TOKEN=your_token_here

# Program Filters (comma-separated, NO SPACES)
# Example: Monitor PumpSwap, LetsBonk, and Moonshot
PROGRAM_FILTERS=pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj,MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG

# Optional: Logging Level
RUST_LOG=info
```

**Common Program IDs Reference:**

| DEX/Protocol | Program ID | Active Trades? |
|--------------|------------|----------------|
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | ✅ High volume |
| LetsBonk Launchpad | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | ✅ Medium volume |
| Moonshot | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | ✅ High volume |

### Step 3: Validate ENV File

```bash
# Check syntax (no spaces around commas)
cat .env | grep PROGRAM_FILTERS
# Expected: PROGRAM_FILTERS=id1,id2,id3

# Verify no trailing spaces
cat .env | grep -E '\s$'
# Expected: (no output)

# Count program IDs
cat .env | grep PROGRAM_FILTERS | tr ',' '\n' | tail -n +1 | wc -l
# Expected: 3 (or number of programs you configured)
```

### Step 4: Load ENV Variables (Test)

```bash
# Source .env (for testing only)
export $(cat .env | xargs)

# Verify loaded
echo $GEYSER_URL
# Expected: https://basic.grpc.solanavibestation.com

echo $PROGRAM_FILTERS
# Expected: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj,...
```

**Note:** `cargo run` automatically loads `.env` via `dotenv` crate; manual export is for testing only.

### Acceptance Criteria: Configuration Phase

- [x] `.env` file exists in project root
- [x] `GEYSER_URL` set (starts with https:// or http://)
- [x] `PROGRAM_FILTERS` set (at least 1 program ID)
- [x] No syntax errors (no spaces after commas)
- [x] Program IDs are valid base58 (44 characters each)

---

## Run Commands

### Basic Run Command

```bash
cd /home/dgem8/projects/carbon/examples/solflow

cargo run --release --bin grpc_verify
```

**Expected Startup Output:**
```
🚀 Starting gRPC Discriminator Verification Script
📊 Configuration:
   GEYSER_URL: https://basic.grpc.solanavibestation.com
   PROGRAM_FILTERS: 3 program(s)
     1. pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
     2. LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj
     3. MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG
   Detection: Metadata-based (BUY/SELL from SOL flow direction)
   Filter logic: OR (transactions matching ANY program will be included)
🔌 Connecting to Yellowstone gRPC: https://basic.grpc.solanavibestation.com
✅ Pipeline configured, starting data stream...
📡 Monitoring 3 program(s) for trades
Press Ctrl+C to stop
```

### Run with Debug Logging

```bash
RUST_LOG=debug cargo run --release --bin grpc_verify
```

**Use Case:** Troubleshooting connection issues, seeing skipped instructions

### Run with Output Capture

```bash
# Save to file
cargo run --release --bin grpc_verify | tee grpc_verify_output.log

# Save and monitor simultaneously
cargo run --release --bin grpc_verify 2>&1 | tee -a output.log
```

### Run with Timeout (Testing)

```bash
# Run for 60 seconds, then exit
timeout 60s cargo run --release --bin grpc_verify

# Check exit code
echo $?
# Expected: 124 (timeout exit code) or 0 (graceful exit)
```

### Run Pre-Built Binary (Faster)

```bash
# After initial cargo build --release
./target/release/grpc_verify

# Or with explicit path
/home/dgem8/projects/carbon/examples/solflow/target/release/grpc_verify
```

**Advantage:** Skips cargo overhead (~2s faster startup)

### Graceful Shutdown

**Method 1: Ctrl+C**
```bash
# In running terminal, press:
Ctrl+C
```

**Expected Output:**
```
^C
   (process exits cleanly)
```

**Method 2: SIGTERM**
```bash
# In another terminal:
pkill -TERM grpc_verify

# Or find PID first:
ps aux | grep grpc_verify
kill -TERM <PID>
```

### Acceptance Criteria: Run Phase

- [x] Process starts within 5 seconds
- [x] Startup logs appear (🚀, 📊, 🔌, ✅, 📡)
- [x] No connection errors
- [x] No authentication errors
- [x] No panics or crashes

---

## Golden-Path Validation

### Check 1: Connection Verification

**Objective:** Confirm gRPC connection is established

**Command:**
```bash
cargo run --release --bin grpc_verify 2>&1 | head -20
```

**Expected Output:**
```
🚀 Starting gRPC Discriminator Verification Script
...
✅ Pipeline configured, starting data stream...
📡 Monitoring 3 program(s) for trades
Press Ctrl+C to stop
```

**Pass Criteria:**
- ✅ See all startup logs within 30 seconds
- ✅ No "Connection refused" errors
- ✅ No "Authentication failed" errors
- ✅ No timeout errors

**Failure Signs:**
```
❌ Error: Connection refused
❌ Error: Authentication required
❌ Error: tonic::transport::Error(Transport, ...)
```

**If Failed:** Check [Troubleshooting](#troubleshooting) section

---

### Check 2: Receipt Verification

**Objective:** Confirm transactions are being received

**Command:**
```bash
timeout 120s cargo run --release --bin grpc_verify | grep -m 5 "action=BUY\|action=SELL"
```

**Expected Output:**
```
[2025-11-13 08:15:23 UTC] sig=5aB3cD... program=pAMMBay... discriminator=66063d12 action=BUY mint=8xY9zA... sol=1.500000 token=1000000.000000
[2025-11-13 08:15:25 UTC] sig=7kL9mN... program=LanMV9s... discriminator=a0b1c2d3 action=SELL mint=3pQ4rS... sol=0.850000 token=500000.000000
...
```

**Pass Criteria:**
- ✅ Receive at least 1 trade within 120 seconds
- ✅ Log format matches pattern: `[timestamp] sig=... action=BUY/SELL ...`
- ✅ Transactions contain all required fields

**Failure Signs:**
```
❌ No output after 120 seconds
❌ Only "no matching instruction" messages
❌ Incomplete log lines (missing fields)
```

**If Low Volume:**
- Verify program IDs are actively trading (check Solscan/DEX screener)
- Try adding more programs to `PROGRAM_FILTERS`
- Run during peak trading hours

---

### Check 3: Field Consistency Verification

**Objective:** Validate extracted data matches on-chain truth

**Step 1: Capture Sample Signature**

```bash
# Run for 30 seconds, capture first trade
timeout 30s cargo run --release --bin grpc_verify | grep -m 1 "action=BUY\|action=SELL" > sample_trade.txt

# View captured trade
cat sample_trade.txt
```

**Example Output:**
```
[2025-11-13 08:15:23 UTC] sig=5aB3cD4eF6gH7iJ8kL9mN0pQ1rS2tU3vW4xY5zA6B7C8D9E program=pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA discriminator=66063d1201daebea action=BUY mint=8xY9zA3BcDeF4GhIj5KlMn6OpQr7StUv8WxYz9A0B1C sol=1.500000 token=1000000.000000
```

**Step 2: Extract Signature**

```bash
# Parse signature from log
SIGNATURE=$(cat sample_trade.txt | grep -oP 'sig=\K[^ ]+')
echo "Signature: $SIGNATURE"

# Example: 5aB3cD4eF6gH7iJ8kL9mN0pQ1rS2tU3vW4xY5zA6B7C8D9E
```

**Step 3: Open in SolScan**

```bash
# Generate URL
echo "https://solscan.io/tx/$SIGNATURE"

# Open in browser (Linux)
xdg-open "https://solscan.io/tx/$SIGNATURE"

# Or manually copy URL to browser
```

**Step 4: Manual Verification Checklist**

On SolScan transaction page:

- [ ] **SOL Amount:** Compare "SOL Balance Change" for user account
  - grpc_verify: `sol=1.500000`
  - SolScan: `-1.500123 SOL` (includes fee, so slightly higher)
  - **Pass:** Difference ≤ 0.000200 SOL (fee tolerance)

- [ ] **Token Mint:** Verify token address matches
  - grpc_verify: `mint=8xY9zA...`
  - SolScan: Check "Token Balances" section
  - **Pass:** Mint address matches exactly

- [ ] **Action (BUY/SELL):** Validate direction
  - grpc_verify: `action=BUY`
  - SolScan: User SOL balance decreased → BUY (correct)
  - **Pass:** Direction aligns with user SOL flow

- [ ] **Token Amount:** Compare token balance change
  - grpc_verify: `token=1000000.000000`
  - SolScan: `+1,000,000 tokens` (user received)
  - **Pass:** Amount matches ±1% (decimal rounding tolerance)

**Repeat for 3 Random Samples:**

| Sample | Signature | SOL Match? | Mint Match? | Action Match? | Token Match? | Overall |
|--------|-----------|------------|-------------|---------------|--------------|---------|
| 1      | 5aB3...   | ✅          | ✅           | ✅             | ✅            | PASS    |
| 2      | 7kL9...   | ✅          | ✅           | ✅             | ✅            | PASS    |
| 3      | 9mN0...   | ✅          | ✅           | ✅             | ✅            | PASS    |

**Pass Criteria:**
- ✅ 3/3 samples match on SOL amount (±0.000200 tolerance)
- ✅ 3/3 samples match on mint address (exact)
- ✅ 3/3 samples match on action (BUY/SELL)
- ✅ 3/3 samples match on token amount (±1% tolerance)

**Acceptable Discrepancies:**
- SOL amount: Up to 0.0002 SOL difference (transaction fees)
- Token amount: Up to 1% difference (decimal rounding)

**Unacceptable Discrepancies:**
- SOL amount: > 0.001 SOL difference
- Mint address: Any mismatch
- Action: Opposite direction (BUY vs SELL)
- Token amount: > 5% difference

---

## Acceptance Criteria

### Architecture-Clean Declaration

**Overall Checklist:**

- [ ] **Compilation:** Clean build with 0 warnings
- [ ] **Configuration:** Valid .env with required variables
- [ ] **Connection:** Connects to gRPC within 30 seconds
- [ ] **Receipt:** Receives transactions for all configured programs within 2 minutes
- [ ] **Logging:** All required fields present (timestamp, signature, program, discriminator, action, mint, sol, token)
- [ ] **Accuracy:** Manual SolScan verification: 3/3 sampled trades match within tolerance
- [ ] **Stability:** No memory leaks (RSS stable after 5 minutes)
- [ ] **Reliability:** No panics or crashes during 10-minute continuous run
- [ ] **Shutdown:** Graceful shutdown on Ctrl+C (no hung processes)

### Performance Benchmarks

**Memory Stability Test:**

```bash
# Run in background
cargo run --release --bin grpc_verify &
PID=$!

# Monitor memory every 30s for 5 minutes
for i in {1..10}; do
  ps -p $PID -o rss,vsz,cmd
  sleep 30
done

# Expected: RSS stable (< 5% growth over 5 minutes)
```

**Throughput Test:**

```bash
# Count trades received in 60 seconds
timeout 60s cargo run --release --bin grpc_verify | grep -c "action=BUY\|action=SELL"

# Expected: 10-100 trades (depends on market activity)
```

### Quality Gates

| Gate | Metric | Threshold | Pass/Fail |
|------|--------|-----------|-----------|
| Compile Time | Duration | < 10 minutes | ⬜ |
| Binary Size | File size | 5-15 MB | ⬜ |
| Startup Time | Time to "Pipeline configured" | < 30s | ⬜ |
| First Trade | Time to first log line | < 120s | ⬜ |
| Accuracy | SolScan verification | 3/3 match | ⬜ |
| Memory Stability | RSS growth | < 10% per hour | ⬜ |
| CPU Usage | Average CPU % | < 20% | ⬜ |
| Uptime | Continuous runtime | > 10 minutes | ⬜ |

**Pass Threshold:** 7/8 gates PASS

---

## Troubleshooting

### Issue 1: Compilation Errors

**Symptom:**
```
error[E0425]: cannot find value `X` in this scope
```

**Diagnosis:**
```bash
# Check Rust version
rustc --version
# Must be 1.75.0+

# Update Rust
rustup update stable
```

**Solution:**
- Update Rust toolchain to 1.75+
- Clean and rebuild: `cargo clean && cargo build --release --bin grpc_verify`

---

### Issue 2: Connection Refused

**Symptom:**
```
Error: Connection refused (os error 111)
```

**Diagnosis:**
```bash
# Check network connectivity
ping -c 3 solanavibestation.com

# Check DNS resolution
nslookup basic.grpc.solanavibestation.com

# Test HTTPS connection
curl -I https://basic.grpc.solanavibestation.com
```

**Common Causes:**
- Firewall blocking port 443
- Invalid GEYSER_URL
- VPN/proxy interference

**Solutions:**
1. Verify `GEYSER_URL` in `.env` (correct spelling, https://)
2. Check firewall rules: `sudo ufw status`
3. Try alternative endpoint (if available)
4. Disable VPN temporarily

---

### Issue 3: Authentication Failed

**Symptom:**
```
Error: Unauthenticated: Missing or invalid authentication token
```

**Diagnosis:**
```bash
# Check X_TOKEN is set
cat .env | grep X_TOKEN
# Expected: X_TOKEN=your_token_here (not empty)

# Verify token format (base64-like)
echo $X_TOKEN | wc -c
# Expected: > 20 characters
```

**Solutions:**
1. Verify `X_TOKEN` in `.env` is correct
2. Contact gRPC provider to confirm token is active
3. Check for trailing spaces: `cat -A .env | grep X_TOKEN`

---

### Issue 4: No Transactions Received

**Symptom:**
- Startup logs appear
- No trade lines after 2+ minutes

**Diagnosis:**
```bash
# Check program IDs
cat .env | grep PROGRAM_FILTERS

# Verify programs are active
# Visit https://solscan.io/account/<PROGRAM_ID>
# Check "Recent Transactions" tab
```

**Common Causes:**
- Program IDs have no active trades
- Wrong program IDs (typos)
- Market is slow (low trading volume)

**Solutions:**
1. Add more programs to `PROGRAM_FILTERS`
2. Verify program IDs on Solscan (check recent activity)
3. Run during peak trading hours (US market open)
4. Try high-volume programs (PumpSwap, Moonshot)

---

### Issue 5: Memory Leak

**Symptom:**
- RSS grows unbounded over time

**Diagnosis:**
```bash
# Monitor memory for 10 minutes
while true; do ps aux | grep grpc_verify | grep -v grep; sleep 60; done

# Expected: RSS stable (< 50 MB)
# Actual: RSS growing (100 MB → 200 MB → 300 MB ...)
```

**Solution:**
- This is a known limitation (no state persistence in grpc_verify)
- For long-running processes, use SolFlow instead (has cleanup logic)
- Workaround: Restart process every hour via cron

---

### Issue 6: Panics or Crashes

**Symptom:**
```
thread 'main' panicked at 'index out of bounds: ...', src/bin/grpc_verify.rs:354
```

**Diagnosis:**
```bash
# Run with backtrace
RUST_BACKTRACE=full cargo run --release --bin grpc_verify
```

**Common Causes:**
- Malformed transaction (ALT index out of bounds)
- Missing account keys

**Solution:**
- Report panic details to maintainer
- Temporary workaround: Filter out problematic transactions (add bounds checks)

---

### Issue 7: Slow Performance

**Symptom:**
- Trade logs appear with 10-30s delay
- High CPU usage (> 50%)

**Diagnosis:**
```bash
# Check CPU usage
top -p $(pgrep grpc_verify)

# Profile with perf (Linux)
perf record -g cargo run --release --bin grpc_verify
perf report
```

**Common Causes:**
- Network latency (slow gRPC endpoint)
- Debug builds instead of release
- System resource contention

**Solutions:**
1. Ensure using `--release` flag (optimized build)
2. Switch to lower-latency gRPC endpoint
3. Close other resource-heavy processes

---

## Additional Resources

### Useful Commands

**Monitor live output:**
```bash
# Show last 20 lines, follow new output
cargo run --release --bin grpc_verify | tail -20 -f
```

**Filter by action:**
```bash
# Only show BUY trades
cargo run --release --bin grpc_verify | grep "action=BUY"

# Only show SELL trades
cargo run --release --bin grpc_verify | grep "action=SELL"
```

**Count trades per minute:**
```bash
# Run for 60s, count trades
timeout 60s cargo run --release --bin grpc_verify | grep -c "action=BUY\|action=SELL"
```

**Extract unique mints:**
```bash
# Get list of all token mints seen
cargo run --release --bin grpc_verify | grep -oP 'mint=\K[^ ]+' | sort -u
```

**Monitor system resources:**
```bash
# Watch process stats (refresh every 2s)
watch -n 2 "ps aux | grep grpc_verify | grep -v grep"
```

### Log Analysis Scripts

**Parse SOL volumes:**
```bash
# Extract all SOL amounts from logs
grep "action=BUY\|action=SELL" output.log | grep -oP 'sol=\K[0-9.]+' | sort -n
```

**Calculate total volume:**
```bash
# Sum all SOL amounts
grep "action=BUY\|action=SELL" output.log | grep -oP 'sol=\K[0-9.]+' | awk '{sum+=$1} END {print "Total SOL Volume:", sum}'
```

**Count trades by program:**
```bash
# Group by program ID
grep "program=" output.log | grep -oP 'program=\K[^ ]+' | sort | uniq -c | sort -rn
```

---

**End of Checklist**
