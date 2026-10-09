import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { EngineRequest, EngineResponse, MenuCommand } from '../shared/protocol'
import type { InstaOpticsApi } from './api'

// Engine requests go straight to the compute process over the port the main process hands us.
let port: MessagePort | null = null
let nextId = 1
const pending = new Map<number, { resolve: (value: { json: string; ms: number; buffer?: ArrayBuffer }) => void; reject: (error: Error) => void; progress?: (json: string) => void }>()
const queued: EngineRequest[] = []
const readyListeners = new Set<() => void>()

ipcRenderer.on('engine:port', (event: IpcRendererEvent) => {
  // A new port means the compute process (re)started: requests sent to the old one will never be answered.
  const waiting = new Set(queued.map(request => request.id))
  for (const [id, request] of pending) {
    if (waiting.has(id)) continue
    pending.delete(id)
    request.reject(new Error('The engine restarted'))
  }
  port?.close()
  port = event.ports[0]
  port.onmessage = ({ data }: MessageEvent<EngineResponse>) => {
    const request = pending.get(data.id)
    if (!request) return
    if ('progress' in data) { request.progress?.(data.progress); return }
    pending.delete(data.id)
    if (data.ok) request.resolve({ json: data.json, ms: data.ms, buffer: data.buffer })
    else request.reject(new Error(data.error))
  }
  for (const request of queued.splice(0)) port.postMessage(request)
  readyListeners.forEach(listener => listener())
})

function send(method: EngineRequest['method'], params: Omit<EngineRequest, 'id' | 'method'>, progress?: (json: string) => void) {
  const id = nextId++
  const result = new Promise<{ json: string; ms: number; buffer?: ArrayBuffer }>((resolve, reject) => {
    const request: EngineRequest = { id, method, ...params }
    pending.set(id, { resolve, reject, progress })
    if (port) port.postMessage(request)
    else queued.push(request)
  })
  return { id, result }
}

const api: InstaOpticsApi = {
  platform: process.platform,
  engine: {
    request: (method, params = {}) => send(method, params).result,
    start(method, params, onProgress) {
      const { id, result } = send(method, params, onProgress)
      return { result, cancel: () => { void send('cancel', { target: id }).result } }
    },
    onReady(callback) {
      readyListeners.add(callback)
      return () => { readyListeners.delete(callback) }
    },
  },
  file: {
    open: () => ipcRenderer.invoke('file:open'),
    save: (path, content, suggestedName) => ipcRenderer.invoke('file:save', path, content, suggestedName),
    importBinary: (name, extensions) => ipcRenderer.invoke('file:importBinary', name, extensions),
    export: (content, defaultName, name, extensions) => ipcRenderer.invoke('file:export', content, defaultName, name, extensions),
    exportPng: (rect, defaultName) => ipcRenderer.invoke('export:png', rect, defaultName),
  },
  onMenu(callback) {
    const listener = (_event: IpcRendererEvent, command: MenuCommand) => callback(command)
    ipcRenderer.on('menu', listener)
    return () => { ipcRenderer.off('menu', listener) }
  },
  setDocumentState: state => ipcRenderer.send('document:state', state),
  closeWindow: () => ipcRenderer.send('window:close'),
  catalogs: {
    list: () => ipcRenderer.invoke('catalogs:list'),
    add: () => ipcRenderer.invoke('catalogs:add'),
    remove: name => ipcRenderer.invoke('catalogs:remove', name),
  },
  menu: {
    get: () => ipcRenderer.invoke('menu:get'),
    invoke: path => ipcRenderer.send('menu:invoke', path),
  },
  setTitleBarColors: colors => ipcRenderer.send('window:titleBar', colors),
  update: {
    get: () => ipcRenderer.invoke('update:get'),
    onState(callback) {
      const listener = (_event: IpcRendererEvent, state: Parameters<typeof callback>[0]) => callback(state)
      ipcRenderer.on('update:state', listener)
      return () => { ipcRenderer.off('update:state', listener) }
    },
    check: () => ipcRenderer.send('update:check'),
    download: () => ipcRenderer.send('update:download'),
    install: () => ipcRenderer.send('update:install'),
  },
  setSoftwareRendering: enabled => ipcRenderer.send('app:softwareRendering', enabled),
}

contextBridge.exposeInMainWorld('instaOptics', api)
