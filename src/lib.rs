#![allow(unexpected_cfgs)]
pub mod error;
pub mod instructions;
pub mod math;
pub mod state;

use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

// Declare the program entrypoint
entrypoint!(process_instruction);

/// Instruction enum matching the specification
pub enum EntropyFundingInstruction {
    /// Initializes the GlobalState and Shards
    Initialize { total_shards: u64 },
    /// Updates position volume and local funding delta on a specific shard
    UpdatePosition {
        volume: u64,
        side: u8,
        funding_delta: i128,
    },
    /// Merges shard data into GlobalState and calculates entropy-adjusted funding
    Reconcile,
}

impl EntropyFundingInstruction {
    /// Unpacks a byte buffer into a [EntropyFundingInstruction]
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = input
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;

        Ok(match tag {
            0 => {
                let total_shards = rest
                    .get(..8)
                    .and_then(|slice| slice.try_into().ok())
                    .map(u64::from_le_bytes)
                    .ok_or(ProgramError::InvalidInstructionData)?;
                Self::Initialize { total_shards }
            }
            1 => {
                if rest.len() < 25 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let volume = u64::from_le_bytes(rest[0..8].try_into().unwrap());
                let side = rest[8];
                let funding_delta = i128::from_le_bytes(rest[9..25].try_into().unwrap());

                Self::UpdatePosition {
                    volume,
                    side,
                    funding_delta,
                }
            }
            2 => Self::Reconcile,
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }
}

/// Program entrypoint dispatcher
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = EntropyFundingInstruction::unpack(instruction_data)?;

    match instruction {
        EntropyFundingInstruction::Initialize { total_shards } => {
            instructions::initialize::process(program_id, accounts, total_shards)
        }
        EntropyFundingInstruction::UpdatePosition {
            volume,
            side,
            funding_delta,
        } => instructions::update_position::process(
            program_id,
            accounts,
            volume,
            side,
            funding_delta,
        ),
        EntropyFundingInstruction::Reconcile => {
            instructions::reconcile::process(program_id, accounts)
        }
    }
}
