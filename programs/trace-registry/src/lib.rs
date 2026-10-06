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
const LOADER_V4_METADATA_SIZE: usize = 48;
const MAX_DEPENDENCIES: usize = 16;

#[program]
pub mod trace_registry {
    use super::*;

    /// Recompute the target program's upgrade-authority risk from its on-chain
    /// accounts and store it. Permissionless and idempotent: calling it again
    /// refreshes the record, and emits `AuthorityChanged` if the authority moved.
    pub fn attest(ctx: Context<Attest>) -> Result<()> {
        let target = &ctx.accounts.target_program;

        // 1. The target must be an executable program owned by a supported loader.
        require!(target.executable, TraceError::TargetNotExecutable);
        require!(
            is_supported_loader(target.owner),
            TraceError::NotUpgradeable
        );

        let loader = classify_loader(target.owner);
        let (program_data_key, authority, last_deploy_slot) = match loader {
            ProgramLoader::Upgradeable => {
                let program_data = ctx
                    .accounts
                    .program_data
                    .as_ref()
                    .ok_or(TraceError::ProgramDataRequired)?;
                let data = target.try_borrow_data()?;
                let linked = parse_program_account(&data)?;
                require_keys_eq!(linked, program_data.key(), TraceError::ProgramDataMismatch);
                require_keys_eq!(
                    *program_data.owner,
                    bpf_loader_upgradeable::ID,
                    TraceError::BadProgramDataOwner
                );

                // Read only the fixed header; the bytecode is never touched.
                let data = program_data.try_borrow_data()?;
                let header = parse_programdata_header(&data)?;
                (Some(program_data.key()), header.authority, header.slot)
            }
            ProgramLoader::Legacy => {
                require!(
                    ctx.accounts.program_data.is_none(),
                    TraceError::UnexpectedProgramData
                );
                (None, None, 0)
            }
            ProgramLoader::V4 => {
                require!(
                    ctx.accounts.program_data.is_none(),
                    TraceError::UnexpectedProgramData
                );
                let data = target.try_borrow_data()?;
                let header = parse_loader_v4_header(&data)?;
                (None, header.authority, header.slot)
            }
        };
        let (risk, multisig_config) = classify_attestation(
            authority,
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
        att.program_data = program_data_key;
        att.authority = authority;
        att.risk = risk;
        att.multisig_config = multisig_config.as_ref().and(
            ctx.accounts
                .multisig_config
                .as_ref()
                .map(|account| account.key()),
        );
        att.multisig_threshold = multisig_config.map_or(0, |config| config.threshold);
        att.multisig_member_count = multisig_config.map_or(0, |config| config.member_count);
        att.loader = loader;
        att.last_deploy_slot = last_deploy_slot;
        att.attested_slot = clock.slot;
        att.attested_at = clock.unix_timestamp;
        att.attester = ctx.accounts.payer.key();
        att.refresh_count = att.refresh_count.saturating_add(1);
        att.bump = ctx.bumps.attestation;

        if !is_new && previous_authority != authority {
            emit!(AuthorityChanged {
                target_program: target.key(),
                old_authority: previous_authority,
                new_authority: authority,
                slot: clock.slot,
            });
        }
        emit!(Attested {
            target_program: target.key(),
            authority,
            risk,
            multisig_config: att.multisig_config,
            loader,
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

    pub fn initialize_safe_registry(ctx: Context<InitializeSafeRegistry>) -> Result<()> {
        super::initialize_safe_registry_handler(ctx)
    }

    pub fn register_safe_program(
        ctx: Context<RegisterSafeProgram>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        super::register_safe_program_handler(ctx, max_age_slots, max_risk)
    }

    pub fn assert_known_safe(
        ctx: Context<AssertKnownSafe>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let record = &ctx.accounts.safe_program;
        let attestation = &ctx.accounts.attestation;
        require!(record.enabled, TraceError::SafeProgramDisabled);
        require!(
            clock.slot.saturating_sub(attestation.attested_slot) <= max_age_slots,
            TraceError::AttestationStale
        );
        require!(
            attestation.risk.severity() <= record.max_risk.severity()
                && attestation.risk.severity() <= max_risk.severity(),
            TraceError::RiskTooHigh
        );
        Ok(())
    }

    pub fn record_cpi_dependency(
        ctx: Context<RecordCpiDependency>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        super::record_cpi_dependency_handler(ctx, max_age_slots, max_risk)
    }

    pub fn assert_dependencies_safe(
        ctx: Context<AssertDependenciesSafe>,
        max_age_slots: u64,
        max_risk: RiskClass,
    ) -> Result<()> {
        let current_slot = Clock::get()?.slot;
        for dependency in &ctx.accounts.manifest.dependencies {
            require!(
                current_slot.saturating_sub(dependency.attested_slot) <= max_age_slots,
                TraceError::AttestationStale
            );
            require!(
                dependency.risk.severity() <= max_risk.severity(),
                TraceError::RiskTooHigh
            );
        }
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Attest<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: validated by hand in `attest` (executable, loader-owned, ProgramData link).
    pub target_program: UncheckedAccount<'info>,
    /// CHECK: validated by hand in `attest` (must equal the address the target links to).
    pub program_data: Option<UncheckedAccount<'info>>,
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
    pub program_data: Option<Pubkey>,
    pub authority: Option<Pubkey>,
    pub risk: RiskClass,
    pub multisig_config: Option<Pubkey>,
    pub multisig_threshold: u8,
    pub multisig_member_count: u8,
    pub loader: ProgramLoader,
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum ProgramLoader {
    Upgradeable,
    Legacy,
    V4,
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
    pub loader: ProgramLoader,
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

#[event]
pub struct DependencyObserved {
    pub parent_program: Pubkey,
    pub dependency_program: Pubkey,
    pub risk: RiskClass,
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
    #[msg("Upgradeable targets require a ProgramData account")]
    ProgramDataRequired,
    #[msg("This loader does not use a ProgramData account")]
    UnexpectedProgramData,
    #[msg("The attestation does not belong to the requested target")]
    AttestationTargetMismatch,
    #[msg("The safe-program record is disabled")]
    SafeProgramDisabled,
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
    #[msg("The CPI caller is not the declared parent program")]
    CpiCallerMismatch,
    #[msg("The dependency account is not executable")]
    DependencyNotExecutable,
    #[msg("The dependency attestation targets a different program")]
    DependencyAttestationTargetMismatch,
    #[msg("The dependency manifest belongs to a different parent")]
    DependencyManifestTargetMismatch,
    #[msg("The dependency manifest is full")]
    DependencyManifestFull,
}

// ---------------------------------------------------------------------------
// Pure logic (no account access), unit-tested below.
// ---------------------------------------------------------------------------

pub struct ProgramDataHeader {
    pub slot: u64,
    pub authority: Option<Pubkey>,
}

pub struct LoaderV4Header {
    pub slot: u64,
    pub authority: Option<Pubkey>,
}

pub fn classify_loader(owner: &Pubkey) -> ProgramLoader {
    if owner.as_ref() == bpf_loader_upgradeable::ID.as_ref() {
        ProgramLoader::Upgradeable
    } else if owner.as_ref() == solana_sdk_ids::bpf_loader::ID.as_ref()
        || owner.as_ref() == solana_sdk_ids::bpf_loader_deprecated::ID.as_ref()
    {
        ProgramLoader::Legacy
    } else if owner.as_ref() == solana_sdk_ids::loader_v4::ID.as_ref() {
        ProgramLoader::V4
    } else {
        // The caller converts this sentinel into the normal loader error before use.
        ProgramLoader::Upgradeable
    }
}

pub fn is_supported_loader(owner: &Pubkey) -> bool {
    owner.as_ref() == bpf_loader_upgradeable::ID.as_ref()
        || owner.as_ref() == solana_sdk_ids::bpf_loader::ID.as_ref()
        || owner.as_ref() == solana_sdk_ids::bpf_loader_deprecated::ID.as_ref()
        || owner.as_ref() == solana_sdk_ids::loader_v4::ID.as_ref()
}

pub fn parse_loader_v4_header(data: &[u8]) -> Result<LoaderV4Header> {
    require!(
        data.len() >= LOADER_V4_METADATA_SIZE,
        TraceError::MalformedProgramAccount
    );
    let slot = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let authority = Pubkey::new_from_array(data[8..40].try_into().unwrap());
    let status = u64::from_le_bytes(data[40..48].try_into().unwrap());
    require!(status <= 2, TraceError::MalformedProgramAccount);
    Ok(LoaderV4Header {
        slot,
        authority: (status != 2).then_some(authority),
    })
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

#[derive(Accounts)]
pub struct InitializeSafeRegistry<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + SafeRegistry::INIT_SPACE,
        seeds = [b"safe-registry"],
        bump
    )]
    pub registry: Account<'info, SafeRegistry>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterSafeProgram<'info> {
    #[account(mut, has_one = authority)]
    pub registry: Account<'info, SafeRegistry>,
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: the address is bound to both the attestation and record PDAs.
    pub target_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"attestation", target_program.key().as_ref()],
        bump = attestation.bump
    )]
    pub attestation: Account<'info, Attestation>,
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + SafeProgram::INIT_SPACE,
        seeds = [b"safe-program", target_program.key().as_ref()],
        bump
    )]
    pub safe_program: Account<'info, SafeProgram>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AssertKnownSafe<'info> {
    /// CHECK: only its address is used to derive the records.
    pub target_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"attestation", target_program.key().as_ref()],
        bump = attestation.bump
    )]
    pub attestation: Account<'info, Attestation>,
    #[account(
        seeds = [b"safe-program", target_program.key().as_ref()],
        bump = safe_program.bump
    )]
    pub safe_program: Account<'info, SafeProgram>,
}

#[derive(Accounts)]
pub struct RecordCpiDependency<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: executable status and caller identity are checked in the handler.
    pub parent_program: UncheckedAccount<'info>,
    /// CHECK: executable status is checked in the handler.
    pub dependency_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"attestation", dependency_program.key().as_ref()],
        bump = dependency_attestation.bump
    )]
    pub dependency_attestation: Account<'info, Attestation>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + DependencyManifest::INIT_SPACE,
        seeds = [b"dependencies", parent_program.key().as_ref()],
        bump
    )]
    pub manifest: Account<'info, DependencyManifest>,
    /// CHECK: must be the instructions sysvar.
    #[account(address = solana_instructions_sysvar::ID)]
    pub instructions: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AssertDependenciesSafe<'info> {
    /// CHECK: only its address is used to derive the manifest PDA.
    pub parent_program: UncheckedAccount<'info>,
    #[account(
        seeds = [b"dependencies", parent_program.key().as_ref()],
        bump = manifest.bump
    )]
    pub manifest: Account<'info, DependencyManifest>,
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

pub fn initialize_safe_registry_handler(ctx: Context<InitializeSafeRegistry>) -> Result<()> {
    let registry = &mut ctx.accounts.registry;
    registry.authority = ctx.accounts.payer.key();
    registry.bump = ctx.bumps.registry;
    Ok(())
}

pub fn register_safe_program_handler(
    ctx: Context<RegisterSafeProgram>,
    max_age_slots: u64,
    max_risk: RiskClass,
) -> Result<()> {
    let clock = Clock::get()?;
    let attestation = &ctx.accounts.attestation;
    require_keys_eq!(
        attestation.target_program,
        ctx.accounts.target_program.key(),
        TraceError::AttestationTargetMismatch
    );
    require!(
        clock.slot.saturating_sub(attestation.attested_slot) <= max_age_slots,
        TraceError::AttestationStale
    );
    require!(
        attestation.risk.severity() <= max_risk.severity(),
        TraceError::RiskTooHigh
    );

    let safe_program = &mut ctx.accounts.safe_program;
    safe_program.program_id = ctx.accounts.target_program.key();
    safe_program.max_risk = max_risk;
    safe_program.registered_at = clock.unix_timestamp;
    safe_program.attested_slot = attestation.attested_slot;
    safe_program.enabled = true;
    safe_program.bump = ctx.bumps.safe_program;
    Ok(())
}

#[account]
#[derive(InitSpace)]
pub struct SafeRegistry {
    pub authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct SafeProgram {
    pub program_id: Pubkey,
    pub max_risk: RiskClass,
    pub registered_at: i64,
    pub attested_slot: u64,
    pub enabled: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct DependencyManifest {
    pub target_program: Pubkey,
    #[max_len(16)]
    pub dependencies: Vec<DependencyEntry>,
    pub last_observed_slot: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub struct DependencyEntry {
    pub program_id: Pubkey,
    pub risk: RiskClass,
    pub attested_slot: u64,
    pub first_observed_slot: u64,
    pub last_observed_slot: u64,
}

pub fn record_cpi_dependency_handler(
    ctx: Context<RecordCpiDependency>,
    max_age_slots: u64,
    max_risk: RiskClass,
) -> Result<()> {
    require!(
        ctx.accounts.parent_program.executable,
        TraceError::TargetNotExecutable
    );
    require!(
        ctx.accounts.dependency_program.executable,
        TraceError::DependencyNotExecutable
    );

    let current_index = solana_instructions_sysvar::load_current_index_checked(
        &ctx.accounts.instructions.to_account_info(),
    )?;
    let caller = solana_instructions_sysvar::load_instruction_at_checked(
        usize::from(current_index),
        &ctx.accounts.instructions.to_account_info(),
    )?;
    require_keys_eq!(
        caller.program_id,
        ctx.accounts.parent_program.key(),
        TraceError::CpiCallerMismatch
    );

    let current_slot = Clock::get()?.slot;
    let attestation = &ctx.accounts.dependency_attestation;
    require_keys_eq!(
        attestation.target_program,
        ctx.accounts.dependency_program.key(),
        TraceError::DependencyAttestationTargetMismatch
    );
    require!(
        current_slot.saturating_sub(attestation.attested_slot) <= max_age_slots,
        TraceError::AttestationStale
    );
    require!(
        attestation.risk.severity() <= max_risk.severity(),
        TraceError::RiskTooHigh
    );

    let manifest = &mut ctx.accounts.manifest;
    if manifest.target_program == Pubkey::default() {
        manifest.target_program = ctx.accounts.parent_program.key();
    }
    require_keys_eq!(
        manifest.target_program,
        ctx.accounts.parent_program.key(),
        TraceError::DependencyManifestTargetMismatch
    );
    observe_dependency(
        &mut manifest.dependencies,
        ctx.accounts.dependency_program.key(),
        attestation.risk,
        attestation.attested_slot,
        current_slot,
    )?;
    manifest.last_observed_slot = current_slot;
    manifest.bump = ctx.bumps.manifest;
    emit!(DependencyObserved {
        parent_program: ctx.accounts.parent_program.key(),
        dependency_program: ctx.accounts.dependency_program.key(),
        risk: attestation.risk,
        slot: current_slot,
    });
    Ok(())
}

pub fn observe_dependency(
    dependencies: &mut Vec<DependencyEntry>,
    program_id: Pubkey,
    risk: RiskClass,
    attested_slot: u64,
    observed_slot: u64,
) -> Result<()> {
    if let Some(existing) = dependencies
        .iter_mut()
        .find(|dependency| dependency.program_id == program_id)
    {
        existing.risk = risk;
        existing.attested_slot = attested_slot;
        existing.last_observed_slot = observed_slot;
        return Ok(());
    }

    require!(
        dependencies.len() < MAX_DEPENDENCIES,
        TraceError::DependencyManifestFull
    );
    dependencies.push(DependencyEntry {
        program_id,
        risk,
        attested_slot,
        first_observed_slot: observed_slot,
        last_observed_slot: observed_slot,
    });
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
    fn dependency_observations_are_deduplicated() {
        let dependency = Pubkey::new_unique();
        let mut entries = Vec::new();
        observe_dependency(&mut entries, dependency, RiskClass::Immutable, 10, 20).unwrap();
        observe_dependency(&mut entries, dependency, RiskClass::SingleKey, 30, 40).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].risk, RiskClass::SingleKey);
        assert_eq!(entries[0].first_observed_slot, 20);
        assert_eq!(entries[0].last_observed_slot, 40);
    }

    #[test]
    fn severity_orders_risk() {
        assert!(RiskClass::Immutable.severity() < RiskClass::ProgramControlled.severity());
        assert!(RiskClass::MultisigProtected.severity() < RiskClass::ProgramControlled.severity());
        assert!(RiskClass::ProgramControlled.severity() < RiskClass::SingleKey.severity());
    }

    #[test]
    fn attestation_space_is_as_expected() {
        // 32 + 33 + 33 + 1 + 33 + 1 + 1 + 1 + 8 + 8 + 8 + 32 + 4 + 1
        assert_eq!(Attestation::INIT_SPACE, 196);
    }
}
