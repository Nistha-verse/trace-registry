import { ChevronDown, Circle } from 'lucide-react'
import { useState } from 'react'
import { networkLabels, setNetwork, useNetwork, type Network } from '../lib/network'

const options: Network[] = ['mainnet-beta', 'devnet', 'localnet']

export function NetworkSelector() {
  const network = useNetwork()
  const [open, setOpen] = useState(false)
  return (
    <div className="network-selector">
      <button className="network-pill" type="button" onClick={() => setOpen((value) => !value)} aria-expanded={open} aria-haspopup="listbox">
        <Circle className={`network-selector__dot network-selector__dot--${network}`} size={7} fill="currentColor" />
        <span>{networkLabels[network]}</span>
        <ChevronDown size={14} />
      </button>
      {open && <div className="network-menu" role="listbox" aria-label="Select network">{options.map((option) => <button key={option} type="button" className={option === network ? 'network-menu__item network-menu__item--active' : 'network-menu__item'} onClick={() => { setNetwork(option); setOpen(false) }}>{networkLabels[option]}<span>{option === 'mainnet-beta' ? 'Production' : option === 'devnet' ? 'Testing' : 'Local RPC'}</span></button>)}</div>}
    </div>
  )
}
