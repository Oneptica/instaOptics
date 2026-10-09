// Glass catalogs loaded from OpticStudio .agf files. Glasses a system uses are copied into the system itself, so a
// saved lens opens anywhere without the catalog.
import { useSyncExternalStore } from 'react'
import type { GlassDef, GlassInfo, LensSystem } from '../../shared/lens'

export interface Catalog { name: string; glasses: GlassDef[] }

/** Parses AGF text: NM lines start a glass, CD gives the dispersion coefficients, LD the wavelength range. */
export function parseAgf(text: string, catalog: string): GlassDef[] {
  const glasses: GlassDef[] = []
  let current: GlassDef | null = null
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim()
    if (line.length < 3) continue
    const tag = line.slice(0, 2)
    const fields = line.slice(2).trim().split(/\s+/)
    if (tag === 'NM') {
      // NM name formula MIL# nd vd status [melt frequency]
      const [name, formula, , nd, vd] = fields
      current = { name, formula: Number(formula), coefficients: [], nd: Number(nd) || 0, vd: Number(vd) || 0, catalog }
      glasses.push(current)
    } else if (current && tag === 'CD') {
      current.coefficients = fields.map(Number).filter(Number.isFinite)
    } else if (current && tag === 'LD') {
      const [lo, hi] = fields.map(Number)
      if (Number.isFinite(lo) && Number.isFinite(hi)) current.range = [lo, hi]
    }
  }
  // Keep glasses with a known formula and coefficients; AGF also lists obsolete and incomplete entries.
  return glasses.filter(g => g.formula >= 1 && g.formula <= 13 && g.coefficients.length > 0)
}

export function decodeAgf(bytes: Uint8Array): string {
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return new TextDecoder('utf-16le').decode(bytes.subarray(2))
  if (bytes[0] === 0xfe && bytes[1] === 0xff) return new TextDecoder('utf-16be').decode(bytes.subarray(2))
  return new TextDecoder('utf-8').decode(bytes)
}

const same = (a: string, b: string) => a.trim().toUpperCase() === b.trim().toUpperCase()

let catalogs: Catalog[] = []
let builtinNames = new Set<string>()
const listeners = new Set<() => void>()

export function setCatalogs(next: Catalog[]) {
  catalogs = next
  listeners.forEach(listener => listener())
}

export function setBuiltinNames(names: string[]) {
  builtinNames = new Set(names.map(name => name.trim().toUpperCase()))
}

export const useCatalogs = () => useSyncExternalStore(callback => { listeners.add(callback); return () => { listeners.delete(callback) } }, () => catalogs)

export const catalogGlass = (name: string): GlassDef | undefined => {
  for (const catalog of catalogs) {
    const found = catalog.glasses.find(g => same(g.name, name))
    if (found) return found
  }
  return undefined
}

export const toInfo = (g: GlassDef): GlassInfo => ({ name: g.name, nd: g.nd, vd: g.vd, catalog: g.catalog })

const AIRLIKE = new Set(['', 'AIR', 'MIRROR'])

/**
 * Makes the system carry the definitions of the catalog glasses it uses and drops the ones it no longer uses.
 * Returns the same object when nothing changes.
 */
export function attachGlasses(system: LensSystem): LensSystem {
  const wanted: GlassDef[] = []
  for (const surface of system.surfaces) {
    const name = surface.material.trim()
    if (AIRLIKE.has(name.toUpperCase()) || builtinNames.has(name.toUpperCase()) || /^\d/.test(name)) continue
    if (wanted.some(g => same(g.name, name))) continue
    const definition = system.glasses?.find(g => same(g.name, name)) ?? catalogGlass(name)
    if (definition) wanted.push(definition)
  }
  const current = system.glasses ?? []
  if (wanted.length === current.length && wanted.every((g, i) => g === current[i] || JSON.stringify(g) === JSON.stringify(current[i]))) return system
  const { glasses: _old, ...rest } = system
  return wanted.length ? { ...rest, glasses: wanted } : rest
}
