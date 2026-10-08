import { DockviewReact, type DockviewApi, type DockviewReadyEvent, type DockviewTheme, type IDockviewPanelProps } from 'dockview-react'
import { useCallback, useEffect, useMemo, useReducer, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { type GlassInfo, type LensSystem, type Overview, type Sample, isLensSystem, newSystem } from '../../shared/lens'
import type { MenuCommand } from '../../shared/protocol'
import {
  type EngineStatus, type Workbench, WorkbenchContext, documentReducer, fileName, initialDocument, isDirty, parseDocument, serializeDocument,
} from './document'
import { formatFixed } from './format'
import { PANELS, panelTitle } from './panels/registry'
import { showWelcomeOnStartup } from './panels/Welcome'
import { TitleBar } from './TitleBar'
import { UpdateStatus } from './UpdateStatus'
import { decodeZmx, exportZmx, importZmx } from './zemax'
import { SIDEBAR_VIEWS, Sidebar, type SidebarView } from './sidebar/Sidebar'

const api = window.instaOptics

const storage = {
  get(key: string): string | null { try { return localStorage.getItem(key) } catch { return null } },
  set(key: string, value: string) { try { localStorage.setItem(key, value) } catch { /* storage unavailable */ } },
  remove(key: string) { try { localStorage.removeItem(key) } catch { /* storage unavailable */ } },
}

const KEYS = { layout: 'instaoptics:dock-layout', theme: 'instaoptics:theme', sidebar: 'instaoptics:sidebar' }

const dockTheme: DockviewTheme = { name: 'instaoptics', className: 'dockview-theme-instaoptics' }

const components: Record<string, (props: IDockviewPanelProps) => React.ReactNode> = Object.fromEntries(PANELS.map(panel => [panel.id, panel.render]))

function defaultLayout(dock: DockviewApi) {
  dock.clear()
  const add = (id: string, position?: { referencePanel: string; direction: 'below' | 'right' | 'within' }) =>
    dock.addPanel({ id, component: id, title: panelTitle(id), position })
  add('lensData')
  add('systemData', { referencePanel: 'lensData', direction: 'within' })
  add('welcome', { referencePanel: 'lensData', direction: 'within' })
  add('layout', { referencePanel: 'lensData', direction: 'below' })
  add('spot', { referencePanel: 'layout', direction: 'right' })
  add('mtf', { referencePanel: 'spot', direction: 'within' })
  add('rayFan', { referencePanel: 'spot', direction: 'within' })
  dock.getPanel('spot')?.api.setActive()
  dock.getPanel('welcome')?.api.setActive()
  dock.getPanel('lensData')?.group.api.setSize({ height: Math.round(dock.height * 0.36) })
  dock.getPanel('spot')?.group.api.setSize({ width: Math.round(dock.width * 0.45) })
}

function openPanel(dock: DockviewApi | null, id: string) {
  if (!dock || !components[id]) return
  if (dock.hasMaximizedGroup()) dock.exitMaximizedGroup()
  // The welcome page is a start page: opening any window closes it.
  if (id !== 'welcome') dock.getPanel('welcome')?.api.close()
  const existing = dock.getPanel(id)
  if (existing) { existing.api.setActive(); return }
  // Editor-side windows share the Lens Data group; analysis windows join the group of an open analysis window.
  const editorSide = new Set(['lensData', 'systemData', 'welcome'])
  const anchor = (editorSide.has(id)
    ? dock.panels.find(panel => editorSide.has(panel.id))
    : dock.panels.find(panel => !editorSide.has(panel.id) && panel.id !== 'layout')) ?? dock.activePanel
  dock.addPanel({ id, component: id, title: panelTitle(id), position: anchor ? { referencePanel: anchor.id, direction: 'within' } : undefined })
}

type Theme = 'dark' | 'light'

function initialTheme(): Theme {
  const saved = storage.get(KEYS.theme)
  if (saved === 'dark' || saved === 'light') return saved
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

export function App() {
  const [doc, dispatch] = useReducer(documentReducer, undefined, () => initialDocument(newSystem()))
  const [engine, setEngine] = useState<EngineStatus>({ version: null, overview: null, error: null, ms: null })
  const [engineEpoch, setEngineEpoch] = useState(0)
  const [glasses, setGlasses] = useState<GlassInfo[]>([])
  const [samples, setSamples] = useState<Sample[]>([])
  const [raysPerField, setRaysPerField] = useState(7)
  const [theme, setTheme] = useState<Theme>(initialTheme)
  const [view, setView] = useState<SidebarView | null>('system')
  const [sidebarWidth, setSidebarWidth] = useState(() => Number(storage.get(KEYS.sidebar)) || 280)
  const dockRef = useRef<DockviewApi | null>(null)
  const docRef = useRef(doc)
  const startedRef = useRef(false) // the Cooke triplet opens once, on the first engine connection
  useEffect(() => { docRef.current = doc }, [doc])

  const edit = useCallback((update: (system: LensSystem) => LensSystem) => dispatch({ type: 'edit', update }), [])

  // Engine: static data once per connection, and a fresh overview after every edit.
  useEffect(() => api.engine.onReady(() => setEngineEpoch(epoch => epoch + 1)), [])
  useEffect(() => {
    let cancelled = false
    void Promise.all([api.engine.request('version'), api.engine.request('glassCatalog'), api.engine.request('samples')]).then(([version, catalog, list]) => {
      if (cancelled) return
      setEngine(state => ({ ...state, version: JSON.parse(version.json) as string }))
      setGlasses(JSON.parse(catalog.json) as GlassInfo[])
      const parsed = JSON.parse(list.json) as Sample[]
      setSamples(parsed)
      if (!startedRef.current) {
        startedRef.current = true
        const cooke = parsed.find(sample => sample.id === 'cooke')
        if (cooke && docRef.current.past.length === 0 && docRef.current.path === null) dispatch({ type: 'load', system: cooke.system, path: null })
      }
    }).catch(() => { /* reported by the overview request */ })
    return () => { cancelled = true }
  }, [engineEpoch])

  useEffect(() => {
    let cancelled = false
    api.engine.request('overview', { system: JSON.stringify(doc.system), raysPerField }).then(
      result => { if (!cancelled) setEngine(state => ({ ...state, overview: JSON.parse(result.json) as Overview, error: null, ms: result.ms })) },
      (error: Error) => { if (!cancelled) setEngine(state => ({ ...state, error: error.message, ms: null })) },
    )
    return () => { cancelled = true }
  }, [doc.system, raysPerField, engineEpoch])

  // Window title and the main process's view of unsaved changes.
  const dirty = isDirty(doc)
  const windowTitle = `${dirty ? '● ' : ''}${doc.path ? fileName(doc.path) : doc.system.name} — instaOptics`
  useEffect(() => {
    document.title = windowTitle
    api.setDocumentState({ dirty, path: doc.path })
  }, [dirty, doc.path, windowTitle])

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    storage.set(KEYS.theme, theme)
    const style = getComputedStyle(document.documentElement)
    api.setTitleBarColors({ color: style.getPropertyValue('--bg-header').trim(), symbolColor: style.getPropertyValue('--text').trim() })
  }, [theme])

  const confirmDiscard = useCallback(() => !isDirty(docRef.current) || window.confirm('Discard unsaved changes to the current lens?'), [])

  const save = useCallback(async (saveAs: boolean) => {
    const current = docRef.current
    const path = await api.file.save(saveAs ? null : current.path, serializeDocument(current.system), current.system.name)
    if (path) dispatch({ type: 'saved', path })
    return path !== null
  }, [])

  const open = useCallback(async () => {
    if (!confirmDiscard()) return
    const file = await api.file.open()
    if (!file) return
    try {
      const system = parseDocument(file.content)
      if (!isLensSystem(system)) throw new Error('not an instaOptics lens')
      dispatch({ type: 'load', system: { ...newSystem(), ...system }, path: file.path })
    } catch (error) {
      window.alert(`Could not open ${fileName(file.path)}: ${(error as Error).message}`)
    }
  }, [confirmDiscard])

  const importZemax = useCallback(async () => {
    if (!confirmDiscard()) return
    const file = await api.file.importBinary('Zemax lens', ['zmx'])
    if (!file) return
    try {
      const { system, warnings } = importZmx(decodeZmx(file.bytes), glasses)
      dispatch({ type: 'load', system, path: null })
      if (warnings.length) window.alert(`Imported ${fileName(file.path)} with notes:\n\n• ${warnings.join('\n• ')}`)
    } catch (error) {
      window.alert(`Could not import ${fileName(file.path)}: ${(error as Error).message}`)
    }
  }, [confirmDiscard, glasses])

  const runCommand = useCallback((command: MenuCommand) => {
    const editingText = document.activeElement instanceof HTMLInputElement || document.activeElement instanceof HTMLTextAreaElement
    switch (command) {
      case 'file:new': if (confirmDiscard()) dispatch({ type: 'load', system: newSystem(), path: null }); break
      case 'file:open': void open(); break
      case 'file:save': void save(false); break
      case 'file:saveAs': void save(true); break
      case 'file:importZmx': void importZemax(); break
      case 'file:exportZmx': void api.file.export(exportZmx(docRef.current.system), `${docRef.current.system.name}.zmx`, 'Zemax lens', ['zmx']); break
      case 'file:saveAndClose': void save(false).then(saved => { if (saved) api.closeWindow() }); break
      case 'edit:undo': if (editingText) document.execCommand('undo'); else dispatch({ type: 'undo' }); break
      case 'edit:redo': if (editingText) document.execCommand('redo'); else dispatch({ type: 'redo' }); break
      case 'view:theme': setTheme(current => current === 'dark' ? 'light' : 'dark'); break
      case 'view:resetLayout': if (dockRef.current) { storage.remove(KEYS.layout); defaultLayout(dockRef.current) } break
      default: if (command.startsWith('window:')) openPanel(dockRef.current, command.slice('window:'.length))
    }
  }, [confirmDiscard, open, save, importZemax])

  useEffect(() => api.onMenu(runCommand), [runCommand])

  const onReady = useCallback((event: DockviewReadyEvent) => {
    dockRef.current = event.api
    // UI automation (screenshots, end-to-end tests) loads the page with ?automation to reach the dock directly.
    if (new URLSearchParams(location.search).has('automation')) Object.assign(window, { __dock: event.api })
    const saved = storage.get(KEYS.layout)
    try {
      if (!saved) throw new Error('no saved layout')
      event.api.fromJSON(JSON.parse(saved))
      if (event.api.panels.length === 0) throw new Error('empty layout')
    } catch {
      defaultLayout(event.api)
    }
    if (showWelcomeOnStartup()) {
      // Like an editor's start page: the welcome tab fills the window until the user opens something.
      openPanel(event.api, 'welcome')
      const welcome = event.api.getPanel('welcome')
      if (welcome) event.api.maximizeGroup(welcome)
    }
    event.api.onDidLayoutChange(() => storage.set(KEYS.layout, JSON.stringify(event.api.toJSON())))
  }, [])

  const openSample = useCallback((sample: Sample) => {
    if (confirmDiscard()) dispatch({ type: 'load', system: sample.system, path: null })
  }, [confirmDiscard])

  function startResize(event: ReactPointerEvent<HTMLDivElement>) {
    const startX = event.clientX, startWidth = sidebarWidth
    const target = event.currentTarget
    target.setPointerCapture(event.pointerId)
    const move = (e: PointerEvent) => setSidebarWidth(Math.max(200, Math.min(520, startWidth + e.clientX - startX)))
    const up = () => {
      target.removeEventListener('pointermove', move)
      target.removeEventListener('pointerup', up)
      setSidebarWidth(width => { storage.set(KEYS.sidebar, String(width)); return width })
    }
    target.addEventListener('pointermove', move)
    target.addEventListener('pointerup', up)
  }

  const actions = useMemo(() => ({
    command: (command: MenuCommand) => runCommand(command),
    openWindow: (id: string) => openPanel(dockRef.current, id),
    openSample,
  }), [runCommand, openSample])
  const workbench: Workbench = useMemo(
    () => ({ doc, dispatch, edit, engine, glasses, raysPerField, setRaysPerField, samples, actions }),
    [doc, edit, engine, glasses, raysPerField, samples, actions],
  )
  const paraxial = engine.overview?.paraxial
  const primary = doc.system.wavelengths[doc.system.primaryWavelength]

  return (
    <WorkbenchContext.Provider value={workbench}>
      <div className="workbench">
        <TitleBar title={windowTitle} />
        <div className="workbench-main">
          <nav className="activity-bar" aria-label="Views">
            {SIDEBAR_VIEWS.map(item => (
              <button
                key={item.id}
                className={`activity${view === item.id ? ' active' : ''}`}
                title={item.title}
                aria-pressed={view === item.id}
                onClick={() => setView(view === item.id ? null : item.id)}
              ><i className={`codicon codicon-${item.icon}`} /></button>
            ))}
            <span className="activity-fill" />
            <button className="activity" title={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`} onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>
              <i className="codicon codicon-color-mode" />
            </button>
          </nav>
          {view && (
            <>
              <div style={{ width: sidebarWidth }} className="sidebar-host">
                <Sidebar view={view} samples={samples} onOpenSample={openSample} onOpenWindow={id => openPanel(dockRef.current, id)} />
              </div>
              <div className="sash" onPointerDown={startResize} />
            </>
          )}
          <main className="editor-area">
            <DockviewReact components={components} onReady={onReady} theme={dockTheme} />
          </main>
        </div>
        <footer className={`status-bar${engine.error ? ' error' : ''}`}>
          <span className="status-item" title="Rust engine in the compute process">
            <i className="codicon codicon-server-process" /> {engine.version ? `Engine ${engine.version}` : 'Engine starting…'}
          </span>
          {engine.error
            ? <span className="status-item"><i className="codicon codicon-error" /> {engine.error}</span>
            : engine.ms !== null && <span className="status-item" title="Time for paraxial data, apertures and layout">{engine.ms.toFixed(2)} ms</span>}
          <span className="status-fill" />
          <UpdateStatus />
          {paraxial && <>
            <span className="status-item mono">EFL {formatFixed(paraxial.efl)}</span>
            <span className="status-item mono">F/# {formatFixed(paraxial.fNumber, 3)}</span>
            <span className="status-item mono">TTL {formatFixed(paraxial.totalTrack, 3)} mm</span>
          </>}
          {primary !== undefined && <span className="status-item mono">λ {primary.toFixed(4)} µm</span>}
          <span className="status-item">{doc.system.surfaces.length} surfaces</span>
        </footer>
      </div>
    </WorkbenchContext.Provider>
  )
}
