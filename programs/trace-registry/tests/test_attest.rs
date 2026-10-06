//! End-to-end tests: the compiled program runs inside LiteSVM against crafted
//! Program / ProgramData accounts, one per upgrade-authority state.
//!
//! Build the program first (`anchor build --arch v2`), because these tests load
//! target/deploy/trace_registry.so.

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
    let mut d = vec![0u8; 45 + 64];
    d[0..4].copy_from_slice(&3u32.to_le_bytes());
    d[4..12].copy_from_slice(&slot.to_le_bytes());
    if let Some(a) = authority {
        d[12] = 1;
        d[13..45].copy_from_slice(a.as_ref());
    }
    d
}

fn program_bytes(program_data: &Pubkey) -> Vec<u8> {
    let mut d = vec![0u8; 36];
    d[0..4].copy_from_slice(&2u32.to_le_bytes());
    d[4..36].copy_from_slice(program_data.as_ref());
    d
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
    attest_ix_with_config(payer, target, program_data, None)
}

fn attest_ix_with_config(
    payer: &Pubkey,
    target: &Pubkey,
    program_data: &Pubkey,
    multisig_config: Option<Pubkey>,
) -> Instruction {
    Instruction::new_with_bytes(
        trace_registry::id(),
        &trace_registry::instruction::Attest {}.data(),
        trace_registry::accounts::Attest {
            payer: *payer,
            target_program: *target,
            program_data: *program_data,
            multisig_config,
            attestation: attestation_pda(target),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn multisig_config_pda(id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"multisig", id.as_ref()], &trace_registry::id()).0
}

fn initialize_multisig_ix(
    payer: &Pubkey,
    id: Pubkey,
    members: Vec<Pubkey>,
    threshold: u8,
) -> Instruction {
    let config = multisig_config_pda(&id);
    Instruction::new_with_bytes(
        trace_registry::id(),
        &trace_registry::instruction::InitializeMultisig {
            id,
            members,
            threshold,
        }
        .data(),
        trace_registry::accounts::InitializeMultisig {
            payer: *payer,
            config,
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
    let msg = Message::new_with_blockhash(&[ix], Some(&env.payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&env.payer]).unwrap();
    env.svm.send_transaction(tx)
}

fn logs_of(res: &TransactionResult) -> Vec<String> {
    match res {
        Ok(m) => m.logs.clone(),
        Err(e) => e.meta.logs.clone(),
    }
}

fn event_count(res: &TransactionResult) -> usize {
    logs_of(res)
        .iter()
        .filter(|l| l.starts_with("Program data:"))
        .count()
}

#[allow(clippy::result_large_err)]
fn attest(env: &mut Env) -> TransactionResult {
    let ix = attest_ix(&env.payer.pubkey(), &env.target, &env.program_data);
    send(env, ix)
}

fn read_attestation(env: &Env) -> Attestation {
    let acc = env
        .svm
        .get_account(&attestation_pda(&env.target))
        .expect("attestation exists");
    let mut data: &[u8] = &acc.data;
    Attestation::try_deserialize(&mut data).unwrap()
}

fn expect_ok(res: &TransactionResult) {
    assert!(res.is_ok(), "expected success, logs: {:#?}", logs_of(res));
}

fn expect_err(res: &TransactionResult, code: &str) {
    assert!(res.is_err(), "expected failure with {code}");
    let joined = logs_of(res).join("\n");
    assert!(joined.contains(code), "expected {code}, logs: {joined}");
}

#[test]
fn bare_keypair_authority_is_single_key() {
    let key = Keypair::new().pubkey();
    let mut env = setup(Some(key));
    let res = attest(&mut env);
    expect_ok(&res);
    assert_eq!(
        event_count(&res),
        1,
        "first attestation emits only Attested"
    );

    let a = read_attestation(&env);
    assert_eq!(a.risk, RiskClass::SingleKey);
    assert_eq!(a.authority, Some(key));
    assert_eq!(a.target_program, env.target);
    assert_eq!(a.program_data, env.program_data);
    assert_eq!(a.last_deploy_slot, 1234);
    assert_eq!(a.attester, env.payer.pubkey());
    assert_eq!(a.refresh_count, 1);
}

#[test]
fn pda_authority_is_program_controlled() {
    let (pda, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
    let mut env = setup(Some(pda));
    expect_ok(&attest(&mut env));
    let a = read_attestation(&env);
    assert_eq!(a.risk, RiskClass::ProgramControlled);
    assert_eq!(a.authority, Some(pda));
}

#[test]
fn trace_multisig_authority_is_multisig_protected() {
    let id = Pubkey::new_unique();
    let config = multisig_config_pda(&id);
    let authority = trace_registry::multisig_authority(&config);
    let mut env = setup(Some(authority));
    let mut members = vec![
        Keypair::new().pubkey(),
        Keypair::new().pubkey(),
        Keypair::new().pubkey(),
    ];
    members.sort_by_key(|member| member.to_bytes());
    let payer = env.payer.pubkey();
    let target = env.target;
    let program_data = env.program_data;

    expect_ok(&send(
        &mut env,
        initialize_multisig_ix(&payer, id, members, 2),
    ));
    expect_ok(&send(
        &mut env,
        attest_ix_with_config(&payer, &target, &program_data, Some(config)),
    ));
    let attestation = read_attestation(&env);
    assert_eq!(attestation.risk, RiskClass::MultisigProtected);
    assert_eq!(attestation.multisig_config, Some(config));
    assert_eq!(attestation.multisig_threshold, 2);
    assert_eq!(attestation.multisig_member_count, 3);
}

#[test]
fn revoked_authority_is_immutable() {
    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    let a = read_attestation(&env);
    assert_eq!(a.risk, RiskClass::Immutable);
    assert_eq!(a.authority, None);
}

#[test]
fn authority_moving_through_all_three_states_is_tracked() {
    let key = Keypair::new().pubkey();
    let (pda, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
    let mut env = setup(Some(key));

    let r1 = attest(&mut env);
    expect_ok(&r1);
    assert_eq!(read_attestation(&env).risk, RiskClass::SingleKey);

    set_authority(&mut env, Some(pda));
    let r2 = attest(&mut env);
    expect_ok(&r2);
    assert_eq!(event_count(&r2), 2, "Attested + AuthorityChanged");
    let a = read_attestation(&env);
    assert_eq!(a.risk, RiskClass::ProgramControlled);
    assert_eq!(a.refresh_count, 2);

    set_authority(&mut env, None);
    let r3 = attest(&mut env);
    expect_ok(&r3);
    assert_eq!(event_count(&r3), 2, "Attested + AuthorityChanged");
    let a = read_attestation(&env);
    assert_eq!(a.risk, RiskClass::Immutable);
    assert_eq!(a.authority, None);
    assert_eq!(a.refresh_count, 3);

    // Refreshing with nothing changed emits only Attested.
    let r4 = attest(&mut env);
    expect_ok(&r4);
    assert_eq!(event_count(&r4), 1);
    assert_eq!(read_attestation(&env).refresh_count, 4);
}

#[test]
fn rejects_program_data_that_does_not_belong_to_the_target() {
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
fn rejects_target_not_owned_by_the_upgradeable_loader() {
    let mut env = setup(None);
    let legacy_loader = Pubkey::new_from_array(solana_sdk_ids::bpf_loader::ID.to_bytes());
    env.svm
        .set_account(
            env.target,
            account(
                include_bytes!(concat!(
                    env!("CARGO_TARGET_TMPDIR"),
                    "/../deploy/trace_registry.so"
                ))
                .to_vec(),
                legacy_loader,
                true,
            ),
        )
        .unwrap();
    expect_err(&attest(&mut env), "NotUpgradeable");
}

#[test]
fn rejects_program_data_with_the_wrong_owner() {
    let mut env = setup(None);
    env.svm
        .set_account(
            env.program_data,
            account(programdata_bytes(None, 1), system_program::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "BadProgramDataOwner");
}

#[test]
fn rejects_a_truncated_program_data_header() {
    let mut env = setup(None);
    env.svm
        .set_account(
            env.program_data,
            account(vec![0u8; 44], bpf_loader_upgradeable::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "MalformedProgramData");
}

#[test]
fn rejects_a_program_data_with_the_wrong_loader_tag() {
    let mut env = setup(None);
    let mut data = programdata_bytes(None, 1);
    data[0..4].copy_from_slice(&88u32.to_le_bytes());
    env.svm
        .set_account(
            env.program_data,
            account(data, bpf_loader_upgradeable::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "MalformedProgramData");
}

#[test]
fn rejects_a_program_data_with_an_invalid_authority_flag() {
    let mut env = setup(None);
    let mut data = programdata_bytes(None, 1);
    data[12] = 2;
    env.svm
        .set_account(
            env.program_data,
            account(data, bpf_loader_upgradeable::ID, false),
        )
        .unwrap();
    expect_err(&attest(&mut env), "MalformedProgramData");
}

#[test]
fn assert_safe_gates_on_risk() {
    let mut env = setup(Some(Keypair::new().pubkey()));
    expect_ok(&attest(&mut env));

    let ix = assert_safe_ix(&env.target, 0, RiskClass::ProgramControlled);
    expect_err(&send(&mut env, ix), "RiskTooHigh");
    let ix = assert_safe_ix(&env.target, 0, RiskClass::SingleKey);
    expect_ok(&send(&mut env, ix));

    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    let ix = assert_safe_ix(&env.target, 0, RiskClass::Immutable);
    expect_ok(&send(&mut env, ix));
}

#[test]
fn assert_safe_rejects_stale_attestations() {
    let mut env = setup(None);
    expect_ok(&attest(&mut env));
    env.svm.warp_to_slot(10_000);

    let ix = assert_safe_ix(&env.target, 0, RiskClass::Immutable);
    expect_err(&send(&mut env, ix), "AttestationStale");
    let ix = assert_safe_ix(&env.target, 100_000, RiskClass::Immutable);
    expect_ok(&send(&mut env, ix));
}
