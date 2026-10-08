import { useRef, type ReactNode } from 'react'
import type { AnalysisState } from './useAnalysis'

/** Rows to CSV text; numbers keep full precision, nulls become empty cells. */
export function toCsv(header: string[], rows: Array<Array<number | string | null | undefined>>): string {
  const cell = (v: number | string | null | undefined) => v === null || v === undefined || (typeof v === 'number' && !Number.isFinite(v)) ? '' : typeof v === 'string' && /[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : String(v)
  return [header, ...rows].map(row => row.map(cell).join(',')).join('\n') + '\n'
}

/** Toolbar, status, export and error handling shared by every analysis window. */
export function AnalysisFrame<T>({ state, controls, legend, children, name, csv }: {
  state: AnalysisState<T>
  controls?: ReactNode
  legend?: ReactNode
  children: (result: T) => ReactNode
  /** Default file name for exports. */
  name?: string
  /** Builds CSV text from the current result. */
  csv?: (result: T) => string
}) {
  const bodyRef = useRef<HTMLDivElement>(null)
  const file = window.instaOptics.file
  const exportPng = () => {
    const rect = bodyRef.current?.getBoundingClientRect()
    if (rect) void file.exportPng({ x: rect.x, y: rect.y, width: rect.width, height: rect.height }, name ?? 'analysis')
  }
  const result = state.result
  return (
    <div className="panel analysis">
      <div className="panel-toolbar">
        {controls}
        <span className="toolbar-fill" />
        {legend}
        {result && csv && <button className="tool" title="Export data as CSV" onClick={() => void file.export(csv(result), `${name ?? 'analysis'}.csv`, 'CSV', ['csv'])}><i className="codicon codicon-table" /></button>}
        {result && <button className="tool" title="Save as PNG image" onClick={exportPng}><i className="codicon codicon-device-camera" /></button>}
        <span className="analysis-status" title="Engine time for this analysis">
          {state.busy ? <i className="codicon codicon-loading codicon-modifier-spin" /> : state.ms !== null && `${state.ms.toFixed(0)} ms`}
        </span>
      </div>
      <div className="analysis-body" ref={bodyRef}>
        {state.result ? children(state.result) : <div className="placeholder">{state.error ?? 'Computing…'}</div>}
        {state.result && state.error && <div className="plot-banner"><i className="codicon codicon-warning" /> {state.error} — showing the last valid result</div>}
      </div>
    </div>
  )
}

export function Select<T extends string | number>({ label, value, options, onChange }: {
  label: string; value: T; options: Array<{ value: T; label: string }>; onChange: (value: T) => void
}) {
  return (
    <label className="tool-field">
      {label}
      <select value={String(value)} onChange={event => onChange(options.find(o => String(o.value) === event.target.value)!.value)}>
        {options.map(option => <option key={String(option.value)} value={String(option.value)}>{option.label}</option>)}
      </select>
    </label>
  )
}

/** Approximate display colour for a wavelength in µm; infrared and ultraviolet fall back to grey tones. */
export function wavelengthColor(wavelength: number): string {
  const nm = wavelength * 1000
  if (nm < 380) return '#8b7fd6'
  if (nm > 750) return '#b0605a'
  const stops: Array<[number, number]> = [[380, 270], [440, 230], [490, 195], [520, 125], [565, 75], [590, 48], [625, 12], [645, 0], [750, 0]]
  let hue = 0
  for (let i = 0; i < stops.length - 1; i++) {
    const [a, ha] = stops[i], [b, hb] = stops[i + 1]
    if (nm >= a && nm <= b) { hue = ha + (hb - ha) * (nm - a) / (b - a); break }
  }
  return `hsl(${Math.round(hue)} 85% 52%)`
}
