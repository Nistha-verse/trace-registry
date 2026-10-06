import { ArrowLeft, ArrowUpRight, CircleAlert, GitBranch, LoaderCircle, ShieldCheck } from 'lucide-react'
import { useEffect, useState } from 'react'
import { fetchProgramReport } from '../lib/api'
import { useNetwork } from '../lib/network'
import type { DependencyView, ProgramReport } from '../lib/types'

type Props = { address: string }

const labels: Record<DependencyView['risk'], string> = {
  immutable: 'Immutable',
  multisig_protected: 'Multisig protected',
  program_controlled: 'Program controlled',
  single_key: 'Single key',
}

function short(value: string, chars = 8) {
  return `${value.slice(0, chars)}…${value.slice(-chars)}`
}

function riskTone(risk: DependencyView['risk']) {
  return `dependency-node dependency-node--${risk}`
}

export function DependenciesPage({ address }: Props) {
  const [report, setReport] = useState<ProgramReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const network = useNetwork()

  useEffect(() => {
    setLoading(true)
    fetchProgramReport(address, network).then(setReport).catch((cause) => setError(cause instanceof Error ? cause.message : 'Dependency read failed')).finally(() => setLoading(false))
  }, [address, network])

  if (loading) return <GraphState icon={<LoaderCircle className="spin" size={30} />} label="READING DEPENDENCY MANIFEST" />
  if (error) return <GraphState icon={<CircleAlert size={30} />} label="DEPENDENCY READ FAILED" message={error} />

  const dependencies = report?.dependencies?.dependencies ?? []
  return (
    <div className="inspection-page">
      <div className="inspection-page__top"><a className="back-link" href={`#/program/${encodeURIComponent(address)}`}><ArrowLeft size={15} /> Back to verdict</a><span className="source-chip"><span className="status-dot" /> Runtime observations</span></div>
      <div className="inspection-page__heading"><div><div className="eyebrow"><span className="eyebrow__rule" /> DEPENDENCY MAP / 04</div><h1>What it <em>calls.</em></h1><p>Observed runtime dependencies anchored to the program’s public manifest.</p></div><div className="inspection-page__metric"><strong>{String(dependencies.length).padStart(2, '0')}</strong><span>observed targets</span></div></div>
      {dependencies.length === 0 ? <EmptyGraph address={address} /> : <DependencyGraph address={address} dependencies={dependencies} />}
    </div>
  )
}

function DependencyGraph({ address, dependencies }: { address: string; dependencies: DependencyView[] }) {
  return (
    <section className="dependency-layout">
      <div className="dependency-canvas">
        <div className="dependency-orbit dependency-orbit--outer" />
        <div className="dependency-orbit dependency-orbit--inner" />
        <span className="dependency-line dependency-line--one" />
        <span className="dependency-line dependency-line--two" />
        <span className="dependency-line dependency-line--three" />
        <div className="dependency-root"><ShieldCheck size={21} /><span>PROGRAM</span><code>{short(address, 7)}</code></div>
        {dependencies.map((dependency, index) => <div className={`dependency-node-wrap dependency-node-wrap--${index % 4}`} key={dependency.program_id}><article className={riskTone(dependency.risk)}><div className="dependency-node__top"><span>{String(index + 1).padStart(2, '0')}</span><ArrowUpRight size={14} /></div><strong>{labels[dependency.risk]}</strong><code>{short(dependency.program_id)}</code><small>observed at {dependency.last_observed_slot.toLocaleString()}</small></article></div>)}
      </div>
      <aside className="dependency-legend"><div className="detail-panel__header"><span className="eyebrow">OBSERVATION LOGIC</span><span className="panel-index">LIVE</span></div><p>Trace records a dependency when the declared parent invokes the Trace program and supplies the dependency’s live attestation.</p><div className="legend-row"><GitBranch size={15} /><span>Parent → dependency</span></div><div className="legend-row"><ShieldCheck size={15} /><span>Attestation-backed risk</span></div><a className="text-link" href={`#/program/${encodeURIComponent(address)}/history`}>See dependency events <ArrowUpRight size={14} /></a></aside>
    </section>
  )
}

function EmptyGraph({ address }: { address: string }) {
  return <div className="empty-graph"><GitBranch size={28} /><span className="eyebrow">NO RUNTIME OBSERVATIONS</span><h2>The graph is quiet.</h2><p>Trace has not observed a CPI dependency for <code>{short(address, 9)}</code> yet.</p><a className="text-link" href={`#/program/${encodeURIComponent(address)}`}>Return to program facts <ArrowUpRight size={14} /></a></div>
}

function GraphState({ icon, label, message }: { icon: React.ReactNode; label: string; message?: string }) {
  return <div className="state-panel state-panel--loading">{icon}<span className="eyebrow">{label}</span><p>{message ?? 'Trace is checking the public dependency record.'}</p></div>
}
