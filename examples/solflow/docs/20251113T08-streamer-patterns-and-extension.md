# Streamer Patterns & Extension Guide

**Created:** 2025-11-13T08:00  
**Purpose:** Templates and patterns for extending the Multi-Streamer System  
**Audience:** Developers adding new program streamers  
**Related Docs:** [architecture-streamer-system.md](./20251113T08-architecture-streamer-system.md)

---

## Table of Contents

1. [Quick Start Template](#quick-start-template)
2. [Step-by-Step Guide: Adding Jupiter DCA](#step-by-step-guide-adding-jupiter-dca)
3. [Extension Patterns](#extension-patterns)
4. [Testing Checklist](#testing-checklist)
5. [Common Customizations](#common-customizations)
6. [Troubleshooting](#troubleshooting)
7. [Best Practices](#best-practices)

---

## Quick Start Template

### Minimal Streamer (15 lines)

Copy this template to add a new program streamer:

```rust
// src/bin/{program_name}_streamer.rs
use streamer_core::{StreamerConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig {
        program_id: "{SOLANA_PROGRAM_ID}".to_string(),
        program_name: "{HumanReadableName}".to_string(),
        output_path: std::env::var("OUTPUT_PATH")
            .unwrap_or_else(|_| "/streams/{program_name}/events.jsonl".to_string()),
    };
    
    run(config).await
}
```

### Cargo.toml Addition

```toml
[[bin]]
name = "{program_name}_streamer"
path = "src/bin/{program_name}_streamer.rs"
```

### Environment Configuration

```bash
# .env (optional per-streamer overrides)
{PROGRAM_NAME}_OUTPUT_PATH=/custom/path/events.jsonl
```

---

## Step-by-Step Guide: Adding Jupiter DCA

### Goal
Monitor Jupiter's DCA (Dollar-Cost Averaging) program and emit trade events to JSONL.

### Prerequisites

- [ ] Jupiter DCA program ID: `DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M`
- [ ] Access to Yellowstone gRPC endpoint
- [ ] Existing streamer_core implemented (Phase 1 complete)

### Step 1: Create Binary File

**File:** `src/bin/jupiter_dca_streamer.rs`

```rust
//! Jupiter DCA Streamer
//!
//! Monitors Jupiter's Dollar-Cost Averaging program for trade events.
//! 
//! Program ID: DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M
//! 
//! Usage:
//!   cargo run --release --bin jupiter_dca_streamer
//! 
//! Environment Variables:
//!   GEYSER_URL - Yellowstone gRPC endpoint (required)
//!   X_TOKEN - Authentication token (optional)
//!   JUPITER_DCA_OUTPUT_PATH - Output file path (optional, default: /streams/jupiter_dca/events.jsonl)

use streamer_core::{StreamerConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig {
        program_id: "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string(),
        program_name: "JupiterDCA".to_string(),
        output_path: std::env::var("JUPITER_DCA_OUTPUT_PATH")
            .unwrap_or_else(|_| "/streams/jupiter_dca/events.jsonl".to_string()),
    };
    
    run(config).await
}
```

**Lines:** 23 (including comments and formatting)

### Step 2: Update Cargo.toml

**File:** `Cargo.toml`

**Add to `[[bin]]` section:**

```toml
[[bin]]
name = "jupiter_dca_streamer"
path = "src/bin/jupiter_dca_streamer.rs"
```

**Location:** Add after existing streamer binaries (pumpswap, bonkswap, moonshot).

### Step 3: Create Output Directory

```bash
# Create directory for JSONL output
mkdir -p /streams/jupiter_dca

# Verify permissions
ls -ld /streams/jupiter_dca
# Should show: drwxr-xr-x ... /streams/jupiter_dca
```

### Step 4: Configure Environment (Optional)

**File:** `.env`

```bash
# Jupiter DCA Configuration (optional overrides)
JUPITER_DCA_OUTPUT_PATH=/var/log/solflow/jupiter_dca/events.jsonl
```

### Step 5: Build and Test

```bash
# Build the streamer
cargo build --release --bin jupiter_dca_streamer

# Expected output:
#    Compiling jupiter_dca_streamer v0.1.0
#    Finished release [optimized] target(s) in 1m 23s

# Verify binary exists
ls -lh target/release/jupiter_dca_streamer
# Should show: -rwxr-xr-x ... 8.5M ... jupiter_dca_streamer

# Test run (Ctrl+C to stop after 30 seconds)
cargo run --release --bin jupiter_dca_streamer

# Expected startup output:
# 🚀 Starting JupiterDCA streamer
#    Program ID: DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M
#    Output: /streams/jupiter_dca/events.jsonl
#    Geyser URL: https://basic.grpc.solanavibestation.com
# ✅ Connected to Yellowstone gRPC
# 📡 Streaming transactions...
```

### Step 6: Verify Output

```bash
# Wait for at least one trade (may take 1-5 minutes depending on activity)
tail -f /streams/jupiter_dca/events.jsonl

# Expected format (one JSON object per line):
# {"timestamp":1731484523,"signature":"5aB...","program_id":"DCA265...","program_name":"JupiterDCA","action":"BUY","mint":"8xY...","sol_amount":1.5,"token_amount":1000000.0,"token_decimals":9,"user_account":"7vW...","discriminator":"a1b2c3d4e5f6g7h8"}
```

### Step 7: Validate Data Quality

**Manual Check (SolScan cross-reference):**

1. Copy signature from JSONL output
2. Open: `https://solscan.io/tx/{SIGNATURE}`
3. Verify:
   - ✅ Program ID matches (`DCA265...`)
   - ✅ SOL amounts within ±0.000001
   - ✅ Token mint matches one of the tokens in transaction
   - ✅ Action (BUY/SELL) aligns with SOL flow direction

**Repeat for 3 random samples.**

### Step 8: Deploy to Production

**Option 1: systemd Service**

**File:** `/etc/systemd/system/jupiter-dca-streamer.service`

```ini
[Unit]
Description=Jupiter DCA Streamer - SolFlow Multi-Streamer System
After=network.target
Requires=network.target

[Service]
Type=simple
User=solflow
Group=solflow
WorkingDirectory=/home/solflow/carbon-terminal
Environment="RUST_LOG=info"
EnvironmentFile=/home/solflow/carbon-terminal/.env
ExecStart=/home/solflow/carbon-terminal/target/release/jupiter_dca_streamer
Restart=always
RestartSec=10s
KillMode=mixed
TimeoutStopSec=10s

[Install]
WantedBy=multi-user.target
```

**Enable and start:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable jupiter-dca-streamer
sudo systemctl start jupiter-dca-streamer
sudo systemctl status jupiter-dca-streamer
```

**Option 2: Docker Compose**

**File:** `docker-compose.yml` (add to existing services)

```yaml
services:
  jupiter-dca-streamer:
    image: solflow/streamer:latest
    container_name: jupiter-dca-streamer
    command: /app/jupiter_dca_streamer
    environment:
      - GEYSER_URL=${GEYSER_URL}
      - X_TOKEN=${X_TOKEN}
      - RUST_LOG=info
    volumes:
      - ./streams/jupiter_dca:/streams/jupiter_dca
    restart: unless-stopped
```

**Start:**

```bash
docker-compose up -d jupiter-dca-streamer
docker-compose logs -f jupiter-dca-streamer
```

### Step 9: Monitor

```bash
# Check process status
ps aux | grep jupiter_dca_streamer

# Check output file growth
watch -n 5 'ls -lh /streams/jupiter_dca/events.jsonl*'

# Check logs (systemd)
journalctl -u jupiter-dca-streamer -f

# Check logs (Docker)
docker-compose logs -f jupiter-dca-streamer
```

### Step 10: Document

**Update:** `AGENTS.md` (root-level project documentation)

```markdown
### Current Binaries (Approved)

```toml
[[bin]]
name = "pumpswap_streamer"
path = "src/bin/pumpswap_streamer.rs"

[[bin]]
name = "bonkswap_streamer"
path = "src/bin/bonkswap_streamer.rs"

[[bin]]
name = "moonshot_streamer"
path = "src/bin/moonshot_streamer.rs"

[[bin]]
name = "jupiter_dca_streamer"  # NEW
path = "src/bin/jupiter_dca_streamer.rs"
```

**Total: 4 binaries**
```

---

## Extension Patterns

### Pattern 1: Custom Output Path Strategy

**Use Case:** Organize output by date or shard by token.

**Example: Date-Based Sharding**

```rust
// src/bin/custom_streamer.rs
use chrono::Utc;
use streamer_core::{StreamerConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate date-based path: /streams/program/2025-11-13/events.jsonl
    let date_path = format!(
        "/streams/custom/{}/events.jsonl",
        Utc::now().format("%Y-%m-%d")
    );
    
    let config = StreamerConfig {
        program_id: "CustomProgram123...".to_string(),
        program_name: "CustomProgram".to_string(),
        output_path: std::env::var("OUTPUT_PATH")
            .unwrap_or(date_path),
    };
    
    run(config).await
}
```

**Note:** Directory must exist or be created before run. Consider adding `fs::create_dir_all()`.

---

### Pattern 2: Multi-Program Monitoring

**Use Case:** Single streamer monitors multiple related programs (e.g., Jupiter Aggregator + Jupiter DCA).

**⚠️ Warning:** This violates the "one streamer = one program" principle. Only use if programs are tightly coupled.

**Example: Jupiter Suite Streamer**

```rust
// src/bin/jupiter_suite_streamer.rs
use streamer_core::{StreamerConfig, run_multi_program};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let programs = vec![
        StreamerConfig {
            program_id: "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),
            program_name: "JupiterAggregator".to_string(),
            output_path: "/streams/jupiter_aggregator/events.jsonl".to_string(),
        },
        StreamerConfig {
            program_id: "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string(),
            program_name: "JupiterDCA".to_string(),
            output_path: "/streams/jupiter_dca/events.jsonl".to_string(),
        },
    ];
    
    run_multi_program(programs).await
}
```

**Implementation Note:** `run_multi_program()` is not implemented in Phase 1. Requires:
- Multiple gRPC filters (OR logic)
- Per-program output writers
- Shared Carbon pipeline

**Alternative (Recommended):** Run two separate streamers (one per program).

---

### Pattern 3: Custom Filters (Beyond Program ID)

**Use Case:** Filter by specific accounts (e.g., only trades involving a specific token mint).

**Example: USDC-Only Streamer**

```rust
// src/bin/usdc_trades_streamer.rs
use streamer_core::{StreamerConfig, run_with_filter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwapUSDC".to_string(),
        output_path: "/streams/pumpswap_usdc/events.jsonl".to_string(),
    };
    
    let filter = |event: &TradeEvent| {
        // Only emit events for USDC mint
        event.mint == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    };
    
    run_with_filter(config, filter).await
}
```

**Implementation Note:** `run_with_filter()` requires adding filter parameter to `TradeProcessor`. Estimated effort: 2-3 hours.

---

### Pattern 4: Custom Enrichment (Add Metadata)

**Use Case:** Enrich events with additional data (e.g., price, market cap) before writing.

**Example: Price-Enriched Streamer**

```rust
// src/bin/enriched_streamer.rs
use streamer_core::{StreamerConfig, TradeEvent, run_with_enrichment};

async fn enrich_with_price(event: &mut TradeEvent) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch price from external API
    let price = fetch_price_from_api(&event.mint).await?;
    
    // Add to event (requires extending TradeEvent struct)
    event.price_sol = Some(price);
    event.market_cap_sol = event.token_amount * price;
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwapEnriched".to_string(),
        output_path: "/streams/pumpswap_enriched/events.jsonl".to_string(),
    };
    
    run_with_enrichment(config, enrich_with_price).await
}
```

**Implementation Note:**
1. Extend `TradeEvent` struct with optional fields (`price_sol`, `market_cap_sol`)
2. Add enrichment callback parameter to `run()`
3. Call enrichment function before `writer.write_event()`

**Estimated Effort:** 4-6 hours (struct extension + API integration)

---

### Pattern 5: Testing/Staging Streamer

**Use Case:** Separate streamer for testing without affecting production data.

**Example: Staging Streamer**

```rust
// src/bin/pumpswap_staging_streamer.rs
use streamer_core::{StreamerConfig, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig {
        program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
        program_name: "PumpSwapStaging".to_string(),
        output_path: "/tmp/staging/pumpswap/events.jsonl".to_string(), // Separate output
    };
    
    run(config).await
}
```

**Benefits:**
- Test configuration changes without affecting production
- Separate output directory (no collision)
- Can run in parallel with production streamer

**Deployment:**

```bash
# Run staging streamer on different port (if using HTTP API in future)
GEYSER_URL=https://staging.grpc.example.com \
  cargo run --release --bin pumpswap_staging_streamer
```

---

## Testing Checklist

### Pre-Deployment Testing

**1. Compilation Check**

```bash
cargo build --release --bin {streamer_name}
```

- [ ] Build completes without errors
- [ ] No compiler warnings
- [ ] Binary size reasonable (< 10 MB)

**2. Startup Test**

```bash
cargo run --release --bin {streamer_name}
```

- [ ] Startup logs appear within 5 seconds
- [ ] No "Connection refused" errors
- [ ] No "Authentication failed" errors
- [ ] Process doesn't crash within 30 seconds

**3. Connection Test**

```bash
timeout 60s cargo run --release --bin {streamer_name} | head -10
```

- [ ] Connects to Yellowstone gRPC successfully
- [ ] Logs show "Connected" or "Streaming transactions"
- [ ] No infinite retry loop

**4. Output Test**

```bash
# Run for 2 minutes, check output
timeout 120s cargo run --release --bin {streamer_name} &
sleep 120
ls -lh {output_path}
```

- [ ] Output file created
- [ ] File size > 0 bytes
- [ ] At least one JSONL event written (if program has activity)

**5. Data Quality Test**

```bash
# Check JSONL validity
tail -1 {output_path} | jq .
```

- [ ] JSON is valid (jq parses without errors)
- [ ] All required fields present (timestamp, signature, program_id, etc.)
- [ ] No null values for required fields

**6. Manual Verification (SolScan)**

```bash
# Extract 3 random signatures
shuf -n 3 {output_path} | jq -r '.signature'
```

For each signature:
- [ ] Open `https://solscan.io/tx/{SIGNATURE}`
- [ ] Program ID matches
- [ ] SOL amounts match within ±0.000001
- [ ] Token mint is present in transaction

**7. Fault Tolerance Test**

```bash
# Start streamer
cargo run --release --bin {streamer_name} &
PID=$!

# Wait 30 seconds
sleep 30

# Kill process (SIGTERM)
kill -TERM $PID

# Wait for graceful shutdown
sleep 5

# Check exit code
wait $PID
echo "Exit code: $?"
```

- [ ] Exit code is 0 (clean shutdown)
- [ ] No hung processes (`ps aux | grep {streamer_name}`)
- [ ] Output file is not corrupted (last line may be incomplete, OK)

**8. Parallel Test (if multiple streamers)**

```bash
# Start 3 streamers in parallel
cargo run --release --bin pumpswap_streamer &
cargo run --release --bin bonkswap_streamer &
cargo run --release --bin {new_streamer} &

# Wait 60 seconds
sleep 60

# Check all are running
ps aux | grep streamer | wc -l  # Should output: 3

# Kill one
killall -9 pumpswap_streamer

# Verify others still running
ps aux | grep streamer | wc -l  # Should output: 2
```

- [ ] All 3 start successfully
- [ ] All 3 write to separate files (no collision)
- [ ] Killing one doesn't affect others

---

### Post-Deployment Monitoring

**1. Resource Usage**

```bash
# Monitor CPU, memory, disk I/O
pidstat -r -u -d -p $(pgrep {streamer_name}) 5
```

- [ ] CPU < 15% (steady state)
- [ ] Memory < 50 MB (steady state)
- [ ] No memory growth over time (monitor for 1 hour)

**2. Output Growth**

```bash
# Monitor file size every 5 minutes
watch -n 300 'ls -lh {output_path}'
```

- [ ] File grows at expected rate (~20 KB/s per streamer)
- [ ] Rotation occurs at 100 MB (or configured threshold)
- [ ] No unbounded growth (old files deleted)

**3. Error Logs**

```bash
# Check for errors
journalctl -u {streamer_service} | grep -i "error\|warn" | tail -20
```

- [ ] No recurring errors
- [ ] No "rate limit" warnings (or infrequent)
- [ ] No "disk full" errors

**4. Data Quality (Sample Check)**

```bash
# Weekly validation: Check 10 random transactions
shuf -n 10 {output_path} | jq -r '.signature' | while read sig; do
    echo "Verifying $sig..."
    curl -s "https://api.solscan.io/transaction?tx=$sig" | jq '.status'
done
```

- [ ] All 10 signatures exist on-chain
- [ ] No "not found" errors (would indicate invalid signatures)

---

## Common Customizations

### Customization 1: Adjust Rotation Size

**Default:** 100 MB per file

**Change via ENV:**

```bash
# .env
OUTPUT_MAX_SIZE_MB=50  # Rotate at 50 MB instead of 100 MB
```

**Change in Code (if ENV not supported):**

```rust
// src/bin/custom_streamer.rs
use streamer_core::{StreamerConfig, run_with_options, StreamerOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig { /* ... */ };
    
    let options = StreamerOptions {
        max_file_size_mb: 50,  // Custom rotation size
        max_rotations: 10,
        ..Default::default()
    };
    
    run_with_options(config, options).await
}
```

**Implementation Note:** Requires adding `StreamerOptions` parameter to `run()`.

---

### Customization 2: Change Commitment Level

**Default:** `Confirmed` (10-20s finality)

**Change via ENV:**

```bash
# .env
COMMITMENT_LEVEL=Finalized  # Slower but irreversible
```

**Options:**
- `Processed` - Fastest (~400ms), may be reverted
- `Confirmed` - Fast (~1s), ~0.1% revert risk
- `Finalized` - Slow (~10-20s), 0% revert risk

**Recommendation:** Use `Finalized` for production (data integrity priority).

---

### Customization 3: Custom Log Format

**Default:** Structured logs with tracing

**Change to JSON logs (for parsing):**

```bash
# .env
RUST_LOG=json  # Requires tracing_subscriber::fmt().json()
```

**Change log level:**

```bash
# .env
RUST_LOG=debug  # More verbose (for debugging)
RUST_LOG=warn   # Less verbose (for production)
```

---

### Customization 4: Add Alerting

**Example: PagerDuty Integration**

```rust
// src/bin/alerted_streamer.rs
use streamer_core::{StreamerConfig, run};
use reqwest::Client;

async fn send_alert(message: &str) {
    let client = Client::new();
    let _ = client
        .post("https://events.pagerduty.com/v2/enqueue")
        .json(&serde_json::json!({
            "routing_key": std::env::var("PAGERDUTY_KEY").unwrap(),
            "event_action": "trigger",
            "payload": {
                "summary": message,
                "severity": "error",
                "source": "streamer",
            }
        }))
        .send()
        .await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamerConfig { /* ... */ };
    
    match run(config).await {
        Ok(_) => Ok(()),
        Err(e) => {
            send_alert(&format!("Streamer failed: {:?}", e)).await;
            Err(e)
        }
    }
}
```

**Alternative:** Use systemd `OnFailure=` to trigger external script.

---

### Customization 5: Prometheus Metrics

**Add metrics endpoint (HTTP server):**

```rust
// src/bin/metrics_streamer.rs
use streamer_core::{StreamerConfig, run};
use prometheus::{Counter, Registry, TextEncoder, Encoder};
use warp::Filter;

lazy_static! {
    static ref TRADES_TOTAL: Counter = Counter::new("trades_total", "Total trades processed").unwrap();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start metrics HTTP server on port 9090
    tokio::spawn(async {
        let metrics_route = warp::path("metrics").map(|| {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).unwrap();
            String::from_utf8(buffer).unwrap()
        });
        warp::serve(metrics_route).run(([0, 0, 0, 0], 9090)).await;
    });
    
    let config = StreamerConfig { /* ... */ };
    run(config).await
}
```

**Scrape metrics:**

```bash
curl http://localhost:9090/metrics
# trades_total{program="PumpSwap"} 12345
```

---

## Troubleshooting

### Issue 1: "Connection refused" Error

**Symptom:**

```
ERROR: Failed to connect to gRPC endpoint: Connection refused
```

**Possible Causes:**

1. Yellowstone gRPC endpoint is down
2. Firewall blocking connection
3. Invalid GEYSER_URL (typo or wrong port)

**Diagnosis:**

```bash
# Test gRPC endpoint with grpcurl
grpcurl -plaintext {GEYSER_URL} list

# Test network connectivity
ping {GEYSER_HOST}
telnet {GEYSER_HOST} {GEYSER_PORT}
```

**Solutions:**

- Verify `GEYSER_URL` in `.env` is correct
- Check firewall rules: `sudo ufw status`
- Try alternative endpoint: `GEYSER_URL=https://backup.grpc.example.com`

---

### Issue 2: "Authentication failed" Error

**Symptom:**

```
ERROR: gRPC authentication failed: Unauthorized
```

**Possible Causes:**

1. Missing `X_TOKEN` in `.env`
2. Expired or invalid token
3. Token doesn't have access to endpoint

**Diagnosis:**

```bash
# Check if X_TOKEN is set
echo $X_TOKEN

# Test authentication manually
grpcurl -H "x-token: $X_TOKEN" {GEYSER_URL} list
```

**Solutions:**

- Add `X_TOKEN=...` to `.env`
- Generate new token from Yellowstone provider
- Contact provider to verify token permissions

---

### Issue 3: No Output Written

**Symptom:**

```
Streamer runs without errors, but output file is empty or doesn't exist.
```

**Possible Causes:**

1. Program has no activity (no transactions)
2. Output directory doesn't exist
3. Permission denied (can't write to output path)
4. Filter too strict (all trades filtered out)

**Diagnosis:**

```bash
# Check if output directory exists
ls -ld $(dirname {output_path})

# Check permissions
touch {output_path}  # Should succeed

# Check streamer logs for "Trade:" messages
journalctl -u {streamer_service} | grep "Trade:"
```

**Solutions:**

- Create output directory: `mkdir -p $(dirname {output_path})`
- Fix permissions: `chmod 755 $(dirname {output_path})`
- Verify program has recent activity on Solscan
- Check program ID is correct (no typo)

---

### Issue 4: Memory Leak (Growing RSS)

**Symptom:**

```
Streamer process memory grows unbounded over time (hours/days).
```

**Possible Causes:**

1. gRPC client buffer leak
2. Unclosed file handles
3. Event accumulation in Carbon pipeline

**Diagnosis:**

```bash
# Monitor memory over time
pidstat -r -p $(pgrep {streamer_name}) 60

# Check open file descriptors
lsof -p $(pgrep {streamer_name}) | wc -l
```

**Solutions:**

- Restart streamer periodically (systemd timer)
- Update Carbon framework (may have memory leak fix)
- Enable debug logs to identify leak source
- Report issue with memory profile (heaptrack, valgrind)

---

### Issue 5: High CPU Usage

**Symptom:**

```
Streamer process consistently uses > 50% CPU.
```

**Possible Causes:**

1. High transaction volume (program is very active)
2. Inefficient balance extraction (nested loops)
3. Excessive logging (debug level in production)

**Diagnosis:**

```bash
# Profile CPU usage
perf record -p $(pgrep {streamer_name}) -g sleep 30
perf report

# Check log level
echo $RUST_LOG
```

**Solutions:**

- Reduce log level: `RUST_LOG=info` (not `debug`)
- Optimize hot functions (use profiler to identify)
- Consider sharding (multiple streamers per program)

---

### Issue 6: Rotation Not Working

**Symptom:**

```
Output file grows beyond 100 MB, no rotation occurs.
```

**Possible Causes:**

1. Rotation logic not implemented
2. File size check incorrect (bytes vs. MB)
3. Rename fails due to permissions

**Diagnosis:**

```bash
# Check file size
ls -lh {output_path}

# Check for .1, .2 files (rotated)
ls -lh {output_path}.*

# Check streamer logs for "Rotating file" messages
journalctl -u {streamer_service} | grep -i "rotat"
```

**Solutions:**

- Verify `OUTPUT_MAX_SIZE_MB` is set correctly
- Check write permissions on output directory
- Manually trigger rotation test (future: add signal handler)

---

## Best Practices

### 1. Always Use Environment Variables

**❌ Bad: Hardcoded Configuration**

```rust
let config = StreamerConfig {
    program_id: "pAMMBay...".to_string(),
    output_path: "/home/user/streams/pumpswap/events.jsonl".to_string(), // Hardcoded!
    ..
};
```

**✅ Good: ENV-Based Configuration**

```rust
let config = StreamerConfig {
    program_id: std::env::var("PROGRAM_ID")
        .unwrap_or_else(|_| "pAMMBay...".to_string()),
    output_path: std::env::var("OUTPUT_PATH")
        .unwrap_or_else(|_| "/streams/pumpswap/events.jsonl".to_string()),
    ..
};
```

**Rationale:** Makes configuration flexible without recompilation.

---

### 2. Document Program ID Sources

**Add comment with program ID source:**

```rust
// Program ID: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
// Source: https://solscan.io/account/pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
// Verified: 2025-11-13 (PumpSwap official deployment)
let config = StreamerConfig {
    program_id: "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
    ..
};
```

**Rationale:** Future developers can verify program ID is correct.

---

### 3. Use Descriptive Binary Names

**❌ Bad: Generic Names**

```toml
[[bin]]
name = "streamer1"  # What does this stream?
```

**✅ Good: Specific Names**

```toml
[[bin]]
name = "pumpswap_streamer"  # Clear: streams PumpSwap trades
```

**Rationale:** Easier to identify processes in `ps aux` and logs.

---

### 4. Test in Staging First

**Workflow:**

1. Deploy to staging environment
2. Run for 24 hours
3. Verify data quality (sample checks)
4. Monitor resource usage
5. Deploy to production

**Never:**
- Deploy directly to production without testing
- Skip data quality checks
- Ignore resource usage (can lead to outages)

---

### 5. Version Control Output Schema

**Add schema version to events (future enhancement):**

```json
{
  "schema_version": "1.0",
  "timestamp": 1731484523,
  ...
}
```

**Rationale:** Enables schema evolution without breaking consumers.

---

### 6. Monitor Error Rates

**Add alerting for high error rates:**

```bash
# Alert if > 10 errors in 5 minutes
ERROR_COUNT=$(journalctl -u {streamer_service} --since "5 minutes ago" | grep -c ERROR)
if [ $ERROR_COUNT -gt 10 ]; then
    send_alert "High error rate: $ERROR_COUNT errors in 5 minutes"
fi
```

---

### 7. Keep Binaries Small

**Minimize dependencies:**

```rust
// ❌ Bad: Unnecessary dependency
use reqwest::Client;  // Only needed for enrichment, not core streaming

// ✅ Good: Only essential dependencies
use streamer_core::{StreamerConfig, run};
```

**Rationale:** Smaller binaries = faster compilation, easier deployment.

---

### 8. Use Graceful Shutdown

**Handle SIGTERM properly:**

```rust
// Tokio handles this automatically, but verify:
// 1. Process exits with code 0
// 2. Output writer flushes remaining buffer
// 3. No hung processes

// Test with:
kill -TERM $(pgrep {streamer_name})
```

**Rationale:** Prevents data loss on restart/shutdown.

---

### 9. Separate Concerns

**❌ Bad: Mixing streaming and aggregation**

```rust
// Don't do this in streamer binary:
async fn main() {
    run(config).await?;
    aggregate_data().await?;  // Aggregation belongs in separate service
}
```

**✅ Good: Streamer only streams**

```rust
async fn main() {
    run(config).await?;  // Only streaming, nothing else
}
```

**Rationale:** Keep streamers simple and focused. Aggregation is a separate phase.

---

### 10. Log Startup Configuration

**Always log key config at startup:**

```rust
log::info!("🚀 Starting {} streamer", config.program_name);
log::info!("   Program ID: {}", config.program_id);
log::info!("   Output: {}", config.output_path);
log::info!("   Geyser URL: {}", runtime_config.geyser_url);
log::info!("   Commitment: {:?}", runtime_config.commitment_level);
```

**Rationale:** Makes debugging easier (can verify config from logs).

---

## Summary

### Quick Reference Card

| Task | Command | Notes |
|------|---------|-------|
| **Create Streamer** | `touch src/bin/{name}_streamer.rs` | Use template from Quick Start |
| **Add to Cargo.toml** | `[[bin]]` entry | Binary name + path |
| **Build** | `cargo build --release --bin {name}` | Check for errors |
| **Test Run** | `cargo run --release --bin {name}` | Ctrl+C after 30s |
| **Verify Output** | `tail -f {output_path}` | Check JSONL validity |
| **Deploy (systemd)** | `sudo systemctl enable {name}` | Create .service file first |
| **Deploy (Docker)** | `docker-compose up -d {name}` | Add to docker-compose.yml |
| **Monitor** | `journalctl -u {name} -f` | Real-time logs |

### Extension Checklist

Before adding a new streamer, verify:

- [ ] Program ID is correct (verify on Solscan)
- [ ] Program has recent activity (not dormant)
- [ ] Output directory exists and is writable
- [ ] GEYSER_URL and X_TOKEN are set in .env
- [ ] Binary name follows convention (`{program}_streamer`)
- [ ] Startup configuration is logged
- [ ] Manual testing completed (3 signature samples)
- [ ] Documentation updated (AGENTS.md)

---

**End of Extension Guide**

**Document Metadata:**
- **Filename:** `20251113T08-streamer-patterns-and-extension.md`
- **Word Count:** ~6,500 words
- **Code Examples:** 30+ snippets
- **Checklists:** 5 comprehensive checklists
- **Lines:** 1,100+ lines
