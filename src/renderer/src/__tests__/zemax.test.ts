import { describe, expect, it } from 'vitest'
import type { LensSystem } from '../../../shared/lens'
import { exportZmx, importZmx } from '../zemax'

const catalog = [{ name: 'N-BK7', nd: 1.5168, vd: 64.17 }, { name: 'F2', nd: 1.62, vd: 36.4 }]

const doublet: LensSystem = {
  name: 'Doublet', objectDistance: null, stopIndex: 0, entrancePupilDiameter: 20, fields: [0, 3], wavelengths: [0.486, 0.587, 0.656], primaryWavelength: 1, rayAiming: false,
  surfaces: [
    { radius: 62.5, thickness: 4, material: 'N-BK7' },
    { radius: -45, thickness: 2.5, material: 'F2' },
    { radius: -128, thickness: 97, material: 'AIR', conic: -0.5 },
  ],
}

describe('Zemax round trip', () => {
  const text = exportZmx(doublet)
  const { system, warnings } = importZmx(text, catalog)

  it('keeps the surfaces', () => {
    expect(warnings).toEqual([])
    expect(system.surfaces).toHaveLength(3)
    system.surfaces.forEach((s, i) => {
      expect(s.radius).toBeCloseTo(doublet.surfaces[i].radius, 9)
      expect(s.thickness).toBeCloseTo(doublet.surfaces[i].thickness, 9)
      expect(s.material.toUpperCase()).toBe(doublet.surfaces[i].material.toUpperCase())
    })
    expect(system.surfaces[2].conic).toBeCloseTo(-0.5, 9)
  })

  it('keeps the aperture, fields and wavelengths', () => {
    expect(system.entrancePupilDiameter).toBeCloseTo(20, 9)
    expect(system.fields).toEqual([0, 3])
    expect(system.wavelengths).toEqual(doublet.wavelengths)
    expect(system.primaryWavelength).toBe(1)
    expect(system.stopIndex).toBe(0)
  })

  it('substitutes a model glass for an unknown name and warns', () => {
    const unknown = text.replace('GLAS F2', 'GLAS MYSTERY').replace(/GLAS MYSTERY.*/, 'GLAS MYSTERY 0 0 1.6 40 0 0 0 0 0 0')
    const imported = importZmx(unknown, catalog)
    expect(imported.warnings.some(w => w.includes('MYSTERY'))).toBe(true)
    expect(imported.system.surfaces[1].material).toBe('1.6/40')
  })

  it('rejects files without surfaces', () => {
    expect(() => importZmx('VERS 1\nNAME x\n', catalog)).toThrow()
  })
})
