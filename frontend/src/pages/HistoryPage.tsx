import { Activity, ArrowLeft, ArrowUpRight, CircleAlert, Clock3, GitBranch, LoaderCircle, ShieldCheck, UserRound } from 'lucide-react'
import { useEffect, useState } from 'react'
import { fetchProgramHistory } from '../lib/api'
import { useNetwork } from '../lib/network'
import type { HistoryRecord } from '../lib/types'

type Props = { address: string }

function short(value: string | null, chars = 8) {
  if (!value) return 'None'
  return `${value.slice(0, chars)}…${value.slice(-chars)}`
}

function iconFor(kind: HistoryRecord['kind']) {
  if (kind === 'authority_changed') return <UserRound size={17} />
  if (kind === 'dependency_observed') return <GitBranch size={17} />
  return <ShieldCheck size={17} />
}

function labelFor(kind: HistoryRecord['kind']) {
  if (kind === 'authority_changed') return 'Authority changed'
  if (kind === 'dependency_observed') return 'Dependency observed'
  return 'Attestation refreshed'
}

export function HistoryPage({ address }: Props) {
  const [records, setRecords] = useState<HistoryRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const network = useNetwork()

  useEffect(() => {
    setLoading(true)
    fetchProgramHistory(address, network).then(setRecords).catch((cause) => setError(cause instanceof Error ? cause.message : 'History read failed')).finally(() => setLoading(false))
  }, [address, network])

  return (
    <div className="inspection-page">
      <div className="inspection-page__top"><a className="back-link" href={`#/program/${encodeURIComponent(address)}`}><ArrowLeft size={15} /> Back to verdict</a><span className="source-chip"><span className="status-dot" /> Event history</span></div>
      <div className="inspection-page__heading"><div><div className="eyebrow"><span className="eyebrow__rule" /> PUBLIC EVENT LOG / 05</div><h1>What <em>changed.</em></h1><p>Events emitted by the Trace program, resolved from public transaction logs.</p></div><div className="inspection-page__metric"><strong>{String(records.length).padStart(2, '0')}</strong><span>events returned</span></div></div>
      {loading && <HistoryState icon={<LoaderCircle className="spin" size={30} />} label="READING EVENT LOG" message="Trace is checking public transaction history." />}
      {!loading && error && <HistoryState icon={<CircleAlert size={30} />} label="HISTORY READ FAILED" message={error} />}
      {!loading && !error && records.length === 0 && <HistoryState icon={<Clock3 size={30} />} label="NO EVENTS FOUND" message="This program has no indexed Trace events on the selected network." />}
      {!loading && !error && records.length > 0 && <div className="history-list">{records.map((record, index) => <HistoryItem key={`${record.signature}-${index}`} record={record} />)}</div>}
    </div>
  )
}

function HistoryItem({ record }: { record: HistoryRecord }) {
  return <article className={`history-item history-item--${record.kind}`}><div className="history-item__rail"><span className="history-item__dot">{iconFor(record.kind)}</span></div><div className="history-item__body"><div className="history-item__top"><span className="eyebrow">{labelFor(record.kind)}</span><span className="history-item__slot">SLOT {record.slot.toLocaleString()}</span></div><h2>{record.kind === 'authority_changed' ? `${short(record.old_authority)} → ${short(record.new_authority)}` : record.kind === 'dependency_observed' ? `${short(record.dependency_program)} joined the manifest` : `${record.risk?.replace('_', ' ') ?? 'Risk'} attested`}</h2><div className="history-item__meta"><span><Activity size={13} /> {record.signature.slice(0, 12)}…</span>{record.attested_slot && <span>Attested at {record.attested_slot.toLocaleString()}</span>}{record.refresh_count && <span>Refresh #{record.refresh_count}</span>}<a href={`https://explorer.solana.com/tx/${record.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Explorer <ArrowUpRight size={13} /></a></div></div></article>
}

function HistoryState({ icon, label, message }: { icon: React.ReactNode; label: string; message: string }) {
  return <div className="state-panel state-panel--loading">{icon}<span className="eyebrow">{label}</span><p>{message}</p></div>
}
