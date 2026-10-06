import type { HistoryRecord, ProgramReport } from './types'
import type { Network } from './network'

const baseUrl = (import.meta.env.VITE_TRACE_API_URL as string | undefined)?.replace(/\/$/, '') || 'http://127.0.0.1:8787'

function apiUrl(network: Network) {
  const key = network === 'mainnet-beta' ? 'VITE_TRACE_API_URL_MAINNET' : network === 'localnet' ? 'VITE_TRACE_API_URL_LOCALNET' : 'VITE_TRACE_API_URL_DEVNET'
  return ((import.meta.env[key] as string | undefined)?.replace(/\/$/, '') || baseUrl)
}

async function request<T>(path: string, network: Network): Promise<T> {
  const response = await fetch(`${apiUrl(network)}${path}`)
  let payload: unknown
  try {
    payload = await response.json()
  } catch {
    throw new Error(`Trace API returned ${response.status} without readable JSON`)
  }
  if (!response.ok) {
    const message = typeof payload === 'object' && payload && 'error' in payload ? String(payload.error) : `Trace API returned ${response.status}`
    throw new Error(message)
  }
  return payload as T
}

export function fetchProgramReport(address: string, network: Network = 'devnet') {
  return request<ProgramReport>(`/v1/programs/${encodeURIComponent(address)}`, network)
}

export function fetchProgramHistory(address: string, network: Network = 'devnet') {
  return request<HistoryRecord[]>(`/v1/programs/${encodeURIComponent(address)}/history`, network)
}

export type RegistryRecord = {
  record_address: string
  program_id: string
  max_risk: string
  registered_at: number
  attested_slot: number
  enabled: boolean
}

export type RegistryView = {
  registry_address: string
  authority: string | null
  records: RegistryRecord[]
}

export function fetchRegistry(network: Network = 'devnet') {
  return request<RegistryView>('/v1/registry', network)
}
