import { useEffect, useMemo, useRef, useState, type PointerEvent, type WheelEvent } from 'react'
import type { Layout, Point } from '../../../shared/lens'
import { useWorkbench } from '../document'
import { fieldColor, formatShort } from '../format'

interface View { scale: number; x: number; y: number } // screen = (z·scale + x, −y·scale + y)

const MARGIN = 28

function bounds(layout: Layout) {
  let minZ = layout.startZ, maxZ = layout.imageZ, maxY = layout.stopSemiDiameter
  const visit = ([z, y]: Point) => { minZ = Math.min(minZ, z); maxZ = Math.max(maxZ, z); maxY = Math.max(maxY, Math.abs(y)) }
  layout.surfaces.forEach(profile => profile.forEach(visit))
  layout.rays.forEach(ray => ray.points.forEach(visit))
  return { minZ, maxZ, maxY: Math.max(maxY, 1e-3) }
}

function fit(layout: Layout, width: number, height: number): View {
  const { minZ, maxZ, maxY } = bounds(layout)
  const scale = Math.max(1e-6, Math.min((width - 2 * MARGIN) / Math.max(maxZ - minZ, 1e-6), (height - 2 * MARGIN) / (2 * maxY)))
  return { scale, x: (width - (maxZ - minZ) * scale) / 2 - minZ * scale, y: height / 2 }
}

/** A round scale-bar length (1, 2 or 5 × 10ⁿ mm) close to 100 px. */
function scaleBar(scale: number) {
  const target = 100 / scale
  const power = 10 ** Math.floor(Math.log10(target))
  const length = [1, 2, 5, 10].map(m => m * power).find(m => m >= target * 0.7) ?? power * 10
  return { length, pixels: length * scale }
}

const path = (points: Point[], view: View, close = false) =>
  points.map(([z, y], i) => `${i ? 'L' : 'M'}${(z * view.scale + view.x).toFixed(2)} ${(-y * view.scale + view.y).toFixed(2)}`).join(' ') + (close ? ' Z' : '')

export function LayoutView() {
  const { engine, doc, raysPerField, setRaysPerField } = useWorkbench()
  const layout = engine.overview?.layout ?? null
  const hostRef = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [manual, setManual] = useState<View | null>(null) // set once the user pans or zooms
  const drag = useRef<{ x: number; y: number; view: View } | null>(null)

  useEffect(() => {
    const host = hostRef.current
    if (!host) return
    const observer = new ResizeObserver(([entry]) => setSize({ width: entry.contentRect.width, height: entry.contentRect.height }))
    observer.observe(host)
    return () => observer.disconnect()
  }, [])

  const fitted = useMemo(() => layout && size.width > 0 ? fit(layout, size.width, size.height) : null, [layout, size.width, size.height])
  const view = manual ?? fitted

  function onWheel(event: WheelEvent<SVGSVGElement>) {
    if (!view) return
    const rect = event.currentTarget.getBoundingClientRect()
    const px = event.clientX - rect.left, py = event.clientY - rect.top
    const factor = Math.exp(-event.deltaY * 0.0015)
    setManual({ scale: view.scale * factor, x: px - (px - view.x) * factor, y: py - (py - view.y) * factor })
  }

  function onPointerDown(event: PointerEvent<SVGSVGElement>) {
    if (!view || event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { x: event.clientX, y: event.clientY, view }
  }

  function onPointerMove(event: PointerEvent<SVGSVGElement>) {
    const start = drag.current
    if (!start) return
    setManual({ ...start.view, x: start.view.x + event.clientX - start.x, y: start.view.y + event.clientY - start.y })
  }

  const bar = view ? scaleBar(view.scale) : null
  const system = doc.system

  return (
    <div className="panel layout-view">
      <div className="panel-toolbar">
        <button className="tool" title="Fit to window (double-click the plot)" onClick={() => setManual(null)}><i className="codicon codicon-screen-full" /></button>
        <span className="tool-separator" />
        <label className="tool-field">
          Rays / field
          <select value={raysPerField} onChange={event => setRaysPerField(Number(event.target.value))}>
            {[3, 5, 7, 9, 11, 15].map(count => <option key={count} value={count}>{count}</option>)}
          </select>
        </label>
        <span className="toolbar-fill" />
        <span className="legend">
          {system.fields.map((field, i) => (
            <span key={i} className="legend-item"><span className="swatch" style={{ background: fieldColor(i) }} />{formatShort(field)}°</span>
          ))}
        </span>
      </div>
      <div className="plot-host" ref={hostRef}>
        {layout && view ? (
          <svg
            className="plot"
            width={size.width}
            height={size.height}
            onWheel={onWheel}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={() => { drag.current = null }}
            onDoubleClick={() => setManual(null)}
          >
            <line className="axis" x1={0} x2={size.width} y1={view.y} y2={view.y} />
            {layout.elements.map((outline, i) => <path key={`e${i}`} className="element" d={path(outline, view, true)} />)}
            {layout.surfaces.map((profile, i) => <path key={`s${i}`} className="surface" d={path(profile, view)} />)}
            {layout.rays.map((ray, i) => (
              <path key={`r${i}`} className={`ray${ray.failure ? ' failed' : ''}`} stroke={fieldColor(ray.field)} d={path(ray.points, view)} />
            ))}
            {[1, -1].map(sign => {
              const x = layout.stopZ * view.scale + view.x
              const y0 = view.y - sign * layout.stopSemiDiameter * view.scale
              return <line key={sign} className="stop" x1={x} x2={x} y1={y0} y2={y0 - sign * 10} />
            })}
            <line className="image-plane" x1={layout.imageZ * view.scale + view.x} x2={layout.imageZ * view.scale + view.x} y1={MARGIN / 2} y2={size.height - MARGIN / 2} />
            {bar && (
              <g className="scale-bar" transform={`translate(14 ${size.height - 16})`}>
                <line x1={0} x2={bar.pixels} y1={0} y2={0} />
                <line x1={0} x2={0} y1={-4} y2={4} />
                <line x1={bar.pixels} x2={bar.pixels} y1={-4} y2={4} />
                <text x={bar.pixels / 2} y={-7}>{formatShort(bar.length)} mm</text>
              </g>
            )}
          </svg>
        ) : (
          <div className="placeholder">{engine.error ?? 'Waiting for the engine…'}</div>
        )}
        {layout && engine.error && <div className="plot-banner"><i className="codicon codicon-warning" /> {engine.error} — showing the last valid layout</div>}
      </div>
    </div>
  )
}
