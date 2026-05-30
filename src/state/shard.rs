use borsh::{BorshDeserialize, BorshSerialize};

/// Unique discriminator for the Shard account to prevent account spoofing
pub const SHARD_DISCRIMINATOR: u8 = 2;

/// The parallelized shard state for the entropy funding CRDT program.
/// PDA seeds = [b"shard", shard_id.to_le_bytes()]
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct Shard {
    /// Must be set to SHARD_DISCRIMINATOR (2)
    pub discriminator: u8,
    /// The PDA bump seed
    pub bump: u8,
    /// The index of this shard (0..total_shards)
    pub shard_id: u64,
    /// Pending funding changes not yet merged into the global state
    pub local_funding_delta: i128,
    /// Cumulative long volume since the last reconcile
    pub long_volume: u64,
    /// Cumulative short volume since the last reconcile
    pub short_volume: u64,
    /// The Solana slot number when this shard was last written to
    pub last_update_slot: u64,
}

impl Shard {
    /// Exact byte size of the Shard struct when serialized via Borsh:
    /// discriminator (1) + bump (1) + shard_id (8) + local_funding_delta (16) +
    /// long_volume (8) + short_volume (8) + last_update_slot (8)
    /// Total = 50 bytes
    pub const SIZE: usize = 1 + 1 + 8 + 16 + 8 + 8 + 8;
}
