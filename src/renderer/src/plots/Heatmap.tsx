import { useEffect, useMemo, useRef, useState, type PointerEvent } from 'react'
import { useSize } from './LinePlot'

export type Colormap = 'jet' | 'inferno' | 'magma' | 'phase' | 'gray'

const STOPS: Record<Colormap, Array<[number, number, number]>> = {
  jet: [[0, 0, 143], [0, 0, 255], [0, 255, 255], [255, 255, 0], [255, 0, 0], [128, 0, 0]],
  inferno: [[0, 0, 4], [40, 11, 84], [101, 21, 110], [159, 42, 99], [212, 72, 66], [245, 125, 21], [250, 193, 39], [252, 255, 164]],
  magma: [[0, 0, 4], [28, 16, 68], [79, 18, 123], [129, 37, 129], [181, 54, 122], [229, 80, 100], [251, 135, 97], [254, 194, 135], [252, 253, 191]],
  // Cyclic, for phase in [−π, π].
  phase: [[255, 64, 64], [255, 220, 64], [64, 220, 96], [64, 200, 255], [96, 96, 255], [220, 64, 255], [255, 64, 64]],
  gray: [[0, 0, 0], [255, 255, 255]],
}

export function colorAt(map: Colormap, t: number): [number, number, number] {
  const stops = STOPS[map]
  const x = Math.max(0, Math.min(1, t)) * (stops.length - 1)
  const i = Math.min(stops.length - 2, Math.floor(x)), f = x - i
  return [0, 1, 2].map(c => Math.round(stops[i][c] * (1 - f) + stops[i + 1][c] * f)) as [number, number, number]
}

interface HeatmapProps {
  /** Row-major values, row 0 at the top; null/NaN cells are transparent. */
  values: Array<number | null>
  size: number
  min: number
  max: number
  colormap: Colormap
  /** Physical half-width shown on the scale label, e.g. "±0.02 mm". */
  caption?: string
  unit: string
  smooth?: boolean
  transform?: (value: number) => number
}

const LUT_SIZE = 256
const luts = new Map<Colormap, Uint32Array>()
function lut(map: Colormap): Uint32Array {
  let table = luts.get(map)
  if (!table) {
    table = new Uint32Array(LUT_SIZE)
    for (let k = 0; k < LUT_SIZE; k++) {
      const [r, g, b] = colorAt(map, k / (LUT_SIZE - 1))
      table[k] = (255 << 24) | (b << 16) | (g << 8) | r // little-endian RGBA
    }
    luts.set(map, table)
  }
  return table
}

interface Viewport { scale: number; cx: number; cy: number }
const HOME: Viewport = { scale: 1, cx: 0.5, cy: 0.5 }

/**
 * Square false-colour map with a colour bar. The grid is resampled bilinearly at the screen resolution, so it stays
 * smooth when enlarged; the wheel zooms, dragging pans, double-click resets.
 */
export function Heatmap({ values, size, min, max, colormap, caption, unit, smooth = true, transform }: HeatmapProps) {
  const [hostRef, { width, height }] = useSize<HTMLDivElement>()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [viewport, setViewport] = useState<Viewport>(HOME)
  const [hover, setHover] = useState<{ column: number; row: number } | null>(null)
  const drag = useRef<{ x: number; y: number; viewport: Viewport } | null>(null)
  const side = Math.max(0, Math.min(width - 90, height - 28))
  const dpr = window.devicePixelRatio || 1
  const pixels = Math.min(1200, Math.round(side * dpr))

  // Values after the optional transform, NaN where the cell is empty.
  const field = useMemo(() => Float32Array.from(values, v => v === null || !Number.isFinite(v) ? NaN : transform ? transform(v) : v), [values, transform])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || pixels < 2) return
    canvas.width = pixels
    canvas.height = pixels
    const context = canvas.getContext('2d')!
    const image = context.createImageData(pixels, pixels)
    const out = new Uint32Array(image.data.buffer)
    const table = lut(colormap)
    const span = max - min || 1
    const { scale, cx, cy } = viewport
    const valid = (c: number, r: number) => c >= 0 && r >= 0 && c < size && r < size && !Number.isNaN(field[r * size + c])
    for (let j = 0; j < pixels; j++) {
      const gy = (cy + ((j + 0.5) / pixels - 0.5) / scale) * size - 0.5
      const r0 = Math.floor(gy), fy = gy - r0
      for (let i = 0; i < pixels; i++) {
        const gx = (cx + ((i + 0.5) / pixels - 0.5) / scale) * size - 0.5
        const c0 = Math.floor(gx), fx = gx - c0
        let weight = 0, sum = 0
        if (smooth) {
          for (let dr = 0; dr < 2; dr++) for (let dc = 0; dc < 2; dc++) {
            const c = c0 + dc, r = r0 + dr
            if (!valid(c, r)) continue
            const w = (dc ? fx : 1 - fx) * (dr ? fy : 1 - fy)
            weight += w
            sum += w * field[r * size + c]
          }
          // Cells next to empty ones fade out over the weight range 0.35 to 0.65, which rounds the staircase edge.
          if (weight < 0.35) continue
          const t = (sum / weight - min) / span
          const color = table[Math.max(0, Math.min(LUT_SIZE - 1, Math.round(t * (LUT_SIZE - 1))))]
          const alpha = weight >= 0.65 ? 255 : Math.round((weight - 0.35) / 0.3 * 255)
          out[j * pixels + i] = alpha === 255 ? color : (color & 0x00ffffff) | (alpha << 24)
        } else {
          const c = Math.round(gx), r = Math.round(gy)
          if (!valid(c, r)) continue
          const t = (field[r * size + c] - min) / span
          out[j * pixels + i] = table[Math.max(0, Math.min(LUT_SIZE - 1, Math.round(t * (LUT_SIZE - 1))))]
        }
      }
    }
    context.putImageData(image, 0, 0)
  }, [field, size, min, max, colormap, smooth, pixels, viewport])

  // Wheel zoom about the cursor; needs a non-passive listener.
  const viewportRef = useRef(viewport)
  viewportRef.current = viewport
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      const rect = canvas.getBoundingClientRect()
      const fx = (event.clientX - rect.left) / rect.width, fy = (event.clientY - rect.top) / rect.height
      const { scale, cx, cy } = viewportRef.current
      const next = Math.max(1, Math.min(64, scale * Math.exp(-Math.max(-200, Math.min(200, event.deltaY)) * 0.0015)))
      // Keep the image point under the cursor fixed.
      const px = cx + (fx - 0.5) / scale, py = cy + (fy - 0.5) / scale
      const clamp = (c: number) => Math.max(0.5 / next, Math.min(1 - 0.5 / next, c))
      setViewport(next === 1 ? HOME : { scale: next, cx: clamp(px - (fx - 0.5) / next), cy: clamp(py - (fy - 0.5) / next) })
    }
    canvas.addEventListener('wheel', onWheel, { passive: false })
    return () => canvas.removeEventListener('wheel', onWheel)
  }, [side > 0])

  function onPointerDown(event: PointerEvent<HTMLCanvasElement>) {
    if (event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { x: event.clientX, y: event.clientY, viewport }
  }
  function onPointerMove(event: PointerEvent<HTMLCanvasElement>) {
    const rect = event.currentTarget.getBoundingClientRect()
    const start = drag.current
    if (start && start.viewport.scale > 1) {
      const { scale } = start.viewport
      const clamp = (c: number) => Math.max(0.5 / scale, Math.min(1 - 0.5 / scale, c))
      setViewport({ scale, cx: clamp(start.viewport.cx - (event.clientX - start.x) / rect.width / scale), cy: clamp(start.viewport.cy - (event.clientY - start.y) / rect.height / scale) })
    }
    const { scale, cx, cy } = viewport
    const column = Math.floor((cx + ((event.clientX - rect.left) / rect.width - 0.5) / scale) * size)
    const row = Math.floor((cy + ((event.clientY - rect.top) / rect.height - 0.5) / scale) * size)
    setHover(column >= 0 && row >= 0 && column < size && row < size ? { column, row } : null)
  }

  const ticks = [max, (max + min) / 2, min]
  const hovered = hover ? values[hover.row * size + hover.column] : null
  const format = (t: number) => Math.abs(t) >= 100 || (Math.abs(t) < 0.01 && t !== 0) ? t.toExponential(2) : t.toFixed(4)
  return (
    <div className="heatmap" ref={hostRef}>
      <div className="heatmap-body" style={{ width: side + 80 }}>
        <div className="heatmap-image" style={{ width: side, height: side }}>
          <canvas
            ref={canvasRef}
            style={{ width: side, height: side, cursor: viewport.scale > 1 ? 'grab' : 'crosshair', touchAction: 'none' }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={() => { drag.current = null }}
            onPointerLeave={() => setHover(null)}
            onDoubleClick={() => setViewport(HOME)}
          />
          {caption && <span className="heatmap-caption">{caption}</span>}
          {hover && hovered !== null && Number.isFinite(hovered) && (
            <span className="heatmap-readout mono">({((hover.column + 0.5) / size * 2 - 1).toFixed(2)}, {(1 - (hover.row + 0.5) / size * 2).toFixed(2)}) {format(hovered)} {unit}</span>
          )}
          {viewport.scale > 1 && <button className="plot-reset" title="Reset zoom (double-click the map)" onClick={() => setViewport(HOME)}><i className="codicon codicon-screen-full" /></button>}
        </div>
        <div className="colorbar" style={{ height: side }}>
          <div className="colorbar-ramp" style={{ background: `linear-gradient(to top, ${Array.from({ length: 11 }, (_, i) => `rgb(${colorAt(colormap, i / 10).join(',')})`).join(',')})` }} />
          <div className="colorbar-labels">{ticks.map((t, i) => <span key={i}>{Math.abs(t) >= 100 || (Math.abs(t) < 0.01 && t !== 0) ? t.toExponential(1) : t.toFixed(3)}</span>)}</div>
          <div className="colorbar-unit">{unit}</div>
        </div>
      </div>
    </div>
  )
}
