import { readFileSync, writeFileSync } from 'node:fs'
import { readFile, writeFile } from 'node:fs/promises'
import { basename, join } from 'node:path'
import { BrowserWindow, MessageChannelMain, app, dialog, ipcMain, nativeTheme, shell, utilityProcess, type UtilityProcess } from 'electron'
import type { MenuCommand } from '../shared/protocol'
import { buildMenu } from './menu'

// Settings that must be known before the app is ready (they change Chromium switches).
interface StartupSettings { softwareRendering?: boolean }
const settingsPath = () => join(app.getPath('userData'), 'startup.json')
function readStartupSettings(): StartupSettings {
  try { return JSON.parse(readFileSync(settingsPath(), 'utf8')) as StartupSettings } catch { return {} }
}
const startup = readStartupSettings()
if (startup.softwareRendering) {
  // SwiftShader WebGL for displays without usable hardware OpenGL (remote desktops, some VMs).
  app.commandLine.appendSwitch('use-angle', 'swiftshader')
  app.commandLine.appendSwitch('enable-unsafe-swiftshader')
}

function setSoftwareRendering(enabled: boolean) {
  writeFileSync(settingsPath(), JSON.stringify({ ...readStartupSettings(), softwareRendering: enabled }))
  app.relaunch()
  app.exit(0)
}

const FILE_FILTERS = [{ name: 'instaOptics lens', extensions: ['iol'] }, { name: 'JSON', extensions: ['json'] }]

let compute: UtilityProcess | null = null
let quitting = false
const dirtyWindows = new Set<number>()

function nativeModulePath() {
  return app.isPackaged ? join(process.resourcesPath, 'native', 'optics.node') : join(app.getAppPath(), 'native', 'optics.node')
}

/** Starts the compute process (restarting it if it dies) and reconnects every open window. */
function startCompute() {
  compute = utilityProcess.fork(join(__dirname, 'compute.js'), [nativeModulePath()], { serviceName: 'instaOptics engine' })
  compute.on('exit', code => {
    compute = null
    if (quitting) return
    console.error(`Compute process exited with code ${code}; restarting`)
    startCompute()
    for (const window of BrowserWindow.getAllWindows()) connectEngine(window)
  })
}

/** Gives the window's renderer a direct MessagePort to the compute process. */
function connectEngine(window: BrowserWindow) {
  if (!compute) return
  const { port1, port2 } = new MessageChannelMain()
  compute.postMessage({ type: 'connect' }, [port1])
  window.webContents.postMessage('engine:port', null, [port2])
}

function sendMenu(command: MenuCommand) {
  BrowserWindow.getFocusedWindow()?.webContents.send('menu', command)
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 900,
    minHeight: 560,
    show: false,
    title: 'instaOptics',
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#1f1f1f' : '#ffffff',
    webPreferences: { preload: join(__dirname, '../preload/index.js'), sandbox: true, contextIsolation: true },
  })
  window.once('ready-to-show', () => window.show())
  window.webContents.on('did-finish-load', () => connectEngine(window))
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https://')) void shell.openExternal(url)
    return { action: 'deny' }
  })
  const id = window.webContents.id
  window.on('close', event => {
    if (!dirtyWindows.has(id)) return
    const choice = dialog.showMessageBoxSync(window, {
      type: 'warning',
      message: 'Save changes to the lens before closing?',
      detail: 'Your changes will be lost if you don\'t save them.',
      buttons: ['Save', 'Don\'t Save', 'Cancel'],
      defaultId: 0,
      cancelId: 2,
    })
    if (choice === 1) return
    event.preventDefault()
    quitting = false
    if (choice === 0) window.webContents.send('menu', 'file:saveAndClose')
  })
  window.on('closed', () => dirtyWindows.delete(id))

  if (process.env.ELECTRON_RENDERER_URL) void window.loadURL(process.env.ELECTRON_RENDERER_URL)
  else void window.loadFile(join(__dirname, '../renderer/index.html'))
  return window
}

ipcMain.handle('file:open', async event => {
  const window = BrowserWindow.fromWebContents(event.sender)!
  const result = await dialog.showOpenDialog(window, { properties: ['openFile'], filters: FILE_FILTERS })
  if (result.canceled || !result.filePaths[0]) return null
  const path = result.filePaths[0]
  app.addRecentDocument(path)
  return { path, content: await readFile(path, 'utf8') }
})

ipcMain.handle('file:save', async (event, path: string | null, content: string, suggestedName: string) => {
  let target = path
  if (!target) {
    const window = BrowserWindow.fromWebContents(event.sender)!
    const result = await dialog.showSaveDialog(window, { defaultPath: `${suggestedName}.iol`, filters: FILE_FILTERS })
    if (result.canceled || !result.filePath) return null
    target = result.filePath
  }
  await writeFile(target, content, 'utf8')
  app.addRecentDocument(target)
  return target
})

ipcMain.handle('file:importBinary', async (event, name: string, extensions: string[]) => {
  const window = BrowserWindow.fromWebContents(event.sender)!
  const result = await dialog.showOpenDialog(window, { properties: ['openFile'], filters: [{ name, extensions }] })
  if (result.canceled || !result.filePaths[0]) return null
  return { path: result.filePaths[0], bytes: new Uint8Array(await readFile(result.filePaths[0])) }
})

ipcMain.handle('file:export', async (event, content: string, defaultName: string, name: string, extensions: string[]) => {
  const window = BrowserWindow.fromWebContents(event.sender)!
  const result = await dialog.showSaveDialog(window, { defaultPath: defaultName, filters: [{ name, extensions }] })
  if (result.canceled || !result.filePath) return null
  await writeFile(result.filePath, content, 'utf8')
  return result.filePath
})

ipcMain.on('document:state', (event, state: { dirty: boolean; path: string | null }) => {
  const window = BrowserWindow.fromWebContents(event.sender)
  if (!window) return
  if (state.dirty) dirtyWindows.add(event.sender.id)
  else dirtyWindows.delete(event.sender.id)
  window.setDocumentEdited(state.dirty)
  if (state.path) window.setRepresentedFilename(state.path)
})

ipcMain.on('window:close', event => {
  dirtyWindows.delete(event.sender.id)
  BrowserWindow.fromWebContents(event.sender)?.close()
})

ipcMain.on('app:softwareRendering', (_event, enabled: boolean) => setSoftwareRendering(enabled))

ipcMain.handle('file:basename', (_event, path: string) => basename(path))

app.whenReady().then(() => {
  app.setName('instaOptics')
  startCompute()
  buildMenu(sendMenu, () => createWindow(), { softwareRendering: !!startup.softwareRendering, setSoftwareRendering })
  createWindow()
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow() })
})

app.on('before-quit', () => { quitting = true })
app.on('will-quit', () => compute?.kill())
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit() })
