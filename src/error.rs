use solana_program::program_error::ProgramError;
use std::fmt;

/// Custom errors for the Entropy Funding CRDT program
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntropyError {
    /// 0. Invalid instruction data passed in
    InvalidInstruction,
    /// 1. A mathematical operation resulted in an overflow
    MathOverflow,
    /// 2. The provided shard account does not match the derived PDA for the given trader
    InvalidShardAccount,
    /// 3. The account has already been initialized
    AlreadyInitialized,
    /// 4. The account is missing or uninitialized
    UninitializedAccount,
}

impl From<EntropyError> for ProgramError {
    fn from(e: EntropyError) -> Self {
        // Cast the enum variant to a u32 for the ProgramError::Custom variant
        ProgramError::Custom(e as u32)
    }
}

impl fmt::Display for EntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntropyError::InvalidInstruction => write!(f, "Invalid Instruction"),
            EntropyError::MathOverflow => write!(f, "Math Overflow in fixed-point operations"),
            EntropyError::InvalidShardAccount => {
                write!(f, "Invalid Shard Account provided for routing")
            }
            EntropyError::AlreadyInitialized => write!(f, "Account is already initialized"),
            EntropyError::UninitializedAccount => write!(f, "Account is not initialized"),
        }
    }
}

impl std::error::Error for EntropyError {}
