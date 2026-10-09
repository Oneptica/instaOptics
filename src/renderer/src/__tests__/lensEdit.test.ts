import { describe, expect, it } from 'vitest'
import type { LensSystem } from '../../../shared/lens'
import {
  addConfigRow, addConfiguration, configRowFor, deleteSurface, enableConfigurations, formatSolve, insertSurface, parseNumber, parseSolve, removeConfiguration,
  setAspheric, setConfigValue, toggleVariable, updateSurface,
} from '../lensEdit'

const base = (): LensSystem => ({
  name: 't', objectDistance: null, stopIndex: 1, entrancePupilDiameter: 10, fields: [0], wavelengths: [0.55], primaryWavelength: 0, rayAiming: false,
  surfaces: [
    { radius: 50, thickness: 5, material: 'N-BK7' },
    { radius: -50, thickness: 20, material: 'AIR' },
    { radius: 0, thickness: 30, material: 'AIR' },
  ],
})

describe('parseNumber', () => {
  it('reads numbers and infinity', () => {
    expect(parseNumber(' 1.5 ')).toBe(1.5)
    expect(parseNumber('inf')).toBe(Infinity)
    expect(parseNumber('∞')).toBe(Infinity)
    expect(parseNumber('')).toBeNull()
    expect(parseNumber('abc')).toBeNull()
  })
})

describe('surface edits', () => {
  it('returns the same object when nothing changes', () => {
    const system = base()
    expect(updateSurface(system, 0, { radius: 50 })).toBe(system)
  })

  it('removes a key set to undefined', () => {
    const system = updateSurface({ ...base(), surfaces: [{ ...base().surfaces[0], semiDiameter: 4 }, ...base().surfaces.slice(1)] }, 0, { semiDiameter: undefined })
    expect('semiDiameter' in system.surfaces[0]).toBe(false)
  })

  it('moves the stop with inserted and deleted surfaces', () => {
    const inserted = insertSurface(base(), 0)
    expect(inserted.stopIndex).toBe(2)
    expect(inserted.surfaces[0].thickness).toBe(0)
    expect(deleteSurface(inserted, 0).stopIndex).toBe(1)
  })

  it('keeps the last surface', () => {
    const one = { ...base(), surfaces: [base().surfaces[0]] }
    expect(deleteSurface(one, 0)).toBe(one)
  })

  it('toggles variables and drops empty trailing aspheric flags', () => {
    let system = toggleVariable(base(), 0, 'radius')
    expect(system.surfaces[0].variable).toEqual({ radius: true })
    system = toggleVariable(system, 0, 'a1')
    expect(system.surfaces[0].variable?.aspheric).toEqual([false, true])
    system = toggleVariable(toggleVariable(system, 0, 'a1'), 0, 'radius')
    expect(system.surfaces[0].variable).toBeUndefined()
  })

  it('drops trailing zero aspheric terms', () => {
    let system = setAspheric(base(), 0, 2, 1e-6)
    expect(system.surfaces[0].aspheric).toEqual([0, 0, 1e-6])
    system = setAspheric(system, 0, 2, 0)
    expect(system.surfaces[0].aspheric).toBeUndefined()
  })
})

describe('solves', () => {
  it('parses marginal ray and pickup text', () => {
    expect(parseSolve('', 3)).toBeUndefined()
    expect(parseSolve('M', 3)).toEqual({ kind: 'marginalRayHeight', height: 0 })
    expect(parseSolve('m 0.5', 3)).toEqual({ kind: 'marginalRayHeight', height: 0.5 })
    expect(parseSolve('Pr1', 3)).toEqual({ kind: 'pickup', surface: 0, parameter: 'radius', scale: 1, offset: 0 })
    expect(parseSolve('Pt2 -1 0.5', 3)).toEqual({ kind: 'pickup', surface: 1, parameter: 'thickness', scale: -1, offset: 0.5 })
  })

  it('rejects bad text', () => {
    expect(parseSolve('Pr9', 3)).toBeNull()
    expect(parseSolve('Pr0', 3)).toBeNull()
    expect(parseSolve('M x', 3)).toBeNull()
    expect(parseSolve('hello', 3)).toBeNull()
  })

  it('round-trips through format', () => {
    for (const text of ['M', 'M 0.5', 'Pr1', 'Pk2 -1', 'Pt3 2 1']) {
      expect(formatSolve(parseSolve(text, 5) ?? undefined)).toBe(text)
    }
  })

  it('renumbers pickups and drops those whose source is deleted', () => {
    const system = base()
    system.surfaces[2] = { ...system.surfaces[2], solve: { kind: 'pickup', surface: 1, parameter: 'radius', scale: 1, offset: 0 } }
    const inserted = insertSurface(system, 0)
    expect(inserted.surfaces[3].solve).toMatchObject({ surface: 2 })
    const removed = deleteSurface(system, 1)
    expect(removed.surfaces[1].solve).toBeUndefined()
  })
})

describe('configurations', () => {
  it('adds configurations that copy the previous values', () => {
    let system = enableConfigurations(base())
    system = addConfigRow(system, 1, 'thickness')
    expect(system.configs?.rows[0].values).toEqual([20, 20])
    system = addConfiguration(setConfigValue(system, 0, 1, 25))
    expect(system.configs?.names).toHaveLength(3)
    expect(system.configs?.rows[0].values).toEqual([20, 25, 25])
    expect(configRowFor(system, 1, 'thickness')).toBeDefined()
    expect(addConfigRow(system, 1, 'thickness')).toBe(system)
  })

  it('removes a configuration along with its values and operands', () => {
    let system = addConfigRow(addConfiguration(enableConfigurations(base())), 0, 'radius')
    system = { ...system, optimization: { operands: [{ kind: 'efl', target: 50, config: 1 }, { kind: 'efl', target: 50, config: 2 }, { kind: 'efl', target: 50 }] } }
    const next = removeConfiguration(system, 1)
    expect(next.configs?.names).toHaveLength(2)
    expect(next.configs?.rows[0].values).toHaveLength(2)
    expect(next.optimization?.operands?.map(o => o.config)).toEqual([1, undefined])
  })

  it('keeps one configuration', () => {
    const single = { ...base(), configs: { names: ['A'], rows: [], active: 0 } }
    expect(removeConfiguration(single, 0)).toBe(single)
  })

  it('drops rows of a deleted surface', () => {
    const system = addConfigRow(enableConfigurations(base()), 2, 'thickness')
    expect(deleteSurface(system, 2).configs?.rows).toEqual([])
    expect(deleteSurface(system, 0).configs?.rows[0].surface).toBe(1)
  })
})
