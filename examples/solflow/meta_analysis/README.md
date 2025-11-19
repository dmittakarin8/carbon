# Phase Zero: Streamer Meta-Analysis

**Branch:** `feature/streamer-meta-analysis`  
**Purpose:** Capture and analyze complete `TransactionStatusMeta` surface area for all 4 program IDs

---

## Quick Start

### Test Capture (10 transactions)
```bash
cd ~/projects/carbon/examples/solflow

# Test PumpSwap capture
META_ANALYSIS_SAMPLE_SIZE=10 cargo run --release --bin pumpswap_meta
```

### Full Capture (450 transactions total)
```bash
# PumpSwap (100)
META_ANALYSIS_SAMPLE_SIZE=100 cargo run --release --bin pumpswap_meta

# BonkSwap (100)
META_ANALYSIS_SAMPLE_SIZE=100 cargo run --release --bin bonkswap_meta

# Moonshot (150 - increased for CPI analysis)
META_ANALYSIS_MOONSHOT_SAMPLE_SIZE=150 cargo run --release --bin moonshot_meta

# Jupiter DCA (100)
META_ANALYSIS_SAMPLE_SIZE=100 cargo run --release --bin jupiter_dca_meta
```

---

## Output Structure

```
data/meta_analysis/
├── pumpswap/raw/
│   ├── YYYY-MM-DDTHH-MM-SS_session.jsonl
│   └── YYYY-MM-DDTHH-MM-SS_session_meta.json
├── bonkswap/raw/
├── moonshot/raw/
└── jupiter_dca/raw/
```

### JSONL Schema

Each line in `*_session.jsonl` contains a complete transaction capture:

```json
{
  "capture_metadata": {
    "program_id": "pAMM...",
    "program_name": "PumpSwap",
    "capture_tool_version": "0.1.0",
    "captured_at": 1700000000
  },
  "slot": 234567890,
  "signature": "5xH2...",
  "account_keys": ["BYvF...", "pAMM...", ...],
  "pre_balances": [1000000000, ...],
  "post_balances": [999500000, ...],
  "pre_token_balances": [...],
  "post_token_balances": [...],
  "sol_deltas": [
    {
      "account_index": 0,
      "mint": "So11111...",
      "raw_change": -500000,
      "ui_change": -0.0005,
      "decimals": 9,
      "is_sol": true
    }
  ],
  "token_deltas": [...],
  "inner_instructions": [
    {
      "top_level_index": 0,
      "program_id": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
      "data_hex_prefix": "0123456789abcdef..."
    }
  ],
  "fee": 5000,
  "account_classifications": []
}
```

### Session Metadata Schema

`*_session_meta.json` contains capture summary:

```json
{
  "program_name": "PumpSwap",
  "program_id": "pAMM...",
  "session_start_time": "2025-11-19T10:00:00Z",
  "session_end_time": "2025-11-19T10:03:45Z",
  "duration_seconds": 225,
  "transactions_captured": 100,
  "transactions_target": 100,
  "capture_complete": true,
  "output_file": "2025-11-19T10-00-00_session.jsonl",
  "inner_instruction_stats": {
    "transactions_with_inner": 87,
    "total_inner_instructions": 234,
    "unique_inner_programs": ["TokenkegQfe...", "..."]
  }
}
```

---

## Refinements Implemented

### ✅ Refinement #1: Inner Instruction Program IDs
- Extracts all inner instructions with resolved program IDs
- Includes discriminator prefix (first 16 bytes of instruction data)
- Tracks unique programs invoked per session

### ✅ Refinement #2: Program Context
- Every capture includes program ID and name
- Embedded in each JSONL line (no external metadata dependency)
- Tool version tracking for reproducibility

### ✅ Refinement #3: Configurable Sample Size
- Default: 100 transactions per program
- Moonshot override: 150 transactions (via `META_ANALYSIS_MOONSHOT_SAMPLE_SIZE`)
- Auto-stops when target reached

### ✅ Refinement #4: Session Metadata File
- Dual output: JSONL (captures) + JSON (session summary)
- Includes capture completeness status
- Inner instruction statistics pre-computed

### ✅ Refinement #5: Co-Occurrence Analysis (Phase 3)
- Post-processing tool (not during capture)
- Analyzes mint appearance patterns
- Detects LP tokens via frequency and co-occurrence

---

## Architecture

### Meta Streamers (Phase 1 - Complete)
- `pumpswap_meta` - PumpSwap analysis streamer
- `bonkswap_meta` - BonkSwap analysis streamer
- `moonshot_meta` - Moonshot analysis streamer (150 tx default)
- `jupiter_dca_meta` - Jupiter DCA analysis streamer

### Analysis Tools (Phase 3 - Pending)
- `mint_pattern_detector` - Co-occurrence frequency analysis
- `classify_accounts` - Heuristic account classification
- `metadata_viewer` - Interactive capture explorer

---

## Data Capture Checklist

- [x] Module structure created (`src/meta_analysis/`)
- [x] TransactionCapture with all refinements
- [x] MetadataCaptureProcessor with auto-stop
- [x] 4 meta streamer binaries
- [x] Cargo.toml updated with [[bin]] entries
- [x] Compilation successful (all 4 binaries)
- [ ] Test capture (10 transactions, 1 program)
- [ ] Full capture execution (450 transactions total)
- [ ] Analysis tools built
- [ ] Pattern discovery completed
- [ ] Findings documented

---

## Next Steps

1. **Test Phase:** Run 10-transaction capture to verify output format
2. **Full Capture:** Execute all 4 streamers (450 transactions)
3. **Build Tools:** Implement `mint_pattern_detector` and `classify_accounts`
4. **Analyze:** Process all captured data
5. **Document:** Create `FINDINGS.md` with discovered patterns

---

**Status:** Phase 1 Complete (Capture Infrastructure)  
**Last Updated:** 2025-11-19
