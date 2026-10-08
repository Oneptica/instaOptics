import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import type { LensSystem } from '../../../shared/lens'
import { useWorkbench } from '../document'
import { formatFixed } from '../format'
import {
  type VariableKey, deleteSurface, insertSurface, isKnownMaterial, isVariable, parseNumber, setAspheric, setStop, toggleVariable, updateSurface,
} from '../lensEdit'

type ColumnKey = 'radius' | 'thickness' | 'material' | 'semiDiameter' | 'conic' | 'a0' | 'a1' | 'a2' | 'a3'

const COLUMNS: { key: ColumnKey; label: string; title: string }[] = [
  { key: 'radius', label: 'Radius', title: 'Radius of curvature (mm); Infinity for a flat surface' },
  { key: 'thickness', label: 'Thickness', title: 'Distance to the next surface (mm)' },
  { key: 'material', label: 'Material', title: 'Glass after the surface: catalog name, nd/vd model, or blank for air' },
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
    const variable = VARIABLE_KEYS.has(key) && isVariable(surface, key as VariableKey)
    switch (key) {
      case 'radius': return { text: surface.radius === 0 ? 'Infinity' : formatFixed(surface.radius), editable: true, variable }
      case 'thickness': return { text: formatFixed(surface.thickness), editable: true, variable }
      case 'material': {
        const name = surface.material.trim().toUpperCase() === 'AIR' ? '' : surface.material
        return { text: name, editable: true, invalid: !isKnownMaterial(surface.material, glasses) }
      }
      case 'semiDiameter': {
        if (surface.semiDiameter !== undefined) return { text: formatFixed(surface.semiDiameter), editable: true, note: 'U' }
        const auto = overview?.automaticSemiDiameters[r - 1]
        return { text: auto === undefined ? '' : formatFixed(auto), editable: true }
      }
      case 'conic': return { text: formatFixed(surface.conic ?? 0), editable: true, variable }
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
    if (key === 'radius') return surface.radius === 0 ? 'Infinity' : String(surface.radius)
    if (key === 'thickness') return String(surface.thickness)
    if (key === 'material') return cell(r, c).text
    if (key === 'semiDiameter') return surface.semiDiameter === undefined ? '' : String(surface.semiDiameter)
    if (key === 'conic') return String(surface.conic ?? 0)
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
    let patch: Partial<LensSystem['surfaces'][number]>
    if (key === 'material') {
      const material = text.trim().toUpperCase()
      patch = { material: material === '' ? 'AIR' : material }
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
            {COLUMNS.map(column => <div key={column.key} className="grid-cell head" role="columnheader" title={column.title}>{column.label}</div>)}
          </div>
          {Array.from({ length: rowCount }, (_, r) => {
            const isStop = r - 1 === system.stopIndex
            const label = r === 0 ? 'OBJ' : r === n + 1 ? 'IMA' : isStop ? 'STO' : String(r)
            const surface = r >= 1 && r <= n ? system.surfaces[r - 1] : null
            const type = surface?.aspheric?.some(Boolean) ? 'Even Asphere' : 'Standard'
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
                      className={`grid-cell ${column.key === 'material' ? 'text' : 'number'}${active ? ' active' : ''}${value.muted ? ' muted' : ''}${value.invalid ? ' invalid' : ''}`}
                      onMouseDown={() => setSelection({ row: r, col: c })}
                      onDoubleClick={() => { setSelection({ row: r, col: c }); if (value.editable) setEditing({ row: r, col: c, text: editText(r, c), invalid: false }) }}
                      title={value.invalid ? 'Unknown material' : undefined}
                    >
                      {value.text}
                      {value.note && <span className="cell-note" title="Fixed by the user">{value.note}</span>}
                      {r >= 1 && r <= n && VARIABLE_KEYS.has(column.key) && (
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
