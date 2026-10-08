import { useEffect, useState } from 'react'
import type { LensSystem, Overview, Point, Sample } from '../../../shared/lens'
import logo from '../assets/logo.png'
import { useWorkbench } from '../document'
import { fieldColor, formatFixed, formatShort } from '../format'

export const WELCOME_KEY = 'instaoptics:show-welcome'

export function showWelcomeOnStartup(): boolean {
  try { return localStorage.getItem(WELCOME_KEY) !== 'false' } catch { return true }
}

/** Small static cross-section used on sample cards. */
function MiniLayout({ system }: { system: LensSystem }) {
  const [overview, setOverview] = useState<Overview | null>(null)
  useEffect(() => {
    let cancelled = false
    window.instaOptics.engine.request('overview', { system: JSON.stringify(system), raysPerField: 5 })
      .then(response => { if (!cancelled) setOverview(JSON.parse(response.json) as Overview) }, () => {})
    return () => { cancelled = true }
  }, [system])
  if (!overview) return <div className="mini-layout" />
  const { layout } = overview
  let minZ = layout.startZ, maxZ = layout.imageZ, maxY = 1e-3
  const visit = ([z, y]: Point) => { minZ = Math.min(minZ, z); maxZ = Math.max(maxZ, z); maxY = Math.max(maxY, Math.abs(y)) }
  layout.surfaces.forEach(p => p.forEach(visit))
  layout.rays.forEach(r => r.points.forEach(visit))
  const width = 240, height = 96, margin = 8
  const scale = Math.min((width - 2 * margin) / (maxZ - minZ), (height - 2 * margin) / (2 * maxY))
  const ox = (width - (maxZ - minZ) * scale) / 2 - minZ * scale
  const d = (points: Point[], close = false) => points.map(([z, y], i) => `${i ? 'L' : 'M'}${(z * scale + ox).toFixed(1)} ${(height / 2 - y * scale).toFixed(1)}`).join('') + (close ? 'Z' : '')
  return (
    <svg className="mini-layout" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="xMidYMid meet">
      {layout.elements.map((outline, i) => <path key={i} d={d(outline, true)} className="element" />)}
      {layout.rays.map((ray, i) => <path key={`r${i}`} d={d(ray.points)} stroke={fieldColor(ray.field)} className="ray" />)}
      <line x1={layout.imageZ * scale + ox} x2={layout.imageZ * scale + ox} y1={margin / 2} y2={height - margin / 2} className="image-plane" />
    </svg>
  )
}

function SampleCard({ sample, onOpen }: { sample: Sample; onOpen: () => void }) {
  const s = sample.system
  // Each glass between two surfaces is one element; a cemented doublet has two.
  const lenses = s.surfaces.filter(surface => surface.material.trim() !== '' && surface.material.trim().toUpperCase() !== 'AIR').length
  const fov = 2 * Math.max(...s.fields.map(Math.abs))
  return (
    <button className="sample-card" onClick={onOpen} title={`Open ${s.name}`}>
      <MiniLayout system={s} />
      <span className="sample-name">{s.name}</span>
      <span className="sample-specs mono">
        {lenses} {lenses === 1 ? 'element' : 'elements'} · EPD {formatShort(s.entrancePupilDiameter)} mm · FOV {formatShort(fov)}°
      </span>
    </button>
  )
}

const FEATURES: Array<{ icon: string; title: string; text: string; window: string }> = [
  { icon: 'graph-line', title: 'MTF, PSF, wavefront', text: 'FFT diffraction, Strehl ratio, encircled energy.', window: 'mtf' },
  { icon: 'file-media', title: 'Image simulation', text: 'Render a test chart or an image through the system.', window: 'imageSim' },
  { icon: 'graph', title: 'Aberrations', text: 'Seidel, ray fans, field curvature, distortion, illumination.', window: 'seidel' },
  { icon: 'symbol-namespace', title: '3D layout', text: 'Real rays through every surface.', window: 'layout3d' },
  { icon: 'symbol-ruler', title: 'Tolerancing', text: 'Sensitivity and Monte Carlo.', window: 'tolerance' },
  { icon: 'table', title: 'System editor', text: 'Surfaces, glasses, aperture, fields, wavelengths.', window: 'lensData' },
  { icon: 'rocket', title: 'Optimization', text: 'Damped least squares on spot or wavefront error.', window: 'optimize' },
]

export function Welcome() {
  const { samples, actions, engine } = useWorkbench()
  const [onStartup, setOnStartup] = useState(showWelcomeOnStartup)
  const toggleStartup = (value: boolean) => {
    setOnStartup(value)
    try { localStorage.setItem(WELCOME_KEY, String(value)) } catch { /* storage unavailable */ }
  }
  const paraxial = engine.overview?.paraxial
  // Leave the start page for the editor so the result of the action is visible.
  const start = (command: 'file:new' | 'file:open' | 'file:importZmx') => { actions.openWindow('lensData'); actions.command(command) }

  return (
    <div className="panel welcome">
      <div className="welcome-scroll">
        <div className="welcome-inner">
          <header className="welcome-hero">
            <img src={logo} alt="" className="welcome-logo" />
            <div>
              <h1>instaOptics</h1>
              <p className="welcome-tagline">Optical simulation</p>
              <p className="welcome-meta mono">
                Engine {engine.version ?? '…'}
                {paraxial && <> · current system EFL {formatFixed(paraxial.efl, 2)} mm, F/{formatFixed(paraxial.fNumber, 2)}</>}
              </p>
            </div>
          </header>

          <div className="welcome-columns">
            <section>
              <h2>Start</h2>
              <ul className="welcome-links">
                <li><button onClick={() => start('file:new')}><i className="codicon codicon-new-file" /> New system<kbd>Ctrl+N</kbd></button></li>
                <li><button onClick={() => start('file:open')}><i className="codicon codicon-folder-opened" /> Open system…<kbd>Ctrl+O</kbd></button></li>
                <li><button onClick={() => start('file:importZmx')}><i className="codicon codicon-cloud-download" /> Import Zemax .zmx…</button></li>
                <li><button onClick={() => actions.openWindow('lensData')}><i className="codicon codicon-table" /> Edit the current system<kbd>Ctrl+L</kbd></button></li>
              </ul>

              <h2>Sample systems</h2>
              <div className="sample-grid">
                {samples.map(sample => <SampleCard key={sample.id} sample={sample} onOpen={() => { actions.openSample(sample); actions.openWindow('layout') }} />)}
              </div>
            </section>

            <section>
              <h2>Tools</h2>
              <div className="feature-list">
                {FEATURES.map(feature => (
                  <button key={feature.title} className="feature" onClick={() => actions.openWindow(feature.window)}>
                    <i className={`codicon codicon-${feature.icon}`} />
                    <span>
                      <span className="feature-title">{feature.title}</span>
                      <span className="feature-text">{feature.text}</span>
                    </span>
                  </button>
                ))}
              </div>
            </section>
          </div>

          <footer className="welcome-footer">
            <label className="check"><input type="checkbox" checked={onStartup} onChange={e => toggleStartup(e.target.checked)} /> Show this page on startup</label>
          </footer>
        </div>
      </div>
    </div>
  )
}
