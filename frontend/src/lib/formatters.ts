const BASE58 = /^[1-9A-HJ-NP-Za-km-z]+$/

export function isSolanaAddress(value: string) {
  const trimmed = value.trim()
  return trimmed.length >= 32 && trimmed.length <= 44 && BASE58.test(trimmed)
}

export function shortenAddress(value: string | null | undefined, chars = 8) {
  if (!value) return '—'
  return `${value.slice(0, chars)}…${value.slice(-chars)}`
}

export function riskLabel(value: string | null | undefined) {
  if (!value) return 'Unknown'
  return value.split('_').map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(' ')
}
