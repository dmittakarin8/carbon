# Carbon Terminal

A real-time terminal UI for monitoring Solana DEX trades using Carbon framework.

## Features

- **Real-time Trade Monitoring**: Displays live trades from Solana DEX transactions
- **Metadata-Based Extraction**: Uses Carbon's `TransactionStatusMeta` for accurate balance tracking
- **Unfiltered Baseline**: Processes all transactions by default (optional program filtering)
- **Channel-Based Architecture**: Bounded channel for backpressure handling
- **Adaptive UI Refresh**: Dynamic throttle based on trade rate
- **Time-Window Aggregations**: Strict time-cutoff windows (1m, 5m, 15m)
- **JSON Persistence**: Lightweight snapshot with 60s autosave

## Quick Start

### Prerequisites

- Rust 1.82+
- `.env` file with:
  - `GEYSER_URL` - Yellowstone gRPC endpoint
  - `X_TOKEN` - Authentication token (optional)

### Running

```bash
cd examples/carbon_terminal
cargo run --release
```

### Configuration

**Environment Variables:**

- `GEYSER_URL` (required) - Yellowstone gRPC endpoint
- `X_TOKEN` (optional) - Authentication token
- `PROGRAM_FILTERS` (optional) - Comma-separated program IDs to filter
- `RUST_LOG` (optional) - Logging level (debug, info, warn, error)

**Example `.env`:**

```bash
GEYSER_URL=https://basic.grpc.solanavibestation.com
X_TOKEN=your_token_here
RUST_LOG=info
# PROGRAM_FILTERS=pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA,LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj
```

## Architecture

### Data Flow

```
Yellowstone gRPC → Carbon Pipeline → Trade Extractor → Channel → State Aggregator → UI Display
```

### Key Components

- **Trade Extractor**: Extracts SOL/token balance changes from `TransactionStatusMeta`
- **State Aggregator**: Background task that receives trades via channel and aggregates
- **Volume Aggregator**: Strict time-cutoff windows for rolling volume calculations
- **UI**: Ratatui-based terminal interface with adaptive refresh

### Trade Detection

- **BUY**: Negative SOL change (spending SOL to get tokens)
- **SELL**: Positive SOL change (receiving SOL from selling tokens)
- **Filter**: Minimum SOL delta of 0.0001 SOL to filter noise

## Keyboard Shortcuts

- `q` or `Esc` - Quit terminal

## Persistence

Trades are automatically saved to `trades.json` every 60 seconds. On startup, the terminal loads previous trades from this file.

## Testing

Run tests with:

```bash
cargo test
```

Includes mock Carbon gRPC stream test that simulates 10 transactions to verify BUY/SELL detection and mint extraction.

## Program IDs (Reference)

These program IDs are available for optional filtering via `PROGRAM_FILTERS`:

- **PumpSwap**: `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`
- **LetsBonk Launchpad**: `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj`
- **Meteora DLMM**: `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo`

## License

MIT

