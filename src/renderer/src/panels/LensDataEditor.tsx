import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { setSelectedSurface } from '../selection'
import type { LensSystem } from '../../../shared/lens'
import { useWorkbench } from '../document'
import { formatFixed } from '../format'
import {
  type VariableKey, configRowFor, deleteSurface, formatSolve, parseSolve, setConfigValue, solvedParameter, activeConfiguration, toggleCoordinateBreak, insertSurface, isKnownMaterial, isVariable, parseNumber, setAspheric, setStop, toggleVariable, updateSurface,
} from '../lensEdit'

type ColumnKey = 'radius' | 'thickness' | 'material' | 'coating' | 'solve' | 'semiDiameter' | 'conic' | 'a0' | 'a1' | 'a2' | 'a3'

const COLUMNS: { key: ColumnKey; label: string; title: string }[] = [
  { key: 'radius', label: 'Radius', title: 'Radius of curvature (mm); Infinity for a flat surface' },
  { key: 'thickness', label: 'Thickness', title: 'Distance to the next surface (mm)' },
  { key: 'material', label: 'Material', title: 'Glass after the surface: catalog name, nd/vd model, or blank for air' },
  { key: 'coating', label: 'Coating', title: 'Thin-film coating, e.g. MgF2, QW1.38@550, 1.38:99.6, HR@1064, AL; blank for none' },
  { key: 'solve', label: 'Solve', title: 'M [height]: thickness from the marginal ray height (blank height 0 focuses the image). Pr1 [scale] [offset]: copy the radius (r), thickness (t) or conic (k) of surface 1' },
  { key: 'semiDiameter', label: 'Clear Semi-Dia', title: 'Clear semi-aperture (mm); blank for automatic' },
  { key: 'conic', label: 'Conic', title: 'Conic constant k' },
  { key: 'a0', label: 'A4', title: 'Even asphere coefficient of r⁴' },
  { key: 'a1', label: 'A6', title: 'Even asphere coefficient of r⁶' },
  { key: 'a2', label: 'A8', title: 'Even asphere coefficient of r⁸' },
  { key: 'a3', label: 'A10', title: 'Even asphere coefficient of r¹⁰' },
]

const VARIABLE_KEYS = new Set<ColumnKey>(['radius', 'thickness', 'conic', 'a0', 'a1', 'a2', 'a3'])
const term = (key: ColumnKey) => Number(key.slice(1))
const formatCoefficient = (value: number) => value === 0 ? '0' : value.toExponential(6)

// On a coordinate break row the parameter columns hold the break's decenters and tilts.
const CB_COLUMNS: Partial<Record<ColumnKey, { label: string; read: (cb: CoordinateBreak) => number; write: (cb: CoordinateBreak, v: number) => CoordinateBreak }>> = {
  conic: { label: 'Decenter X', read: cb => cb.decenter[0], write: (cb, v) => ({ ...cb, decenter: [v, cb.decenter[1]] }) },
  a0: { label: 'Decenter Y', read: cb => cb.decenter[1], write: (cb, v) => ({ ...cb, decenter: [cb.decenter[0], v] }) },
  a1: { label: 'Tilt X', read: cb => cb.tilt[0], write: (cb, v) => ({ ...cb, tilt: [v, cb.tilt[1], cb.tilt[2]] }) },
  a2: { label: 'Tilt Y', read: cb => cb.tilt[1], write: (cb, v) => ({ ...cb, tilt: [cb.tilt[0], v, cb.tilt[2]] }) },
  a3: { label: 'Tilt Z', read: cb => cb.tilt[2], write: (cb, v) => ({ ...cb, tilt: [cb.tilt[0], cb.tilt[1], v] }) },
}
type CoordinateBreak = NonNullable<LensSystem['surfaces'][number]['coordinateBreak']>

interface Cell { text: string; editable: boolean; invalid?: boolean; note?: string; variable?: boolean; muted?: boolean }

interface Editing { row: number; col: number; text: string; invalid: boolean }

export function LensDataEditor() {
  const { doc, edit, engine, glasses } = useWorkbench()
  const system = doc.system
  const n = system.surfaces.length
  const rowCount = n + 2
  const [selection, setSelection] = useState({ row: 1, col: 0 })
  const [editing, setEditing] = useState<Editing | null>(null)
  const gridRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const overview = engine.overview
  const row = Math.min(selection.row, rowCount - 1)
  const col = selection.col
  const surfaceIndex = row >= 1 && row <= n ? row - 1 : null

  useEffect(() => { if (editing) inputRef.current?.focus() }, [editing?.row, editing?.col])
  useEffect(() => { setSelectedSurface(surfaceIndex) }, [surfaceIndex])

  /** Surface values after the active configuration and solves, as the engine sees them. */
  function resolvedValues(index: number) {
    const s = system.surfaces[index]
    const engineSide = overview?.resolvedSurfaces.length === n ? overview.resolvedSurfaces[index] : undefined
    return engineSide ?? { radius: s.radius, thickness: s.thickness, conic: s.conic ?? 0, material: s.material }
  }

  function cell(r: number, c: number): Cell {
    const key = COLUMNS[c].key
    if (r === 0) {
      if (key === 'radius') return { text: 'Infinity', editable: false, muted: true }
      if (key === 'thickness') return { text: formatFixed(system.objectDistance), editable: true }
      return { text: '', editable: false, muted: true }
    }
    if (r === n + 1) {
      if (key === 'radius') return { text: 'Infinity', editable: false, muted: true }
      if (key === 'semiDiameter') {
        const height = overview?.paraxial.imageHeight
        return { text: height === null || height === undefined ? '' : formatFixed(Math.abs(height)), editable: false, muted: true }
      }
      return { text: '', editable: false, muted: true }
    }
    const surface = system.surfaces[r - 1]
    if (surface.coordinateBreak) {
      if (key === 'thickness') return { text: formatFixed(surface.thickness), editable: true }
      const param = CB_COLUMNS[key]
      return param ? { text: formatFixed(param.read(surface.coordinateBreak)), editable: true } : { text: '', editable: false, muted: true }
    }
    const resolved = resolvedValues(r - 1)
    const locked = solvedParameter(surface.solve) === key
    const controlled = (key === 'radius' || key === 'thickness' || key === 'conic' || key === 'material') && configRowFor(system, r - 1, key) !== undefined
    const note = locked ? 'S' : controlled ? 'M' : undefined
    const variable = VARIABLE_KEYS.has(key) && isVariable(surface, key as VariableKey) && !locked && !controlled
    switch (key) {
      case 'radius': return { text: resolved.radius === 0 ? 'Infinity' : formatFixed(resolved.radius), editable: !locked, variable, note }
      case 'thickness': return { text: formatFixed(resolved.thickness), editable: !locked, variable, note }
      case 'solve': return { text: formatSolve(surface.solve), editable: true }
      case 'material': {
        const name = resolved.material.trim().toUpperCase() === 'AIR' ? '' : resolved.material
        return { text: name, editable: true, invalid: !isKnownMaterial(resolved.material, glasses), note }
      }
      case 'coating': return { text: surface.coating ?? '', editable: true }
      case 'semiDiameter': {
        if (surface.semiDiameter !== undefined) return { text: formatFixed(surface.semiDiameter), editable: true, note: 'U' }
        const auto = overview?.automaticSemiDiameters[r - 1]
        return { text: auto === undefined ? '' : formatFixed(auto), editable: true }
      }
      case 'conic': return { text: formatFixed(resolved.conic), editable: !locked, variable, note }
      default: {
        const value = surface.aspheric?.[term(key)] ?? 0
        return { text: formatCoefficient(value), editable: true, variable, muted: value === 0 && !variable }
      }
    }
  }

  /** Text shown when editing starts: values without padding, blank for automatic apertures. */
  function editText(r: number, c: number): string {
    const key = COLUMNS[c].key
    if (r === 0) return system.objectDistance === null ? 'Infinity' : String(system.objectDistance)
    const surface = system.surfaces[r - 1]
    const param = surface.coordinateBreak && CB_COLUMNS[key]
    if (param && surface.coordinateBreak) return String(param.read(surface.coordinateBreak))
    if (key === 'solve') return formatSolve(surface.solve)
    if (key === 'radius') return resolvedValues(r - 1).radius === 0 ? 'Infinity' : String(resolvedValues(r - 1).radius)
    if (key === 'thickness') return String(resolvedValues(r - 1).thickness)
    if (key === 'material') return cell(r, c).text
    if (key === 'coating') return surface.coating ?? ''
    if (key === 'semiDiameter') return surface.semiDiameter === undefined ? '' : String(surface.semiDiameter)
    if (key === 'conic') return String(resolvedValues(r - 1).conic)
    return String(surface.aspheric?.[term(key)] ?? 0)
  }

  /** Applies a cell's text; returns false when it is not a valid value. */
  function apply(r: number, c: number, text: string): boolean {
    const key = COLUMNS[c].key
    if (r === 0) {
      const value = parseNumber(text)
      if (value === null || value <= 0) return false
      const objectDistance = value === Infinity ? null : value
      edit(s => s.objectDistance === objectDistance ? s : { ...s, objectDistance })
      return true
    }
    const index = r - 1
    const cb = system.surfaces[index].coordinateBreak
    const param = cb && CB_COLUMNS[key]
    if (cb && param) {
      const value = parseNumber(text)
      if (value === null || !Number.isFinite(value)) return false
      edit(s => updateSurface(s, index, { coordinateBreak: param.write(cb, value) }))
      return true
    }
    if (key === 'solve') {
      const solve = parseSolve(text, n)
      if (solve === null || (solve?.kind === 'pickup' && solve.surface === index)) return false
      edit(s => updateSurface(s, index, { solve }))
      return true
    }
    // A parameter in the configuration table is edited in the active configuration.
    if (key === 'radius' || key === 'thickness' || key === 'conic' || key === 'material') {
      const configRow = system.configs?.rows.findIndex(row => row.surface === index && row.parameter === key) ?? -1
      if (configRow >= 0) {
        const value = key === 'material' ? (text.trim().toUpperCase() || 'AIR') : parseNumber(text)
        if (value === null || (typeof value === 'number' && !Number.isFinite(value) && !(key === 'radius' && Math.abs(value) === Infinity))) return false
        edit(s => setConfigValue(s, configRow, activeConfiguration(s), key === 'radius' && typeof value === 'number' && !Number.isFinite(value) ? 0 : value))
        return true
      }
    }
    let patch: Partial<LensSystem['surfaces'][number]>
    if (key === 'material') {
      const material = text.trim().toUpperCase()
      patch = { material: material === '' ? 'AIR' : material }
    } else if (key === 'coating') {
      patch = { coating: text.trim() === '' ? undefined : text.trim() }
    } else if (key === 'semiDiameter') {
      const value = parseNumber(text)
      if (text.trim() === '') patch = { semiDiameter: undefined }
      else if (value === null || !(value > 0) || value === Infinity) return false
      else patch = { semiDiameter: value }
    } else {
      const value = parseNumber(text)
      if (value === null) return false
      if (key === 'radius') patch = { radius: value === Infinity || value === -Infinity ? 0 : value }
      else if (!Number.isFinite(value)) return false
      else if (key === 'thickness') patch = { thickness: value }
      else if (key === 'conic') patch = { conic: value === 0 ? undefined : value }
      else { edit(s => setAspheric(s, index, term(key), value)); return true }
    }
    edit(s => updateSurface(s, index, patch))
    return true
  }

  const move = (dr: number, dc: number) => setSelection({ row: Math.max(0, Math.min(rowCount - 1, row + dr)), col: Math.max(0, Math.min(COLUMNS.length - 1, col + dc)) })

  function startEdit(initial?: string) {
    if (!cell(row, col).editable) return
    setEditing({ row, col, text: initial ?? editText(row, col), invalid: false })
  }

  function finishEdit(next?: [number, number]) {
    if (!editing) return
    if (!apply(editing.row, editing.col, editing.text)) {
      setEditing({ ...editing, invalid: true })
      return
    }
    setEditing(null)
    if (next) move(next[0], next[1])
    gridRef.current?.focus()
  }

  function insert(after: boolean) {
    const at = surfaceIndex === null ? (row === 0 ? 0 : n) : surfaceIndex + (after ? 1 : 0)
    edit(s => insertSurface(s, at))
    setSelection({ row: at + 1, col })
  }

  const remove = () => { if (surfaceIndex !== null) edit(s => deleteSurface(s, surfaceIndex)) }
  const canToggle = surfaceIndex !== null && VARIABLE_KEYS.has(COLUMNS[col].key)
  const toggle = (r = row, c = col) => {
    const key = COLUMNS[c].key
    if (r >= 1 && r <= n && VARIABLE_KEYS.has(key)) edit(s => toggleVariable(s, r - 1, key as VariableKey))
  }
  const makeStop = () => { if (surfaceIndex !== null) edit(s => setStop(s, surfaceIndex)) }

  function onGridKey(event: KeyboardEvent<HTMLDivElement>) {
    if (editing) return
    const mod = event.ctrlKey || event.metaKey
    const keys: Record<string, () => void> = {
      ArrowUp: () => move(-1, 0),
      ArrowDown: () => move(1, 0),
      ArrowLeft: () => move(0, -1),
      ArrowRight: () => move(0, 1),
      Home: () => setSelection({ row, col: 0 }),
      End: () => setSelection({ row, col: COLUMNS.length - 1 }),
      PageUp: () => setSelection({ row: 0, col }),
      PageDown: () => setSelection({ row: rowCount - 1, col }),
      Tab: () => move(0, event.shiftKey ? -1 : 1),
      Enter: () => startEdit(),
      F2: () => startEdit(),
      Insert: () => insert(mod),
      Delete: remove,
    }
    if (mod && event.key.toLowerCase() === 't') { event.preventDefault(); toggle(); return }
    const action = keys[event.key]
    if (action) { event.preventDefault(); action(); return }
    if (event.key.length === 1 && !mod && !event.altKey) { event.preventDefault(); startEdit(event.key) }
  }

  const material = surfaceIndex !== null ? system.surfaces[surfaceIndex].material : ''
  const glass = glasses.find(g => g.name.toUpperCase() === material.trim().toUpperCase())

  return (
    <div className="panel lde">
      <div className="panel-toolbar">
        <button className="tool" title="Insert surface before (Insert)" onClick={() => insert(false)}><i className="codicon codicon-insert" /></button>
        <button className="tool" title="Insert surface after (Ctrl+Insert)" onClick={() => insert(true)}><i className="codicon codicon-add" /></button>
        <button className="tool" title="Delete surface (Delete)" disabled={surfaceIndex === null || n <= 1} onClick={remove}><i className="codicon codicon-trash" /></button>
        <span className="tool-separator" />
        <button className="tool labeled" title="Make the selected surface the aperture stop" disabled={surfaceIndex === null || surfaceIndex === system.stopIndex} onClick={makeStop}>
          <i className="codicon codicon-circle-large" /> Make Stop
        </button>
        <button className="tool labeled" title="Toggle the selected cell as an optimization variable (Ctrl+T)" disabled={!canToggle} onClick={() => toggle()}>
          <i className="codicon codicon-symbol-variable" /> Variable
        </button>
        <button className="tool labeled" title="Turn the selected surface into a coordinate break, or back" disabled={surfaceIndex === null} onClick={() => { if (surfaceIndex !== null) edit(s => toggleCoordinateBreak(s, surfaceIndex)) }}>
          <i className="codicon codicon-debug-step-over" /> Coordinate Break
        </button>
        <span className="toolbar-fill" />
        <span className="toolbar-info">
          {row === 0 ? 'Object' : row === n + 1 ? 'Image' : `Surface ${row}${row - 1 === system.stopIndex ? ' (stop)' : ''}`}
          {glass && <> · {glass.name} · n<sub>d</sub> {glass.nd.toFixed(5)} · v<sub>d</sub> {glass.vd.toFixed(2)}</>}
        </span>
      </div>
      <div className="grid-scroll">
        <div className="grid" role="grid" tabIndex={0} ref={gridRef} onKeyDown={onGridKey} aria-label="Lens data">
          <div className="grid-row header" role="row">
            <div className="grid-cell head label" role="columnheader">Surf</div>
            <div className="grid-cell head type" role="columnheader">Type</div>
            {COLUMNS.map(column => {
              // Like Zemax, parameter headers follow the selected row's surface type.
              const cbLabel = surfaceIndex !== null && system.surfaces[surfaceIndex].coordinateBreak ? CB_COLUMNS[column.key]?.label : undefined
              return <div key={column.key} className={`grid-cell head${cbLabel ? ' cb' : ''}`} role="columnheader" title={column.title}>{cbLabel ?? column.label}</div>
            })}
          </div>
          {Array.from({ length: rowCount }, (_, r) => {
            const isStop = r - 1 === system.stopIndex
            const label = r === 0 ? 'OBJ' : r === n + 1 ? 'IMA' : isStop ? 'STO' : String(r)
            const surface = r >= 1 && r <= n ? system.surfaces[r - 1] : null
            const type = surface?.coordinateBreak ? 'Coord Break' : surface?.aspheric?.some(Boolean) ? 'Even Asphere' : 'Standard'
            return (
              <div key={r} className={`grid-row${r === row ? ' selected' : ''}${r % 2 ? ' alt' : ''}`} role="row">
                <div className={`grid-cell label${isStop ? ' stop' : ''}`} role="rowheader" onMouseDown={() => setSelection({ row: r, col })}>{label}</div>
                <div className="grid-cell type muted" role="gridcell" onMouseDown={() => setSelection({ row: r, col })}>{type}</div>
                {COLUMNS.map((column, c) => {
                  const value = cell(r, c)
                  const active = r === row && c === col
                  if (editing && editing.row === r && editing.col === c) {
                    return (
                      <div key={column.key} className="grid-cell editing" role="gridcell">
                        <input
                          ref={inputRef}
                          className={editing.invalid ? 'invalid' : ''}
                          value={editing.text}
                          list={column.key === 'material' ? 'glass-names' : undefined}
                          spellCheck={false}
                          onChange={event => setEditing({ ...editing, text: event.target.value, invalid: false })}
                          onBlur={() => finishEdit()}
                          onKeyDown={event => {
                            if (event.key === 'Enter') { event.preventDefault(); finishEdit([1, 0]) }
                            else if (event.key === 'Tab') { event.preventDefault(); finishEdit([0, event.shiftKey ? -1 : 1]) }
                            else if (event.key === 'Escape') { event.preventDefault(); setEditing(null); gridRef.current?.focus() }
                          }}
                        />
                      </div>
                    )
                  }
                  return (
                    <div
                      key={column.key}
                      role="gridcell"
                      className={`grid-cell ${column.key === 'material' || column.key === 'coating' || column.key === 'solve' ? 'text' : 'number'}${active ? ' active' : ''}${value.muted ? ' muted' : ''}${value.invalid ? ' invalid' : ''}`}
                      onMouseDown={() => setSelection({ row: r, col: c })}
                      onDoubleClick={() => { setSelection({ row: r, col: c }); if (value.editable) setEditing({ row: r, col: c, text: editText(r, c), invalid: false }) }}
                      title={value.invalid ? 'Unknown material' : undefined}
                    >
                      {value.text}
                      {value.note && <span className="cell-note" title={{ S: 'Set by a solve', M: 'Set by the configuration table; edits change the active configuration' }[value.note] ?? 'Fixed by the user'}>{value.note}</span>}
                      {r >= 1 && r <= n && VARIABLE_KEYS.has(column.key) && !system.surfaces[r - 1].coordinateBreak && (
                        <span
                          className={`solve${value.variable ? ' on' : ''}`}
                          title={value.variable ? 'Variable (click to fix)' : 'Make variable'}
                          onMouseDown={event => { event.stopPropagation(); event.preventDefault() }}
                          onClick={event => { event.stopPropagation(); toggle(r, c) }}
                        >V</span>
                      )}
                    </div>
                  )
                })}
              </div>
            )
          })}
        </div>
        <datalist id="glass-names">{glasses.map(g => <option key={g.name} value={g.name} />)}</datalist>
      </div>
    </div>
  )
}
