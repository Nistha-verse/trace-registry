import type { HistoryRecord, ProgramReport } from './types'

const baseUrl = (import.meta.env.VITE_TRACE_API_URL as string | undefined)?.replace(/\/$/, '') || 'http://127.0.0.1:8787'

async function request<T>(path: string): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`)
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

export function fetchProgramReport(address: string) {
  return request<ProgramReport>(`/v1/programs/${encodeURIComponent(address)}`)
}

export function fetchProgramHistory(address: string) {
  return request<HistoryRecord[]>(`/v1/programs/${encodeURIComponent(address)}/history`)
}

export function apiOrigin() {
  return baseUrl
}
