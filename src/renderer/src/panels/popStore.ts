// State of the beam propagation window. It lives outside the component so the settings and the last result survive
// the window being hidden behind another tab.
import { useSyncExternalStore } from 'react'
import type { LensSystem } from '../../../shared/lens'

export type SourceKind = 'gaussian' | 'superGaussian' | 'topHat'

export interface PopUi {
  kind: SourceKind
  radius: number | null // null: derive from the entrance pupil
  waist: number // Gaussian: distance of the waist from surface 1, mm
  order: number // super-Gaussian order
  curvature: number | null // radius of curvature of non-Gaussian sources, mm; null is collimated
  wavelength: number
  samples: number
  aberrations: boolean
  apertures: boolean
}

export interface Slice { z: number; pitch: number; wX: number; wY: number; power: number; peak: number; xCut: number[]; yCut: number[]; surface: number | null }
export interface PlaneMap { surface: number; z: number; halfWidth: number; size: number; intensity: number[]; phase: Array<number | null>; wX: number; wY: number; power: number; peak: number }
export interface PopResult {
  wavelength: number
  samples: number
  slices: Slice[]
  planes: PlaneMap[]
  surfaceZ: number[]
  imageZ: number
  zStart: number
  analytic: Array<[number, number]> | null
  aberration: { rmsWaves: number; pvWaves: number } | null
  warnings: string[]
}

interface Store {
  ui: PopUi
  result: PopResult | null
  /** Identifies the system and settings the result was computed for. */
  ranFor: string | null
  busy: boolean
  error: string | null
  ms: number | null
}

const DEFAULT_UI: PopUi = { kind: 'gaussian', radius: null, waist: 0, order: 4, curvature: null, wavelength: 0, samples: 256, aberrations: true, apertures: true }
const KEY = 'instaoptics:pop'

function load(): PopUi {
  try { return { ...DEFAULT_UI, ...(JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<PopUi>) } } catch { return DEFAULT_UI }
}

let store: Store = { ui: load(), result: null, ranFor: null, busy: false, error: null, ms: null }
const listeners = new Set<() => void>()

function update(patch: Partial<Store>) {
  store = { ...store, ...patch }
  listeners.forEach(listener => listener())
}

export const usePopStore = () => useSyncExternalStore(callback => { listeners.add(callback); return () => { listeners.delete(callback) } }, () => store)

export function setPopUi(patch: Partial<PopUi>) {
  const ui = { ...store.ui, ...patch }
  try { localStorage.setItem(KEY, JSON.stringify(ui)) } catch { /* storage unavailable */ }
  update({ ui })
}

/** Beam radius used when the field is empty: a Gaussian fills half the pupil radius, other beams fill it. */
export const defaultRadius = (ui: PopUi, system: LensSystem) => ui.kind === 'gaussian' ? system.entrancePupilDiameter / 4 : system.entrancePupilDiameter / 2

export function popRequest(ui: PopUi, system: LensSystem) {
  const radius = ui.radius ?? defaultRadius(ui, system)
  const source = ui.kind === 'gaussian'
    ? { kind: 'gaussian', radius, waist: ui.waist }
    : ui.kind === 'superGaussian'
      ? { kind: 'superGaussian', radius, order: ui.order, curvatureRadius: ui.curvature }
      : { kind: 'topHat', radius, curvatureRadius: ui.curvature }
  const settings = { source, wavelength: Math.min(ui.wavelength, system.wavelengths.length - 1), samples: ui.samples, aberrations: ui.aberrations, apertures: ui.apertures }
  return { request: { kind: 'pop', settings }, key: JSON.stringify([system, settings]) }
}

export async function runPop(system: LensSystem) {
  const { request, key } = popRequest(store.ui, system)
  update({ busy: true, error: null })
  try {
    const response = await window.instaOptics.engine.request('analyze', { system: JSON.stringify(system), analysis: JSON.stringify(request) })
    update({ result: JSON.parse(response.json) as PopResult, ranFor: key, busy: false, ms: response.ms })
  } catch (error) {
    update({ busy: false, error: (error as Error).message })
  }
}
