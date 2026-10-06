import { describe, expect, it } from 'vitest'
import { isSolanaAddress, riskLabel, shortenAddress } from './formatters'

describe('Trace formatters', () => {
  it('accepts a base58-shaped Solana address', () => {
    expect(isSolanaAddress('11111111111111111111111111111111')).toBe(true)
  })

  it('rejects malformed or non-base58 input', () => {
    expect(isSolanaAddress('not-an-address')).toBe(false)
    expect(isSolanaAddress('0'.repeat(32))).toBe(false)
  })

  it('shortens public keys without losing the edges', () => {
    expect(shortenAddress('1234567890ABCDEFG', 4)).toBe('1234…DEFG')
  })

  it('turns risk enum values into readable labels', () => {
    expect(riskLabel('multisig_protected')).toBe('Multisig Protected')
  })
})
