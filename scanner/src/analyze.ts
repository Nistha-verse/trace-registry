import { PublicKey } from '@solana/web3.js'

export const UPGRADEABLE_LOADER_V3 = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111')
export const SYSTEM_PROGRAM = new PublicKey('11111111111111111111111111111111')
export const SQUADS_V4 = new PublicKey('SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf')
export const SQUADS_V3 = new PublicKey('SMPLecH534NA9acpos4G6x7uf3LWbCAwZQE9e8ZekMu')

const PROGRAM_TAG = 2
const PROGRAM_HEADER_SIZE = 36
const PROGRAMDATA_TAG = 3
const PROGRAMDATA_HEADER_SIZE = 45

export type RiskClass = 'Immutable' | 'ProgramControlled' | 'SingleKey'

export type ProgramDataHeader = {
  deploySlot: bigint
  authority: PublicKey | null
}

export type ReferenceMatch = {
  programId: string
  label: string
  occurrences: number
}

export type ScanResult = {
  programId: string
  programData: string
  authority: string | null
  risk: RiskClass
  lastDeploySlot: string
  inference: {
    controllerGuess: string | null
    controllerBasis: 'recent transaction history' | null
    references: ReferenceMatch[]
    referenceBasis: 'static bytecode address search'
  }
}

export class ScanError extends Error {}

export function parseProgramAccount(data: Uint8Array): PublicKey {
  if (data.length < PROGRAM_HEADER_SIZE) throw new ScanError('Program account is shorter than its 36-byte header')
  if (readU32(data, 0) !== PROGRAM_TAG) throw new ScanError('Program account has the wrong loader tag')
  return new PublicKey(data.slice(4, 36))
}

export function parseProgramDataHeader(data: Uint8Array): ProgramDataHeader {
  if (data.length < PROGRAMDATA_HEADER_SIZE) throw new ScanError('ProgramData account is shorter than its 45-byte header')
  if (readU32(data, 0) !== PROGRAMDATA_TAG) throw new ScanError('ProgramData account has the wrong loader tag')
  const deploySlot = readU64(data, 4)
  const option = data[12]
  if (option === 0) return { deploySlot, authority: null }
  if (option !== 1) throw new ScanError(`ProgramData authority option ${option} is invalid`)
  return { deploySlot, authority: new PublicKey(data.slice(13, 45)) }
}

export function classifyAuthority(authority: PublicKey | null): RiskClass {
  if (authority === null) return 'Immutable'
  return PublicKey.isOnCurve(authority.toBytes()) ? 'SingleKey' : 'ProgramControlled'
}

export function analyzeProgramData(programId: PublicKey, programData: PublicKey, header: ProgramDataHeader): ScanResult {
  return {
    programId: programId.toBase58(),
    programData: programData.toBase58(),
    authority: header.authority?.toBase58() ?? null,
    risk: classifyAuthority(header.authority),
    lastDeploySlot: header.deploySlot.toString(),
    inference: {
      controllerGuess: null,
      controllerBasis: null,
      references: [],
      referenceBasis: 'static bytecode address search',
    },
  }
}

export function scanReferences(bytecode: Uint8Array, knownPrograms: ReadonlyArray<{ id: PublicKey; label: string }>): ReferenceMatch[] {
  const matches: ReferenceMatch[] = []
  for (const program of knownPrograms) {
    if (program.id.equals(SYSTEM_PROGRAM)) continue
    const needle = program.id.toBytes()
    let occurrences = 0
    for (let offset = 0; offset <= bytecode.length - needle.length; offset += 1) {
      let match = true
      for (let index = 0; index < needle.length; index += 1) {
        if (bytecode[offset + index] !== needle[index]) {
          match = false
          break
        }
      }
      if (match) occurrences += 1
    }
    if (occurrences > 0) matches.push({ programId: program.id.toBase58(), label: program.label, occurrences })
  }
  return matches
}

export const DEFAULT_REFERENCE_PROGRAMS = [
  { id: UPGRADEABLE_LOADER_V3, label: 'Upgradeable BPF Loader v3' },
  { id: SQUADS_V4, label: 'Squads v4' },
  { id: SQUADS_V3, label: 'Squads v3' },
]

function readU32(data: Uint8Array, offset: number): number {
  return new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(offset, true)
}

function readU64(data: Uint8Array, offset: number): bigint {
  return new DataView(data.buffer, data.byteOffset, data.byteLength).getBigUint64(offset, true)
}
