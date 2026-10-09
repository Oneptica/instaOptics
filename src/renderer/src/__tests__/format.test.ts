import { describe, expect, it } from 'vitest'
import { fieldLabel, formatFixed, formatShort } from '../format'

describe('format', () => {
  it('formats fixed values', () => {
    expect(formatFixed(1.23456789)).toBe('1.2346')
    expect(formatFixed(-0.00001)).toBe('0.0000')
    expect(formatFixed(null)).toBe('Infinity')
    expect(formatFixed(-Infinity)).toBe('-Infinity')
  })

  it('formats typed values compactly', () => {
    expect(formatShort(0.5876)).toBe('0.5876')
    expect(formatShort(0.1 + 0.2)).toBe('0.3')
    expect(formatShort(Infinity)).toBe('Infinity')
  })

  it('labels fields with their unit', () => {
    expect(fieldLabel(5, 'angle')).toContain('°')
    expect(fieldLabel(5, 'objectHeight')).toContain('mm')
  })
})
