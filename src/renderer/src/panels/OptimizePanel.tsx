import { useEffect, useRef, useState } from 'react'
import type { LensSystem, OptimizationSettings } from '../../../shared/lens'
import type { OptimizeProgress, OptimizeResult } from '../../../shared/protocol'
import { Select } from '../analysis/AnalysisFrame'
import { NumberField } from '../components/NumberField'
import { useWorkbench } from '../document'
import { formatFixed } from '../format'
import { clearVariables } from '../lensEdit'
import { LinePlot } from '../plots/LinePlot'

interface Run { history: number[]; iteration: number; running: boolean; result: OptimizeResult | null; error: string | null; ms: number }

const ASPHERE = ['A4', 'A6', 'A8', 'A10']

function variableList(system: LensSystem) {
  const rows: Array<{ surface: number; name: string; value: string }> = []
  system.surfaces.forEach((s, i) => {
    const v = s.variable
    if (!v) return
    if (v.radius) rows.push({ surface: i + 1, name: 'Radius', value: s.radius === 0 ? 'Infinity' : formatFixed(s.radius) })
    if (v.thickness) rows.push({ surface: i + 1, name: 'Thickness', value: formatFixed(s.thickness) })
    if (v.conic) rows.push({ surface: i + 1, name: 'Conic', value: formatFixed(s.conic ?? 0) })
    v.aspheric?.forEach((on, t) => { if (on) rows.push({ surface: i + 1, name: ASPHERE[t] ?? `A${2 * t + 4}`, value: (s.aspheric?.[t] ?? 0).toExponential(6) }) })
  })
  return rows
}

export function OptimizePanel() {
  const { doc, dispatch, edit } = useWorkbench()
  const system = doc.system
  const settings = system.optimization ?? {}
  const [iterations, setIterations] = useState(50)
  const [run, setRun] = useState<Run>({ history: [], iteration: 0, running: false, result: null, error: null, ms: 0 })
  const cancelRef = useRef<(() => void) | null>(null)
  useEffect(() => () => cancelRef.current?.(), [])

  const setSetting = (patch: Partial<OptimizationSettings>) => edit(s => {
    const next = { ...s.optimization, ...patch }
    for (const key of Object.keys(next) as (keyof OptimizationSettings)[]) if (next[key] === undefined || next[key] === null) delete next[key]
    return { ...s, optimization: next }
  })
  const variables = variableList(system)

  function start() {
    const base = system
    const began = performance.now()
    let lastPreview = 0
    setRun({ history: [], iteration: 0, running: true, result: null, error: null, ms: 0 })
    const job = window.instaOptics.engine.start('optimize', { system: JSON.stringify(base), iterations }, json => {
      const progress = JSON.parse(json) as OptimizeProgress
      setRun(r => ({ ...r, history: [...r.history, progress.merit], iteration: progress.iteration, ms: performance.now() - began }))
      // Show progress live, at most a few times per second; history is recorded once at the end.
      const now = performance.now()
      if (now - lastPreview > 200) { lastPreview = now; dispatch({ type: 'preview', system: progress.system as LensSystem }) }
    })
    cancelRef.current = job.cancel
    job.result.then(
      response => {
        const result = JSON.parse(response.json) as OptimizeResult
        dispatch({ type: 'preview', system: result.system as LensSystem })
        dispatch({ type: 'commit', base })
        setRun(r => ({ ...r, running: false, result, history: r.history.length ? r.history : [result.initialMerit], ms: performance.now() - began }))
      },
      (error: Error) => {
        dispatch({ type: 'preview', system: base })
        setRun(r => ({ ...r, running: false, error: error.message }))
      },
    ).finally(() => { cancelRef.current = null })
  }

  const current = run.history[run.history.length - 1] ?? run.result?.finalMerit
  const history = run.result && run.history.length ? [run.result.initialMerit, ...run.history] : run.history

  return (
    <div className="panel optimize">
      <div className="panel-toolbar">
        {run.running
          ? <button className="tool labeled danger" onClick={() => cancelRef.current?.()}><i className="codicon codicon-debug-stop" /> Stop</button>
          : <button className="tool labeled primary" disabled={!variables.length} onClick={start} title={variables.length ? 'Run damped least squares' : 'Mark variables in the Lens Data editor first'}><i className="codicon codicon-play" /> Optimize</button>}
        <Select label="Cycles" value={iterations} options={[10, 25, 50, 100, 200].map(v => ({ value: v, label: String(v) }))} onChange={setIterations} />
        <span className="tool-separator" />
        <button className="tool" title="Remove all variables" disabled={!variables.length || run.running} onClick={() => edit(clearVariables)}><i className="codicon codicon-clear-all" /></button>
        <span className="toolbar-fill" />
        <span className="toolbar-info mono">
          {run.running && `Cycle ${run.iteration} · `}
          {current !== undefined && `Merit ${current.toExponential(4)}`}
          {run.result && ` · ${run.result.iterations} cycles${run.result.stopped ? ' (stopped)' : ''} · ${(run.ms / 1000).toFixed(1)} s`}
        </span>
      </div>
      <div className="optimize-body">
        <div className="optimize-form">
          <h3>Merit Function</h3>
          <div className="form-grid">
            <label>Criterion</label>
            <select value={settings.objective ?? 'spot'} onChange={e => setSetting({ objective: e.target.value as 'spot' | 'wavefront' })}>
              <option value="spot">RMS spot radius (centroid)</option>
              <option value="wavefront">RMS wavefront error</option>
            </select>
            <span />
            <label>Pupil rings</label>
            <select value={settings.rings ?? 3} onChange={e => setSetting({ rings: Number(e.target.value) })}>
              {[2, 3, 4, 5, 6, 8].map(v => <option key={v} value={v}>{v}</option>)}
            </select>
            <span />
          </div>
          <h3>Targets and Constraints</h3>
          <div className="form-grid">
            <label>Effective focal length</label>
            <NumberField optional ariaLabel="Target EFL" placeholder="none" value={system.targetEfl ?? null} onCommit={v => edit(s => { const next = { ...s }; if (v === null) delete next.targetEfl; else next.targetEfl = v; return next })} />
            <span className="unit">mm</span>
            <label>Max total track</label>
            <NumberField optional min={0} ariaLabel="Maximum total track" placeholder="none" value={settings.maxTotalTrack ?? null} onCommit={v => setSetting({ maxTotalTrack: v ?? undefined })} />
            <span className="unit">mm</span>
            <label>Min back focus</label>
            <NumberField optional min={0} ariaLabel="Minimum back focus" placeholder="none" value={settings.minBackFocus ?? null} onCommit={v => setSetting({ minBackFocus: v ?? undefined })} />
            <span className="unit">mm</span>
            <label>Max chief ray angle</label>
            <NumberField optional min={0} ariaLabel="Maximum chief ray angle" placeholder="none" value={settings.maxChiefRayAngle ?? null} onCommit={v => setSetting({ maxChiefRayAngle: v ?? undefined })} />
            <span className="unit">deg</span>
            <label>Min glass centre</label>
            <NumberField optional min={0} ariaLabel="Minimum glass centre thickness" placeholder="0.5" value={settings.minGlassCenter ?? null} onCommit={v => setSetting({ minGlassCenter: v ?? undefined })} />
            <span className="unit">mm</span>
            <label>Min glass edge</label>
            <NumberField optional min={0} ariaLabel="Minimum glass edge thickness" placeholder="0.3" value={settings.minGlassEdge ?? null} onCommit={v => setSetting({ minGlassEdge: v ?? undefined })} />
            <span className="unit">mm</span>
            <label>Min air gap</label>
            <NumberField optional ariaLabel="Minimum air gap" placeholder="0" value={settings.minAir ?? null} onCommit={v => setSetting({ minAir: v ?? undefined })} />
            <span className="unit">mm</span>
          </div>
          <h3>Variables ({variables.length})</h3>
          {variables.length ? (
            <table className="data-table mono">
              <thead><tr><th>Surf</th><th>Parameter</th><th>Value</th></tr></thead>
              <tbody>{variables.map((v, i) => <tr key={i}><td>{v.surface}</td><td className="left">{v.name}</td><td>{v.value}</td></tr>)}</tbody>
            </table>
          ) : <p className="muted hint">Click the V marker in a Lens Data cell (or press Ctrl+T) to make a radius, thickness, conic or aspheric term variable.</p>}
        </div>
        <div className="optimize-chart">
          {run.error && <div className="plot-banner static"><i className="codicon codicon-error" /> {run.error}</div>}
          {history.length > 1 ? (
            <LinePlot
              title="Merit function"
              xLabel="Cycle"
              yLabel="Merit"
              xDomain={[0, Math.max(1, history.length - 1)]}
              series={[{ points: history.map((m, i) => [i, m]), color: 'var(--field-1)', width: 1.6 }]}
            />
          ) : <div className="placeholder">{run.running ? 'Optimizing…' : 'Merit history appears here during optimization.'}</div>}
          {run.result && (
            <div className="analysis-footer mono">
              Initial {run.result.initialMerit.toExponential(4)} → final {run.result.finalMerit.toExponential(4)}
              {run.result.initialMerit > 0 && ` (${(100 * (1 - run.result.finalMerit / run.result.initialMerit)).toFixed(1)}% lower)`} · undo restores the starting lens
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
