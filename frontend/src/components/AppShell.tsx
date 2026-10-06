import { Activity, ChevronDown, Menu, Search, ShieldCheck, X } from 'lucide-react'
import { useState } from 'react'
import { TraceLogo } from './TraceLogo'
import { NetworkSelector } from './NetworkSelector'

type AppShellProps = {
  children: React.ReactNode
}

export function AppShell({ children }: AppShellProps) {
  const [open, setOpen] = useState(false)

  return (
    <div className="app-shell">
      <div className="app-shell__ambient" aria-hidden="true">
        <span className="ambient-line ambient-line--one" />
        <span className="ambient-line ambient-line--two" />
        <span className="ambient-arc ambient-arc--one" />
        <span className="ambient-arc ambient-arc--two" />
      </div>
      <header className="topbar">
        <a className="topbar__brand" href="#/" aria-label="Trace home">
          <TraceLogo compact />
        </a>
        <nav className={`topbar__nav ${open ? 'topbar__nav--open' : ''}`} aria-label="Main navigation">
          <a className="nav-link nav-link--active" href="#/">Check program</a>
          <a className="nav-link" href="#/registry">Known-safe registry</a>
          <a className="nav-link" href="#/history">History</a>
          <button className="nav-close" type="button" onClick={() => setOpen(false)} aria-label="Close navigation">
            <X size={18} />
          </button>
        </nav>
        <div className="topbar__actions">
          <NetworkSelector />
          <button className="icon-button" type="button" aria-label="Search">
            <Search size={18} />
          </button>
          <button className="menu-button" type="button" onClick={() => setOpen(true)} aria-label="Open navigation">
            <Menu size={20} />
          </button>
        </div>
      </header>
      <main className="page-content">{children}</main>
      <footer className="footer">
        <div className="footer__mark"><ShieldCheck size={15} /> Facts over faith.</div>
        <div className="footer__meta"><Activity size={14} /> On-chain read layer / v0.1</div>
      </footer>
    </div>
  )
}
