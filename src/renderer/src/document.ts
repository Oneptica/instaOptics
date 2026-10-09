// The open lens document: current system, undo/redo history, file path and saved state.
import { createContext, useContext } from 'react'
import type { GlassInfo, LensSystem, Overview, Sample } from '../../shared/lens'
import { attachGlasses } from './glassLibrary'
import type { MenuCommand } from '../../shared/protocol'

const HISTORY_LIMIT = 200

export interface DocumentState {
  system: LensSystem
  path: string | null
  saved: LensSystem // the system as last loaded or saved; dirty when it differs from `system`
  past: LensSystem[]
  future: LensSystem[]
}

export type DocumentAction =
  | { type: 'edit'; update: (system: LensSystem) => LensSystem }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'load'; system: LensSystem; path: string | null }
  /** Shows a system without recording history (live optimizer progress). */
  | { type: 'preview'; system: LensSystem }
  /** Records `base` as the undo point for the current (previewed) system. */
  | { type: 'commit'; base: LensSystem }
  | { type: 'saved'; path: string }
  /** Re-attaches catalog glass definitions after the catalogs changed; not an edit. */
  | { type: 'glasses' }

export function initialDocument(system: LensSystem): DocumentState {
  return { system, path: null, saved: system, past: [], future: [] }
}

export function documentReducer(state: DocumentState, action: DocumentAction): DocumentState {
  switch (action.type) {
    case 'edit': {
      const system = attachGlasses(action.update(state.system))
      if (system === state.system) return state
      return { ...state, system, past: [...state.past, state.system].slice(-HISTORY_LIMIT), future: [] }
    }
    case 'undo': {
      const previous = state.past[state.past.length - 1]
      if (!previous) return state
      return { ...state, system: previous, past: state.past.slice(0, -1), future: [state.system, ...state.future] }
    }
    case 'redo': {
      const [next, ...future] = state.future
      if (!next) return state
      return { ...state, system: next, past: [...state.past, state.system], future }
    }
    case 'preview':
      return { ...state, system: action.system }
    case 'commit':
      return state.system === action.base ? state : { ...state, past: [...state.past, action.base].slice(-HISTORY_LIMIT), future: [] }
    case 'load': {
      const system = attachGlasses(action.system)
      return { system, path: action.path, saved: system, past: [], future: [] }
    }
    case 'glasses': {
      const system = attachGlasses(state.system)
      return system === state.system ? state : { ...state, system, saved: state.saved === state.system ? system : state.saved }
    }
    case 'saved':
      return { ...state, path: action.path, saved: state.system }
  }
}

export const isDirty = (state: DocumentState) => state.system !== state.saved

export interface EngineStatus {
  version: string | null
  overview: Overview | null // last successful result; may be stale while `error` is set
  error: string | null
  ms: number | null
}

export interface Workbench {
  doc: DocumentState
  dispatch: (action: DocumentAction) => void
  edit: (update: (system: LensSystem) => LensSystem) => void
  engine: EngineStatus
  glasses: GlassInfo[]
  raysPerField: number
  setRaysPerField: (rays: number) => void
  samples: Sample[]
  actions: {
    command: (command: MenuCommand) => void
    openWindow: (id: string) => void
    openSample: (sample: Sample) => void
  }
}

export const WorkbenchContext = createContext<Workbench | null>(null)

export function useWorkbench(): Workbench {
  const value = useContext(WorkbenchContext)
  if (!value) throw new Error('useWorkbench outside WorkbenchContext')
  return value
}

const FORMAT = 'instaoptics-lens'

export function serializeDocument(system: LensSystem): string {
  return JSON.stringify({ format: FORMAT, version: 1, system }, null, 2) + '\n'
}

/** Accepts a saved document or a bare system object. */
export function parseDocument(text: string): unknown {
  const value = JSON.parse(text) as { format?: string; system?: unknown }
  return value && value.format === FORMAT ? value.system : value
}

export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path
