import { beforeEach, describe, expect, it } from 'vitest'
import type { LensSystem } from '../../../shared/lens'
import { attachGlasses, decodeAgf, parseAgf, setBuiltinNames, setCatalogs } from '../glassLibrary'

const AGF = `CC Test catalog
NM N-TEST 2 123456.789 1.5168 64.17 0 0
CD 1.03961212 0.231792344 1.01046945 0.00600069867 0.0200179144 103.560653 0 0 0 0
LD 0.3 2.5 0 0 0 0
NM OLD-GLASS 99 000000.000 1.5 50 1 0
CD 1 2 3
NM NO-COEFF 1 000000.000 1.5 50 0 0
`

const system = (...materials: string[]): LensSystem => ({
  name: 't', objectDistance: null, stopIndex: 0, entrancePupilDiameter: 10, fields: [0], wavelengths: [0.55], primaryWavelength: 0, rayAiming: false,
  surfaces: materials.map(material => ({ radius: 50, thickness: 5, material })),
})

describe('parseAgf', () => {
  it('reads glasses with a known formula and coefficients', () => {
    const glasses = parseAgf(AGF, 'TEST')
    expect(glasses.map(g => g.name)).toEqual(['N-TEST'])
    expect(glasses[0]).toMatchObject({ formula: 2, nd: 1.5168, vd: 64.17, catalog: 'TEST', range: [0.3, 2.5] })
    expect(glasses[0].coefficients).toHaveLength(10)
  })

  it('decodes UTF-16 files', () => {
    const bytes = new Uint8Array([0xff, 0xfe, ...[...'NM A'].flatMap(c => [c.charCodeAt(0), 0])])
    expect(decodeAgf(bytes)).toBe('NM A')
  })
})

describe('attachGlasses', () => {
  beforeEach(() => {
    setBuiltinNames(['N-BK7'])
    setCatalogs([{ name: 'TEST', glasses: parseAgf(AGF, 'TEST') }])
  })

  it('embeds only the catalog glasses a system uses', () => {
    const attached = attachGlasses(system('N-TEST', 'N-BK7', 'AIR', '1.5/60'))
    expect(attached.glasses?.map(g => g.name)).toEqual(['N-TEST'])
  })

  it('returns the same object when nothing changes', () => {
    const attached = attachGlasses(system('N-TEST'))
    expect(attachGlasses(attached)).toBe(attached)
  })

  it('drops glasses that are no longer used and keeps embedded ones when the catalog is gone', () => {
    const attached = attachGlasses(system('N-TEST'))
    setCatalogs([])
    expect(attachGlasses(attached).glasses?.map(g => g.name)).toEqual(['N-TEST'])
    const replaced = attachGlasses({ ...attached, surfaces: [{ radius: 50, thickness: 5, material: 'N-BK7' }] })
    expect(replaced.glasses).toBeUndefined()
  })
})
