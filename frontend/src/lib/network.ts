import { useEffect, useState } from 'react'

export type Network = 'mainnet-beta' | 'devnet' | 'localnet'

export const networkLabels: Record<Network, string> = {
  'mainnet-beta': 'Mainnet',
  devnet: 'Devnet',
  localnet: 'Localnet',
}

const storageKey = 'trace.network'
const eventName = 'trace-network-change'

export function getNetwork(): Network {
  const value = window.localStorage.getItem(storageKey)
  return value === 'mainnet-beta' || value === 'localnet' || value === 'devnet' ? value : 'devnet'
}

export function setNetwork(network: Network) {
  window.localStorage.setItem(storageKey, network)
  window.dispatchEvent(new CustomEvent(eventName, { detail: network }))
}

export function useNetwork() {
  const [network, setValue] = useState<Network>(getNetwork)
  useEffect(() => {
    const update = (event: Event) => setValue((event as CustomEvent<Network>).detail)
    window.addEventListener(eventName, update)
    return () => window.removeEventListener(eventName, update)
  }, [])
  return network
}
