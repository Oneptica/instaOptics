// Pure edits of a lens system. Each returns a new system, or the same object when nothing changes.
import type { GlassInfo, LensSystem, Surface } from '../../shared/lens'

export function updateSurface(system: LensSystem, index: number, patch: Partial<Surface>): LensSystem {
  const current = system.surfaces[index]
  const next: Surface = { ...current, ...patch }
  for (const key of Object.keys(patch) as (keyof Surface)[]) {
    if (patch[key] === undefined) delete next[key]
  }
  if (JSON.stringify(next) === JSON.stringify(current)) return system
  return { ...system, surfaces: system.surfaces.map((surface, i) => i === index ? next : surface) }
}

/**
 * Inserts a flat dummy surface before `index` (index = surfaces.length appends). It has zero thickness and continues
 * the medium it sits in, so the system is optically unchanged.
 */
export function insertSurface(system: LensSystem, index: number): LensSystem {
  const surfaces = [...system.surfaces]
  surfaces.splice(index, 0, { radius: 0, thickness: 0, material: index > 0 ? system.surfaces[index - 1].material : 'AIR' })
  return { ...system, surfaces, stopIndex: system.stopIndex >= index ? system.stopIndex + 1 : system.stopIndex }
}

export function deleteSurface(system: LensSystem, index: number): LensSystem {
  if (system.surfaces.length <= 1) return system
  const surfaces = system.surfaces.filter((_, i) => i !== index)
  const stopIndex = system.stopIndex > index ? system.stopIndex - 1 : Math.min(system.stopIndex, surfaces.length - 1)
  return { ...system, surfaces, stopIndex }
}

export type VariableKey = 'radius' | 'thickness' | 'conic' | `a${number}`

export function isVariable(surface: Surface, key: VariableKey): boolean {
  const v = surface.variable
  if (!v) return false
  return key.startsWith('a') ? !!v.aspheric?.[Number(key.slice(1))] : !!v[key as 'radius' | 'thickness' | 'conic']
}

export function toggleVariable(system: LensSystem, index: number, key: VariableKey): LensSystem {
  const surface = system.surfaces[index]
  const v = { ...surface.variable }
  if (key.startsWith('a')) {
    const term = Number(key.slice(1))
    const aspheric = [...(v.aspheric ?? [])]
    while (aspheric.length <= term) aspheric.push(false)
    aspheric[term] = !aspheric[term]
    while (aspheric.length && !aspheric[aspheric.length - 1]) aspheric.pop()
    if (aspheric.length) v.aspheric = aspheric
    else delete v.aspheric
  } else {
    const name = key as 'radius' | 'thickness' | 'conic'
    if (v[name]) delete v[name]
    else v[name] = true
  }
  return updateSurface(system, index, { variable: Object.keys(v).length ? v : undefined })
}

/** Sets an even asphere coefficient, dropping trailing zero terms. */
export function setAspheric(system: LensSystem, index: number, term: number, value: number): LensSystem {
  const aspheric = [...(system.surfaces[index].aspheric ?? [])]
  while (aspheric.length <= term) aspheric.push(0)
  aspheric[term] = value
  while (aspheric.length && aspheric[aspheric.length - 1] === 0) aspheric.pop()
  return updateSurface(system, index, { aspheric: aspheric.length ? aspheric : undefined })
}

export function clearVariables(system: LensSystem): LensSystem {
  if (!system.surfaces.some(s => s.variable)) return system
  return { ...system, surfaces: system.surfaces.map(({ variable: _variable, ...surface }) => surface) }
}

export function toggleCoordinateBreak(system: LensSystem, index: number): LensSystem {
  const surface = system.surfaces[index]
  if (surface.coordinateBreak) return updateSurface(system, index, { coordinateBreak: undefined })
  return updateSurface(system, index, { coordinateBreak: { decenter: [0, 0], tilt: [0, 0, 0] }, radius: 0, conic: undefined, aspheric: undefined, variable: undefined })
}

export function setStop(system: LensSystem, index: number): LensSystem {
  return system.stopIndex === index ? system : { ...system, stopIndex: index }
}

const MODEL_GLASS = /^\s*\d+(\.\d+)?\s*\/\s*\d+(\.\d+)?\s*$/

export function isKnownMaterial(material: string, glasses: GlassInfo[]): boolean {
  const name = material.trim().toUpperCase()
  return name === '' || name === 'AIR' || name === 'MIRROR' || MODEL_GLASS.test(name) || glasses.some(glass => glass.name.toUpperCase() === name)
}

/** Parses a numeric cell; "inf"/"infinity" gives Infinity. Returns null when the text is not a number. */
export function parseNumber(text: string): number | null {
  const value = text.trim().toLowerCase()
  if (value === 'inf' || value === 'infinity' || value === '∞') return Infinity
  if (value === '') return null
  const number = Number(value)
  return Number.isFinite(number) ? number : null
}
