// Compute process: an Electron utility process that loads the Rust engine (native/optics.node) and serves
// requests from renderers over MessagePorts. Long computations here never block the UI or the main process.
import type { MessagePortMain } from 'electron'
import type { EngineRequest, EngineResponse, OptimizeResult } from '../shared/protocol'

interface NativeEngine {
  version(): string
  overview(system: string, raysPerField: number): string
  analyze(system: string, request: string): Promise<string>
  optimizeStep(system: string, damping: number): Promise<string>
  toleranceSensitivity(system: string, settings: string): Promise<string>
  toleranceMonteCarlo(system: string, settings: string, start: number, count: number): Promise<string>
  simulateImage(system: string, settings: string, rgba: Buffer, width: number, height: number): Promise<Buffer>
  samples(): string
  glassCatalog(): string
}

const native = require(process.argv[process.argv.length - 1]) as NativeEngine

const cancelled = new Set<number>()

interface Step { system: unknown; merit: number; previous: number; damping: number; improved: boolean; variables: number }

/** Damped least squares, one native step at a time so progress can be reported and the run stopped. */
async function optimize(request: EngineRequest, progress: (json: string) => void): Promise<string> {
  let system = request.system ?? ''
  let damping = 1e-3, stalled = 0, iterations = 0, initialMerit = NaN, finalMerit = NaN, variables = 0
  const limit = request.iterations ?? 50
  try {
    while (iterations < limit && !cancelled.has(request.id)) {
      const step = JSON.parse(await native.optimizeStep(system, damping)) as Step
      variables = step.variables
      if (Number.isNaN(initialMerit)) initialMerit = step.previous
      finalMerit = step.merit
      if (!variables) throw new Error('No variables: mark radii, thicknesses, conics or aspheric terms as variable in the Lens Data editor')
      if (!Number.isFinite(step.previous)) throw new Error('The merit function cannot be evaluated for this system')
      if (!step.improved) break
      iterations++
      system = JSON.stringify(step.system)
      damping = step.damping
      progress(JSON.stringify({ iteration: iterations, merit: step.merit, system: step.system }))
      stalled = (step.previous - step.merit) / step.previous < 1e-6 ? stalled + 1 : 0
      if (stalled >= 3) break
    }
    const result: OptimizeResult = { system: JSON.parse(system), initialMerit, finalMerit, iterations, variables, stopped: cancelled.has(request.id) }
    return JSON.stringify(result)
  } finally {
    cancelled.delete(request.id)
  }
}

/** Monte Carlo in chunks so progress streams and the run can stop; trials are independent of the chunking. */
async function monteCarlo(request: EngineRequest, progress: (json: string) => void): Promise<string> {
  const system = request.system ?? '', settings = request.settings ?? '{}'
  const total = Math.max(1, (JSON.parse(settings) as { trials?: number }).trials ?? 200)
  const values: number[] = []
  let nominal = NaN
  try {
    while (values.length < total && !cancelled.has(request.id)) {
      const count = Math.min(Math.max(50, Math.ceil(total / 20)), total - values.length)
      const chunk = JSON.parse(await native.toleranceMonteCarlo(system, settings, values.length, count)) as { nominal: number; values: number[] }
      nominal = chunk.nominal
      values.push(...chunk.values)
      progress(JSON.stringify({ nominal, values, total }))
    }
    return JSON.stringify({ nominal, values, total })
  } finally {
    cancelled.delete(request.id)
  }
}

/** Returns the pitch as JSON and the linear RGB image as a binary payload. */
async function simulate(request: EngineRequest): Promise<{ json: string; buffer: ArrayBuffer }> {
  if (!request.buffer || !request.width || !request.height) throw new Error('No image to simulate')
  const out = await native.simulateImage(request.system ?? '', request.settings ?? '{}', Buffer.from(request.buffer), request.width, request.height)
  const pitch = out.readDoubleLE(0)
  const pixels = out.buffer.slice(out.byteOffset + 8, out.byteOffset + out.byteLength) as ArrayBuffer
  return { json: JSON.stringify({ pitch, width: request.width, height: request.height }), buffer: pixels }
}

async function run(request: EngineRequest, progress: (json: string) => void): Promise<string | { json: string; buffer: ArrayBuffer }> {
  switch (request.method) {
    case 'simulate': return simulate(request)
    case 'optimize': return optimize(request, progress)
    case 'sensitivity': return native.toleranceSensitivity(request.system ?? '', request.settings ?? '{}')
    case 'monteCarlo': return monteCarlo(request, progress)
    case 'cancel': if (request.target !== undefined) cancelled.add(request.target); return 'null'
    case 'version': return JSON.stringify(native.version())
    case 'overview': return native.overview(request.system ?? '', request.raysPerField ?? 7)
    case 'analyze': return native.analyze(request.system ?? '', request.analysis ?? '')
    case 'samples': return native.samples()
    case 'glassCatalog': return native.glassCatalog()
  }
}

function serve(port: MessagePortMain) {
  // Analyses run on worker threads, so requests may complete out of order; responses carry the request id.
  port.on('message', async ({ data }: { data: EngineRequest }) => {
    const start = performance.now()
    let response: EngineResponse
    try {
      const result = await run(data, progress => port.postMessage({ id: data.id, progress } satisfies EngineResponse))
      response = typeof result === 'string'
        ? { id: data.id, ok: true, json: result, ms: performance.now() - start }
        : { id: data.id, ok: true, json: result.json, buffer: result.buffer, ms: performance.now() - start }
    } catch (error) {
      response = { id: data.id, ok: false, error: error instanceof Error ? error.message : String(error), ms: performance.now() - start }
    }
    port.postMessage(response)
  })
  port.start()
}

process.parentPort.on('message', event => {
  if ((event.data as { type?: string })?.type === 'connect' && event.ports[0]) serve(event.ports[0])
})
