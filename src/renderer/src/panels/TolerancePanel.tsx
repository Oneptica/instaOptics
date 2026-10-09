import { useEffect, useMemo, useRef, useState } from 'react'
import type { MonteCarloProgress } from '../../../shared/protocol'
import { NumberField } from '../components/NumberField'
import { useWorkbench } from '../document'
import { LinePlot, niceTicks, useSize } from '../plots/LinePlot'
import { useDomainZoom } from '../plots/useZoomPan'

interface Settings { radius: number; thickness: number; decenter: number; tilt: number; index: number; abbe: number; compensator: 'focus' | 'none'; trials: number; seed: number }
const DEFAULTS: Settings = { radius: 0.2, thickness: 0.05, decenter: 0.02, tilt: 1, index: 0.0005, abbe: 0.5, compensator: 'focus', trials: 500, seed: 1 }

interface Row { parameter: { surface: number; kind: string; tolerance: number }; plus: number; minus: number; worst: number }
interface SensitivityResult { nominal: number; rows: Row[]; estimated: number }

const KIND: Record<string, { label: string; unit: string }> = {
  radius: { label: 'Radius', unit: '%' }, thickness: { label: 'Thickness', unit: 'mm' }, decenter: { label: 'Decenter Y', unit: 'mm' },
  tilt: { label: 'Tilt X', unit: '′' }, index: { label: 'Index nd', unit: '' }, abbe: { label: 'Abbe vd', unit: '%' },
}
const um = (mm: number) => Number.isFinite(mm) ? (mm * 1000).toFixed(3) : '—'

const FIELDS: Array<{ key: keyof Settings; label: string; unit: string }> = [
  { key: 'radius', label: 'Radius', unit: '± %' },
  { key: 'thickness', label: 'Thickness', unit: '± mm' },
  { key: 'decenter', label: 'Surface decenter', unit: '± mm' },
  { key: 'tilt', label: 'Surface tilt', unit: '± ′' },
  { key: 'index', label: 'Index nd', unit: '±' },
  { key: 'abbe', label: 'Abbe vd', unit: '± %' },
]

function percentile(sorted: number[], p: number) {
  return sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(p / 100 * sorted.length))] : NaN
}

function Histogram({ values, nominal }: { values: number[]; nominal: number }) {
  const [ref, { width, height }] = useSize<HTMLDivElement>()
  const finite = values.filter(Number.isFinite).map(v => v * 1000)
  const lo = finite.length ? Math.min(...finite, nominal * 1000) : 0, hi = finite.length ? Math.max(...finite) : 1
  const bins = 30, span = hi - lo || 1
  const counts = new Array<number>(bins).fill(0)
  for (const v of finite) counts[Math.min(bins - 1, Math.floor((v - lo) / span * bins))]++
  const peak = Math.max(...counts, 1)
  const left = 48, bottom = 34, top = 22, right = 12
  const plotW = Math.max(1, width - left - right), plotH = Math.max(1, height - top - bottom)
  const yFit = Math.max(peak * 1.08, niceTicks(0, peak * 1.08, 4).at(-1) ?? 1)
  const zoom = useDomainZoom<HTMLDivElement>({ h: [lo, lo + span], v: [0, yFit] }, { left, top, width: plotW, height: plotH }, ref)
  if (!finite.length) return <div className="line-plot" ref={ref} />
  const [x0, x1] = zoom.shown.h, [y0, y1] = zoom.shown.v
  const xTicks = niceTicks(x0, x1, 5), yTicks = niceTicks(y0, y1, 4)
  const sx = (v: number) => left + (v - x0) / (x1 - x0) * plotW
  const sy = (c: number) => top + plotH - (c - y0) / (y1 - y0) * plotH
  return (
    <div className="line-plot" ref={ref} {...zoom.handlers}>
      {width > 0 && (
        <svg width={width} height={height}>
          <text className="plot-title" x={left + plotW / 2} y={14}>Distribution of RMS spot radius</text>
          <g className="grid-lines">{yTicks.map(t => <line key={t} x1={left} x2={left + plotW} y1={sy(t)} y2={sy(t)} />)}</g>
          <svg x={left} y={top} width={plotW} height={plotH} overflow="hidden"><g transform={`translate(${-left} ${-top})`}>
            {counts.map((c, i) => <rect key={i} x={sx(lo + i / bins * span) + 0.5} width={Math.max(0.5, plotW * span / bins / (x1 - x0) - 1)} y={sy(c)} height={Math.max(0, sy(0) - sy(c))} fill="var(--field-1)" opacity={0.75} />)}
            <line x1={sx(nominal * 1000)} x2={sx(nominal * 1000)} y1={top} y2={top + plotH} stroke="var(--field-3)" strokeDasharray="4 3" />
          </g></svg>
          <rect className="frame" x={left} y={top} width={plotW} height={plotH} />
          <g className="tick-labels">
            {xTicks.map(t => <text key={t} x={sx(t)} y={top + plotH + 14} textAnchor="middle">{t.toFixed(xTicks.length > 1 && xTicks[1] - xTicks[0] < 1 ? 2 : 1)}</text>)}
            {yTicks.map(t => <text key={t} x={left - 6} y={sy(t) + 4} textAnchor="end">{t}</text>)}
          </g>
          <text className="axis-label" x={left + plotW / 2} y={height - 4} textAnchor="middle">RMS spot radius (µm) · dashed: nominal</text>
        </svg>
      )}
      {zoom.zoomed && <button className="plot-reset" title="Reset zoom (double-click the plot)" onClick={zoom.reset} onPointerDown={e => e.stopPropagation()}><i className="codicon codicon-screen-full" /></button>}
    </div>
  )
}

export function TolerancePanel() {
  const { doc } = useWorkbench()
  const [settings, setSettings] = useState<Settings>(DEFAULTS)
  const [mode, setMode] = useState<'sensitivity' | 'monteCarlo'>('sensitivity')
  const [sensitivity, setSensitivity] = useState<SensitivityResult | null>(null)
  const [mc, setMc] = useState<MonteCarloProgress | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [ms, setMs] = useState<number | null>(null)
  const cancelRef = useRef<(() => void) | null>(null)
  useEffect(() => () => cancelRef.current?.(), [])
  const set = (patch: Partial<Settings>) => setSettings(s => ({ ...s, ...patch }))

  function run(which: 'sensitivity' | 'monteCarlo') {
    setMode(which)
    setBusy(true)
    setError(null)
    const began = performance.now()
    const params = { system: JSON.stringify(doc.system), settings: JSON.stringify(settings) }
    const job = window.instaOptics.engine.start(which, params, json => setMc(JSON.parse(json) as MonteCarloProgress))
    cancelRef.current = job.cancel
    if (which === 'monteCarlo') setMc(null)
    job.result.then(
      response => {
        if (which === 'sensitivity') setSensitivity(JSON.parse(response.json) as SensitivityResult)
        else setMc(JSON.parse(response.json) as MonteCarloProgress)
        setMs(performance.now() - began)
      },
      (e: Error) => setError(e.message),
    ).finally(() => { setBusy(false); cancelRef.current = null })
  }

  const sorted = useMemo(() => mc ? mc.values.filter(Number.isFinite).sort((a, b) => a - b) : [], [mc])
  const mean = sorted.length ? sorted.reduce((a, b) => a + b, 0) / sorted.length : NaN
  const std = sorted.length ? Math.sqrt(sorted.reduce((s, v) => s + (v - mean) ** 2, 0) / sorted.length) : NaN

  return (
    <div className="panel tolerance">
      <div className="panel-toolbar">
        <button className="tool labeled primary" disabled={busy} onClick={() => run('sensitivity')}><i className="codicon codicon-list-ordered" /> Sensitivity</button>
        <button className="tool labeled primary" disabled={busy} onClick={() => run('monteCarlo')}><i className="codicon codicon-symbol-event" /> Monte Carlo</button>
        {busy && <button className="tool labeled danger" onClick={() => cancelRef.current?.()}><i className="codicon codicon-debug-stop" /> Stop</button>}
        <span className="toolbar-fill" />
        <span className="toolbar-info mono">
          {busy && mode === 'monteCarlo' && mc && `${mc.values.length} / ${mc.total} trials · `}
          {busy ? <i className="codicon codicon-loading codicon-modifier-spin" /> : ms !== null && `${ms.toFixed(0)} ms`}
        </span>
      </div>
      <div className="optimize-body">
        <div className="optimize-form tool-form">
          <h3>Tolerances</h3>
          <div className="form-grid">
            {FIELDS.map(field => (
              <FieldRow key={field.key} label={field.label} unit={field.unit} value={settings[field.key] as number} onCommit={v => set({ [field.key]: v } as Partial<Settings>)} />
            ))}
          </div>
          <h3>Analysis</h3>
          <div className="form-grid">
            <label>Compensator</label>
            <select value={settings.compensator} onChange={e => set({ compensator: e.target.value as Settings['compensator'] })}>
              <option value="focus">Back focus</option>
              <option value="none">None</option>
            </select>
            <span />
            <label>Monte Carlo trials</label>
            <select value={settings.trials} onChange={e => set({ trials: Number(e.target.value) })}>
              {[100, 200, 500, 1000, 2000, 5000, 10000].map(v => <option key={v} value={v}>{v}</option>)}
            </select>
            <span />
            <FieldRow label="Random seed" unit="" value={settings.seed} onCommit={v => set({ seed: Math.max(1, Math.round(v)) })} />
          </div>
          <p className="muted hint">Criterion: polychromatic RMS spot radius about each field's centroid, RMS over ± fields. Monte Carlo draws each parameter from a normal distribution with 2σ = tolerance.</p>
        </div>
        <div className="optimize-chart">
          {error && <div className="plot-banner static"><i className="codicon codicon-error" /> {error}</div>}
          {mode === 'sensitivity' ? (
            sensitivity ? (
              <div className="tolerance-table">
                <div className="analysis-footer mono summary">
                  Nominal {um(sensitivity.nominal)} µm · estimated as-built (RSS) {um(sensitivity.estimated)} µm · {sensitivity.rows.length} parameters
                </div>
                <table className="data-table mono">
                  <thead><tr><th>Surf</th><th className="left">Parameter</th><th>±Tol</th><th>Δ at + (µm)</th><th>Δ at − (µm)</th><th>Worst (µm)</th><th className="bar-head" /></tr></thead>
                  <tbody>
                    {sensitivity.rows.map((row, i) => {
                      const top = Math.max(sensitivity.rows[0]?.worst ?? 0, 1e-12)
                      const kind = KIND[row.parameter.kind]
                      return (
                        <tr key={i}>
                          <td>{row.parameter.surface + 1}</td>
                          <td className="left">{kind?.label ?? row.parameter.kind}</td>
                          <td>{row.parameter.tolerance}{kind?.unit}</td>
                          <td>{um(row.plus)}</td>
                          <td>{um(row.minus)}</td>
                          <td>{um(row.worst)}</td>
                          <td className="bar-cell"><span style={{ width: `${Math.max(0, row.worst) / top * 100}%` }} /></td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            ) : <div className="placeholder">{busy ? 'Computing sensitivities…' : 'Run Sensitivity to rank each tolerance by its effect.'}</div>
          ) : mc && sorted.length ? (
            <div className="analysis-fill">
              <div className="split-row">
                <div className="split-cell"><Histogram values={mc.values} nominal={mc.nominal} /></div>
                <div className="split-cell">
                  <LinePlot
                    title="Cumulative probability"
                    xLabel="RMS spot radius (µm)"
                    yLabel="Fraction of lenses"
                    yDomain={[0, 1]}
                    series={[{ points: sorted.map((v, i) => [v * 1000, (i + 1) / sorted.length]), color: 'var(--field-1)', width: 1.6 }]}
                  />
                </div>
              </div>
              <div className="analysis-footer mono">
                {sorted.length} trials · nominal {um(mc.nominal)} · mean {um(mean)} · σ {um(std)} · 50% &lt; {um(percentile(sorted, 50))} · 90% &lt; {um(percentile(sorted, 90))} · 98% &lt; {um(percentile(sorted, 98))} µm
              </div>
            </div>
          ) : <div className="placeholder">{busy ? 'Running trials…' : 'Run Monte Carlo to estimate the as-built yield.'}</div>}
        </div>
      </div>
    </div>
  )
}

function FieldRow({ label, unit, value, onCommit }: { label: string; unit: string; value: number; onCommit: (value: number) => void }) {
  return (
    <>
      <label>{label}</label>
      <NumberField ariaLabel={label} value={value} min={-1e-12} onCommit={v => v !== null && v !== value && onCommit(v)} />
      <span className="unit">{unit}</span>
    </>
  )
}
