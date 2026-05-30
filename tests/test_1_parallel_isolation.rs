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

use shard_perp::{process_instruction, state::shard::Shard};

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
async fn test_parallel_isolation() {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new(
        "shard_perp",
        program_id,
        processor!(process_instruction),
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await;

    let total_shards: u64 = 4;

    // 1. Setup Accounts & Initialize
    let (global_pda, _) = Pubkey::find_program_address(&[b"global-state"], &program_id);

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

    let mut init_data = vec![0u8]; // Tag 0 = Initialize
    init_data.extend_from_slice(&total_shards.to_le_bytes());

    let init_ix = Instruction::new_with_bytes(program_id, &init_data, init_accounts);
    let init_tx = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(init_tx).await.unwrap();

    // 2. Mine 4 distinct traders that route to shards 0, 1, 2, and 3
    let traders: Vec<Keypair> = (0..total_shards)
        .map(|i| find_keypair_for_shard(total_shards, i))
        .collect();

    // 3. Execute isolated updates for each trader
    let volume: u64 = 1000;
    let side: u8 = 0; // Long
    let funding_delta: i128 = 500_000_000;

    for (i, trader) in traders.iter().enumerate() {
        let target_shard = shard_pdas[i];

        let update_accounts = vec![
            AccountMeta::new(trader.pubkey(), true),
            AccountMeta::new_readonly(global_pda, false),
            AccountMeta::new(target_shard, false), // Only THIS shard is mutable
        ];

        let mut update_data = vec![1u8]; // Tag 1 = UpdatePosition
        update_data.extend_from_slice(&volume.to_le_bytes());
        update_data.push(side);
        update_data.extend_from_slice(&funding_delta.to_le_bytes());

        let update_ix = Instruction::new_with_bytes(program_id, &update_data, update_accounts);

        let update_tx = Transaction::new_signed_with_payer(
            &[update_ix],
            Some(&payer.pubkey()),
            &[&payer, trader],
            recent_blockhash,
        );

        banks_client.process_transaction(update_tx).await.unwrap();
    }

    // 4. Assert perfect isolation
    for (i, _shard_pda) in shard_pdas.iter().enumerate() {
        let shard_account = banks_client
            .get_account(shard_pdas[i])
            .await
            .unwrap()
            .unwrap();
        let shard_state = Shard::try_from_slice(&shard_account.data).unwrap();

        // Each shard should have received exactly one update from its designated trader
        assert_eq!(
            shard_state.long_volume, volume,
            "Shard {} long volume mismatch",
            i
        );
        assert_eq!(
            shard_state.short_volume, 0,
            "Shard {} short volume should be 0",
            i
        );
        assert_eq!(
            shard_state.local_funding_delta, funding_delta,
            "Shard {} funding delta mismatch",
            i
        );
    }
}
