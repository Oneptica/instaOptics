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

// NATIVE_TARGET cross-compiles, e.g. x86_64-apple-darwin on an Apple silicon runner.
const target = process.env.NATIVE_TARGET
execFileSync(cargo, ['build', '-p', 'optics-node', '--profile', profile, ...(target ? ['--target', target] : [])], { stdio: 'inherit' })

const library = { win32: 'optics_node.dll', darwin: 'liboptics_node.dylib' }[process.platform] ?? 'liboptics_node.so'
mkdirSync('native', { recursive: true })
copyFileSync(join('target', ...(target ? [target] : []), profile, library), join('native', 'optics.node'))
console.log(`native/optics.node (${profile})`)
