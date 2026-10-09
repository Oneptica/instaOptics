import { useState } from 'react'
import type { ConfigParameter } from '../../../shared/lens'
import { NumberField } from '../components/NumberField'
import { useWorkbench } from '../document'
import {
  activeConfiguration, addConfigRow, addConfiguration, enableConfigurations, removeConfigRow, removeConfiguration, renameConfiguration, setActiveConfiguration, setConfigValue,
} from '../lensEdit'

const PARAMETERS: Array<{ value: ConfigParameter; label: string }> = [
  { value: 'radius', label: 'Radius' }, { value: 'thickness', label: 'Thickness' }, { value: 'conic', label: 'Conic' }, { value: 'material', label: 'Material' },
  { value: 'decenterX', label: 'Decenter X' }, { value: 'decenterY', label: 'Decenter Y' }, { value: 'tiltX', label: 'Tilt X' }, { value: 'tiltY', label: 'Tilt Y' },
]
const label = (parameter: ConfigParameter) => PARAMETERS.find(p => p.value === parameter)?.label ?? parameter

function TextCell({ value, onCommit }: { value: string; onCommit: (text: string) => void }) {
  const [text, setText] = useState(value)
  const [synced, setSynced] = useState(value)
  if (synced !== value) { setSynced(value); setText(value) }
  return (
    <input
      className="field-input mono"
      value={text}
      spellCheck={false}
      onChange={e => setText(e.target.value)}
      onBlur={() => { const next = text.trim().toUpperCase(); if (next && next !== value) onCommit(next); else setText(value) }}
      onKeyDown={e => { if (e.key === 'Enter') e.currentTarget.blur(); if (e.key === 'Escape') { setText(value); e.currentTarget.blur() } }}
    />
  )
}

/** Multi-configuration editor: rows are surface parameters, columns are configurations (as in OpticStudio's MCE). */
export function ConfigPanel() {
  const { doc, edit } = useWorkbench()
  const system = doc.system
  const configs = system.configs
  const [surface, setSurface] = useState(1)
  const [parameter, setParameter] = useState<ConfigParameter>('thickness')
  const active = activeConfiguration(system)
  const surfaceIndex = Math.min(surface, system.surfaces.length) - 1

  if (!configs) {
    return (
      <div className="panel">
        <div className="placeholder">
          <p>Multi-configuration data lets surface parameters take a different value in each configuration, for zoom lenses, scanning or tolerance cases.</p>
          <button className="tool labeled primary" onClick={() => edit(enableConfigurations)}><i className="codicon codicon-add" /> Add configurations</button>
        </div>
      </div>
    )
  }
  return (
    <div className="panel">
      <div className="panel-toolbar">
        <button className="tool labeled" onClick={() => edit(addConfiguration)}><i className="codicon codicon-add" /> Configuration</button>
        <button className="tool labeled" disabled={configs.names.length <= 1} onClick={() => edit(s => removeConfiguration(s, active))} title="Remove the active configuration"><i className="codicon codicon-remove" /> Configuration</button>
        <span className="tool-separator" />
        <label className="toolbar-label">Surface</label>
        <select value={surfaceIndex + 1} onChange={e => setSurface(Number(e.target.value))} aria-label="Surface">
          {system.surfaces.map((_, i) => <option key={i} value={i + 1}>{i + 1}</option>)}
        </select>
        <select value={parameter} onChange={e => setParameter(e.target.value as ConfigParameter)} aria-label="Parameter">
          {PARAMETERS.map(p => <option key={p.value} value={p.value}>{p.label}</option>)}
        </select>
        <button className="tool labeled" onClick={() => edit(s => addConfigRow(s, surfaceIndex, parameter))}><i className="codicon codicon-add" /> Row</button>
        <span className="toolbar-fill" />
        <span className="toolbar-info mono">Active: {configs.names[active]}</span>
      </div>
      <div className="table-scroll">
        <table className="data-table mono config-table">
          <thead>
            <tr>
              <th>Surf</th><th>Parameter</th>
              {configs.names.map((name, k) => (
                <th key={k} className={k === active ? 'active' : undefined}>
                  <button className="link-button" onClick={() => edit(s => setActiveConfiguration(s, k))} title="Make this the active configuration">{k === active ? '●' : '○'}</button>
                  <input className="field-input name-input" aria-label={`Configuration ${k + 1} name`} value={name} spellCheck={false} onChange={e => edit(s => renameConfiguration(s, k, e.target.value))} />
                </th>
              ))}
              <th />
            </tr>
          </thead>
          <tbody>
            {configs.rows.length === 0 && <tr><td colSpan={configs.names.length + 3} className="left muted">Choose a surface and parameter above, then add a row.</td></tr>}
            {configs.rows.map((row, r) => (
              <tr key={`${row.surface}-${row.parameter}`}>
                <td>{row.surface + 1}</td>
                <td className="left">{label(row.parameter)}</td>
                {configs.names.map((_, k) => {
                  const value = row.values[k]
                  return (
                    <td key={k} className={k === active ? 'active' : undefined}>
                      {row.parameter === 'material'
                        ? <TextCell value={String(value ?? '')} onCommit={text => edit(s => setConfigValue(s, r, k, text))} />
                        : <NumberField ariaLabel={`${label(row.parameter)} of surface ${row.surface + 1} in configuration ${k + 1}`} value={typeof value === 'number' ? value : 0} onCommit={v => v !== null && edit(s => setConfigValue(s, r, k, v))} />}
                    </td>
                  )
                })}
                <td><button className="tool" title="Remove row" onClick={() => edit(s => removeConfigRow(s, r))}><i className="codicon codicon-trash" /></button></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
