// The API the preload script exposes to the renderer as window.instaOptics.
import type { EngineMethod, MenuCommand } from '../shared/protocol'

export type UpdateState =
  | { status: 'idle' | 'checking' | 'none' }
  | { status: 'available'; version: string; canInstall: boolean }
  | { status: 'downloading'; version: string; percent: number }
  | { status: 'ready'; version: string }
  | { status: 'error'; message: string }

export interface EngineResult { json: string; ms: number; buffer?: ArrayBuffer }

export interface EngineParams { system?: string; raysPerField?: number; analysis?: string; iterations?: number; target?: number; settings?: string; buffer?: ArrayBuffer; width?: number; height?: number }

export interface InstaOpticsApi {
  platform: string
  engine: {
    request(method: EngineMethod, params?: EngineParams): Promise<EngineResult>
    /** Starts a long request; `onProgress` receives intermediate JSON results and `cancel` asks the engine to stop. */
    start(method: EngineMethod, params: EngineParams, onProgress: (json: string) => void): { result: Promise<EngineResult>; cancel: () => void }
    /** Called each time a (re)connected engine becomes available. */
    onReady(callback: () => void): () => void
  }
  file: {
    open(): Promise<{ path: string; content: string } | null>
    /** Writes to `path`, or asks for a path when it is null. Resolves to the path written, or null if cancelled. */
    save(path: string | null, content: string, suggestedName: string): Promise<string | null>
    /** Asks for a file of the given type and returns its raw bytes. */
    importBinary(name: string, extensions: string[]): Promise<{ path: string; bytes: Uint8Array } | null>
    /** Asks where to write `content`; resolves to the path written or null. */
    export(content: string, defaultName: string, name: string, extensions: string[]): Promise<string | null>
    /** Saves a screenshot of a region of the window (CSS pixels) as PNG. */
    exportPng(rect: { x: number; y: number; width: number; height: number }, defaultName: string): Promise<string | null>
  }
  onMenu(callback: (command: MenuCommand) => void): () => void
  setDocumentState(state: { dirty: boolean; path: string | null }): void
  closeWindow(): void
  menu: {
    get(): Promise<Array<{ label: string; items: Array<{ label: string; accelerator?: string; type: string; checked: boolean; enabled: boolean; path: number[] }> }>>
    invoke(path: number[]): void
  }
  update: {
    get(): Promise<UpdateState>
    onState(callback: (state: UpdateState) => void): () => void
    check(): void
    /** Downloads the update, or opens the download page where in-place updates are not possible. */
    download(): void
    install(): void
  }
  /** Colours of the OS window buttons drawn over the title bar (Windows and Linux). */
  setTitleBarColors(colors: { color: string; symbolColor: string }): void
  /** Saves the 3D rendering mode and restarts the app. */
  setSoftwareRendering(enabled: boolean): void
}

declare global {
  interface Window { instaOptics: InstaOpticsApi }
}
