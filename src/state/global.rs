use borsh::{BorshDeserialize, BorshSerialize};

/// Unique discriminator for the GlobalState account to prevent account spoofing
pub const GLOBAL_STATE_DISCRIMINATOR: u8 = 1;

/// The central state account for the entropy funding CRDT program.
/// PDA seeds = [b"global-state"]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct GlobalState {
    /// Must be set to GLOBAL_STATE_DISCRIMINATOR (1)
    pub discriminator: u8,
    /// The PDA bump seed
    pub bump: u8,
    /// Total number of shards allocated (N)
    pub total_shards: u64,
    /// Cumulative funding per unit, represented as an 18 decimal fixed-point integer.
    /// Example: 1_000_000_000_000_000_000 = 1.0
    pub global_funding_accumulator: i128,
    /// Scalar multiplier on entropy adjustment, represented as an 18 decimal fixed-point integer.
    /// Starts at 1.0 (1e18) by default.
    pub funding_sensitivity: i128,
    /// The Solana slot number when the last reconcile occurred
    pub last_reconcile_slot: u64,
}

impl GlobalState {
    /// Exact byte size of the GlobalState struct when serialized via Borsh:
    /// discriminator (1) + bump (1) + total_shards (8) +
    /// global_funding_accumulator (16) + funding_sensitivity (16) + last_reconcile_slot (8)
    /// Total = 50 bytes
    pub const SIZE: usize = 1 + 1 + 8 + 16 + 16 + 8;
}
