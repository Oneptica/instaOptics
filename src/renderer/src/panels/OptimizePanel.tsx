import { useEffect, useRef, useState } from 'react'
import type { LensSystem, Operand, OperandKind, OptimizationSettings } from '../../../shared/lens'
import type { OptimizeProgress, OptimizeResult } from '../../../shared/protocol'
import { Select } from '../analysis/AnalysisFrame'
import { type MeritReport, useAnalysis } from '../analysis/useAnalysis'
import { configurationCount } from '../lensEdit'
import { NumberField } from '../components/NumberField'
import { useWorkbench } from '../document'
import { formatFixed } from '../format'
import { clearVariables } from '../lensEdit'
import { LinePlot } from '../plots/LinePlot'

interface Run { history: number[]; iteration: number; running: boolean; result: OptimizeResult | null; error: string | null; ms: number }

const ASPHERE = ['A4', 'A6', 'A8', 'A10']

const OPERANDS: Array<{ value: OperandKind; label: string; unit: string; needs?: 'surface' | 'field' }> = [
  { value: 'efl', label: 'EFL', unit: 'mm' },
  { value: 'totalTrack', label: 'Total track', unit: 'mm' },
  { value: 'backFocus', label: 'Back focus', unit: 'mm' },
  { value: 'fNumber', label: 'F/#', unit: '' },
  { value: 'imageHeight', label: 'Image height', unit: 'mm' },
  { value: 'chiefRayAngle', label: 'Chief ray angle', unit: 'deg', needs: 'field' },
  { value: 'distortion', label: 'Distortion', unit: '%', needs: 'field' },
  { value: 'spotRadius', label: 'RMS spot radius', unit: 'mm', needs: 'field' },
  { value: 'thickness', label: 'Thickness', unit: 'mm', needs: 'surface' },
  { value: 'radius', label: 'Radius', unit: 'mm', needs: 'surface' },
]

function OperandTable({ system, report, onChange }: { system: LensSystem; report: MeritReport | null; onChange: (operands: Operand[]) => void }) {
  const operands = system.optimization?.operands ?? []
  const configCount = configurationCount(system)
  const update = (i: number, patch: Partial<Operand>) => onChange(operands.map((o, k) => {
    if (k !== i) return o
    const next = { ...o, ...patch }
    for (const key of Object.keys(next) as (keyof Operand)[]) if (next[key] === undefined) delete next[key]
    return next
  }))
  return (
    <>
      <table className="data-table mono operand-table">
        <thead><tr><th>#</th><th className="left">Type</th><th>Surf / Field</th><th className="left">Relation</th><th>Target</th><th>Weight</th>{configCount > 1 && <th>Config</th>}<th>Value</th><th /></tr></thead>
        <tbody>
          {operands.map((o, i) => {
            const info = OPERANDS.find(x => x.value === o.kind)!
            const value = report?.values[i]
            return (
              <tr key={i}>
                <td>{i + 1}</td>
                <td className="left">
                  <select aria-label="Operand type" value={o.kind} onChange={e => { const kind = e.target.value as OperandKind; const needs = OPERANDS.find(x => x.value === kind)?.needs; update(i, { kind, surface: needs === 'surface' ? (o.surface ?? 0) : undefined, field: needs === 'field' ? o.field : undefined }) }}>
                    {OPERANDS.map(x => <option key={x.value} value={x.value}>{x.label}</option>)}
                  </select>
                </td>
                <td>
                  {info.needs === 'surface' && (
                    <select aria-label="Surface" value={o.surface ?? 0} onChange={e => update(i, { surface: Number(e.target.value) })}>
                      {system.surfaces.map((_, k) => <option key={k} value={k}>{k + 1}</option>)}
                    </select>
                  )}
                  {info.needs === 'field' && (
                    <select aria-label="Field" value={o.field ?? -1} onChange={e => update(i, { field: Number(e.target.value) < 0 ? undefined : Number(e.target.value) })}>
                      <option value={-1}>All</option>
                      {system.fields.map((_, k) => <option key={k} value={k}>{k + 1}</option>)}
                    </select>
                  )}
                </td>
                <td className="left">
                  <select aria-label="Relation" value={o.relation ?? 'equal'} onChange={e => update(i, { relation: e.target.value === 'equal' ? undefined : e.target.value as Operand['relation'] })}>
                    <option value="equal">=</option><option value="atMost">≤</option><option value="atLeast">≥</option>
                  </select>
                </td>
                <td><NumberField ariaLabel="Operand target" value={o.target} onCommit={v => v !== null && update(i, { target: v })} /></td>
                <td><NumberField ariaLabel="Operand weight" min={0} value={o.weight ?? 1} onCommit={v => v !== null && update(i, { weight: v === 1 ? undefined : v })} /></td>
                {configCount > 1 && (
                  <td>
                    <select aria-label="Configuration" value={o.config ?? -1} onChange={e => update(i, { config: Number(e.target.value) < 0 ? undefined : Number(e.target.value) })}>
                      <option value={-1}>All</option>
                      {system.configs?.names.map((name, k) => <option key={k} value={k}>{name}</option>)}
                    </select>
                  </td>
                )}
                <td>{value === null || value === undefined ? '—' : `${formatFixed(value)} ${info.unit}`}</td>
                <td><button className="tool" title="Remove operand" onClick={() => onChange(operands.filter((_, k) => k !== i))}><i className="codicon codicon-trash" /></button></td>
              </tr>
            )
          })}
        </tbody>
      </table>
      <p className="muted hint">{operands.length ? 'Operands add to the criterion and constraints above. Targets are held with the weight shown; ≤ and ≥ only count when violated.' : 'Add operands to target the focal length, track, distortion, chief ray angle or a surface value, per field and configuration.'}</p>
    </>
  )
}

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
  const { doc, dispatch, edit, engine } = useWorkbench()
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
  const report = useAnalysis<MeritReport>({ kind: 'meritFunction' })
  const engineEfl = engine.overview?.paraxial.efl

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
            <select value={settings.objective ?? 'spot'} onChange={e => setSetting({ objective: e.target.value as 'spot' | 'wavefront' | 'none' })}>
              <option value="spot">RMS spot radius (centroid)</option>
              <option value="wavefront">RMS wavefront error</option>
              <option value="none">None (operands only)</option>
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
          <h3>Operands ({settings.operands?.length ?? 0})
            <button className="tool" title="Add an operand" onClick={() => setSetting({ operands: [...(settings.operands ?? []), { kind: 'efl', target: engineEfl ?? 0 }] })}><i className="codicon codicon-add" /></button>
          </h3>
          <OperandTable system={system} report={report.result} onChange={operands => setSetting({ operands: operands.length ? operands : undefined })} />
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
