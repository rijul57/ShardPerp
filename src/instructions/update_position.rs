use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{clock::Clock, Sysvar},
};

use crate::error::EntropyError;
use crate::state::global::{GlobalState, GLOBAL_STATE_DISCRIMINATOR};
use crate::state::shard::{Shard, SHARD_DISCRIMINATOR};

/// Processes the UpdatePosition instruction.
///
/// Expected Accounts:
/// 0. `[signer]` Trader - Their pubkey dictates which shard they are routed to
/// 1. `[]` GlobalState - Read-only, used to fetch `total_shards` for routing math
/// 2. `[write]` Shard PDA - The specific shard assigned to this trader
pub fn process(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    volume: u64,
    side: u8,
    funding_delta: i128,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let trader_info = next_account_info(account_info_iter)?;
    let global_state_info = next_account_info(account_info_iter)?;
    let shard_info = next_account_info(account_info_iter)?;

    if !trader_info.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // 1. Validate and load GlobalState (Read-Only)
    let (global_pda, _bump) = Pubkey::find_program_address(&[b"global-state"], program_id);
    if global_pda != *global_state_info.key {
        return Err(ProgramError::InvalidSeeds);
    }

    let global_state = GlobalState::try_from_slice(&global_state_info.data.borrow())
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if global_state.discriminator != GLOBAL_STATE_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }

    // 2. Deterministic Shard Routing
    // Take the first 8 bytes of the trader's pubkey and convert to u64
    let trader_bytes = trader_info.key.to_bytes();
    let pubkey_prefix: [u8; 8] = trader_bytes[0..8].try_into().unwrap();
    let trader_hash_val = u64::from_le_bytes(pubkey_prefix);

    // Route to a shard using modulo arithmetic against total_shards
    let target_shard_id = trader_hash_val
        .checked_rem(global_state.total_shards)
        .ok_or(EntropyError::MathOverflow)?;

    // 3. Validate Shard PDA
    let target_shard_bytes = target_shard_id.to_le_bytes();
    let (expected_shard_pda, _shard_bump) =
        Pubkey::find_program_address(&[b"shard", &target_shard_bytes], program_id);

    if expected_shard_pda != *shard_info.key {
        msg!(
            "Routing Error: Trader routed to shard {}, expected pubkey {}",
            target_shard_id,
            expected_shard_pda
        );
        return Err(EntropyError::InvalidShardAccount.into());
    }

    // 4. Update the Shard state
    let mut shard = Shard::try_from_slice(&shard_info.data.borrow())
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if shard.discriminator != SHARD_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }

    // CRDT operation: safely accumulate the local changes
    shard.local_funding_delta = shard
        .local_funding_delta
        .checked_add(funding_delta)
        .ok_or(EntropyError::MathOverflow)?;

    match side {
        0 => {
            // Long
            shard.long_volume = shard
                .long_volume
                .checked_add(volume)
                .ok_or(EntropyError::MathOverflow)?;
        }
        1 => {
            // Short
            shard.short_volume = shard
                .short_volume
                .checked_add(volume)
                .ok_or(EntropyError::MathOverflow)?;
        }
        _ => return Err(EntropyError::InvalidInstruction.into()),
    }

    let clock = Clock::get()?;
    shard.last_update_slot = clock.slot;

    // Serialize changes back to the shard account
    shard.serialize(&mut *shard_info.data.borrow_mut())?;

    Ok(())
}
