//! Trace: a trustless on-chain registry of Solana program upgrade-authority risk.
//!
//! Design rule: nothing is written to an attestation unless this program
//! recomputed it from on-chain accounts during the same transaction. There is
//! no trusted attester. Anyone can call `attest`, and the result is always the
//! chain's current truth.
//!
//! What can be proven from the chain alone:
//! - Immutable: upgrade authority is None.
//! - SingleKey: authority is an ordinary ed25519 keypair address.
//! - ProgramControlled: authority is a PDA; no private key exists, so only an
//!   on-chain program (multisig, governance, ...) can sign.
//!
//! What is NOT proven here: which program controls a PDA authority, or its
//! threshold/timelock. Those stay off-chain and are labelled as such.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

declare_id!("H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w");

/// UpgradeableLoaderState::Program tag.
const PROGRAM_TAG: u32 = 2;
/// 4 (tag) + 32 (ProgramData address).
const PROGRAM_METADATA_SIZE: usize = 36;
const MAX_MULTISIG_MEMBERS: usize = 8;
/// UpgradeableLoaderState::ProgramData tag.
const PROGRAMDATA_TAG: u32 = 3;
/// 4 (tag) + 8 (slot) + 1 (option flag) + 32 (authority).
const PROGRAMDATA_METADATA_SIZE: usize = 45;

#[program]
pub mod trace_registry {
    use super::*;

    /// Recompute the target program's upgrade-authority risk from its on-chain
    /// accounts and store it. Permissionless and idempotent: calling it again
    /// refreshes the record, and emits `AuthorityChanged` if the authority moved.
    pub fn attest(ctx: Context<Attest>) -> Result<()> {
        let target = &ctx.accounts.target_program;
        let program_data = &ctx.accounts.program_data;

        // 1. The target must be a real upgradeable program.
        require!(target.executable, TraceError::TargetNotExecutable);
        require_keys_eq!(
            *target.owner,
            bpf_loader_upgradeable::ID,
            TraceError::NotUpgradeable
        );

        // 2. Its Program account must point at exactly the ProgramData we were given.
        {
            let data = target.try_borrow_data()?;
            let linked = parse_program_account(&data)?;
            require_keys_eq!(linked, program_data.key(), TraceError::ProgramDataMismatch);
        }
        require_keys_eq!(
            *program_data.owner,
            bpf_loader_upgradeable::ID,
            TraceError::BadProgramDataOwner
        );

        // 3. Read only the fixed header; the (possibly megabytes of) bytecode is never touched.
        let header = {
            let data = program_data.try_borrow_data()?;
            parse_programdata_header(&data)?
        };
        let (risk, multisig_config) = classify_attestation(
            header.authority,
            ctx.accounts.multisig_config.as_deref(),
            is_on_curve,
        );

        // 4. Store it, and emit events so history survives overwrites.
        let clock = Clock::get()?;
        let att = &mut ctx.accounts.attestation;
        // refresh_count is 0 only for a freshly created account (it saturates, never wraps).
        let is_new = att.refresh_count == 0;
        let previous_authority = att.authority;

        att.target_program = target.key();
        att.program_data = program_data.key();
        att.authority = header.authority;
        att.risk = risk;
        att.multisig_config = multisig_config.as_ref().and(
            ctx.accounts
                .multisig_config
                .as_ref()
                .map(|account| account.key()),
        );
        att.multisig_threshold = multisig_config.map_or(0, |config| config.threshold);
        att.multisig_member_count = multisig_config.map_or(0, |config| config.member_count);
        att.last_deploy_slot = header.slot;
        att.attested_slot = clock.slot;
        att.attested_at = clock.unix_timestamp;
        att.attester = ctx.accounts.payer.key();
        att.refresh_count = att.refresh_count.saturating_add(1);
        att.bump = ctx.bumps.attestation;

        if !is_new && previous_authority != header.authority {
            emit!(AuthorityChanged {
                target_program: target.key(),
                old_authority: previous_authority,
                new_authority: header.authority,
                slot: clock.slot,
            });
        }
        emit!(Attested {
            target_program: target.key(),
            authority: header.authority,
            risk,
            multisig_config: att.multisig_config,
            slot: clock.slot,
            refresh_count: att.refresh_count,
        });
        Ok(())
    }

    /// Gate for other programs. Reverts unless the target's attestation is
    /// fresh enough and no riskier than `max_risk`.
    ///
    /// To get a guarantee about the CURRENT state, put `attest` and
    /// `assert_safe` in the same transaction and pass `max_age_slots = 0`.
    pub fn assert_safe(
        ctx: Context<AssertSafe>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        let att = &ctx.accounts.attestation;
        let age = Clock::get()?.slot.saturating_sub(att.attested_slot);
        require!(age <= max_age_slots, TraceError::AttestationStale);
        require!(
            att.risk.severity() <= max_risk.severity(),
            TraceError::RiskTooHigh
        );
        Ok(())
    }

    pub fn initialize_multisig(
        ctx: Context<InitializeMultisig>,
        id: Pubkey,
        members: Vec<Pubkey>,
        threshold: u8,
    ) -> Result<()> {
        super::initialize_multisig_handler(ctx, id, members, threshold)
    }
}

#[derive(Accounts)]
pub struct Attest<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: validated by hand in `attest` (executable, loader-owned, ProgramData link).
    pub target_program: UncheckedAccount<'info>,
    /// CHECK: validated by hand in `attest` (must equal the address the target links to).
    pub program_data: UncheckedAccount<'info>,
    pub multisig_config: Option<Account<'info, MultisigConfig>>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Attestation::INIT_SPACE,
        seeds = [b"attestation", target_program.key().as_ref()],
        bump
    )]
    pub attestation: Account<'info, Attestation>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AssertSafe<'info> {
    /// CHECK: only its address is used, to derive the attestation PDA.
    pub target_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"attestation", target_program.key().as_ref()],
        bump = attestation.bump
    )]
    pub attestation: Account<'info, Attestation>,
}

#[account]
#[derive(InitSpace)]
pub struct Attestation {
    pub target_program: Pubkey,
    pub program_data: Pubkey,
    pub authority: Option<Pubkey>,
    pub risk: RiskClass,
    pub multisig_config: Option<Pubkey>,
    pub multisig_threshold: u8,
    pub multisig_member_count: u8,
    /// Slot of the target's last deploy/upgrade, from the ProgramData header.
    pub last_deploy_slot: u64,
    pub attested_slot: u64,
    pub attested_at: i64,
    /// Informational only: whoever paid for this refresh. Not a trust anchor.
    pub attester: Pubkey,
    pub refresh_count: u32,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum RiskClass {
    Immutable,
    MultisigProtected,
    ProgramControlled,
    SingleKey,
}

impl RiskClass {
    /// Higher is riskier.
    pub fn severity(&self) -> u8 {
        match self {
            RiskClass::Immutable => 0,
            RiskClass::MultisigProtected => 1,
            RiskClass::ProgramControlled => 2,
            RiskClass::SingleKey => 3,
        }
    }
}

#[event]
pub struct Attested {
    pub target_program: Pubkey,
    pub authority: Option<Pubkey>,
    pub risk: RiskClass,
    pub multisig_config: Option<Pubkey>,
    pub slot: u64,
    pub refresh_count: u32,
}

#[event]
pub struct AuthorityChanged {
    pub target_program: Pubkey,
    pub old_authority: Option<Pubkey>,
    pub new_authority: Option<Pubkey>,
    pub slot: u64,
}

#[error_code]
pub enum TraceError {
    #[msg("Target account is not an executable program")]
    TargetNotExecutable,
    #[msg("Target is not owned by the upgradeable BPF loader")]
    NotUpgradeable,
    #[msg("Target program account is malformed")]
    MalformedProgramAccount,
    #[msg("ProgramData account does not belong to the target program")]
    ProgramDataMismatch,
    #[msg("ProgramData account is not owned by the upgradeable BPF loader")]
    BadProgramDataOwner,
    #[msg("ProgramData header is malformed")]
    MalformedProgramData,
    #[msg("Attestation is older than the allowed age")]
    AttestationStale,
    #[msg("Attested risk exceeds the caller's allowed maximum")]
    RiskTooHigh,
    #[msg("Multisig must have between one and eight members")]
    InvalidMultisigMemberCount,
    #[msg("Multisig threshold must be between one and the member count")]
    InvalidMultisigThreshold,
    #[msg("Multisig members must be strictly sorted and unique")]
    InvalidMultisigMembers,
}

// ---------------------------------------------------------------------------
// Pure logic (no account access), unit-tested below.
// ---------------------------------------------------------------------------

pub struct ProgramDataHeader {
    pub slot: u64,
    pub authority: Option<Pubkey>,
}

#[account]
#[derive(InitSpace)]
pub struct MultisigConfig {
    pub id: Pubkey,
    pub authority: Pubkey,
    pub threshold: u8,
    pub member_count: u8,
    #[max_len(8)]
    pub members: Vec<Pubkey>,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(id: Pubkey)]
pub struct InitializeMultisig<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + MultisigConfig::INIT_SPACE,
        seeds = [b"multisig", id.as_ref()],
        bump
    )]
    pub config: Account<'info, MultisigConfig>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_multisig_handler(
    ctx: Context<InitializeMultisig>,
    id: Pubkey,
    members: Vec<Pubkey>,
    threshold: u8,
) -> Result<()> {
    require!(
        !members.is_empty() && members.len() <= MAX_MULTISIG_MEMBERS,
        TraceError::InvalidMultisigMemberCount
    );
    require!(
        threshold > 0 && usize::from(threshold) <= members.len(),
        TraceError::InvalidMultisigThreshold
    );
    require!(
        members.windows(2).all(|window| window[0] < window[1]),
        TraceError::InvalidMultisigMembers
    );

    let config = &mut ctx.accounts.config;
    config.id = id;
    config.authority = multisig_authority(&config.key());
    config.threshold = threshold;
    config.member_count = members.len() as u8;
    config.members = members;
    config.bump = ctx.bumps.config;
    Ok(())
}

pub fn multisig_authority(config: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"multisig-authority", config.as_ref()], &crate::ID).0
}

pub fn classify_attestation(
    authority: Option<Pubkey>,
    config: Option<&MultisigConfig>,
    on_curve: impl Fn(&Pubkey) -> bool,
) -> (RiskClass, Option<&MultisigConfig>) {
    match authority {
        None => (RiskClass::Immutable, None),
        Some(authority) if on_curve(&authority) => (RiskClass::SingleKey, None),
        Some(authority) => match config {
            Some(config) if config.authority == authority => {
                (RiskClass::MultisigProtected, Some(config))
            }
            _ => (RiskClass::ProgramControlled, None),
        },
    }
}

pub fn parse_program_account(data: &[u8]) -> Result<Pubkey> {
    require!(
        data.len() >= PROGRAM_METADATA_SIZE,
        TraceError::MalformedProgramAccount
    );
    let tag = u32::from_le_bytes(data[0..4].try_into().unwrap());
    require!(tag == PROGRAM_TAG, TraceError::MalformedProgramAccount);
    Ok(Pubkey::new_from_array(data[4..36].try_into().unwrap()))
}

pub fn parse_programdata_header(data: &[u8]) -> Result<ProgramDataHeader> {
    require!(
        data.len() >= PROGRAMDATA_METADATA_SIZE,
        TraceError::MalformedProgramData
    );
    let tag = u32::from_le_bytes(data[0..4].try_into().unwrap());
    require!(tag == PROGRAMDATA_TAG, TraceError::MalformedProgramData);
    let slot = u64::from_le_bytes(data[4..12].try_into().unwrap());
    let authority = match data[12] {
        0 => None,
        1 => Some(Pubkey::new_from_array(data[13..45].try_into().unwrap())),
        _ => return err!(TraceError::MalformedProgramData),
    };
    Ok(ProgramDataHeader { slot, authority })
}

pub fn classify(authority: Option<Pubkey>, on_curve: impl Fn(&Pubkey) -> bool) -> RiskClass {
    match authority {
        None => RiskClass::Immutable,
        Some(a) if on_curve(&a) => RiskClass::SingleKey,
        Some(_) => RiskClass::ProgramControlled,
    }
}

#[cfg(target_os = "solana")]
fn is_on_curve(key: &Pubkey) -> bool {
    key.is_on_curve()
}

// Host builds (IDL generation, tests) never execute instruction logic.
#[cfg(not(target_os = "solana"))]
fn is_on_curve(_key: &Pubkey) -> bool {
    unreachable!("is_on_curve is only called on-chain; tests inject their own")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flag: u8, authority: Option<Pubkey>, tag: u32) -> Vec<u8> {
        let mut d = vec![0u8; PROGRAMDATA_METADATA_SIZE + 16];
        d[0..4].copy_from_slice(&tag.to_le_bytes());
        d[4..12].copy_from_slice(&777u64.to_le_bytes());
        d[12] = flag;
        if let Some(a) = authority {
            d[13..45].copy_from_slice(a.as_ref());
        }
        d
    }

    fn host_on_curve(k: &Pubkey) -> bool {
        k.is_on_curve()
    }

    #[test]
    fn parses_immutable_header() {
        let h = parse_programdata_header(&header(0, None, 3)).unwrap();
        assert_eq!(h.authority, None);
        assert_eq!(h.slot, 777);
    }

    #[test]
    fn parses_program_account_link() {
        let linked = Pubkey::new_unique();
        let mut data = vec![0u8; PROGRAM_METADATA_SIZE];
        data[0..4].copy_from_slice(&PROGRAM_TAG.to_le_bytes());
        data[4..36].copy_from_slice(linked.as_ref());
        assert_eq!(parse_program_account(&data).unwrap(), linked);
    }

    #[test]
    fn parses_header_with_authority() {
        let a = Pubkey::new_unique();
        let h = parse_programdata_header(&header(1, Some(a), 3)).unwrap();
        assert_eq!(h.authority, Some(a));
    }

    #[test]
    fn rejects_bad_tag_flag_and_truncation() {
        assert!(parse_programdata_header(&header(0, None, 2)).is_err());
        assert!(parse_programdata_header(&header(7, None, 3)).is_err());
        assert!(parse_programdata_header(&[0u8; 10]).is_err());
    }

    #[test]
    fn rejects_malformed_program_accounts() {
        assert!(parse_program_account(&[0u8; 35]).is_err());

        let mut data = vec![0u8; PROGRAM_METADATA_SIZE];
        data[0..4].copy_from_slice(&99u32.to_le_bytes());
        assert!(parse_program_account(&data).is_err());
    }

    #[test]
    fn classifies_all_three_states() {
        assert_eq!(classify(None, host_on_curve), RiskClass::Immutable);

        // An ed25519 public key is on the curve by construction.
        let kp_pub = {
            // Known on-curve point: the ed25519 base point compressed encoding.
            let mut b = [0u8; 32];
            b[0] = 0x58;
            for byte in b.iter_mut().take(31).skip(1) {
                *byte = 0x66;
            }
            b[31] = 0x66;
            Pubkey::new_from_array(b)
        };
        assert!(host_on_curve(&kp_pub), "base point must be on curve");
        assert_eq!(classify(Some(kp_pub), host_on_curve), RiskClass::SingleKey);

        let (pda, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
        assert!(!host_on_curve(&pda));
        assert_eq!(
            classify(Some(pda), host_on_curve),
            RiskClass::ProgramControlled
        );
    }

    #[test]
    fn only_a_matching_trace_multisig_is_multisig_protected() {
        let config = MultisigConfig {
            id: Pubkey::new_unique(),
            authority: Pubkey::new_unique(),
            threshold: 2,
            member_count: 3,
            members: vec![
                Pubkey::new_unique(),
                Pubkey::new_unique(),
                Pubkey::new_unique(),
            ],
            bump: 255,
        };

        let (risk, matched) =
            classify_attestation(Some(config.authority), Some(&config), |_| false);
        assert_eq!(risk, RiskClass::MultisigProtected);
        assert!(matched.is_some());

        let (risk, matched) =
            classify_attestation(Some(Pubkey::new_unique()), Some(&config), |_| false);
        assert_eq!(risk, RiskClass::ProgramControlled);
        assert!(matched.is_none());
    }

    #[test]
    fn severity_orders_risk() {
        assert!(RiskClass::Immutable.severity() < RiskClass::ProgramControlled.severity());
        assert!(RiskClass::MultisigProtected.severity() < RiskClass::ProgramControlled.severity());
        assert!(RiskClass::ProgramControlled.severity() < RiskClass::SingleKey.severity());
    }

    #[test]
    fn attestation_space_is_as_expected() {
        // 32 + 32 + 33 + 1 + 33 + 1 + 1 + 8 + 8 + 8 + 32 + 4 + 1
        assert_eq!(Attestation::INIT_SPACE, 194);
    }
}
