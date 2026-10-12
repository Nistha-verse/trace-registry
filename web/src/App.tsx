import { useMemo, useState, type FormEvent } from 'react'
import { AlertTriangle, ArrowUpRight, Check, CircleDot, FileSearch, Fingerprint, LockKeyhole, Search, ShieldCheck, Sparkles } from 'lucide-react'
import { Connection, PublicKey } from '@solana/web3.js'

type Risk = 'Immutable' | 'ProgramControlled' | 'SingleKey'
type Reference = { programId: string; label: string; occurrences: number }
type Result = {
  programId: string
  programData: string | null
  authority: string | null
  risk: Risk
  lastDeploySlot: string | null
  attested?: boolean
  attestedSlot?: string | null
  inference: {
    controllerGuess: string | null
    references: Reference[]
    referenceBasis: string
  }
}
type StoredResult = Result | { programId: string; error: string }

const LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111'
const RPC = import.meta.env.VITE_RPC_URL || 'https://api.mainnet-beta.solana.com'
const DEFAULT_PROGRAM = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'

const riskMeta: Record<Risk, { icon: typeof LockKeyhole; description: string; treatment: string }> = {
  Immutable: { icon: LockKeyhole, description: 'No upgrade authority is present in ProgramData.', treatment: 'quiet' },
  ProgramControlled: { icon: ShieldCheck, description: 'The authority is an off-curve PDA; only an on-chain program can sign.', treatment: 'controlled' },
  SingleKey: { icon: AlertTriangle, description: 'An on-curve key can replace the program code.', treatment: 'single' },
}

function short(value: string | null, edge = 7) {
  if (!value) return 'None'
  return `${value.slice(0, edge)}…${value.slice(-edge)}`
}

function readU32(bytes: Uint8Array, offset: number) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(offset, true)
}

function readU64(bytes: Uint8Array, offset: number) {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(offset, true)
}

async function liveScan(programId: string): Promise<Result> {
  const program = new PublicKey(programId)
  const connection = new Connection(RPC, 'confirmed')
  const account = await connection.getAccountInfo(program)
  if (!account) throw new Error('No account exists for that address.')
  if (!account.executable) throw new Error('That account is not an executable program.')
  if (!account.owner.equals(new PublicKey(LOADER))) throw new Error('Trace covers upgradeable loader v3 programs only.')
  if (account.data.length < 36 || readU32(account.data, 0) !== 2) throw new Error('The Program account header is malformed.')
  const programData = new PublicKey(account.data.slice(4, 36))
  const dataAccount = await connection.getAccountInfo(programData)
  if (!dataAccount) throw new Error('The linked ProgramData account could not be read.')
  if (dataAccount.data.length < 45 || readU32(dataAccount.data, 0) !== 3) throw new Error('The ProgramData header is malformed.')
  const option = dataAccount.data[12]
  if (option !== 0 && option !== 1) throw new Error('The ProgramData authority flag is invalid.')
  const authority = option === 1 ? new PublicKey(dataAccount.data.slice(13, 45)) : null
  const risk: Risk = authority === null ? 'Immutable' : PublicKey.isOnCurve(authority.toBytes()) ? 'SingleKey' : 'ProgramControlled'
  return {
    programId: program.toBase58(),
    programData: programData.toBase58(),
    authority: authority?.toBase58() ?? null,
    risk,
    lastDeploySlot: readU64(dataAccount.data, 4).toString(),
    attested: false,
    attestedSlot: null,
    inference: { controllerGuess: null, references: [], referenceBasis: 'static bytecode address search' },
  }
}

function Logo() {
  return (
    <div className="logo" aria-label="Trace">
      <span className="logo-mark" aria-hidden="true"><span className="logo-person" /></span>
      <span className="logo-word">TRACE</span>
    </div>
  )
}

function ProvenanceBadge({ attested }: { attested: boolean }) {
  return (
    <div className={`provenance ${attested ? 'provenance--verified' : 'provenance--scan'}`}>
      {attested ? <ShieldCheck size={15} /> : <FileSearch size={15} />}
      <span><strong>{attested ? 'Verified on-chain' : 'Verified on-chain'}</strong><small>{attested ? 'Trace Attestation PDA' : 'read-only scan · not attested'}</small></span>
    </div>
  )
}

function RiskBadge({ risk }: { risk: Risk }) {
  const meta = riskMeta[risk]
  const Icon = meta.icon
  return <div className={`risk-badge risk-badge--${meta.treatment}`}><Icon size={22} /><span>{risk}</span></div>
}

function Fact({ label, value, note, icon: Icon }: { label: string; value: string; note: string; icon: typeof Fingerprint }) {
  return <article className="fact"><div className="fact-icon"><Icon size={17} /></div><p>{label}</p><strong>{value}</strong><small>{note}</small></article>
}

function InferencePanel({ result }: { result: Result }) {
  return (
    <aside className="inference-panel">
      <div className="panel-kicker"><Sparkles size={14} /> OFF-CHAIN INFERENCE</div>
      <h2>Useful context, separate proof.</h2>
      <p className="panel-intro">These signals are produced by the scanner, not written by Trace and not used for the risk class.</p>
      <div className="inference-row"><span>Controller guess</span><strong>{result.inference.controllerGuess ?? 'Not run'}</strong><small>Best-effort recent transaction history</small></div>
      <div className="inference-row"><span>Reference scan</span><strong>{result.inference.references.length ? `${result.inference.references.length} address match${result.inference.references.length === 1 ? '' : 'es'}` : 'No matches'}</strong><small>{result.inference.referenceBasis}; not a CPI graph</small></div>
      {result.inference.references.map((reference) => <div className="reference" key={reference.programId}><CircleDot size={13} /><span>{reference.label}</span><code>{reference.occurrences}×</code></div>)}
    </aside>
  )
}

export default function App() {
  const [query, setQuery] = useState(DEFAULT_PROGRAM)
  const [result, setResult] = useState<Result | null>(null)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [stored, setStored] = useState<StoredResult[] | null>(null)
  const age = useMemo(() => result?.attestedSlot ? `Slot ${result.attestedSlot}` : result ? 'Not attested' : '—', [result])

  async function inspect(event?: FormEvent) {
    event?.preventDefault()
    const address = query.trim()
    setError('')
    setResult(null)
    setLoading(true)
    try {
      if (!stored) {
        const response = await fetch('/scan-results.json')
        setStored(await response.json() as StoredResult[])
      }
      const cached = (stored ?? []).find((entry) => entry.programId === address)
      if (cached && 'risk' in cached) setResult(cached)
      else if (cached && 'error' in cached) throw new Error(cached.error)
      else setResult(await liveScan(address))
      setLoaded(true)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Trace could not read this program.')
    } finally {
      setLoading(false)
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar"><Logo /><div className="topbar-right"><span className="network-dot" /> MAINNET READ-ONLY <a href="https://github.com/Nistha-verse/trace-registry" target="_blank" rel="noreferrer">SOURCE <ArrowUpRight size={13} /></a></div></header>
      <section className="hero">
        <div className="hero-copy"><p className="eyebrow">SOLANA PROGRAM RISK REGISTRY <span>///</span> FACTS OVER FAITH</p><h1>Know who can<br /><em>change the code.</em></h1><p className="lede">A trustless on-chain registry of Solana program upgrade-authority risk: immutable, program-controlled, or a single key.</p></div>
        <div className="search-card"><div className="search-card-head"><span>INSPECT PROGRAM</span><span className="mono">RPC / PUBLIC</span></div><form onSubmit={inspect}><label htmlFor="program-id">Program address</label><div className="search-row"><Search size={18} /><input id="program-id" value={query} onChange={(event) => setQuery(event.target.value)} spellCheck={false} placeholder="Paste a Solana program ID" /><button type="submit" disabled={loading}>{loading ? 'READING…' : 'CHECK'}<ArrowUpRight size={16} /></button></div></form><p className="search-note"><Check size={14} /> No wallet required <span>·</span> public account data only</p></div>
      </section>
      <section className="classes"><div className="section-label"><span>01</span><p>THE THREE CLASSES</p><small>Classification is computed from the authority bytes.</small></div><div className="class-grid">{(Object.keys(riskMeta) as Risk[]).map((risk, index) => { const Icon = riskMeta[risk].icon; return <article className={`class-card class-card--${riskMeta[risk].treatment}`} key={risk}><span className="class-number">0{index + 1}</span><Icon size={20} /><h2>{risk}</h2><p>{riskMeta[risk].description}</p></article> })}</div></section>
      {error && <section className="message error-message" role="alert"><AlertTriangle size={20} /><div><strong>Could not inspect this address.</strong><p>{error}</p></div></section>}
      {loading && <section className="message"><CircleDot className="pulse" size={20} /><div><strong>Reading public loader accounts…</strong><p>Trace never needs a private RPC key in the browser.</p></div></section>}
      {result && !loading && <section className="result-wrap"><div className="result-head"><div><p className="eyebrow">02 / PROGRAM READOUT</p><code>{result.programId}</code></div><ProvenanceBadge attested={Boolean(result.attested)} /></div><div className="result-grid"><article className={`verdict verdict--${riskMeta[result.risk].treatment}`}><div className="verdict-top"><span>RISK CLASS</span><span className="mono">LOADER V3</span></div><RiskBadge risk={result.risk} /><p>{riskMeta[result.risk].description}</p><div className="authority"><Fingerprint size={16} /><span>UPGRADE AUTHORITY</span><code>{result.authority ?? 'None — immutable'}</code></div></article><div className="facts"><Fact label="Authority" value={short(result.authority)} note={result.authority ? 'From ProgramData header' : 'No active authority'} icon={Fingerprint} /><Fact label="Last deploy" value={result.lastDeploySlot ? `Slot ${result.lastDeploySlot}` : 'Unknown'} note="Loader metadata" icon={ArrowUpRight} /><Fact label="Attestation age" value={age} note={result.attested ? 'Trace PDA snapshot' : 'Read-only scan'} icon={CircleDot} /></div></div><div className="readout-note"><LockKeyhole size={16} /><span><strong>What this proves:</strong> Trace can prove the authority shape at the read slot. It cannot prove a ProgramControlled controller's threshold, signers, or timelock.</span></div><InferencePanel result={result} /></section>}
      {!result && !error && !loading && <p className="empty-state">Try the preloaded Token-2022 address, or paste any upgradeable Solana program.</p>}
      <footer><span>TRACE / ON-CHAIN AUTHORITY REGISTRY</span><span>Upgradeable loader v3 only</span><a href="https://explorer.solana.com/address/TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" target="_blank" rel="noreferrer">OPEN EXPLORER <ArrowUpRight size={13} /></a></footer>
    </main>
  )
}
