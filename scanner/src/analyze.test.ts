import assert from 'node:assert/strict'
import { Keypair, PublicKey } from '@solana/web3.js'
import {
  DEFAULT_REFERENCE_PROGRAMS,
  ScanError,
  analyzeProgramData,
  classifyAuthority,
  parseProgramAccount,
  parseProgramDataHeader,
  scanReferences,
} from './analyze.js'

const key = Keypair.generate().publicKey
const [pda] = PublicKey.findProgramAddressSync([Buffer.from('vault')], key)

function programBytes(programData: PublicKey, tag = 2) {
  const data = new Uint8Array(36)
  new DataView(data.buffer).setUint32(0, tag, true)
  data.set(programData.toBytes(), 4)
  return data
}

function programDataBytes(authority: PublicKey | null, slot = 1234n, tag = 3, option = authority ? 1 : 0) {
  const data = new Uint8Array(45)
  const view = new DataView(data.buffer)
  view.setUint32(0, tag, true)
  view.setBigUint64(4, slot, true)
  data[12] = option
  if (authority) data.set(authority.toBytes(), 13)
  return data
}

let passed = 0
function test(name: string, run: () => void) {
  run()
  passed += 1
  console.log(`ok - ${name}`)
}

test('parses the Program tag and linked ProgramData address', () => {
  const programData = PublicKey.unique()
  assert.equal(parseProgramAccount(programBytes(programData)).toBase58(), programData.toBase58())
})

test('rejects a truncated Program account', () => {
  assert.throws(() => parseProgramAccount(new Uint8Array(35)), ScanError)
})

test('rejects a Program account with the wrong tag', () => {
  assert.throws(() => parseProgramAccount(programBytes(PublicKey.unique(), 9)), /wrong loader tag/)
})

test('parses an immutable ProgramData header', () => {
  const header = parseProgramDataHeader(programDataBytes(null))
  assert.equal(header.deploySlot, 1234n)
  assert.equal(header.authority, null)
})

test('parses an authority-bearing ProgramData header', () => {
  const header = parseProgramDataHeader(programDataBytes(key))
  assert.equal(header.authority?.toBase58(), key.toBase58())
})

test('rejects malformed ProgramData tags and option flags', () => {
  assert.throws(() => parseProgramDataHeader(programDataBytes(null, 1n, 8)), /wrong loader tag/)
  assert.throws(() => parseProgramDataHeader(programDataBytes(null, 1n, 3, 2)), /invalid/)
})

test('classifies no authority as Immutable', () => {
  assert.equal(classifyAuthority(null), 'Immutable')
})

test('classifies an on-curve authority as SingleKey', () => {
  assert.equal(classifyAuthority(key), 'SingleKey')
})

test('classifies an off-curve authority as ProgramControlled', () => {
  assert.equal(classifyAuthority(pda), 'ProgramControlled')
})

test('does not infer a controller from account ownership', () => {
  const result = analyzeProgramData(PublicKey.unique(), PublicKey.unique(), { deploySlot: 1n, authority: pda })
  assert.equal(result.risk, 'ProgramControlled')
  assert.equal(result.inference.controllerGuess, null)
})

test('scans bytecode statically and excludes the all-zero System Program', () => {
  const bytes = new Uint8Array([...SOME_BYTES(DEFAULT_REFERENCE_PROGRAMS[1].id), ...SOME_BYTES(DEFAULT_REFERENCE_PROGRAMS[1].id)])
  const results = scanReferences(bytes, [...DEFAULT_REFERENCE_PROGRAMS, { id: new PublicKey('11111111111111111111111111111111'), label: 'System Program' }])
  assert.deepEqual(results, [{ programId: DEFAULT_REFERENCE_PROGRAMS[1].id.toBase58(), label: 'Squads v4', occurrences: 2 }])
})

console.log(`\n${passed} scanner tests passed`)

function SOME_BYTES(value: PublicKey): number[] {
  return [...value.toBytes()]
}
