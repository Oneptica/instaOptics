import { useMemo, useState } from 'react'
import type { GlassDef, GlassInfo } from '../../../shared/lens'
import { Select } from '../analysis/AnalysisFrame'
import { useWorkbench } from '../document'
import { decodeAgf, parseAgf, setCatalogs, useCatalogs } from '../glassLibrary'
import { updateSurface } from '../lensEdit'
import { useSize } from '../plots/LinePlot'
import { useSelectedSurface } from '../selection'

const FORMULAS = ['Schott', 'Sellmeier 1', 'Herzberger', 'Sellmeier 2', 'Conrady', 'Sellmeier 3', 'Handbook of Optics 1', 'Handbook of Optics 2', 'Sellmeier 4', 'Extended', 'Sellmeier 5', 'Extended 2', 'Extended 3']
const BUILT_IN = 'Built-in (Schott)'
const COLORS = ['var(--field-1)', 'var(--field-2)', 'var(--field-3)', 'var(--field-4)', 'var(--field-5)', 'var(--field-6)']
const ROW_LIMIT = 500

interface Row extends GlassInfo { source: string; formula?: number; range?: [number, number] }

function AbbeDiagram({ rows, colors, onPick, highlight }: { rows: Row[]; colors: Map<string, string>; onPick: (name: string) => void; highlight: string }) {
  const [ref, { width, height }] = useSize<HTMLDivElement>()
  const valid = rows.filter(r => r.nd > 1 && r.vd > 0)
  const margin = { left: 40, right: 12, top: 10, bottom: 28 }
  const [vMin, vMax, nMin, nMax] = [Math.min(...valid.map(r => r.vd), 20), Math.max(...valid.map(r => r.vd), 95), Math.min(...valid.map(r => r.nd), 1.4), Math.max(...valid.map(r => r.nd), 2.0)]
  const px = (vd: number) => margin.left + (vMax - vd) / (vMax - vMin) * (width - margin.left - margin.right) // Abbe number runs right to left
  const py = (nd: number) => margin.top + (nMax - nd) / (nMax - nMin) * (height - margin.top - margin.bottom)
  const vTicks = [20, 30, 40, 50, 60, 70, 80, 90].filter(v => v >= vMin && v <= vMax)
  const nTicks = [1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0].filter(v => v >= nMin && v <= nMax)
  return (
    <div className="abbe" ref={ref}>
      {width > 60 && height > 60 && (
        <svg width={width} height={height}>
          <rect className="frame" x={margin.left} y={margin.top} width={width - margin.left - margin.right} height={height - margin.top - margin.bottom} />
          {vTicks.map(v => <g key={v}><line className="tick" x1={px(v)} x2={px(v)} y1={margin.top} y2={height - margin.bottom} /><text x={px(v)} y={height - 12} textAnchor="middle">{v}</text></g>)}
          {nTicks.map(n => <g key={n}><line className="tick" x1={margin.left} x2={width - margin.right} y1={py(n)} y2={py(n)} /><text x={margin.left - 5} y={py(n) + 3.5} textAnchor="end">{n.toFixed(1)}</text></g>)}
          <text className="axis" x={margin.left + (width - margin.left - margin.right) / 2} y={height - 1} textAnchor="middle">Abbe number v<tspan dy="2" fontSize="8">d</tspan></text>
          {valid.map(r => (
            <circle key={`${r.source}/${r.name}`} cx={px(r.vd)} cy={py(r.nd)} r={r.name === highlight ? 5 : 3} fill={colors.get(r.source)} stroke={r.name === highlight ? 'var(--text-strong)' : 'none'} opacity={r.name === highlight ? 1 : 0.8} onClick={() => onPick(r.name)}>
              <title>{`${r.name} (${r.source}) nd ${r.nd.toFixed(5)}, vd ${r.vd.toFixed(2)}`}</title>
            </circle>
          ))}
        </svg>
      )}
    </div>
  )
}

export function GlassPanel() {
  const { doc, edit, glasses } = useWorkbench()
  const catalogs = useCatalogs()
  const surface = useSelectedSurface()
  const [query, setQuery] = useState('')
  const [source, setSource] = useState('')
  const [picked, setPicked] = useState('')
  const [busy, setBusy] = useState(false)

  const rows: Row[] = useMemo(() => {
    const builtIn = glasses.filter(g => !g.catalog).map(g => ({ ...g, source: BUILT_IN }))
    const loaded: Row[] = catalogs.flatMap(c => c.glasses.map((g: GlassDef) => ({ name: g.name, nd: g.nd, vd: g.vd, source: c.name, formula: g.formula, range: g.range })))
    return [...builtIn, ...loaded]
  }, [glasses, catalogs])
  const sources = useMemo(() => [BUILT_IN, ...catalogs.map(c => c.name)], [catalogs])
  const colors = useMemo(() => new Map(sources.map((s, i) => [s, COLORS[i % COLORS.length]])), [sources])
  const needle = query.trim().toUpperCase()
  const shown = rows.filter(r => (!source || r.source === source) && (!needle || r.name.toUpperCase().includes(needle)))

  const apply = (name: string) => {
    if (surface !== null && surface < doc.system.surfaces.length) edit(s => updateSurface(s, surface, { material: name }))
  }
  const refresh = (list: Array<{ name: string; bytes: Uint8Array }>) => setCatalogs(list.map(c => ({ name: c.name, glasses: parseAgf(decodeAgf(c.bytes), c.name) })))
  const add = async () => { setBusy(true); try { refresh(await window.instaOptics.catalogs.add()) } finally { setBusy(false) } }
  const remove = async (name: string) => { refresh(await window.instaOptics.catalogs.remove(name)); if (source === name) setSource('') }
  const target = surface === null ? null : surface + 1

  return (
    <div className="panel glass-panel">
      <div className="panel-toolbar">
        <button className="tool labeled primary" disabled={busy} onClick={() => void add()} title="Add OpticStudio glass catalogs (.agf)"><i className="codicon codicon-add" /> Add Catalog…</button>
        <Select label="Source" value={source} options={[{ value: '', label: 'All' }, ...sources.map(s => ({ value: s, label: s }))]} onChange={setSource} />
        <input className="field-input search" placeholder="Search glass name" value={query} onChange={e => setQuery(e.target.value)} />
        <span className="toolbar-fill" />
        <button className="tool labeled" disabled={!picked || target === null} onClick={() => apply(picked)} title="Use the selected glass for the surface selected in the Lens Data editor">
          <i className="codicon codicon-arrow-right" /> {target === null ? 'Select a surface' : `Use for surface ${target}`}
        </button>
      </div>
      <div className="glass-body">
        <div className="glass-side">
          <h3>Catalogs</h3>
          <div className="catalog-row"><span className="dot" style={{ background: colors.get(BUILT_IN) }} /> {BUILT_IN} <span className="muted">{rows.filter(r => r.source === BUILT_IN).length}</span></div>
          {catalogs.map(c => (
            <div className="catalog-row" key={c.name}>
              <span className="dot" style={{ background: colors.get(c.name) }} /> {c.name} <span className="muted">{c.glasses.length}</span>
              <button className="icon-button" title="Remove this catalog" onClick={() => void remove(c.name)}><i className="codicon codicon-close" /></button>
            </div>
          ))}
          {!catalogs.length && <p className="muted hint">Add the .agf files of glass manufacturers (Schott, Ohara, CDGM, Hoya, …). Glasses a lens uses are saved inside the lens file, so it opens without the catalog.</p>}
          <AbbeDiagram rows={shown} colors={colors} onPick={setPicked} highlight={picked} />
        </div>
        <div className="glass-table">
          <table className="data-table mono">
            <thead><tr><th className="left">Name</th><th className="left">Catalog</th><th>n<sub>d</sub></th><th>v<sub>d</sub></th><th className="left">Formula</th><th>Range (µm)</th></tr></thead>
            <tbody>
              {shown.slice(0, ROW_LIMIT).map(r => (
                <tr key={`${r.source}/${r.name}`} className={r.name === picked ? 'picked' : ''} onClick={() => setPicked(r.name)} onDoubleClick={() => apply(r.name)}>
                  <td className="left">{r.name}</td>
                  <td className="left">{r.source}</td>
                  <td>{r.nd ? r.nd.toFixed(5) : '—'}</td>
                  <td>{r.vd ? r.vd.toFixed(2) : '—'}</td>
                  <td className="left">{r.formula ? FORMULAS[r.formula - 1] : 'Sellmeier 1'}</td>
                  <td>{r.range ? `${r.range[0].toFixed(2)}–${r.range[1].toFixed(2)}` : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {shown.length > ROW_LIMIT && <div className="analysis-footer mono">Showing {ROW_LIMIT} of {shown.length}; narrow the search to see the rest.</div>}
          {!shown.length && <div className="placeholder">No glass matches.</div>}
        </div>
      </div>
    </div>
  )
}
