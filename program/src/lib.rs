//! Trace: a trustless on-chain registry of Solana upgrade-authority risk.
//!
//! The program stores only facts it recomputes from the target program and its
//! ProgramData account in the same transaction. There is no trusted attester.
//! An off-curve authority is classified as ProgramControlled because a PDA has
//! no private key; this says nothing about the threshold or timelock behind it.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

declare_id!("H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w");

const PROGRAM_TAG: u32 = 2;
const PROGRAM_HEADER_SIZE: usize = 36;
const PROGRAMDATA_TAG: u32 = 3;
/// ProgramData header: tag (4) + deploy slot (8) + authority option (1) + key (32).
const PROGRAMDATA_HEADER_SIZE: usize = 45;

#[program]
pub mod trace_registry {
    use super::*;

    /// Recompute a v3 upgradeable program's authority risk from loader state.
    pub fn attest(ctx: Context<Attest>) -> Result<()> {
        let target = &ctx.accounts.target_program;
        require!(target.executable, TraceError::TargetNotExecutable);
        require_keys_eq!(
            *target.owner,
            bpf_loader_upgradeable::ID,
            TraceError::NotUpgradeable
        );

        let target_data = target.try_borrow_data()?;
        let linked_program_data = parse_program_account(&target_data)?;
        require_keys_eq!(
            linked_program_data,
            ctx.accounts.program_data.key(),
            TraceError::ProgramDataMismatch
        );
        require_keys_eq!(
            *ctx.accounts.program_data.owner,
            bpf_loader_upgradeable::ID,
            TraceError::BadProgramDataOwner
        );

        // Only the fixed header is read. Bytecode is never inspected.
        let program_data = ctx.accounts.program_data.try_borrow_data()?;
        let header = parse_programdata_header(&program_data)?;
        let risk = classify(header.authority, is_on_curve);
        let clock = Clock::get()?;
        let attestation = &mut ctx.accounts.attestation;
        let is_new = attestation.refresh_count == 0;
        let previous_authority = attestation.authority;

        attestation.target_program = target.key();
        attestation.program_data = ctx.accounts.program_data.key();
        attestation.authority = header.authority;
        attestation.risk = risk;
        attestation.last_deploy_slot = header.slot;
        attestation.attested_slot = clock.slot;
        attestation.attested_at = clock.unix_timestamp;
        attestation.attester = ctx.accounts.payer.key();
        attestation.refresh_count = attestation.refresh_count.saturating_add(1);
        attestation.bump = ctx.bumps.attestation;

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
            program_data: ctx.accounts.program_data.key(),
            authority: header.authority,
            risk,
            last_deploy_slot: header.slot,
            slot: clock.slot,
            refresh_count: attestation.refresh_count,
        });
        Ok(())
    }

    /// Require a fresh attestation no riskier than `max_risk`.
    ///
    /// For a guarantee about the current loader state, call `attest` and this
    /// instruction in one transaction with `max_age_slots = 0`.
    pub fn assert_safe(
        ctx: Context<AssertSafe>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        let attestation = &ctx.accounts.attestation;
        let age = Clock::get()?.slot.saturating_sub(attestation.attested_slot);
        require!(age <= max_age_slots, TraceError::AttestationStale);
        require!(
            attestation.risk.severity() <= max_risk.severity(),
            TraceError::RiskTooHigh
        );
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Attest<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: validated by hand: executable, loader-owned, and linked to ProgramData.
    pub target_program: UncheckedAccount<'info>,
    /// CHECK: validated by hand: loader-owned and linked from the target Program account.
    pub program_data: UncheckedAccount<'info>,
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
    /// CHECK: only its address is used to derive the attestation PDA.
    pub target_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"attestation", target_program.key().as_ref()],
        bump = attestation.bump,
        has_one = target_program
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
    ProgramControlled,
    SingleKey,
}

impl RiskClass {
    pub fn severity(self) -> u8 {
        match self {
            Self::Immutable => 0,
            Self::ProgramControlled => 1,
            Self::SingleKey => 2,
        }
    }
}

#[event]
pub struct Attested {
    pub target_program: Pubkey,
    pub program_data: Pubkey,
    pub authority: Option<Pubkey>,
    pub risk: RiskClass,
    pub last_deploy_slot: u64,
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
    #[msg("Target is not owned by the upgradeable BPF loader v3")]
    NotUpgradeable,
    #[msg("Target program account is malformed")]
    MalformedProgramAccount,
    #[msg("ProgramData account does not belong to the target program")]
    ProgramDataMismatch,
    #[msg("ProgramData account is not owned by the upgradeable BPF loader v3")]
    BadProgramDataOwner,
    #[msg("ProgramData header is malformed")]
    MalformedProgramData,
    #[msg("The attestation is older than the allowed age")]
    AttestationStale,
    #[msg("The attested risk exceeds the caller's allowed maximum")]
    RiskTooHigh,
}

pub struct ProgramDataHeader {
    pub slot: u64,
    pub authority: Option<Pubkey>,
}

pub fn parse_program_account(data: &[u8]) -> Result<Pubkey> {
    require!(
        data.len() >= PROGRAM_HEADER_SIZE,
        TraceError::MalformedProgramAccount
    );
    let tag = u32::from_le_bytes(data[0..4].try_into().unwrap());
    require!(tag == PROGRAM_TAG, TraceError::MalformedProgramAccount);
    Ok(Pubkey::new_from_array(data[4..36].try_into().unwrap()))
}

pub fn parse_programdata_header(data: &[u8]) -> Result<ProgramDataHeader> {
    require!(
        data.len() >= PROGRAMDATA_HEADER_SIZE,
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
        Some(authority) if on_curve(&authority) => RiskClass::SingleKey,
        Some(_) => RiskClass::ProgramControlled,
    }
}

// Pubkey::is_on_curve is unavailable in the SBF build. Use the loader's
// documented Edwards25519 syscall instead: curve id 0 and return code 0 mean
// the 32 bytes encode a valid point.
#[cfg(any(target_os = "solana", target_arch = "bpf"))]
fn is_on_curve(key: &Pubkey) -> bool {
    let point = solana_curve25519::edwards::PodEdwardsPoint(key.to_bytes());
    solana_curve25519::edwards::validate_edwards(&point)
}

// Host tests inject the predicate so they never depend on an unavailable SBF syscall.
#[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
fn is_on_curve(_key: &Pubkey) -> bool {
    unreachable!("is_on_curve is only called on-chain; tests inject their own")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flag: u8, authority: Option<Pubkey>, tag: u32) -> Vec<u8> {
        let mut data = vec![0u8; PROGRAMDATA_HEADER_SIZE];
        data[0..4].copy_from_slice(&tag.to_le_bytes());
        data[4..12].copy_from_slice(&777u64.to_le_bytes());
        data[12] = flag;
        if let Some(authority) = authority {
            data[13..45].copy_from_slice(authority.as_ref());
        }
        data
    }

    fn host_on_curve(key: &Pubkey) -> bool {
        key.is_on_curve()
    }

    #[test]
    fn parses_program_and_programdata_headers() {
        let linked = Pubkey::new_unique();
        let mut program = vec![0u8; PROGRAM_HEADER_SIZE];
        program[0..4].copy_from_slice(&PROGRAM_TAG.to_le_bytes());
        program[4..36].copy_from_slice(linked.as_ref());
        assert_eq!(parse_program_account(&program).unwrap(), linked);

        let authority = Pubkey::new_unique();
        let parsed =
            parse_programdata_header(&header(1, Some(authority), PROGRAMDATA_TAG)).unwrap();
        assert_eq!(parsed.slot, 777);
        assert_eq!(parsed.authority, Some(authority));
    }

    #[test]
    fn rejects_malformed_headers() {
        assert!(parse_program_account(&[0u8; 35]).is_err());
        assert!(parse_programdata_header(&[0u8; 44]).is_err());
        assert!(parse_programdata_header(&header(0, None, 2)).is_err());
        assert!(parse_programdata_header(&header(2, None, PROGRAMDATA_TAG)).is_err());
    }

    #[test]
    fn classifies_the_three_authority_states() {
        let key = Pubkey::new_unique();
        let (pda, _) = Pubkey::find_program_address(&[b"vault"], &Pubkey::new_unique());
        assert_eq!(classify(None, host_on_curve), RiskClass::Immutable);
        assert_eq!(classify(Some(key), |_| true), RiskClass::SingleKey);
        assert_eq!(classify(Some(pda), |_| false), RiskClass::ProgramControlled);
    }

    #[test]
    fn risk_severity_is_ordered() {
        assert!(RiskClass::Immutable.severity() < RiskClass::ProgramControlled.severity());
        assert!(RiskClass::ProgramControlled.severity() < RiskClass::SingleKey.severity());
    }

    #[test]
    fn attestation_space_is_stable() {
        // 32 + 32 + 33 + 1 + 8 + 8 + 8 + 32 + 4 + 1
        assert_eq!(Attestation::INIT_SPACE, 159);
    }
}
