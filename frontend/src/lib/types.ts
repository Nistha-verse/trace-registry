export type AttestationView = {
  target_program: string
  program_data: string | null
  authority: string | null
  risk: 'immutable' | 'multisig_protected' | 'program_controlled' | 'single_key'
  multisig_config: string | null
  multisig_threshold: number
  multisig_member_count: number
  loader: 'upgradeable' | 'legacy' | 'loader_v4'
  last_deploy_slot: number
  attested_slot: number
  attested_at: number
  attester: string
  refresh_count: number
}

export type DependencyView = {
  program_id: string
  risk: AttestationView['risk']
  attested_slot: number
  first_observed_slot: number
  last_observed_slot: number
}

export type DependencyManifestView = {
  target_program: string
  dependencies: DependencyView[]
  last_observed_slot: number
}

export type ProgramReport = {
  target_program: string
  trace_program: string
  attestation_address: string
  attestation: AttestationView | null
  dependency_manifest_address: string
  dependencies: DependencyManifestView | null
}

export type HistoryRecord = {
  signature: string
  slot: number
  kind: 'attested' | 'authority_changed' | 'dependency_observed'
  target_program: string
  authority: string | null
  old_authority: string | null
  new_authority: string | null
  dependency_program: string | null
  risk: AttestationView['risk'] | null
  attested_slot: number | null
  refresh_count: number | null
}
