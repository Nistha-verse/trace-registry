import { AlertTriangle, ArrowLeft, Check, Copy, ExternalLink, Fingerprint, LoaderCircle, RefreshCw, Shield, TriangleAlert } from 'lucide-react'
import { useEffect, useState } from 'react'
import { fetchProgramReport } from '../lib/api'
import type { AttestationView, ProgramReport } from '../lib/types'

type ProgramPageProps = { address: string }

const riskLabels: Record<AttestationView['risk'], string> = {
  immutable: 'Immutable',
  multisig_protected: 'Multisig protected',
  program_controlled: 'Program controlled',
  single_key: 'Single key',
}

const riskDescriptions: Record<AttestationView['risk'], string> = {
  immutable: 'The loader exposes no active upgrade authority.',
  multisig_protected: 'This authority matches a Trace-native threshold configuration.',
  program_controlled: 'An off-curve authority controls this program, but Trace has not verified a multisig.',
  single_key: 'A normal key can replace the deployed code without a threshold policy.',
}

function short(value: string | null | undefined, chars = 7) {
  if (!value) return '—'
  return `${value.slice(0, chars)}…${value.slice(-chars)}`
}

function formatSlot(value: number) {
  return new Intl.NumberFormat('en-US').format(value)
}

function formatDate(timestamp: number) {
  if (!timestamp) return 'Unknown'
  return new Date(timestamp * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
}

function riskClass(risk: AttestationView['risk']) {
  return `verdict-panel--${risk}`
}

export function ProgramPage({ address }: ProgramPageProps) {
  const [report, setReport] = useState<ProgramReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [copied, setCopied] = useState(false)

  async function load() {
    setLoading(true)
    setError(null)
    try {
      setReport(await fetchProgramReport(address))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Trace could not read this program')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { void load() }, [address])

  async function copyAddress() {
    await navigator.clipboard?.writeText(address)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1600)
  }

  if (loading) return <ProgramLoading address={address} />
  if (error) return <ProgramError address={address} error={error} onRetry={load} />

  const attestation = report?.attestation
  if (!report || !attestation) return <ProgramEmpty address={address} />

  const dependencyCount = report.dependencies?.dependencies.length ?? 0

  return (
    <div className="program-page">
      <div className="program-page__back-row">
        <a href="#/" className="back-link"><ArrowLeft size={15} /> Check another program</a>
        <span className="source-chip"><span className="status-dot" /> Live on-chain read</span>
      </div>
      <section className={`verdict-panel ${riskClass(attestation.risk)}`}>
        <div className="verdict-panel__signal"><Shield size={21} /><span>TRACE VERDICT / {attestation.loader.replace('_', ' ')}</span></div>
        <div className="verdict-panel__main">
          <div>
            <div className="verdict-panel__label">Current risk classification</div>
            <h1>{riskLabels[attestation.risk]}</h1>
            <p>{riskDescriptions[attestation.risk]}</p>
          </div>
          <div className="verdict-panel__stamp" aria-label={riskLabels[attestation.risk]}>
            <div className="verdict-panel__stamp-ring" />
            <span>{attestation.risk === 'single_key' ? 'REVIEW' : 'CHECKED'}</span>
          </div>
        </div>
        <div className="verdict-panel__footer"><span>Attested at slot {formatSlot(attestation.attested_slot)}</span><span>{formatDate(attestation.attested_at)}</span></div>
      </section>

      <section className="program-identity">
        <div>
          <div className="eyebrow"><span className="eyebrow__rule" /> PROGRAM UNDER REVIEW</div>
          <div className="program-identity__address"><code>{address}</code><button className="copy-button" onClick={copyAddress} type="button" title="Copy program address">{copied ? <Check size={14} /> : <Copy size={14} />}</button></div>
        </div>
        <div className="program-identity__links"><a href={`https://explorer.solana.com/address/${address}?cluster=devnet`} target="_blank" rel="noreferrer">Explorer <ExternalLink size={13} /></a><a href={`#/program/${encodeURIComponent(address)}/dependencies`}>Dependencies <ArrowLeft className="rotate-180" size={13} /></a><a href={`#/program/${encodeURIComponent(address)}/history`}>History <ArrowLeft className="rotate-180" size={13} /></a></div>
      </section>

      <section className="fact-grid">
        <FactCard label="Upgrade authority" value={attestation.authority ? short(attestation.authority, 9) : 'None'} sub={attestation.authority ? 'Read from loader state' : 'No active authority'} icon={<Fingerprint size={18} />} />
        <FactCard label="Last deployment" value={attestation.last_deploy_slot ? `Slot ${formatSlot(attestation.last_deploy_slot)}` : 'Not exposed'} sub="Loader deployment metadata" icon={<RefreshCw size={18} />} />
        <FactCard label="Attestation age" value="Fresh snapshot" sub={`Refresh ${attestation.refresh_count} · slot ${formatSlot(attestation.attested_slot)}`} icon={<Check size={18} />} />
        <FactCard label="Observed dependencies" value={String(dependencyCount).padStart(2, '0')} sub={dependencyCount ? 'Runtime observations available' : 'No observations yet'} icon={<TriangleAlert size={18} />} />
      </section>

      <section className="detail-grid">
        <article className="detail-panel">
          <div className="detail-panel__header"><span className="eyebrow">AUTHORITY FACTS</span><span className="panel-index">02 / 04</span></div>
          <div className="authority-line"><span className="authority-line__label">Authority address</span><code>{attestation.authority ?? 'None / immutable'}</code></div>
          {attestation.multisig_config && <div className="authority-line"><span className="authority-line__label">Trace multisig</span><code>{short(attestation.multisig_config, 9)}</code><span className="mini-badge">{attestation.multisig_threshold}/{attestation.multisig_member_count}</span></div>}
          <div className="authority-line"><span className="authority-line__label">Program data</span><code>{short(attestation.program_data, 9)}</code></div>
          <div className="authority-line"><span className="authority-line__label">Attester</span><code>{short(attestation.attester, 9)}</code></div>
        </article>
        <article className="detail-panel detail-panel--red">
          <div className="detail-panel__header"><span className="eyebrow">SOURCE PROOF</span><span className="panel-index">03 / 04</span></div>
          <p className="proof-copy">This verdict is backed by a public Trace account. Anyone can derive the address, read the same bytes, and compare the loader state.</p>
          <div className="proof-row"><span>Attestation PDA</span><code>{short(report.attestation_address, 10)}</code></div>
          <div className="proof-row"><span>Trace program</span><code>{short(report.trace_program, 10)}</code></div>
        </article>
      </section>
    </div>
  )
}

function FactCard({ label, value, sub, icon }: { label: string; value: string; sub: string; icon: React.ReactNode }) {
  return <article className="fact-card"><div className="fact-card__icon">{icon}</div><div className="fact-card__label">{label}</div><strong>{value}</strong><span>{sub}</span></article>
}

function ProgramLoading({ address }: { address: string }) {
  return <div className="state-panel state-panel--loading"><LoaderCircle className="spin" size={34} /><span className="eyebrow">READING LOADER STATE</span><code>{short(address, 11)}</code><p>Trace is checking the public account record.</p></div>
}

function ProgramError({ address, error, onRetry }: { address: string; error: string; onRetry: () => void }) {
  return <div className="state-panel state-panel--error"><AlertTriangle size={32} /><span className="eyebrow">COULD NOT VERIFY</span><h1>Read interrupted.</h1><code>{short(address, 11)}</code><p>{error}</p><button className="primary-button" type="button" onClick={onRetry}>Try again <RefreshCw size={15} /></button></div>
}

function ProgramEmpty({ address }: { address: string }) {
  return <div className="state-panel"><AlertTriangle size={32} /><span className="eyebrow">NO ATTESTATION FOUND</span><h1>Nothing checked yet.</h1><code>{short(address, 11)}</code><p>There is no public Trace attestation for this program on the selected network.</p><a className="primary-button" href="#/">Check another address</a></div>
}
