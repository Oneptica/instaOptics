// Update checks against GitHub Releases. Windows (NSIS) and Linux AppImage update in place; macOS needs a signed
// app for that and deb packages are managed by the system, so there we only point to the download page.
import { BrowserWindow, app, ipcMain, shell } from 'electron'
import electronUpdater from 'electron-updater'

const { autoUpdater } = electronUpdater
const RELEASES = 'https://github.com/Oneptica/instaOptics/releases/latest'

export type UpdateState =
  | { status: 'idle' | 'checking' | 'none' }
  | { status: 'available'; version: string; canInstall: boolean }
  | { status: 'downloading'; version: string; percent: number }
  | { status: 'ready'; version: string }
  | { status: 'error'; message: string }

let state: UpdateState = { status: 'idle' }

const canInstall = () => process.platform === 'win32' || (process.platform === 'linux' && !!process.env.APPIMAGE)

function publish(next: UpdateState) {
  state = next
  for (const window of BrowserWindow.getAllWindows()) window.webContents.send('update:state', state)
}

export function checkForUpdates(manual = false) {
  if (!app.isPackaged) {
    if (manual) publish({ status: 'error', message: 'Updates are only checked in installed builds' })
    return
  }
  publish({ status: 'checking' })
  autoUpdater.checkForUpdates().catch((error: Error) => publish(manual ? { status: 'error', message: error.message } : { status: 'idle' }))
}

export function setupUpdater() {
  // Where the app can replace itself, the update downloads in the background and only needs a restart.
  autoUpdater.autoDownload = canInstall()
  autoUpdater.autoInstallOnAppQuit = true
  autoUpdater.on('update-available', info => publish({ status: 'available', version: info.version, canInstall: canInstall() }))
  autoUpdater.on('update-not-available', () => publish({ status: 'none' }))
  autoUpdater.on('download-progress', progress => {
    if (state.status === 'available' || state.status === 'downloading') publish({ status: 'downloading', version: state.version, percent: progress.percent })
  })
  autoUpdater.on('update-downloaded', info => publish({ status: 'ready', version: info.version }))
  autoUpdater.on('error', error => { if (state.status !== 'idle') publish({ status: 'error', message: error.message }) })

  ipcMain.handle('update:get', () => state)
  ipcMain.on('update:check', () => checkForUpdates(true))
  ipcMain.on('update:download', () => {
    if (state.status !== 'available') return
    if (!state.canInstall) { void shell.openExternal(RELEASES); return }
    void autoUpdater.downloadUpdate()
  })
  // Silent: no installer wizard, and the app starts again by itself.
  ipcMain.on('update:install', () => autoUpdater.quitAndInstall(true, true))

  // Check shortly after start, then every 6 hours while the app stays open.
  setTimeout(() => checkForUpdates(), 5000)
  setInterval(() => checkForUpdates(), 6 * 60 * 60 * 1000)
}
