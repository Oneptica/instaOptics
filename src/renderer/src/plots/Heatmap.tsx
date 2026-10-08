import { useEffect, useRef } from 'react'
import { useSize } from './LinePlot'

export type Colormap = 'jet' | 'inferno' | 'gray'

const STOPS: Record<Colormap, Array<[number, number, number]>> = {
  jet: [[0, 0, 143], [0, 0, 255], [0, 255, 255], [255, 255, 0], [255, 0, 0], [128, 0, 0]],
  inferno: [[0, 0, 4], [40, 11, 84], [101, 21, 110], [159, 42, 99], [212, 72, 66], [245, 125, 21], [250, 193, 39], [252, 255, 164]],
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

/** Square false-colour map with a colour bar, drawn on a canvas. */
export function Heatmap({ values, size, min, max, colormap, caption, unit, smooth = true, transform }: HeatmapProps) {
  const [hostRef, { width, height }] = useSize<HTMLDivElement>()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    canvas.width = size
    canvas.height = size
    const context = canvas.getContext('2d')!
    const image = context.createImageData(size, size)
    const span = max - min || 1
    values.forEach((value, i) => {
      if (value === null || !Number.isFinite(value)) return
      const [r, g, b] = colorAt(colormap, ((transform ? transform(value) : value) - min) / span)
      image.data.set([r, g, b, 255], i * 4)
    })
    context.putImageData(image, 0, 0)
  }, [values, size, min, max, colormap, transform])
  const side = Math.max(0, Math.min(width - 90, height - 28))
  const ticks = [max, (max + min) / 2, min]
  return (
    <div className="heatmap" ref={hostRef}>
      <div className="heatmap-body" style={{ width: side + 80 }}>
        <div className="heatmap-image" style={{ width: side, height: side }}>
          <canvas ref={canvasRef} style={{ width: side, height: side, imageRendering: smooth ? 'auto' : 'pixelated' }} />
          {caption && <span className="heatmap-caption">{caption}</span>}
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
