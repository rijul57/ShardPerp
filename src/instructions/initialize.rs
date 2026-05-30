use borsh::BorshSerialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    system_instruction, system_program,
    sysvar::{clock::Clock, rent::Rent, Sysvar},
};

use crate::error::EntropyError;
use crate::math::fixed_point::SCALE;
use crate::state::global::{GlobalState, GLOBAL_STATE_DISCRIMINATOR};
use crate::state::shard::{Shard, SHARD_DISCRIMINATOR};

/// Processes the Initialize instruction.
///
/// Expected Accounts:
/// * `[write, signer]` Payer - funds account creation
/// * `[write]` GlobalState PDA - seeds `[b"global-state"]`
/// * `[]` System Program
/// * `[write]` Shard PDAs - seeds `[b"shard", shard_id.to_le_bytes()]`
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], total_shards: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let payer_info = next_account_info(account_info_iter)?;
    let global_state_info = next_account_info(account_info_iter)?;
    let system_program_info = next_account_info(account_info_iter)?;

    // Validate system program
    if system_program_info.key != &system_program::ID {
        return Err(ProgramError::IncorrectProgramId);
    }

    let rent = Rent::get()?;
    let clock = Clock::get()?;

    // 1. Initialize GlobalState
    let (global_pda, global_bump) = Pubkey::find_program_address(&[b"global-state"], program_id);
    if global_pda != *global_state_info.key {
        msg!("Error: GlobalState PDA mismatch.");
        return Err(ProgramError::InvalidSeeds);
    }

    if !global_state_info.data_is_empty() {
        return Err(EntropyError::AlreadyInitialized.into());
    }

    let global_space = GlobalState::SIZE;
    let global_lamports = rent.minimum_balance(global_space);

    invoke_signed(
        &system_instruction::create_account(
            payer_info.key,
            global_state_info.key,
            global_lamports,
            global_space as u64,
            program_id,
        ),
        &[
            payer_info.clone(),
            global_state_info.clone(),
            system_program_info.clone(),
        ],
        &[&[b"global-state", &[global_bump]]],
    )?;

    // We set funding_sensitivity to SCALE (1.0 = 1e18 fixed point)
    let global_state = GlobalState {
        discriminator: GLOBAL_STATE_DISCRIMINATOR,
        bump: global_bump,
        total_shards,
        global_funding_accumulator: 0,
        funding_sensitivity: SCALE,
        last_reconcile_slot: clock.slot,
    };

    global_state.serialize(&mut *global_state_info.data.borrow_mut())?;
    msg!("GlobalState initialized with {} shards.", total_shards);

    // 2. Initialize Shards
    let remaining_accounts = account_info_iter.as_slice();
    if remaining_accounts.len() as u64 != total_shards {
        msg!(
            "Error: Expected {} shard accounts, got {}",
            total_shards,
            remaining_accounts.len()
        );
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let shard_space = Shard::SIZE;
    let shard_lamports = rent.minimum_balance(shard_space);

    for (i, shard_info) in remaining_accounts.iter().enumerate() {
        let shard_id = i as u64;
        let shard_id_bytes = shard_id.to_le_bytes();

        let (shard_pda, shard_bump) =
            Pubkey::find_program_address(&[b"shard", &shard_id_bytes], program_id);

        if shard_pda != *shard_info.key {
            msg!("Error: Shard PDA mismatch for shard_id {}", shard_id);
            return Err(ProgramError::InvalidSeeds);
        }

        if !shard_info.data_is_empty() {
            return Err(EntropyError::AlreadyInitialized.into());
        }

        invoke_signed(
            &system_instruction::create_account(
                payer_info.key,
                shard_info.key,
                shard_lamports,
                shard_space as u64,
                program_id,
            ),
            &[
                payer_info.clone(),
                shard_info.clone(),
                system_program_info.clone(),
            ],
            &[&[b"shard", &shard_id_bytes, &[shard_bump]]],
        )?;

        let shard = Shard {
            discriminator: SHARD_DISCRIMINATOR,
            bump: shard_bump,
            shard_id,
            local_funding_delta: 0,
            long_volume: 0,
            short_volume: 0,
            last_update_slot: clock.slot,
        };

        shard.serialize(&mut *shard_info.data.borrow_mut())?;
    }

    msg!("Successfully initialized {} shards.", total_shards);
    Ok(())
}
