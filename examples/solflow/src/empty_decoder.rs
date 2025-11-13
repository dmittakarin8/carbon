//! Empty decoder collection for transaction processing without instruction decoding
//!
//! This module provides a minimal `InstructionDecoderCollection` implementation
//! that never decodes instructions. It's used when we only need transaction metadata
//! and don't need to decode individual instructions.

use {
    carbon_core::{
        collection::InstructionDecoderCollection,
        instruction::DecodedInstruction,
    },
    serde::Serialize,
    solana_instruction::Instruction,
};

/// Empty instruction type (never used, but required by trait)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum EmptyInstruction {
    /// Placeholder variant (never actually created)
    Empty,
}

/// Empty decoder collection that never decodes instructions
///
/// This implements `InstructionDecoderCollection` but always returns `None`
/// from `parse_instruction`, meaning no instructions are decoded. This is
/// useful when processing transactions at the metadata level only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct EmptyDecoderCollection;

impl InstructionDecoderCollection for EmptyDecoderCollection {
    type InstructionType = EmptyInstruction;

    fn parse_instruction(
        _instruction: &Instruction,
    ) -> Option<DecodedInstruction<Self>> {
        // Never decode anything - we only care about transaction metadata
        None
    }

    fn get_type(&self) -> Self::InstructionType {
        // This should never be called since parse_instruction always returns None
        EmptyInstruction::Empty
    }
}

