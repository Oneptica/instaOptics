// Messages between the renderer and the compute process. They travel over a MessagePort that the main
// process hands to both sides, so engine traffic never passes through the main process.

export type EngineMethod = 'version' | 'overview' | 'analyze' | 'optimize' | 'sensitivity' | 'monteCarlo' | 'simulate' | 'cancel' | 'samples' | 'glassCatalog'

export interface EngineRequest {
  id: number
  method: EngineMethod
  /** JSON of the lens system, for methods that take one. */
  system?: string
  raysPerField?: number
  /** JSON of an analysis request, e.g. {"kind":"spot","rings":6}. */
  analysis?: string
  /** Maximum optimizer iterations. */
  iterations?: number
  /** Request id that a 'cancel' request stops. */
  target?: number
  /** JSON of tolerance or simulation settings. */
  settings?: string
  /** Binary payload (8-bit RGBA image for 'simulate') with its size. */
  buffer?: ArrayBuffer
  width?: number
  height?: number
}

export type EngineResponse =
  | { id: number; ok: true; json: string; ms: number; buffer?: ArrayBuffer }
  | { id: number; ok: false; error: string; ms: number }
  | { id: number; progress: string } // intermediate result of a long request (JSON)

export interface OptimizeProgress { iteration: number; merit: number; system: unknown }
export interface OptimizeResult { system: unknown; initialMerit: number; finalMerit: number; iterations: number; variables: number; stopped: boolean }

/** Commands sent from the native menu to the renderer. */
export type MenuCommand =
  | 'file:new' | 'file:open' | 'file:save' | 'file:saveAs' | 'file:saveAndClose' | 'file:importZmx' | 'file:exportZmx'
  | 'edit:undo' | 'edit:redo'
  | 'view:theme' | 'view:resetLayout'
  | `window:${string}`

export interface MonteCarloProgress { nominal: number; values: number[]; total: number }
