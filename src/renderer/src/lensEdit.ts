// Pure edits of a lens system. Each returns a new system, or the same object when nothing changes.
import type { ConfigParameter, ConfigRow, GlassInfo, LensSystem, Solve, SolveParameter, Surface } from '../../shared/lens'

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
  return remapSurfaceRefs({ ...system, surfaces, stopIndex: system.stopIndex >= index ? system.stopIndex + 1 : system.stopIndex }, i => i >= index ? i + 1 : i)
}

export function deleteSurface(system: LensSystem, index: number): LensSystem {
  if (system.surfaces.length <= 1) return system
  const surfaces = system.surfaces.filter((_, i) => i !== index)
  const stopIndex = system.stopIndex > index ? system.stopIndex - 1 : Math.min(system.stopIndex, surfaces.length - 1)
  return remapSurfaceRefs({ ...system, surfaces, stopIndex }, i => i === index ? null : i > index ? i - 1 : i)
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

// ---------- solves ----------

const SOLVE_PARAMETERS = { r: 'radius', t: 'thickness', k: 'conic' } as const

/**
 * Solve text: "M" or "M 0.5" is a marginal ray height solve on the thickness; "Pr1 -1 0" copies the radius (r),
 * thickness (t) or conic (k) of surface 1 with a scale and an offset. Blank is no solve; null is invalid.
 */
export function parseSolve(text: string, surfaceCount: number): Solve | undefined | null {
  const value = text.trim()
  if (value === '') return undefined
  const marginal = /^m(?:\s+(\S+))?$/i.exec(value)
  if (marginal) {
    const height = marginal[1] === undefined ? 0 : parseNumber(marginal[1])
    return height === null || !Number.isFinite(height) ? null : { kind: 'marginalRayHeight', height }
  }
  const pickup = /^p([rtk])\s*(\d+)(?:\s+(\S+))?(?:\s+(\S+))?$/i.exec(value)
  if (pickup) {
    const surface = Number(pickup[2]) - 1
    const scale = pickup[3] === undefined ? 1 : parseNumber(pickup[3])
    const offset = pickup[4] === undefined ? 0 : parseNumber(pickup[4])
    if (surface < 0 || surface >= surfaceCount || scale === null || offset === null || !Number.isFinite(scale) || !Number.isFinite(offset)) return null
    return { kind: 'pickup', surface, parameter: SOLVE_PARAMETERS[pickup[1].toLowerCase() as 'r' | 't' | 'k'], scale, offset }
  }
  return null
}

export function formatSolve(solve: Solve | undefined): string {
  if (!solve) return ''
  if (solve.kind === 'marginalRayHeight') return solve.height === 0 ? 'M' : `M ${solve.height}`
  const letter = { radius: 'r', thickness: 't', conic: 'k' }[solve.parameter]
  const tail = solve.offset !== 0 ? ` ${solve.scale} ${solve.offset}` : solve.scale !== 1 ? ` ${solve.scale}` : ''
  return `P${letter}${solve.surface + 1}${tail}`
}

/** Which parameter of its own surface a solve controls. */
export function solvedParameter(solve: Solve | undefined): SolveParameter | null {
  if (!solve) return null
  return solve.kind === 'marginalRayHeight' ? 'thickness' : solve.parameter
}

// ---------- surface references ----------

/** Rewrites every stored surface number (pickups, configuration rows, operands) after surfaces are inserted or removed. */
export function remapSurfaceRefs(system: LensSystem, map: (index: number) => number | null): LensSystem {
  const surfaces = system.surfaces.map(surface => {
    if (surface.solve?.kind !== 'pickup') return surface
    const target = map(surface.solve.surface)
    if (target === null) { const { solve: _removed, ...rest } = surface; return rest }
    return target === surface.solve.surface ? surface : { ...surface, solve: { ...surface.solve, surface: target } }
  })
  const configs = system.configs && {
    ...system.configs,
    rows: system.configs.rows.flatMap(row => { const surface = map(row.surface); return surface === null ? [] : [{ ...row, surface }] }),
  }
  const operands = system.optimization?.operands?.flatMap(operand => {
    if (operand.surface === undefined) return [operand]
    const surface = map(operand.surface)
    return surface === null ? [] : [{ ...operand, surface }]
  })
  const next: LensSystem = { ...system, surfaces }
  if (configs) next.configs = configs
  if (operands && system.optimization) next.optimization = { ...system.optimization, operands }
  return next
}

// ---------- configurations ----------

export function configurationCount(system: LensSystem): number {
  return system.configs ? Math.max(1, system.configs.names.length) : 1
}

export function activeConfiguration(system: LensSystem): number {
  return system.configs ? Math.min(system.configs.active, Math.max(0, system.configs.names.length - 1)) : 0
}

/** The row of the configuration table that controls a parameter, if any. */
export function configRowFor(system: LensSystem, surface: number, parameter: ConfigParameter): ConfigRow | undefined {
  return system.configs?.rows.find(row => row.surface === surface && row.parameter === parameter)
}

function currentValue(system: LensSystem, surface: number, parameter: ConfigParameter): number | string {
  const s = system.surfaces[surface]
  switch (parameter) {
    case 'radius': return s.radius
    case 'thickness': return s.thickness
    case 'conic': return s.conic ?? 0
    case 'material': return s.material
    case 'decenterX': return s.decenter?.[0] ?? 0
    case 'decenterY': return s.decenter?.[1] ?? 0
    case 'tiltX': return s.tilt?.[0] ?? 0
    case 'tiltY': return s.tilt?.[1] ?? 0
  }
}

/** Creates the table with two configurations that start equal. */
export function enableConfigurations(system: LensSystem): LensSystem {
  return system.configs ? system : { ...system, configs: { names: ['Config 1', 'Config 2'], rows: [], active: 0 } }
}

export function addConfiguration(system: LensSystem): LensSystem {
  const configs = system.configs
  if (!configs) return enableConfigurations(system)
  const last = configs.names.length - 1
  return {
    ...system,
    configs: {
      ...configs,
      names: [...configs.names, `Config ${configs.names.length + 1}`],
      rows: configs.rows.map(row => ({ ...row, values: [...row.values, row.values[last] ?? currentValue(system, row.surface, row.parameter)] })),
    },
  }
}

export function removeConfiguration(system: LensSystem, index: number): LensSystem {
  const configs = system.configs
  if (!configs || configs.names.length <= 1) return system
  const names = configs.names.filter((_, i) => i !== index)
  const optimization = system.optimization?.operands
    ? { ...system.optimization, operands: system.optimization.operands.flatMap(o => o.config === undefined ? [o] : o.config === index ? [] : [{ ...o, config: o.config > index ? o.config - 1 : o.config }]) }
    : system.optimization
  return {
    ...system,
    optimization,
    configs: { names, active: Math.min(configs.active > index ? configs.active - 1 : configs.active, names.length - 1), rows: configs.rows.map(row => ({ ...row, values: row.values.filter((_, i) => i !== index) })) },
  }
}

/** Adds a row whose value in every configuration starts at the surface's current value. Returns the same system when it exists. */
export function addConfigRow(system: LensSystem, surface: number, parameter: ConfigParameter): LensSystem {
  const configs = system.configs
  if (!configs || configRowFor(system, surface, parameter)) return system
  const value = currentValue(system, surface, parameter)
  return { ...system, configs: { ...configs, rows: [...configs.rows, { surface, parameter, values: configs.names.map(() => value) }] } }
}

export function removeConfigRow(system: LensSystem, index: number): LensSystem {
  const configs = system.configs
  if (!configs) return system
  return { ...system, configs: { ...configs, rows: configs.rows.filter((_, i) => i !== index) } }
}

export function setConfigValue(system: LensSystem, row: number, config: number, value: number | string): LensSystem {
  const configs = system.configs
  if (!configs || !configs.rows[row]) return system
  const rows = configs.rows.map((r, i) => i === row ? { ...r, values: configs.names.map((_, k) => k === config ? value : r.values[k] ?? value) } : r)
  return { ...system, configs: { ...configs, rows } }
}

export function setActiveConfiguration(system: LensSystem, active: number): LensSystem {
  const configs = system.configs
  return configs && configs.active !== active ? { ...system, configs: { ...configs, active } } : system
}

export function renameConfiguration(system: LensSystem, index: number, name: string): LensSystem {
  const configs = system.configs
  return configs ? { ...system, configs: { ...configs, names: configs.names.map((n, i) => i === index ? name : n) } } : system
}
