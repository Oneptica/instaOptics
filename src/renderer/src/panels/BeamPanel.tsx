import { useEffect, useMemo, useRef, useState } from 'react'
import { Select, toCsv } from '../analysis/AnalysisFrame'
import { type AnalysisState, useAnalysis } from '../analysis/useAnalysis'
import { NumberField } from '../components/NumberField'
import { useWorkbench } from '../document'
import { fieldLabel, formatShort } from '../format'
import { Heatmap, colorAt } from '../plots/Heatmap'
import { Legend, LinePlot, niceTicks, useSize } from '../plots/LinePlot'
import { useDomainZoom } from '../plots/useZoomPan'
import { type PopResult, type Slice, type SourceKind, defaultRadius, popRequest, runPop, setPopUi, usePopStore } from './popStore'

interface SurfaceBeam { surface: number; z: number; w: number; radiusOfCurvature: number | null; waistDistance: number | null; waistRadius: number | null }
interface BeamTrace { surfaces: SurfaceBeam[]; profile: Array<[number, number]>; imageWaist: number | null; imageWaistDistance: number | null; divergence: number | null }

type View = 'side' | 'radius' | 'section' | 'table'

const um = (mm: number) => (mm * 1000).toPrecision(4)
const surfaceName = (index: number, count: number) => index >= count ? 'Image' : String(index + 1)

// ---------- side view (xz or yz) ----------

const MARGIN = { left: 52, right: 14, top: 14, bottom: 30 }

interface SideOptions { axis: 'x' | 'y'; normalize: 'plane' | 'global'; zoom: number; region: 'full' | 'image'; gamma: number }

function bracket(slices: Slice[], z: number): [Slice, Slice, number] {
  let lo = 0, hi = slices.length - 1
  while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (slices[mid].z <= z) lo = mid; else hi = mid }
  const a = slices[lo], b = slices[hi]
  return [a, b, b.z > a.z ? Math.min(1, Math.max(0, (z - a.z) / (b.z - a.z))) : 0]
}

function cutAt(slice: Slice, axis: 'x' | 'y', x: number): number {
  const cut = axis === 'x' ? slice.xCut : slice.yCut
  const u = x / slice.pitch + cut.length / 2
  const i = Math.floor(u)
  if (i < 0 || i + 1 >= cut.length) return 0
  const f = u - i
  return cut[i] * (1 - f) + cut[i + 1] * f
}

function SideView({ result, options }: { result: PopResult; options: SideOptions }) {
  const [hostRef, { width, height }] = useSize<HTMLDivElement>()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const nSurfaces = result.surfaceZ.length

  // Full range of the view; the wheel and drag then narrow it (z along the axis, x across it).
  const fullZ0 = options.region === 'image' ? result.surfaceZ[nSurfaces - 1] : result.zStart
  const fullZ1 = result.imageZ
  const fullHalf = (() => {
    const w = (s: Slice) => options.axis === 'x' ? s.wX : s.wY
    const visible = result.slices.filter(s => s.z >= fullZ0 - 1e-9 && s.z <= fullZ1 + 1e-9)
    return 1.15 * Math.max(...visible.map(w), 1e-9) / options.zoom
  })()
  const zoomBox = useDomainZoom<HTMLDivElement>({ h: [fullZ0, fullZ1], v: [-fullHalf, fullHalf] }, { left: MARGIN.left, top: MARGIN.top, width: width - MARGIN.left - MARGIN.right, height: height - MARGIN.top - MARGIN.bottom }, hostRef)
  const [viewZ0, viewZ1] = zoomBox.shown.h, viewHalf = (zoomBox.shown.v[1] - zoomBox.shown.v[0]) / 2
  const viewCentre = (zoomBox.shown.v[1] + zoomBox.shown.v[0]) / 2

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || width < 80 || height < 80) return
    const dpr = window.devicePixelRatio || 1
    canvas.width = Math.round(width * dpr)
    canvas.height = Math.round(height * dpr)
    const ctx = canvas.getContext('2d')!
    ctx.scale(dpr, dpr)
    const style = getComputedStyle(document.documentElement)
    const color = (name: string) => style.getPropertyValue(name).trim()

    const slices = result.slices
    const z0 = viewZ0, z1 = viewZ1
    const visible = slices.filter(s => s.z >= z0 - (z1 - z0) && s.z <= z1 + (z1 - z0))
    const wOf = (s: Slice) => options.axis === 'x' ? s.wX : s.wY
    const half = viewHalf
    const globalPeak = Math.max(...slices.map(s => s.peak))

    const plotW = width - MARGIN.left - MARGIN.right, plotH = height - MARGIN.top - MARGIN.bottom
    const iw = Math.round(plotW * dpr), ih = Math.round(plotH * dpr)
    const image = ctx.createImageData(iw, ih)
    const lut = Array.from({ length: 256 }, (_, i) => colorAt('magma', i / 255))
    for (let c = 0; c < iw; c++) {
      const z = z0 + (c + 0.5) / iw * (z1 - z0)
      const [a, b, t] = bracket(slices, z)
      const fa = options.normalize === 'global' ? a.peak / globalPeak : 1
      const fb = options.normalize === 'global' ? b.peak / globalPeak : 1
      for (let r = 0; r < ih; r++) {
        const x = viewCentre + (0.5 - (r + 0.5) / ih) * 2 * half
        const v = cutAt(a, options.axis, x) * fa * (1 - t) + cutAt(b, options.axis, x) * fb * t
        const [R, G, B] = lut[Math.min(255, Math.round(255 * Math.pow(Math.max(0, Math.min(1, v)), options.gamma)))]
        const at = (r * iw + c) * 4
        image.data[at] = R; image.data[at + 1] = G; image.data[at + 2] = B; image.data[at + 3] = 255
      }
    }
    ctx.clearRect(0, 0, width, height)
    ctx.putImageData(image, Math.round(MARGIN.left * dpr), Math.round(MARGIN.top * dpr))

    const px = (z: number) => MARGIN.left + (z - z0) / (z1 - z0) * plotW
    const py = (x: number) => MARGIN.top + plotH / 2 - (x - viewCentre) / half * plotH / 2
    // Surfaces and the image plane.
    ctx.save()
    ctx.beginPath(); ctx.rect(MARGIN.left, MARGIN.top, plotW, plotH); ctx.clip()
    ctx.font = '10px system-ui, sans-serif'
    ctx.setLineDash([2, 3])
    ctx.strokeStyle = 'rgba(255,255,255,0.45)'; ctx.fillStyle = 'rgba(255,255,255,0.8)'
    result.surfaceZ.forEach((z, i) => {
      if (z < z0 || z > z1) return
      ctx.beginPath(); ctx.moveTo(px(z), MARGIN.top); ctx.lineTo(px(z), MARGIN.top + plotH); ctx.stroke()
      ctx.fillText(String(i + 1), px(z) + 3, MARGIN.top + 11)
    })
    ctx.strokeStyle = 'rgba(255,200,120,0.8)'
    ctx.beginPath(); ctx.moveTo(px(z1) - 1, MARGIN.top); ctx.lineTo(px(z1) - 1, MARGIN.top + plotH); ctx.stroke()
    ctx.setLineDash([])
    // Beam radius envelope.
    ctx.strokeStyle = 'rgba(255,255,255,0.7)'; ctx.lineWidth = 1
    for (const sign of [1, -1]) {
      ctx.beginPath()
      visible.forEach((s, i) => { const x = px(s.z), y = py(sign * wOf(s)); if (i) ctx.lineTo(x, y); else ctx.moveTo(x, y) })
      ctx.stroke()
    }
    ctx.restore()

    // Axes.
    ctx.strokeStyle = color('--plot-axis'); ctx.fillStyle = color('--plot-text'); ctx.lineWidth = 1
    ctx.strokeRect(MARGIN.left + 0.5, MARGIN.top + 0.5, plotW, plotH)
    ctx.font = '10.5px system-ui, sans-serif'
    ctx.textAlign = 'center'
    for (const t of niceTicks(z0, z1, 7)) {
      if (t < z0 || t > z1) continue
      ctx.beginPath(); ctx.moveTo(px(t) + 0.5, MARGIN.top + plotH); ctx.lineTo(px(t) + 0.5, MARGIN.top + plotH + 4); ctx.stroke()
      ctx.fillText(formatShort(t), px(t), MARGIN.top + plotH + 16)
    }
    ctx.fillText('Distance along the axis (mm)', MARGIN.left + plotW / 2, height - 3)
    ctx.textAlign = 'right'
    for (const t of niceTicks(viewCentre - half, viewCentre + half, 6)) {
      if (Math.abs(t - viewCentre) > half) continue
      ctx.beginPath(); ctx.moveTo(MARGIN.left - 4, py(t) + 0.5); ctx.lineTo(MARGIN.left, py(t) + 0.5); ctx.stroke()
      ctx.fillText(formatShort(Number(t.toPrecision(3))), MARGIN.left - 6, py(t) + 3.5)
    }
    ctx.save(); ctx.translate(11, MARGIN.top + plotH / 2); ctx.rotate(-Math.PI / 2); ctx.textAlign = 'center'
    ctx.fillText(`${options.axis} (mm)`, 0, 0); ctx.restore()
  }, [result, options, width, height, nSurfaces, viewZ0, viewZ1, viewHalf, viewCentre])

  return (
    <div className="side-view" ref={hostRef} {...zoomBox.handlers} style={{ touchAction: 'none' }}>
      <canvas ref={canvasRef} style={{ width, height }} />
      {zoomBox.zoomed && <button className="plot-reset" title="Reset zoom (double-click the plot)" onClick={zoomBox.reset} onPointerDown={e => e.stopPropagation()}><i className="codicon codicon-screen-full" /></button>}
    </div>
  )
}

// ---------- window ----------

export function BeamPanel() {
  const { doc } = useWorkbench()
  const system = doc.system
  const store = usePopStore()
  const { ui, result, busy, error } = store
  const [view, setView] = useState<View>('side')
  const [side, setSide] = useState<SideOptions>({ axis: 'x', normalize: 'plane', zoom: 1, region: 'full', gamma: 0.5 })
  const [section, setSection] = useState(0)
  const [logRadius, setLogRadius] = useState(true)
  const bodyRef = useRef<HTMLDivElement>(null)

  const wavelength = Math.min(ui.wavelength, system.wavelengths.length - 1)
  const radius = ui.radius ?? defaultRadius(ui, system)
  const analytic: AnalysisState<BeamTrace> = useAnalysis<BeamTrace>({ kind: 'gaussianBeam', wavelength, radius, waist: ui.waist })
  const current = useMemo(() => popRequest(ui, system).key, [ui, system])
  const outdated = result !== null && store.ranFor !== current
  const nSurfaces = system.surfaces.length
  const gaussian = ui.kind === 'gaussian'

  const exportCsv = () => {
    if (!result) return
    const rows = result.slices.map(s => [s.z, s.wX, s.wY, s.power, s.peak])
    void window.instaOptics.file.export(toCsv(['z_mm', 'w_x_mm', 'w_y_mm', 'power', 'peak_irradiance_per_mm2'], rows), 'beam-propagation.csv', 'CSV', ['csv'])
  }
  const exportPng = () => {
    const rect = bodyRef.current?.getBoundingClientRect()
    if (rect) void window.instaOptics.file.exportPng({ x: rect.x, y: rect.y, width: rect.width, height: rect.height }, 'beam-propagation')
  }

  const plane = result?.planes.find(p => p.surface === Math.min(section, nSurfaces)) ?? result?.planes[result.planes.length - 1]
  const surfaceLines = result ? result.surfaceZ.map((x, i) => ({ x, label: String(i + 1) })) : []

  return (
    <div className="panel beam">
      <div className="panel-toolbar">
        <button className="tool labeled primary" disabled={busy} onClick={() => void runPop(system)} title="Propagate the beam through the system">
          <i className={`codicon ${busy ? 'codicon-loading codicon-modifier-spin' : 'codicon-play'}`} /> {busy ? 'Running…' : 'Run'}
        </button>
        <Select label="View" value={view} options={[{ value: 'side', label: 'Side view' }, { value: 'radius', label: 'Beam radius' }, { value: 'section', label: 'Cross section' }, { value: 'table', label: 'Table' }]} onChange={setView} />
        {view === 'side' && <>
          <Select label="Axis" value={side.axis} options={[{ value: 'x', label: 'x–z' }, { value: 'y', label: 'y–z' }]} onChange={axis => setSide(o => ({ ...o, axis }))} />
          <Select label="Scale" value={side.normalize} options={[{ value: 'plane', label: 'Each plane' }, { value: 'global', label: 'Absolute' }]} onChange={normalize => setSide(o => ({ ...o, normalize }))} />
          <Select label="Zoom" value={side.zoom} options={[1, 2, 5, 10, 20, 50].map(v => ({ value: v, label: `×${v}` }))} onChange={zoom => setSide(o => ({ ...o, zoom }))} />
          <Select label="Range" value={side.region} options={[{ value: 'full', label: 'Whole system' }, { value: 'image', label: 'After last surface' }]} onChange={region => setSide(o => ({ ...o, region }))} />
        </>}
        {view === 'radius' && <label className="tool-field"><input type="checkbox" checked={logRadius} onChange={e => setLogRadius(e.target.checked)} /> Log scale</label>}
        {view === 'section' && result && (
          <Select label="Plane" value={plane?.surface ?? 0} options={result.planes.map(p => ({ value: p.surface, label: p.surface >= nSurfaces ? 'Image' : `After surface ${p.surface + 1}` }))} onChange={setSection} />
        )}
        <span className="toolbar-fill" />
        {result && view === 'radius' && <Legend items={[{ label: 'Simulated', color: 'var(--field-1)' }, ...(result.analytic ? [{ label: 'Gaussian (q)', color: 'var(--text-muted)', dash: '5 3' }] : [])]} />}
        {result && <button className="tool" title="Export data as CSV" onClick={exportCsv}><i className="codicon codicon-table" /></button>}
        {result && <button className="tool" title="Save as PNG image" onClick={exportPng}><i className="codicon codicon-device-camera" /></button>}
        <span className="analysis-status">{store.ms !== null && !busy ? `${(store.ms / 1000).toFixed(1)} s` : ''}</span>
      </div>
      <div className="beam-body">
        <div className="beam-form tool-form">
          <h3>Source</h3>
          <div className="form-grid">
            <label>Beam</label>
            <select value={ui.kind} onChange={e => setPopUi({ kind: e.target.value as SourceKind, radius: null })}>
              <option value="gaussian">Gaussian</option>
              <option value="superGaussian">Super-Gaussian</option>
              <option value="topHat">Flat top (circular)</option>
            </select>
            <span />
            <label title="The beam travels along the chief ray of this field">Field</label>
            <select value={ui.field !== null && ui.field < system.fields.length ? ui.field : -1} onChange={e => setPopUi({ field: Number(e.target.value) < 0 ? null : Number(e.target.value) })}>
              <option value={-1}>On axis</option>
              {system.fields.map((f, i) => <option key={i} value={i}>{i + 1}: {fieldLabel(f, system.fieldType)}</option>)}
            </select>
            <span />
            <label>Wavelength</label>
            <select value={wavelength} onChange={e => setPopUi({ wavelength: Number(e.target.value) })}>
              {system.wavelengths.map((w, i) => <option key={i} value={i}>{w.toFixed(4)} µm</option>)}
            </select>
            <span />
            <label>{gaussian ? 'Waist radius (1/e²)' : 'Radius'}</label>
            <NumberField optional min={0} ariaLabel="Beam radius" placeholder={formatShort(Number(defaultRadius(ui, system).toPrecision(4)))} value={ui.radius} onCommit={v => setPopUi({ radius: v })} />
            <span className="unit">mm</span>
            {gaussian && <>
              <label title="Distance from the first surface to the waist; negative when the waist is before the surface">Waist position</label>
              <NumberField allowInfinity={false} ariaLabel="Waist position" value={ui.waist} onCommit={v => v !== null && setPopUi({ waist: v })} />
              <span className="unit">mm</span>
            </>}
            {ui.kind === 'superGaussian' && <>
              <label>Order</label>
              <NumberField min={0} ariaLabel="Super-Gaussian order" value={ui.order} onCommit={v => v !== null && setPopUi({ order: v })} />
              <span />
            </>}
            {!gaussian && <>
              <label title="Positive for a diverging beam, negative for a converging one; empty is collimated">Curvature radius</label>
              <NumberField optional ariaLabel="Curvature radius" placeholder="collimated" value={ui.curvature} onCommit={v => setPopUi({ curvature: v })} />
              <span className="unit">mm</span>
            </>}
          </div>
          <div className="prop-row"><button className="tool labeled" onClick={() => setPopUi({ radius: null })}><i className="codicon codicon-discard" /> Fit to entrance pupil</button></div>
          <h3>Simulation</h3>
          <div className="form-grid">
            <label>Grid samples</label>
            <select value={ui.samples} onChange={e => setPopUi({ samples: Number(e.target.value) })}>
              <option value={128}>128 × 128 (fast)</option>
              <option value={256}>256 × 256</option>
              <option value={512}>512 × 512 (slow)</option>
            </select>
            <span />
            <label title="Mode field radius (1/e² intensity) of a single-mode fibre centred on the beam. Empty turns the coupling off.">Fibre mode radius</label>
            <NumberField optional min={0} ariaLabel="Fibre mode field radius" placeholder="off" value={ui.fiber} onCommit={v => setPopUi({ fiber: v })} />
            <span className="unit">mm</span>
            <label className="check span2"><input type="checkbox" checked={ui.aberrations} onChange={e => setPopUi({ aberrations: e.target.checked })} /> Include wavefront aberration</label>
            <label className="check span2"><input type="checkbox" checked={ui.apertures} onChange={e => setPopUi({ apertures: e.target.checked })} /> Clip at fixed surface apertures</label>
          </div>
          {result?.coupling && (
            <>
              <h3>Fibre coupling</h3>
              <table className="kv"><tbody>
                <tr><th>At the image plane</th><td className="mono">{(100 * result.coupling.atImage).toFixed(2)}</td><td className="unit">%</td></tr>
                <tr><th>Best in image space</th><td className="mono">{(100 * result.coupling.best).toFixed(2)}</td><td className="unit">%</td></tr>
                <tr><th title="Path position of the best coupling">Best at z</th><td className="mono">{result.coupling.bestZ.toFixed(3)}</td><td className="unit">mm</td></tr>
              </tbody></table>
            </>
          )}
          {gaussian && analytic.result && (
            <>
              <h3>Gaussian beam (analytic)</h3>
              <table className="kv"><tbody>
                <tr><th>Waist radius in image space</th><td className="mono">{analytic.result.imageWaist === null ? '—' : um(analytic.result.imageWaist)}</td><td className="unit">µm</td></tr>
                <tr><th title="Positive when the waist lies beyond the image plane">Waist beyond the image plane</th><td className="mono">{analytic.result.imageWaistDistance === null ? '—' : formatShort(Number(analytic.result.imageWaistDistance.toPrecision(5)))}</td><td className="unit">mm</td></tr>
                <tr><th>Divergence half-angle</th><td className="mono">{analytic.result.divergence === null ? '—' : (analytic.result.divergence * 1000).toPrecision(4)}</td><td className="unit">mrad</td></tr>
              </tbody></table>
            </>
          )}
          {analytic.error && gaussian && <p className="muted hint">{analytic.error}</p>}
          <p className="muted hint">Scalar wave propagation with real-ray wavefront aberration at the last surface. Accuracy falls off for beams faster than about NA 0.1. Coordinate breaks, decenter and tilt are not supported yet.</p>
        </div>
        <div className="beam-view" ref={bodyRef}>
          {error && <div className="plot-banner static"><i className="codicon codicon-error" /> {error}</div>}
          {result && outdated && !busy && <div className="beam-stale"><i className="codicon codicon-info" /> The system or settings changed since this result — press Run to update.</div>}
          {result?.warnings.map((w, i) => <div className="beam-warning" key={i}><i className="codicon codicon-warning" /> {w}</div>)}
          {!result && !busy && !error && <div className="placeholder">Set up a source and press Run.</div>}
          {!result && busy && <div className="placeholder">Propagating…</div>}
          {result && (
            <div className={`beam-result${outdated ? ' stale' : ''}`}>
              {view === 'side' && <SideView result={result} options={side} />}
              {view === 'radius' && result.slices.some(s => s.coupling !== null) && (
                <div className="split-cell coupling-plot">
                  <LinePlot
                    xLabel="Distance along the axis (mm)"
                    yLabel="Fibre coupling"
                    yDomain={[0, 1]}
                    xDomain={[result.surfaceZ[result.surfaceZ.length - 1], result.imageZ]}
                    series={[{ points: result.slices.filter(s => s.z >= result.surfaceZ[result.surfaceZ.length - 1]).map(s => [s.z, s.coupling ?? 0] as [number, number]), color: 'var(--field-2)', width: 1.6 }]}
                  />
                </div>
              )}
              {view === 'radius' && (
                <LinePlot
                  xLabel="Distance along the axis (mm)"
                  yLabel="Beam radius (mm)"
                  yLog={logRadius}
                  vlines={[...surfaceLines, { x: result.imageZ, label: 'image' }]}
                  series={[
                    ...(result.analytic ? [{ points: result.analytic.map(([z, w]) => [z, w] as [number, number]), color: 'var(--text-muted)', dash: '5 3', width: 1.4 }] : []),
                    { points: result.slices.map(s => [s.z, 0.5 * (s.wX + s.wY)] as [number, number]), color: 'var(--field-1)', width: 1.6 },
                  ]}
                />
              )}
              {view === 'section' && plane && (
                <div className="split-row">
                  <div className="split-cell">
                    <Heatmap values={plane.intensity} size={plane.size} min={0} max={1} colormap="magma" unit="I / Iₘₐₓ"
                      caption={`±${um(plane.halfWidth)} µm · w ${um(plane.wX)} × ${um(plane.wY)} µm`} />
                  </div>
                  <div className="split-cell">
                    <Heatmap values={plane.phase} size={plane.size} min={-Math.PI} max={Math.PI} colormap="phase" unit="rad" caption="Phase relative to the best-fit sphere" />
                  </div>
                </div>
              )}
              {view === 'table' && (
                <div className="beam-table">
                  <table className="data-table mono">
                    <thead><tr><th>Plane</th><th>z (mm)</th><th>w<sub>x</sub> (µm)</th><th>w<sub>y</sub> (µm)</th><th>Power</th><th>Peak (1/mm²)</th>{gaussian && <><th>Gaussian w (µm)</th><th>R (mm)</th></>}</tr></thead>
                    <tbody>
                      {result.planes.map(p => {
                        const beam = gaussian ? analytic.result?.surfaces.find(s => s.surface === p.surface) : undefined
                        const imageW = gaussian && p.surface >= nSurfaces ? result.analytic?.[result.analytic.length - 1]?.[1] : undefined
                        return (
                          <tr key={p.surface}>
                            <td>{surfaceName(p.surface, nSurfaces)}</td>
                            <td>{p.z.toFixed(3)}</td><td>{um(p.wX)}</td><td>{um(p.wY)}</td><td>{p.power.toFixed(4)}</td><td>{p.peak.toExponential(3)}</td>
                            {gaussian && <><td>{beam ? um(beam.w) : imageW !== undefined ? um(imageW) : '—'}</td><td>{beam ? (beam.radiusOfCurvature === null ? '∞' : beam.radiusOfCurvature.toFixed(2)) : '—'}</td></>}
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                  {result.aberration && <div className="analysis-footer mono">Wavefront aberration at the last surface: RMS {result.aberration.rmsWaves.toFixed(4)} waves · P-V {result.aberration.pvWaves.toFixed(4)} waves</div>}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
