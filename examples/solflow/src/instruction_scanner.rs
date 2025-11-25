//! Unified instruction scanner for detecting tracked programs in transactions
//!
//! This module provides the InstructionScanner which filters transactions by
//! matching program IDs in both outer and inner (CPI) instructions. It replaces
//! per-program gRPC filtering with a unified scanning approach that ensures
//! complete coverage including nested program calls.

use {
    crate::streamer_core::balance_extractor::build_full_account_keys,
    carbon_core::transaction::TransactionMetadata,
    solana_pubkey::Pubkey,
    std::collections::{HashMap, HashSet},
    std::str::FromStr,
    std::sync::Arc,
};

/// Registry of tracked programs with scanning capabilities
#[derive(Clone)]
pub struct InstructionScanner {
    tracked_programs: HashSet<Pubkey>,
    program_names: HashMap<Pubkey, &'static str>,
}

/// Result when a tracked program is found in a transaction
#[derive(Debug, Clone)]
pub struct InstructionMatch {
    pub program_id: Pubkey,
    pub program_name: &'static str,
    pub instruction_path: InstructionPath,
}

/// Describes where the program match occurred in the transaction
#[derive(Debug, Clone)]
pub enum InstructionPath {
    /// Match found in outer (top-level) instruction
    Outer { index: usize },
    /// Match found in inner (CPI) instruction
    Inner {
        outer_index: usize,
        inner_path: Vec<usize>,
    },
}

impl InstructionScanner {
    /// Create a new instruction scanner with the tracked program registry
    ///
    /// The registry includes 5 programs:
    /// - PumpFun: Token minting and bonding curve protocol
    /// - PumpSwap: Swap protocol for pump tokens
    /// - BonkSwap: LetsBonk launchpad swaps
    /// - Moonshot: Moonshot DEX
    /// - Jupiter DCA: Jupiter DCA protocol
    pub fn new() -> Self {
        let mut program_names = HashMap::new();

        // CRITICAL: All 5 programs must be included
        let pumpfun =
            Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap();
        let pumpswap =
            Pubkey::from_str("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA").unwrap();
        let bonkswap =
            Pubkey::from_str("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj").unwrap();
        let moonshot =
            Pubkey::from_str("MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG").unwrap();
        let jupiter_dca =
            Pubkey::from_str("DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M").unwrap();

        program_names.insert(pumpfun, "PumpFun");
        program_names.insert(pumpswap, "PumpSwap");
        program_names.insert(bonkswap, "BonkSwap");
        program_names.insert(moonshot, "Moonshot");
        program_names.insert(jupiter_dca, "JupiterDCA");

        let tracked_programs = program_names.keys().copied().collect();

        log::info!("📋 InstructionScanner initialized with {} programs", program_names.len());
        log::info!("   ├─ PumpFun: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
        log::info!("   ├─ PumpSwap: pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
        log::info!("   ├─ BonkSwap: LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");
        log::info!("   ├─ Moonshot: MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG");
        log::info!("   └─ JupiterDCA: DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M");

        Self {
            tracked_programs,
            program_names,
        }
    }

    /// Scan a transaction for any tracked program ID
    ///
    /// This method checks both outer (top-level) instructions and inner (CPI)
    /// instructions for matches against the tracked program registry. It returns
    /// on the first match found (early exit optimization).
    ///
    /// # Parameters
    ///
    /// - `metadata`: The transaction metadata to scan
    ///
    /// # Returns
    ///
    /// - `Some(InstructionMatch)` if a tracked program is found
    /// - `None` if no tracked programs are found in the transaction
    ///
    /// # Implementation Notes
    ///
    /// - Scanner is read-only (no mutation of TransactionMetadata)
    /// - Uses `build_full_account_keys()` to handle ALT resolution
    /// - Returns on first match for performance
    pub fn scan(&self, metadata: &Arc<TransactionMetadata>) -> Option<InstructionMatch> {
        // Build complete account key list (static + ALT loaded addresses)
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

        // No tracked program found
        None
    }

    /// Get the total number of tracked programs
    pub fn program_count(&self) -> usize {
        self.tracked_programs.len()
    }

    /// Get all tracked program IDs as a vector
    pub fn tracked_program_ids(&self) -> Vec<String> {
        self.tracked_programs
            .iter()
            .map(|pk| pk.to_string())
            .collect()
    }
}

impl Default for InstructionScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_initialization() {
        let scanner = InstructionScanner::new();
        assert_eq!(scanner.program_count(), 5);
        
        let program_ids = scanner.tracked_program_ids();
        assert!(program_ids.contains(&"6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string()));
        assert!(program_ids.contains(&"pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string()));
        assert!(program_ids.contains(&"LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj".to_string()));
        assert!(program_ids.contains(&"MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG".to_string()));
        assert!(program_ids.contains(&"DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M".to_string()));
    }

    #[test]
    fn test_program_names_mapping() {
        let scanner = InstructionScanner::new();
        
        let pumpfun = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap();
        let pumpswap = Pubkey::from_str("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA").unwrap();
        
        assert_eq!(scanner.program_names.get(&pumpfun), Some(&"PumpFun"));
        assert_eq!(scanner.program_names.get(&pumpswap), Some(&"PumpSwap"));
    }
}
