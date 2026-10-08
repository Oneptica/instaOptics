import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react'
import { Select } from '../analysis/AnalysisFrame'
import { NumberField } from '../components/NumberField'
import { useSize } from '../plots/LinePlot'
import { useWorkbench } from '../document'

interface Scene { width: number; height: number; data: Uint8ClampedArray; name: string }
interface Simulated { width: number; height: number; pitch: number; linear: Float32Array; system: unknown }
interface Isp { exposure: number; gamma: number; wb: [number, number, number]; saturation: number }

const DEFAULT_ISP: Isp = { exposure: 1, gamma: 2.2, wb: [1, 1, 1], saturation: 1 }

function testChart(width: number, height: number): Scene {
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d')!
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, width, height)
  const grid = Math.round(height / 8)
  ctx.strokeStyle = '#d4dbe4'
  ctx.lineWidth = Math.max(1, height / 256)
  for (let x = 0; x <= width; x += grid) { ctx.beginPath(); ctx.moveTo(x + 0.5, 0); ctx.lineTo(x + 0.5, height); ctx.stroke() }
  for (let y = 0; y <= height; y += grid) { ctx.beginPath(); ctx.moveTo(0, y + 0.5); ctx.lineTo(width, y + 0.5); ctx.stroke() }
  const star = (cx: number, cy: number, r: number, spokes = 36) => {
    ctx.fillStyle = '#111827'
    for (let i = 0; i < spokes; i++) {
      const a0 = 2 * Math.PI * i / spokes
      ctx.beginPath(); ctx.moveTo(cx, cy); ctx.arc(cx, cy, r, a0, a0 + Math.PI / spokes); ctx.closePath(); ctx.fill()
    }
  }
  const side = Math.min(width, height)
  star(width / 2, height / 2, side * 0.2)
  for (const [fx, fy] of [[0.16, 0.16], [0.84, 0.16], [0.16, 0.84], [0.84, 0.84], [0.5, 0.16], [0.5, 0.84], [0.16, 0.5], [0.84, 0.5]]) star(width * fx, height * fy, side * 0.08, 24)
  const colors = ['#ef4444', '#f59e0b', '#22c55e', '#06b6d4', '#3b82f6', '#a855f7']
  colors.forEach((color, i) => { ctx.fillStyle = color; ctx.fillRect(width * 0.3 + i * width * 0.4 / colors.length, height * 0.9, width * 0.4 / colors.length, height * 0.05) })
  ctx.fillStyle = '#111827'
  ctx.font = `600 ${Math.round(height * 0.05)}px sans-serif`
  ctx.textAlign = 'center'
  ctx.fillText('instaOptics', width / 2, height * 0.07)
  return { width, height, data: ctx.getImageData(0, 0, width, height).data, name: `Test chart ${width} × ${height}` }
}

function loadImage(file: File, maxSide: number): Promise<Scene> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const image = new Image()
    image.onload = () => {
      const scale = maxSide > 0 ? Math.min(1, maxSide / Math.max(image.width, image.height)) : 1
      const canvas = document.createElement('canvas')
      canvas.width = Math.max(2, Math.round(image.width * scale))
      canvas.height = Math.max(2, Math.round(image.height * scale))
      const ctx = canvas.getContext('2d')!
      ctx.drawImage(image, 0, 0, canvas.width, canvas.height)
      URL.revokeObjectURL(url)
      resolve({ width: canvas.width, height: canvas.height, data: ctx.getImageData(0, 0, canvas.width, canvas.height).data, name: file.name })
    }
    image.onerror = () => { URL.revokeObjectURL(url); reject(new Error(`Could not read ${file.name}`)) }
    image.src = url
  })
}

function applyIsp(sim: Simulated, isp: Isp): Uint8ClampedArray {
  const out = new Uint8ClampedArray(sim.width * sim.height * 4)
  const inverse = 1 / isp.gamma
  const lut = new Uint8ClampedArray(4097)
  for (let i = 0; i <= 4096; i++) lut[i] = Math.round(255 * (i / 4096) ** inverse)
  const level = (v: number) => lut[Math.round(Math.max(0, Math.min(1, v)) * 4096)]
  const { linear } = sim
  for (let i = 0, n = sim.width * sim.height; i < n; i++) {
    let r = linear[i * 3] * isp.wb[0] * isp.exposure, g = linear[i * 3 + 1] * isp.wb[1] * isp.exposure, b = linear[i * 3 + 2] * isp.wb[2] * isp.exposure
    if (isp.saturation !== 1) {
      const y = 0.2126 * r + 0.7152 * g + 0.0722 * b
      r = y + (r - y) * isp.saturation; g = y + (g - y) * isp.saturation; b = y + (b - y) * isp.saturation
    }
    out[i * 4] = level(r); out[i * 4 + 1] = level(g); out[i * 4 + 2] = level(b); out[i * 4 + 3] = 255
  }
  return out
}

function ImageCanvas({ pixels, width, height, style }: { pixels: Uint8ClampedArray; width: number; height: number; style?: CSSProperties }) {
  const ref = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    canvas.width = width
    canvas.height = height
    canvas.getContext('2d')!.putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0)
  }, [pixels, width, height])
  return <canvas ref={ref} className="sim-canvas" style={style} />
}

export function ImageSimPanel() {
  const { doc } = useWorkbench()
  const [chartSize, setChartSize] = useState(1024)
  const [maxSide, setMaxSide] = useState(1600)
  const [scene, setScene] = useState<Scene>(() => testChart(1024, 683))
  const [rings, setRings] = useState(6)
  const [pitch, setPitch] = useState<number | null>(null)
  const [diffraction, setDiffraction] = useState(true)
  const [view, setView] = useState<'input' | 'simulated' | 'compare'>('compare')
  const [split, setSplit] = useState(50)
  const [isp, setIsp] = useState<Isp>(DEFAULT_ISP)
  const [sim, setSim] = useState<Simulated | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [ms, setMs] = useState<number | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const [stageRef, stage] = useSize<HTMLDivElement>()

  function run() {
    setBusy(true)
    setError(null)
    const began = performance.now()
    const buffer = scene.data.slice().buffer
    const system = doc.system
    window.instaOptics.engine.request('simulate', {
      system: JSON.stringify(system),
      settings: JSON.stringify({ pixelPitch: pitch ?? 0, rings, fieldSamples: 32, diffraction }),
      buffer, width: scene.width, height: scene.height,
    }).then(
      response => {
        const meta = JSON.parse(response.json) as { pitch: number; width: number; height: number }
        setSim({ ...meta, linear: new Float32Array(response.buffer!), system })
        setMs(performance.now() - began)
        if (view === 'input') setView('compare')
      },
      (e: Error) => setError(e.message),
    ).finally(() => setBusy(false))
  }

  const output = useMemo(() => sim ? applyIsp(sim, isp) : null, [sim, isp])
  const stale = sim !== null && sim.system !== doc.system
  const setWb = (c: number, v: number) => setIsp(s => ({ ...s, wb: s.wb.map((x, i) => i === c ? v : x) as Isp['wb'] }))
  const shownSim = output && sim && sim.width === scene.width && sim.height === scene.height

  return (
    <div className="panel image-sim">
      <div className="panel-toolbar">
        <button className="tool labeled primary" disabled={busy} onClick={run}><i className="codicon codicon-play" /> Simulate</button>
        <span className="tool-separator" />
        <Select label="Chart" value={chartSize} options={[512, 1024, 2048].map(v => ({ value: v, label: `${v} px` }))} onChange={v => { setChartSize(v); setScene(testChart(v, Math.round(v * 2 / 3))); setSim(null) }} />
        <button className="tool labeled" onClick={() => fileRef.current?.click()}><i className="codicon codicon-folder-opened" /> Image…</button>
        <input ref={fileRef} type="file" accept="image/*" hidden onChange={event => {
          const file = event.target.files?.[0]
          event.target.value = ''
          if (file) loadImage(file, maxSide).then(next => { setScene(next); setSim(null) }, (e: Error) => setError(e.message))
        }} />
        <Select label="Max side" value={maxSide} options={[800, 1600, 3200, 0].map(v => ({ value: v, label: v ? `${v} px` : 'Full' }))} onChange={setMaxSide} />
        <span className="tool-separator" />
        <Select label="View" value={view} options={[{ value: 'input', label: 'Input' }, { value: 'simulated', label: 'Simulated' }, { value: 'compare', label: 'Compare' }]} onChange={setView} />
        <span className="toolbar-fill" />
        <span className="toolbar-info mono">
          {scene.name} · {scene.width} × {scene.height}
          {sim && ` · pitch ${sim.pitch.toFixed(2)} µm`}
          {stale && ' · lens changed'}
          {busy ? ' · ' : ms !== null && ` · ${ms.toFixed(0)} ms`}
          {busy && <i className="codicon codicon-loading codicon-modifier-spin" />}
        </span>
      </div>
      <div className="sim-body">
        <div className="sim-stage" ref={stageRef}>
          {error && <div className="plot-banner"><i className="codicon codicon-error" /> {error}</div>}
          <div className="sim-frame" style={frameSize(stage, scene, view === 'compare' && !!shownSim)}>
            {(view !== 'simulated' || !shownSim) && <ImageCanvas pixels={scene.data} width={scene.width} height={scene.height} />}
            {view !== 'input' && shownSim && (
              <ImageCanvas pixels={output} width={sim.width} height={sim.height} style={view === 'compare' ? { position: 'absolute', inset: 0, clipPath: `inset(0 0 0 ${split}%)` } : undefined} />
            )}
            {view === 'compare' && shownSim && <div className="sim-split" style={{ left: `${split}%` }} />}
          </div>
          {view === 'compare' && shownSim && (
            <input className="sim-slider" type="range" min={0} max={100} value={split} onChange={e => setSplit(Number(e.target.value))} aria-label="Comparison split" />
          )}
        </div>
        <div className="sim-side tool-form">
          <h3>Simulation</h3>
          <div className="form-grid">
            <label>Pupil rings</label>
            <select value={rings} onChange={e => setRings(Number(e.target.value))}>{[3, 4, 6, 8, 10, 12].map(v => <option key={v} value={v}>{v}</option>)}</select>
            <span />
            <label>Pixel pitch</label>
            <NumberField optional min={0} ariaLabel="Pixel pitch" placeholder="fit" value={pitch} onCommit={setPitch} />
            <span className="unit">µm</span>
            <label className="check span2"><input type="checkbox" checked={diffraction} onChange={e => setDiffraction(e.target.checked)} /> Diffraction (Airy core)</label>
          </div>
          <h3>ISP</h3>
          <div className="form-grid isp">
            <Slider label="Exposure" min={0.2} max={4} step={0.01} value={isp.exposure} onChange={v => setIsp(s => ({ ...s, exposure: v }))} />
            <Slider label="Gamma" min={1} max={3} step={0.01} value={isp.gamma} onChange={v => setIsp(s => ({ ...s, gamma: v }))} />
            <Slider label="Saturation" min={0} max={2} step={0.01} value={isp.saturation} onChange={v => setIsp(s => ({ ...s, saturation: v }))} />
            <Slider label="WB red" min={0.5} max={2} step={0.01} value={isp.wb[0]} onChange={v => setWb(0, v)} />
            <Slider label="WB green" min={0.5} max={2} step={0.01} value={isp.wb[1]} onChange={v => setWb(1, v)} />
            <Slider label="WB blue" min={0.5} max={2} step={0.01} value={isp.wb[2]} onChange={v => setWb(2, v)} />
          </div>
          <button className="tool labeled" onClick={() => setIsp(DEFAULT_ISP)}><i className="codicon codicon-discard" /> Reset ISP</button>
          <p className="muted hint">The input is the ideal image on the sensor. Each pixel is spread along traced rays (red, green and blue use the longest, primary and shortest wavelengths), including distortion, lateral colour, vignetting and cos⁴ fall-off.</p>
        </div>
      </div>
    </div>
  )
}

/** Largest box with the scene's aspect ratio that fits the stage (leaving room for the comparison slider). */
function frameSize(stage: { width: number; height: number }, scene: Scene, slider: boolean): CSSProperties {
  const width = Math.max(0, stage.width - 24), height = Math.max(0, stage.height - 24 - (slider ? 30 : 0))
  const scale = Math.min(width / scene.width, height / scene.height)
  return { width: Math.floor(scene.width * scale), height: Math.floor(scene.height * scale) }
}

function Slider({ label, min, max, step, value, onChange }: { label: string; min: number; max: number; step: number; value: number; onChange: (v: number) => void }) {
  return (
    <>
      <label>{label}</label>
      <input type="range" min={min} max={max} step={step} value={value} onChange={e => onChange(Number(e.target.value))} />
      <span className="unit mono">{value.toFixed(2)}</span>
    </>
  )
}
