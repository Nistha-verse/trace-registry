//! LiteSVM end-to-end coverage for the three loader-derived risk states.
//!
//! Build the program first (`anchor build --arch v2`) because these tests load
//! `target/deploy/trace_registry.so`.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{bpf_loader_upgradeable, instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::{types::TransactionResult, LiteSVM},
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    trace_registry::{Attestation, RiskClass},
};

struct Env {
    svm: LiteSVM,
    payer: Keypair,
    target: Pubkey,
    program_data: Pubkey,
}

fn programdata_bytes(authority: Option<Pubkey>, slot: u64) -> Vec<u8> {
    let mut data = vec![0u8; 45 + 64];
    data[0..4].copy_from_slice(&3u32.to_le_bytes());
    data[4..12].copy_from_slice(&slot.to_le_bytes());
    if let Some(authority) = authority {
        data[12] = 1;
        data[13..45].copy_from_slice(authority.as_ref());
    }
    data
}

fn program_bytes(program_data: &Pubkey) -> Vec<u8> {
    let mut data = vec![0u8; 36];
    data[0..4].copy_from_slice(&2u32.to_le_bytes());
    data[4..36].copy_from_slice(program_data.as_ref());
    data
}

fn account(data: Vec<u8>, owner: Pubkey, executable: bool) -> Account {
    Account {
        lamports: 10_000_000,
        data,
        owner,
        executable,
        rent_epoch: 0,
    }
}

fn setup(authority: Option<Pubkey>) -> Env {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/trace_registry.so"
    ));
    svm.add_program(trace_registry::id(), bytes).unwrap();
    svm.warp_to_slot(100);
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let target = Pubkey::new_unique();
    let program_data = Pubkey::new_unique();
    svm.set_account(
        target,
        account(
            program_bytes(&program_data),
            bpf_loader_upgradeable::ID,
            true,
        ),
    )
    .unwrap();
    svm.set_account(
        program_data,
        account(
            programdata_bytes(authority, 1234),
            bpf_loader_upgradeable::ID,
            false,
        ),
    )
    .unwrap();
    Env {
        svm,
        payer,
        target,
        program_data,
    }
}

fn set_authority(env: &mut Env, authority: Option<Pubkey>) {
    env.svm
        .set_account(
            env.program_data,
            account(
                programdata_bytes(authority, 1234),
                bpf_loader_upgradeable::ID,
                false,
            ),
        )
        .unwrap();
}

fn attestation_pda(target: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"attestation", target.as_ref()], &trace_registry::id()).0
}

fn attest_ix(payer: &Pubkey, target: &Pubkey, program_data: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        trace_registry::id(),
        &trace_registry::instruction::Attest {}.data(),
        trace_registry::accounts::Attest {
            payer: *payer,
            target_program: *target,
            program_data: *program_data,
            attestation: attestation_pda(target),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn assert_safe_ix(target: &Pubkey, max_age_slots: u64, max_risk: RiskClass) -> Instruction {
    Instruction::new_with_bytes(
        trace_registry::id(),
        &trace_registry::instruction::AssertSafe {
            max_age_slots,
            max_risk,
        }
        .data(),
        trace_registry::accounts::AssertSafe {
            target_program: *target,
            attestation: attestation_pda(target),
        }
        .to_account_metas(None),
    )
}

#[allow(clippy::result_large_err)]
fn send(env: &mut Env, ix: Instruction) -> TransactionResult {
    env.svm.expire_blockhash();
    let blockhash = env.svm.latest_blockhash();
    let message = Message::new_with_blockhash(&[ix], Some(&env.payer.pubkey()), &blockhash);
    let transaction =
        VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[&env.payer]).unwrap();
    env.svm.send_transaction(transaction)
}

fn logs_of(result: &TransactionResult) -> Vec<String> {
    match result {
        Ok(meta) => meta.logs.clone(),
        Err(error) => error.meta.logs.clone(),
    }
}

fn event_count(result: &TransactionResult) -> usize {
    logs_of(result)
        .iter()
        .filter(|log| log.starts_with("Program data:"))
        .count()
}

#[allow(clippy::result_large_err)]
fn attest(env: &mut Env) -> TransactionResult {
    let ix = attest_ix(&env.payer.pubkey(), &env.target, &env.program_data);
    send(env, ix)
}

fn read_attestation(env: &Env) -> Attestation {
    let account = env
        .svm
        .get_account(&attestation_pda(&env.target))
        .expect("attestation exists");
    let mut data: &[u8] = &account.data;
    Attestation::try_deserialize(&mut data).unwrap()
}

fn expect_ok(result: &TransactionResult) {
    assert!(
        result.is_ok(),
        "expected success, logs: {:#?}",
        logs_of(result)
    );
}

fn expect_err(result: &TransactionResult, code: &str) {
    assert!(result.is_err(), "expected failure with {code}");
    let logs = logs_of(result).join("\n");
    assert!(logs.contains(code), "expected {code}, logs: {logs}");
}

#[test]
fn bare_keypair_authority_is_single_key() {
    let authority = Keypair::new().pubkey();
    let mut env = setup(Some(authority));
    let result = attest(&mut env);
    expect_ok(&result);
    assert_eq!(event_count(&result), 1);
    let attestation = read_attestation(&env);
    assert_eq!(attestation.risk, RiskClass::SingleKey);
    assert_eq!(attestation.authority, Some(authority));
    assert_eq!(attestation.last_deploy_slot, 1234);
    assert_eq!(attestation.refresh_count, 1);
}

#[test]
fn pda_authority_is_program_controlled() {
    let (authority, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
    let mut env = setup(Some(authority));
    expect_ok(&attest(&mut env));
    assert_eq!(read_attestation(&env).risk, RiskClass::ProgramControlled);
}

#[test]
fn revoked_authority_is_immutable() {
    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    let attestation = read_attestation(&env);
    assert_eq!(attestation.risk, RiskClass::Immutable);
    assert_eq!(attestation.authority, None);
}

#[test]
fn authority_changes_emit_history_and_update_the_snapshot() {
    let first_authority = Keypair::new().pubkey();
    let (second_authority, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
    let mut env = setup(Some(first_authority));
    expect_ok(&attest(&mut env));

    set_authority(&mut env, Some(second_authority));
    let changed = attest(&mut env);
    expect_ok(&changed);
    assert_eq!(event_count(&changed), 2, "Attested + AuthorityChanged");
    assert_eq!(read_attestation(&env).risk, RiskClass::ProgramControlled);

    set_authority(&mut env, None);
    let revoked = attest(&mut env);
    expect_ok(&revoked);
    assert_eq!(event_count(&revoked), 2, "Attested + AuthorityChanged");
    assert_eq!(read_attestation(&env).risk, RiskClass::Immutable);
}

#[test]
fn refreshing_without_a_change_emits_only_attested() {
    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    let refreshed = attest(&mut env);
    expect_ok(&refreshed);
    assert_eq!(event_count(&refreshed), 1);
    assert_eq!(read_attestation(&env).refresh_count, 2);
}

#[test]
fn rejects_program_data_that_does_not_belong_to_target() {
    let mut env = setup(None);
    let other = Pubkey::new_unique();
    env.svm
        .set_account(
            other,
            account(
                programdata_bytes(None, 1),
                bpf_loader_upgradeable::ID,
                false,
            ),
        )
        .unwrap();
    let ix = attest_ix(&env.payer.pubkey(), &env.target, &other);
    expect_err(&send(&mut env, ix), "ProgramDataMismatch");
}

#[test]
fn rejects_non_executable_target() {
    let mut env = setup(None);
    env.svm
        .set_account(
            env.target,
            account(
                program_bytes(&env.program_data),
                bpf_loader_upgradeable::ID,
                false,
            ),
        )
        .unwrap();
    expect_err(&attest(&mut env), "TargetNotExecutable");
}

#[test]
fn rejects_bad_programdata_owner_and_header() {
    let mut env = setup(None);
    env.svm
        .set_account(
            env.program_data,
            account(vec![0u8; 44], bpf_loader_upgradeable::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "MalformedProgramData");

    env.svm
        .set_account(
            env.program_data,
            account(programdata_bytes(None, 1), system_program::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "BadProgramDataOwner");
}

#[test]
fn assert_safe_checks_risk_and_staleness() {
    let mut env = setup(Some(Keypair::new().pubkey()));
    let target = env.target;
    expect_ok(&attest(&mut env));
    expect_err(
        &send(
            &mut env,
            assert_safe_ix(&target, 0, RiskClass::ProgramControlled),
        ),
        "RiskTooHigh",
    );
    expect_ok(&send(
        &mut env,
        assert_safe_ix(&target, 0, RiskClass::SingleKey),
    ));

    env.svm.warp_to_slot(10_000);
    expect_err(
        &send(&mut env, assert_safe_ix(&target, 0, RiskClass::SingleKey)),
        "AttestationStale",
    );
}

#[test]
fn immutable_program_passes_the_strictest_gate() {
    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    let target = env.target;
    expect_ok(&send(
        &mut env,
        assert_safe_ix(&target, 0, RiskClass::Immutable),
    ));
}
