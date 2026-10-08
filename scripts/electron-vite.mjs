// Runs electron-vite, adding --noSandbox on Linux when Electron's chrome-sandbox helper is not setuid root
// (the default after npm install; Ubuntu 24.04+ also blocks the user-namespace sandbox for unpackaged apps).
import { spawnSync } from 'node:child_process'
import { statSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const args = process.argv.slice(2)
if (process.platform === 'linux') {
  const require = createRequire(import.meta.url)
  const helper = join(dirname(require.resolve('electron')), 'dist', 'chrome-sandbox')
  let configured = false
  try { const stat = statSync(helper); configured = stat.uid === 0 && (stat.mode & 0o4000) !== 0 } catch { /* missing helper */ }
  if (!configured) args.push('--noSandbox')
}
const cli = join(dirname(createRequire(import.meta.url).resolve('electron-vite/package.json')), 'bin', 'electron-vite.js')
const result = spawnSync(process.execPath, [cli, ...args], { stdio: 'inherit' })
process.exit(result.status ?? 1)
