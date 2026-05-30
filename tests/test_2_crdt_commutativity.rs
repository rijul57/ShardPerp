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

use shard_perp::{process_instruction, state::global::GlobalState};

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

/// Runs a full scenario: Init -> Update -> Reconcile (using the provided shard order)
async fn run_reconcile_scenario(reconcile_order: [usize; 4]) -> i128 {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new(
        "shard_perp",
        program_id,
        processor!(process_instruction),
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;
    let total_shards: u64 = 4;

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

    // 2. Deterministic Updates for Shards 0 through 3
    let updates = vec![
        (0, 1000u64, 0u8, 500_000_000i128),  // Shard 0: Long, +0.5 funding
        (1, 2000u64, 1u8, -100_000_000i128), // Shard 1: Short, -0.1 funding
        (2, 500u64, 0u8, 250_000_000i128),   // Shard 2: Long, +0.25 funding
        (3, 1500u64, 1u8, -50_000_000i128),  // Shard 3: Short, -0.05 funding
    ];

    for (target_shard_idx, volume, side, funding_delta) in updates {
        let trader = find_keypair_for_shard(total_shards, target_shard_idx);
        let target_shard = shard_pdas[target_shard_idx as usize];

        let update_accounts = vec![
            AccountMeta::new(trader.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(target_shard, false),
        ];

        let mut update_data = vec![1u8];
        update_data.extend_from_slice(&volume.to_le_bytes());
        update_data.push(side);
        update_data.extend_from_slice(&funding_delta.to_le_bytes());

        let update_ix = Instruction::new_with_bytes(program_id, &update_data, update_accounts);
        let update_tx = Transaction::new_signed_with_payer(
            &[update_ix],
            Some(&payer.pubkey()),
            &[&payer, &trader],
            recent_blockhash,
        );
        banks_client.process_transaction(update_tx).await.unwrap();
    }

    // 3. Reconcile using the custom order
    let mut reconcile_accounts = vec![AccountMeta::new(global_pda, false)];
    for &idx in &reconcile_order {
        reconcile_accounts.push(AccountMeta::new(shard_pdas[idx], false));
    }

    let reconcile_ix = Instruction::new_with_bytes(program_id, &[2u8], reconcile_accounts);
    let reconcile_tx = Transaction::new_signed_with_payer(
        &[reconcile_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Note: With the current reconcile.rs, this will fail if the order isn't [0,1,2,3].
    banks_client
        .process_transaction(reconcile_tx)
        .await
        .unwrap();

    // 4. Return the aggregated global funding
    let global_account = banks_client.get_account(global_pda).await.unwrap().unwrap();
    let global_state = GlobalState::try_from_slice(&global_account.data).unwrap();

    global_state.global_funding_accumulator
}

#[tokio::test]
async fn test_crdt_commutativity() {
    // Run scenario with standard sequential order
    let state_a = run_reconcile_scenario([0, 1, 2, 3]).await;

    // Run scenario with mixed order
    let state_b = run_reconcile_scenario([3, 1, 0, 2]).await;

    // Both should yield a bit-identical global_funding_accumulator of 600,000,000
    // (500_000_000 - 100_000_000 + 250_000_000 - 50_000_000)
    assert_eq!(state_a, 600_000_000);
    assert_eq!(state_b, 600_000_000);
    assert_eq!(state_a, state_b, "CRDT merge is not commutative!");
}
