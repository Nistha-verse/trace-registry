import { Connection, PublicKey } from '@solana/web3.js'
import { writeFile } from 'node:fs/promises'
import { DEFAULT_REFERENCE_PROGRAMS, UPGRADEABLE_LOADER_V3, analyzeProgramData, parseProgramAccount, parseProgramDataHeader, scanReferences, type ScanResult } from './analyze.js'

const rpcUrl = process.env.HELIUS_RPC_URL ?? 'https://api.mainnet-beta.solana.com'
const connection = new Connection(rpcUrl, 'confirmed')
const requested = process.argv.slice(2)
const targets = requested.length > 0 ? requested : [
  'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
  'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb',
  'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4',
  '675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8',
]

async function scan(programId: string): Promise<ScanResult | { programId: string; error: string }> {
  try {
    const program = new PublicKey(programId)
    const account = await connection.getAccountInfo(program)
    if (!account) throw new Error('program account not found')
    if (!account.executable) throw new Error('account is not executable')
    if (!account.owner.equals(UPGRADEABLE_LOADER_V3)) throw new Error('only upgradeable loader v3 is supported')
    const programData = parseProgramAccount(account.data)
    const programDataAccount = await connection.getAccountInfo(programData)
    if (!programDataAccount) throw new Error('linked ProgramData account not found')
    if (!programDataAccount.owner.equals(UPGRADEABLE_LOADER_V3)) throw new Error('ProgramData has the wrong owner')
    const header = parseProgramDataHeader(programDataAccount.data)
    const result = analyzeProgramData(program, programData, header)
    result.inference.references = scanReferences(programDataAccount.data.slice(45), DEFAULT_REFERENCE_PROGRAMS)
    return result
  } catch (error) {
    return { programId, error: error instanceof Error ? error.message : String(error) }
  }
}

const results = await Promise.all(targets.map(scan))
console.log(JSON.stringify(results, null, 2))
const output = new URL('../scan-results.json', import.meta.url)
await writeFile(output, JSON.stringify(results, null, 2) + '\n')
