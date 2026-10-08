import { useEffect, useRef, useState, type ReactNode } from 'react'

export function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [size, setSize] = useState({ width: 0, height: 0 })
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => setSize({ width: Math.floor(entry.contentRect.width), height: Math.floor(entry.contentRect.height) }))
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  return [ref, size] as const
}

/** About `count` round tick values covering [min, max]. */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!(max > min)) return [min]
  const raw = (max - min) / count
  const power = 10 ** Math.floor(Math.log10(raw))
  const step = [1, 2, 2.5, 5, 10].map(m => m * power).find(m => m >= raw) ?? power * 10
  const ticks: number[] = []
  for (let v = Math.ceil(min / step - 1e-9) * step; v <= max + step * 1e-9; v += step) ticks.push(Math.abs(v) < step * 1e-9 ? 0 : v)
  return ticks
}

export function formatTick(value: number, ticks: number[]): string {
  const step = ticks.length > 1 ? Math.abs(ticks[1] - ticks[0]) : Math.abs(value) || 1
  if (step >= 1e4 || (step < 1e-3 && value !== 0)) return value.toExponential(1)
  const digits = Math.max(0, Math.min(6, -Math.floor(Math.log10(step) + 1e-9)))
  return value.toFixed(digits + (step / 10 ** Math.floor(Math.log10(step)) === 2.5 ? 1 : 0))
}

export interface Series {
  /** Points with null y for gaps. */
  points: Array<[number, number | null]>
  color: string
  dash?: string
  width?: number
  label?: string
}

interface LinePlotProps {
  series: Series[]
  xLabel: string
  yLabel: string
  title?: ReactNode
  xDomain?: [number, number]
  yDomain?: [number, number]
  /** Make the y range symmetric about zero. */
  symmetricY?: boolean
  /** Swap axes: x values are drawn vertically (e.g. field curves against field angle). */
  vertical?: boolean
  zeroLine?: boolean
}

const M = { left: 56, right: 14, top: 22, bottom: 34 }

function extent(series: Series[], axis: 0 | 1): [number, number] {
  let min = Infinity, max = -Infinity
  for (const s of series) for (const p of s.points) {
    const v = p[axis]
    if (v === null || !Number.isFinite(v)) continue
    min = Math.min(min, v); max = Math.max(max, v)
  }
  if (!Number.isFinite(min)) return [0, 1]
  if (min === max) { const pad = Math.abs(min) * 0.1 || 1; return [min - pad, max + pad] }
  return [min, max]
}

/** Responsive SVG line plot with round ticks, used by every analysis window. */
export function LinePlot({ series, xLabel, yLabel, title, xDomain, yDomain, symmetricY, vertical, zeroLine = true }: LinePlotProps) {
  const [ref, { width, height }] = useSize<HTMLDivElement>()
  let [x0, x1] = xDomain ?? extent(series, 0)
  let [y0, y1] = yDomain ?? extent(series, 1)
  if (symmetricY && !yDomain) { const m = Math.max(Math.abs(y0), Math.abs(y1)) || 1; y0 = -m; y1 = m }
  if (!yDomain) { const ticks = niceTicks(y0, y1); y0 = Math.min(y0, ticks[0]); y1 = Math.max(y1, ticks[ticks.length - 1]) }
  if (!xDomain) { const ticks = niceTicks(x0, x1); x0 = Math.min(x0, ticks[0]); x1 = Math.max(x1, ticks[ticks.length - 1]) }
  const xTicks = niceTicks(x0, x1, vertical ? 5 : 6), yTicks = niceTicks(y0, y1, vertical ? 6 : 5)

  const plotW = Math.max(1, width - M.left - M.right), plotH = Math.max(1, height - M.top - M.bottom)
  // In vertical mode the "x" data runs up the vertical axis and "y" data along the horizontal axis.
  const [hMin, hMax, vMin, vMax] = vertical ? [y0, y1, x0, x1] : [x0, x1, y0, y1]
  const sx = (h: number) => M.left + (h - hMin) / (hMax - hMin) * plotW
  const sy = (v: number) => M.top + plotH - (v - vMin) / (vMax - vMin) * plotH
  const at = (x: number, y: number): [number, number] => vertical ? [sx(y), sy(x)] : [sx(x), sy(y)]
  const hTicks = vertical ? yTicks : xTicks, vTicks = vertical ? xTicks : yTicks

  const path = (s: Series) => {
    let d = '', pen = false
    for (const [x, y] of s.points) {
      if (y === null || !Number.isFinite(y)) { pen = false; continue }
      const [px, py] = at(x, y)
      d += `${pen ? 'L' : 'M'}${px.toFixed(1)} ${py.toFixed(1)}`
      pen = true
    }
    return d
  }

  return (
    <div className="line-plot" ref={ref}>
      {width > 0 && height > 0 && (
        <svg width={width} height={height}>
          {title && <text className="plot-title" x={M.left + plotW / 2} y={14}>{title}</text>}
          <g className="grid-lines">
            {hTicks.map(t => <line key={`h${t}`} x1={sx(t)} x2={sx(t)} y1={M.top} y2={M.top + plotH} />)}
            {vTicks.map(t => <line key={`v${t}`} x1={M.left} x2={M.left + plotW} y1={sy(t)} y2={sy(t)} />)}
          </g>
          {zeroLine && hMin < 0 && hMax > 0 && <line className="zero-line" x1={sx(0)} x2={sx(0)} y1={M.top} y2={M.top + plotH} />}
          {zeroLine && vMin < 0 && vMax > 0 && <line className="zero-line" x1={M.left} x2={M.left + plotW} y1={sy(0)} y2={sy(0)} />}
          <rect className="frame" x={M.left} y={M.top} width={plotW} height={plotH} />
          <g className="tick-labels">
            {hTicks.map(t => <text key={t} x={sx(t)} y={M.top + plotH + 14} textAnchor="middle">{formatTick(t, hTicks)}</text>)}
            {vTicks.map(t => <text key={t} x={M.left - 6} y={sy(t) + 4} textAnchor="end">{formatTick(t, vTicks)}</text>)}
          </g>
          <text className="axis-label" x={M.left + plotW / 2} y={height - 4} textAnchor="middle">{vertical ? yLabel : xLabel}</text>
          <text className="axis-label" transform={`translate(12 ${M.top + plotH / 2}) rotate(-90)`} textAnchor="middle">{vertical ? xLabel : yLabel}</text>
          <svg x={M.left} y={M.top} width={plotW} height={plotH} overflow="hidden">
            <g transform={`translate(${-M.left} ${-M.top})`}>
              {series.map((s, i) => <path key={i} d={path(s)} fill="none" stroke={s.color} strokeWidth={s.width ?? 1.3} strokeDasharray={s.dash} />)}
            </g>
          </svg>
        </svg>
      )}
    </div>
  )
}

export function Legend({ items }: { items: Array<{ label: string; color: string; dash?: string }> }) {
  return (
    <span className="legend">
      {items.map(item => (
        <span key={item.label} className="legend-item">
          <svg width="16" height="6"><line x1="0" x2="16" y1="3" y2="3" stroke={item.color} strokeWidth="2" strokeDasharray={item.dash} /></svg>
          {item.label}
        </span>
      ))}
    </span>
  )
}
