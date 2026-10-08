// Builds the Rust Node-API module and copies it to native/optics.node for Electron to load.
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const release = process.argv.includes('--release')
const profile = release ? 'release' : 'native-dev'
const exe = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
const fallback = join(homedir(), '.cargo', 'bin', exe)
const cargo = process.env.CARGO ?? (existsSync(fallback) ? fallback : exe)

execFileSync(cargo, ['build', '-p', 'optics-node', '--profile', profile], { stdio: 'inherit' })

const library = { win32: 'optics_node.dll', darwin: 'liboptics_node.dylib' }[process.platform] ?? 'liboptics_node.so'
mkdirSync('native', { recursive: true })
copyFileSync(join('target', profile, library), join('native', 'optics.node'))
console.log(`native/optics.node (${profile})`)
