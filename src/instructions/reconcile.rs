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
use crate::math::entropy::compute_entropy;
use crate::math::fixed_point::{fp_mul, SCALE};
use crate::state::global::{GlobalState, GLOBAL_STATE_DISCRIMINATOR};
use crate::state::shard::{Shard, SHARD_DISCRIMINATOR};

/// The dampening constant (alpha) for funding sensitivity adjustments.
/// Set to 0.1 (1e17 in 18-decimal fixed-point).
const ALPHA: i128 = 100_000_000_000_000_000;

/// Processes the Reconcile instruction.
///
/// Expected Accounts:
/// 0. `[write]` GlobalState PDA
/// 1..N. `[write]` All Shard PDAs (Order agnostic)
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let global_state_info = next_account_info(account_info_iter)?;

    // 1. Validate and load GlobalState
    let (global_pda, _bump) = Pubkey::find_program_address(&[b"global-state"], program_id);
    if global_pda != *global_state_info.key {
        return Err(ProgramError::InvalidSeeds);
    }

    let mut global_state = GlobalState::try_from_slice(&global_state_info.data.borrow())
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if global_state.discriminator != GLOBAL_STATE_DISCRIMINATOR {
        return Err(ProgramError::InvalidAccountData);
    }

    // 2. Validate the correct number of shards are provided
    let remaining_accounts = account_info_iter.as_slice();
    if remaining_accounts.len() as u64 != global_state.total_shards {
        msg!(
            "Error: Reconcile requires all {} shards, got {}",
            global_state.total_shards,
            remaining_accounts.len()
        );
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let mut total_long_volume: u64 = 0;
    let mut total_short_volume: u64 = 0;

    // Keep track of processed shards to prevent duplicate account attacks
    let mut processed_shards = vec![false; global_state.total_shards as usize];

    // 3. CRDT Merge Loop (Order Agnostic)
    for shard_info in remaining_accounts.iter() {
        let mut shard = Shard::try_from_slice(&shard_info.data.borrow())
            .map_err(|_| ProgramError::InvalidAccountData)?;

        if shard.discriminator != SHARD_DISCRIMINATOR {
            return Err(ProgramError::InvalidAccountData);
        }

        let shard_id = shard.shard_id;

        // Prevent out-of-bounds or duplicate shards
        if shard_id >= global_state.total_shards {
            msg!("Error: Shard ID {} exceeds total shards", shard_id);
            return Err(ProgramError::InvalidAccountData);
        }
        if processed_shards[shard_id as usize] {
            msg!("Error: Shard ID {} was provided more than once", shard_id);
            return Err(ProgramError::InvalidArgument);
        }

        // Validate PDA based on the ID read from the data
        let shard_id_bytes = shard_id.to_le_bytes();
        let (expected_shard_pda, _shard_bump) =
            Pubkey::find_program_address(&[b"shard", &shard_id_bytes], program_id);

        if expected_shard_pda != *shard_info.key {
            msg!("Error: Invalid PDA for shard_id {}", shard_id);
            return Err(ProgramError::InvalidSeeds);
        }

        // Mark as processed
        processed_shards[shard_id as usize] = true;

        // Aggregate local funding delta to global and reset local to 0
        global_state.global_funding_accumulator = global_state
            .global_funding_accumulator
            .checked_add(shard.local_funding_delta)
            .ok_or(EntropyError::MathOverflow)?;
        shard.local_funding_delta = 0;

        // Aggregate volumes
        total_long_volume = total_long_volume
            .checked_add(shard.long_volume)
            .ok_or(EntropyError::MathOverflow)?;
        total_short_volume = total_short_volume
            .checked_add(shard.short_volume)
            .ok_or(EntropyError::MathOverflow)?;

        // Reset shard volumes to 0
        shard.long_volume = 0;
        shard.short_volume = 0;

        // Save shard state
        shard.serialize(&mut *shard_info.data.borrow_mut())?;
    }

    // 4. Entropy Calculation & Sensitivity Adjustment
    let current_entropy = compute_entropy(total_long_volume, total_short_volume)?;

    // H_max is 1.0 in a perfectly balanced market (two states: long/short)
    let h_max = SCALE;

    let entropy_deficit = h_max
        .checked_sub(current_entropy)
        .ok_or(EntropyError::MathOverflow)?;

    let adjustment = fp_mul(ALPHA, entropy_deficit)?;

    let multiplier = SCALE
        .checked_add(adjustment)
        .ok_or(EntropyError::MathOverflow)?;

    global_state.funding_sensitivity =(global_state.funding_sensitivity, multiplier)?;

    // 5. Finalize GlobalState
    let clock = Clock::get()?;
    global_state.last_reconcile_slot = clock.slot;

    global_state.serialize(&mut *global_state_info.data.borrow_mut())?;

    msg!(
        "Reconcile complete. H(t): {}, New Sensitivity: {}",
        current_entropy,
        global_state.funding_sensitivity
    );

    Ok(())
}
