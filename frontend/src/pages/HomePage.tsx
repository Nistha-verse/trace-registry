import { ArrowUpRight, Check, LockKeyhole, Radar, ScanLine } from 'lucide-react'
import { FormEvent, useState } from 'react'
import { TraceLogo } from '../components/TraceLogo'
import { isSolanaAddress } from '../lib/formatters'

function navigateToProgram(address: string) {
  window.location.hash = `#/program/${encodeURIComponent(address.trim())}`
}

const riskCards = [
  { label: 'Immutable', description: 'No upgrade authority.', icon: LockKeyhole, tone: 'safe' },
  { label: 'Multisig protected', description: 'Threshold control is visible.', icon: Check, tone: 'protected' },
  { label: 'Single key', description: 'One key can replace the code.', icon: Radar, tone: 'danger' },
]

export function HomePage() {
  const [address, setAddress] = useState('')
  const [validation, setValidation] = useState('')

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!address.trim()) {
      setValidation('Enter a Solana program address.')
      return
    }
    if (!isSolanaAddress(address)) {
      setValidation('That does not look like a valid base58 Solana address.')
      return
    }
    setValidation('')
    navigateToProgram(address)
  }

  return (
    <div className="home-page">
      <section className="hero-section">
        <div className="hero-section__copy">
          <div className="eyebrow"><span className="eyebrow__rule" /> Solana program intelligence</div>
          <h1>Verify before <em>you trust.</em></h1>
          <p className="hero-section__intro">
            Trace reads the facts behind a program: who can upgrade it, how fresh the evidence is,
            and what it has been observed calling.
          </p>
          <form className="hero-section__search-placeholder" onSubmit={submit}>
            <ScanLine size={17} />
            <input
              aria-label="Solana program address"
              onChange={(event) => setAddress(event.target.value)}
              placeholder="Paste a program address to inspect its risk"
              value={address}
            />
            <button className="hero-section__key" type="submit">CHECK</button>
          </form>
          {validation && <div className="search-validation" role="alert">{validation}</div>}
          <div className="hero-section__caption">No wallet required <span>·</span> Read-only <span>·</span> Verifiable on-chain</div>
        </div>
        <div className="hero-section__identity">
          <div className="identity-card">
            <div className="identity-card__topline"><span>TRC / 001</span><span>LIVE READ</span></div>
            <TraceLogo />
            <div className="identity-card__bottomline"><span>Facts over faith.</span><span>Solana</span></div>
          </div>
        </div>
      </section>

      <section className="risk-strip" aria-label="Risk classes">
        <div className="section-heading">
          <div><span className="section-heading__index">01</span><span className="eyebrow">THE VERDICT</span></div>
          <span className="section-heading__note">Three facts. One clear read.</span>
        </div>
        <div className="risk-grid">
          {riskCards.map(({ label, description, icon: Icon, tone }, index) => (
            <article className={`risk-card risk-card--${tone}`} key={label}>
              <div className="risk-card__index">0{index + 1}</div>
              <Icon size={19} strokeWidth={1.5} />
              <h2>{label}</h2>
              <p>{description}</p>
              <ArrowUpRight className="risk-card__arrow" size={19} />
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}
