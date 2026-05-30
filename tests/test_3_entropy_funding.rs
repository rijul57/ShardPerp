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

/// Runs a full scenario and returns the resulting funding_sensitivity
async fn run_entropy_scenario(long_vol: u64, short_vol: u64) -> i128 {
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

    // 2. Deterministic Updates
    // Shard 0 handles the long volume
    if long_vol > 0 {
        let trader_0 = find_keypair_for_shard(total_shards, 0);
        let update_accounts = vec![
            AccountMeta::new(trader_0.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(shard_pdas[0], false),
        ];
        let mut update_data = vec![1u8];
        update_data.extend_from_slice(&long_vol.to_le_bytes());
        update_data.push(0u8); // 0 = long
        update_data.extend_from_slice(&0i128.to_le_bytes()); // funding delta is 0 for this test

        let update_ix = Instruction::new_with_bytes(program_id, &update_data, update_accounts);
        let update_tx = Transaction::new_signed_with_payer(
            &[update_ix],
            Some(&payer.pubkey()),
            &[&payer, &trader_0],
            recent_blockhash,
        );
        banks_client.process_transaction(update_tx).await.unwrap();
    }

    // Shard 1 handles the short volume
    if short_vol > 0 {
        let trader_1 = find_keypair_for_shard(total_shards, 1);
        let update_accounts = vec![
            AccountMeta::new(trader_1.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(shard_pdas[1], false),
        ];
        let mut update_data = vec![1u8];
        update_data.extend_from_slice(&short_vol.to_le_bytes());
        update_data.push(1u8); // 1 = short
        update_data.extend_from_slice(&0i128.to_le_bytes());

        let update_ix = Instruction::new_with_bytes(program_id, &update_data, update_accounts);
        let update_tx = Transaction::new_signed_with_payer(
            &[update_ix],
            Some(&payer.pubkey()),
            &[&payer, &trader_1],
            recent_blockhash,
        );
        banks_client.process_transaction(update_tx).await.unwrap();
    }

    // 3. Reconcile
    let reconcile_accounts = vec![
        AccountMeta::new(global_pda, false),
        AccountMeta::new(shard_pdas[0], false),
        AccountMeta::new(shard_pdas[1], false),
    ];

    let reconcile_ix = Instruction::new_with_bytes(program_id, &[2u8], reconcile_accounts);
    let reconcile_tx = Transaction::new_signed_with_payer(
        &[reconcile_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    banks_client
        .process_transaction(reconcile_tx)
        .await
        .unwrap();

    // 4. Return the new funding sensitivity
    let global_account = banks_client.get_account(global_pda).await.unwrap().unwrap();
    let global_state = GlobalState::try_from_slice(&global_account.data).unwrap();

    global_state.funding_sensitivity
}

#[tokio::test]
async fn test_entropy_funding_adjustments() {
    // Initial sensitivity is 1e18 (SCALE)

    // Scenario A: Perfectly balanced market (1000 long, 1000 short)
    let sensitivity_balanced = run_entropy_scenario(1000, 1000).await;

    // Expected: H(t) = 1.0. Multiplier = 1.0 + 0.1 * (1.0 - 1.0) = 1.0.
    // Sensitivity remains exactly unchanged.
    assert_eq!(
        sensitivity_balanced, SCALE,
        "Balanced market should not change sensitivity"
    );

    // Scenario B: Completely one-sided market (2000 long, 0 short)
    let sensitivity_imbalanced = run_entropy_scenario(2000, 0).await;

    // Expected: H(t) = 0. Multiplier = 1.0 + 0.1 * (1.0 - 0) = 1.1.
    // Sensitivity should increase by exactly 10% (1_100_000_000_000_000_000)
    assert!(
        sensitivity_imbalanced > SCALE,
        "Imbalanced market should increase sensitivity"
    );
    assert_eq!(
        sensitivity_imbalanced, 1_100_000_000_000_000_000,
        "Sensitivity should increase by exactly 10% for complete imbalance"
    );
}
