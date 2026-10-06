import { Database, ExternalLink, LoaderCircle, ShieldCheck, TriangleAlert } from 'lucide-react'
import { useEffect, useState } from 'react'
import { fetchRegistry, type RegistryView } from '../lib/api'
import { networkLabels, useNetwork } from '../lib/network'

function short(value: string, chars = 8) { return `${value.slice(0, chars)}…${value.slice(-chars)}` }

export function RegistryPage() {
  const network = useNetwork()
  const [registry, setRegistry] = useState<RegistryView | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    setLoading(true)
    setError(null)
    fetchRegistry(network).then(setRegistry).catch((cause) => setError(cause instanceof Error ? cause.message : 'Registry read failed')).finally(() => setLoading(false))
  }, [network])

  return <div className="inspection-page registry-page">
    <div className="inspection-page__top"><a className="back-link" href="#/"><ShieldCheck size={15} /> Check a program</a><span className="source-chip"><span className="status-dot" /> {networkLabels[network]} policy layer</span></div>
    <div className="inspection-page__heading"><div><div className="eyebrow"><span className="eyebrow__rule" /> KNOWN-SAFE REGISTRY / 06</div><h1>Who gets <em>trusted.</em></h1><p>An explicit policy layer on top of raw loader-derived facts.</p></div><div className="inspection-page__metric"><strong>{registry ? String(registry.records.length).padStart(2, '0') : '—'}</strong><span>registered programs</span></div></div>
    {loading && <RegistryState icon={<LoaderCircle className="spin" size={30} />} title="READING REGISTRY" message="Trace is enumerating public policy records." />}
    {!loading && error && <RegistryState icon={<TriangleAlert size={30} />} title="REGISTRY UNAVAILABLE" message={error} />}
    {!loading && !error && registry && <>
      <div className="registry-summary"><div><span className="eyebrow">REGISTRY AUTHORITY</span><code>{registry.authority ? short(registry.authority, 11) : 'Not initialized'}</code></div><div><span className="eyebrow">REGISTRY PDA</span><code>{short(registry.registry_address, 11)}</code></div><div><span className="eyebrow">SOURCE</span><span className="registry-summary__source"><Database size={14} /> On-chain account scan</span></div></div>
      {registry.records.length === 0 ? <RegistryState icon={<Database size={30} />} title="NO RECORDS YET" message="No programs have been added to the known-safe policy on this network." /> : <div className="registry-table"><div className="registry-table__head"><span>PROGRAM</span><span>MAX RISK</span><span>ATTESTED SLOT</span><span>STATUS</span></div>{registry.records.map((record) => <div className="registry-table__row" key={record.record_address}><div><strong>{short(record.program_id, 10)}</strong><code>{short(record.record_address, 9)}</code></div><span className={`registry-risk registry-risk--${record.max_risk.replace('_protected', '').replace('_controlled', '').replace('_key', '')}`}>{record.max_risk.replace('_', ' ')}</span><code>{record.attested_slot.toLocaleString()}</code><span className={record.enabled ? 'registry-status registry-status--active' : 'registry-status'}>{record.enabled ? 'Enabled' : 'Disabled'}</span><a href={`#/program/${encodeURIComponent(record.program_id)}`} aria-label={`Inspect ${record.program_id}`}><ExternalLink size={14} /></a></div>)}</div>}
    </>}
  </div>
}

function RegistryState({ icon, title, message }: { icon: React.ReactNode; title: string; message: string }) {
  return <div className="state-panel state-panel--loading registry-state">{icon}<span className="eyebrow">{title}</span><p>{message}</p></div>
}
