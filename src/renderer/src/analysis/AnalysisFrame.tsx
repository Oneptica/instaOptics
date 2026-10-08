import type { ReactNode } from 'react'
import type { AnalysisState } from './useAnalysis'

/** Toolbar, status and error handling shared by every analysis window. */
export function AnalysisFrame<T>({ state, controls, legend, children }: {
  state: AnalysisState<T>
  controls?: ReactNode
  legend?: ReactNode
  children: (result: T) => ReactNode
}) {
  return (
    <div className="panel analysis">
      <div className="panel-toolbar">
        {controls}
        <span className="toolbar-fill" />
        {legend}
        <span className="analysis-status" title="Engine time for this analysis">
          {state.busy ? <i className="codicon codicon-loading codicon-modifier-spin" /> : state.ms !== null && `${state.ms.toFixed(0)} ms`}
        </span>
      </div>
      <div className="analysis-body">
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
