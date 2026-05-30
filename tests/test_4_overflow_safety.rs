use borsh::BorshDeserialize;
use solana_program_test::*;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::Transaction,
};
use std::convert::TryInto;

use shard_perp::{
    math::fixed_point::SCALE, process_instruction, state::global::GlobalState,
};

/// Helper to find a keypair that deterministically routes to a specific shard
fn find_keypair_for_shard(total_shards: u64, target_shard: u64) -> Keypair {
    loop {
        let kp = Keypair::new();
        let pk_bytes = kp.pubkey().to_bytes();
        let prefix: [u8; 8] = pk_bytes[0..8].try_into().unwrap();
        let hash_val = u64::from_le_bytes(prefix);

        if hash_val % total_shards == target_shard {
            return kp;
        }
    }
}

#[tokio::test]
async fn test_overflow_safety() {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new(
        "shard_perp",
        program_id,
        processor!(process_instruction),
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;
    let total_shards: u64 = 2;

    let (global_pda, _) = Pubkey::find_program_address(&[b"global-state"], &program_id);

    // 1. Initialize
    let mut init_accounts = vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(global_pda, false),
        AccountMeta::new_readonly(system_program::id(), false),
    ];

    let mut shard_pdas = vec![];
    for i in 0..total_shards {
        let (shard_pda, _) =
            Pubkey::find_program_address(&[b"shard", &i.to_le_bytes()], &program_id);
        shard_pdas.push(shard_pda);
        init_accounts.push(AccountMeta::new(shard_pda, false));
    }

    let mut init_data = vec![0u8];
    init_data.extend_from_slice(&total_shards.to_le_bytes());

    let init_ix = Instruction::new_with_bytes(program_id, &init_data, init_accounts);
    let init_tx = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(init_tx).await.unwrap();

    // 2. Feed MAXIMUM safe u64 volume values.
    // If we used u64::MAX for both, the simple addition during `reconcile`
    // would overflow `u64`. We use `u64::MAX / 2` to test the upper bounds
    // of the i128 fixed-point scaling math without overflowing the u64 sum.
    let max_safe_vol = u64::MAX / 2;

    let trader_0 = find_keypair_for_shard(total_shards, 0);
    let trader_1 = find_keypair_for_shard(total_shards, 1);

    // Update Shard 0 (Long)
    let update_0_ix = Instruction::new_with_bytes(
        program_id,
        &{
            let mut data = vec![1u8];
            data.extend_from_slice(&max_safe_vol.to_le_bytes());
            data.push(0u8); // Long
            data.extend_from_slice(&1_000_000_000_000_000_000i128.to_le_bytes());
            data
        },
        vec![
            AccountMeta::new(trader_0.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(shard_pdas[0], false),
        ],
    );

    // Update Shard 1 (Short)
    let update_1_ix = Instruction::new_with_bytes(
        program_id,
        &{
            let mut data = vec![1u8];
            data.extend_from_slice(&max_safe_vol.to_le_bytes());
            data.push(1u8); // Short
            data.extend_from_slice(&(-1_000_000_000_000_000_000i128).to_le_bytes());
            data
        },
        vec![
            AccountMeta::new(trader_1.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(shard_pdas[1], false),
        ],
    );

    let update_tx = Transaction::new_signed_with_payer(
        &[update_0_ix, update_1_ix],
        Some(&payer.pubkey()),
        &[&payer, &trader_0, &trader_1],
        recent_blockhash,
    );
    // Assert the updates succeed
    banks_client.process_transaction(update_tx).await.unwrap();

    // 3. Reconcile
    let reconcile_ix = Instruction::new_with_bytes(
        program_id,
        &[2u8],
        vec![
            AccountMeta::new(global_pda, false),
            AccountMeta::new(shard_pdas[0], false),
            AccountMeta::new(shard_pdas[1], false),
        ],
    );

    let reconcile_tx = Transaction::new_signed_with_payer(
        &[reconcile_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // The critical assertion: if the math overflows, this unwrap() will panic
    // due to the custom MathOverflow error being returned.
    banks_client
        .process_transaction(reconcile_tx)
        .await
        .unwrap();

    // 4. Verify Final State
    let global_account = banks_client.get_account(global_pda).await.unwrap().unwrap();
    let global_state = GlobalState::try_from_slice(&global_account.data).unwrap();

    // Because the max volumes are perfectly balanced, entropy should be 1.0,
    // meaning the sensitivity remains at SCALE (1e18)
    assert_eq!(
        global_state.funding_sensitivity, SCALE,
        "Max volume math corrupted the entropy calculation"
    );

    // Global funding should have cleanly netted to 0
    assert_eq!(
        global_state.global_funding_accumulator, 0,
        "Funding accumulator did not net correctly"
    );
}
