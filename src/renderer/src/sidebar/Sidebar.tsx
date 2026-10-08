import { useState, type ReactNode } from 'react'
import type { ApertureType, FieldType, LensSystem, Sample } from '../../../shared/lens'
import { useWorkbench } from '../document'
import { WINDOWS } from '../../../shared/windows'
import { NumberField } from '../components/NumberField'

export type SidebarView = 'system' | 'samples' | 'windows'

export const SIDEBAR_VIEWS: { id: SidebarView; title: string; icon: string }[] = [
  { id: 'system', title: 'System Explorer', icon: 'list-tree' },
  { id: 'samples', title: 'Sample Lenses', icon: 'library' },
  { id: 'windows', title: 'Analysis', icon: 'graph-line' },
]

function Section({ title, children, actions }: { title: string; children: ReactNode; actions?: ReactNode }) {
  const [open, setOpen] = useState(true)
  return (
    <section className="side-section">
      <header className="side-section-header" onClick={() => setOpen(!open)}>
        <i className={`codicon codicon-chevron-${open ? 'down' : 'right'}`} />
        <span>{title}</span>
        {open && actions && <span className="side-section-actions" onClick={event => event.stopPropagation()}>{actions}</span>}
      </header>
      {open && <div className="side-section-body">{children}</div>}
    </section>
  )
}

const FDC = [0.4861327, 0.5875618, 0.6562725]

const APERTURE_TYPES: Array<{ value: ApertureType; label: string; unit: string }> = [
  { value: 'entrancePupilDiameter', label: 'Entrance pupil diameter', unit: 'mm' },
  { value: 'imageFNumber', label: 'Image space F/#', unit: '' },
  { value: 'workingFNumber', label: 'Paraxial working F/#', unit: '' },
  { value: 'objectNa', label: 'Object space NA', unit: '' },
  { value: 'floatByStop', label: 'Float by stop size', unit: 'mm' },
]

const FIELD_TYPES: Array<{ value: FieldType; label: string }> = [
  { value: 'angle', label: 'Angle (deg)' },
  { value: 'objectHeight', label: 'Object height (mm)' },
  { value: 'imageHeight', label: 'Paraxial image height (mm)' },
]

function SystemExplorer() {
  const { doc, edit, engine } = useWorkbench()
  const system = doc.system
  const set = (patch: Partial<LensSystem>) => edit(s => ({ ...s, ...patch }))
  const apertureType = system.apertureType ?? 'entrancePupilDiameter'
  const fieldType = system.fieldType ?? 'angle'
  // Switching type starts from the value the current system already has, so the lens does not jump.
  const setAperture = (type: ApertureType) => {
    const o = engine.overview
    const epd = o?.entrancePupilDiameter ?? system.entrancePupilDiameter
    const current: Record<ApertureType, number | null | undefined> = {
      entrancePupilDiameter: epd,
      imageFNumber: o?.paraxial.fNumber,
      workingFNumber: o?.paraxial.workingFNumber,
      objectNa: system.objectDistance !== null && o?.paraxial.entrancePupilZ != null ? Math.sin(Math.atan(epd / 2 / (system.objectDistance + o.paraxial.entrancePupilZ))) : 0.1,
      floatByStop: o?.paraxial.stopSemiDiameter,
    }
    const value = current[type]
    set({ apertureType: type, entrancePupilDiameter: epd, apertureValue: type === 'entrancePupilDiameter' ? undefined : Number((value ?? 1).toPrecision(6)) })
  }
  const replaceAt = <T,>(list: T[], index: number, value: T) => list.map((item, i) => i === index ? value : item)

  return (
    <>
      <Section title="Aperture">
        <div className="prop-row">
          <select className="field-select" aria-label="Aperture type" value={apertureType} onChange={event => setAperture(event.target.value as ApertureType)}>
            {APERTURE_TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
          </select>
        </div>
        <div className="prop-row">
          <label>{APERTURE_TYPES.find(t => t.value === apertureType)!.label}</label>
          {apertureType === 'entrancePupilDiameter'
            ? <NumberField ariaLabel="Entrance pupil diameter" value={system.entrancePupilDiameter} min={0} onCommit={v => v !== null && v !== system.entrancePupilDiameter && set({ entrancePupilDiameter: v })} />
            : <NumberField ariaLabel="Aperture value" value={system.apertureValue ?? null} optional min={0} onCommit={v => v !== null && set({ apertureValue: v })} />}
          <span className="unit">{APERTURE_TYPES.find(t => t.value === apertureType)!.unit}</span>
        </div>
        {apertureType !== 'entrancePupilDiameter' && engine.overview && (
          <div className="prop-caption">Entrance pupil diameter {engine.overview.entrancePupilDiameter.toFixed(4)} mm</div>
        )}
      </Section>
      <Section title="Fields" actions={<button className="icon-button" title="Add field" onClick={() => set({ fields: [...system.fields, system.fields[system.fields.length - 1] ?? 0] })}><i className="codicon codicon-add" /></button>}>
        <div className="prop-row">
          <select className="field-select" aria-label="Field type" value={fieldType} onChange={event => set({ fieldType: event.target.value as FieldType })}>
            {FIELD_TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
          </select>
        </div>
        {system.fields.map((field, i) => (
          <div className="prop-row list" key={`${i}-${system.fields.length}`}>
            <span className="index" style={{ color: `var(--field-${(i % 6) + 1})` }}>{i + 1}</span>
            <NumberField ariaLabel={`Field ${i + 1}`} value={field} onCommit={v => v !== null && v !== field && set({ fields: replaceAt(system.fields, i, v) })} />
            <button className="icon-button" title="Remove field" disabled={system.fields.length <= 1} onClick={() => set({ fields: system.fields.filter((_, j) => j !== i) })}><i className="codicon codicon-close" /></button>
          </div>
        ))}
      </Section>
      <Section title="Wavelengths" actions={<>
        <button className="icon-button text" title="Use F, d, C lines" onClick={() => set({ wavelengths: FDC, primaryWavelength: 1 })}>F,d,C</button>
        <button className="icon-button" title="Add wavelength" onClick={() => set({ wavelengths: [...system.wavelengths, 0.55] })}><i className="codicon codicon-add" /></button>
      </>}>
        <div className="prop-caption">Wavelength (µm) · primary</div>
        {system.wavelengths.map((wavelength, i) => (
          <div className="prop-row list" key={`${i}-${system.wavelengths.length}`}>
            <span className="index">{i + 1}</span>
            <NumberField ariaLabel={`Wavelength ${i + 1}`} value={wavelength} min={0} onCommit={v => v !== null && v !== wavelength && set({ wavelengths: replaceAt(system.wavelengths, i, v) })} />
            <input type="radio" name="primary-wavelength" title="Primary wavelength" checked={i === system.primaryWavelength} onChange={() => set({ primaryWavelength: i })} />
            <button
              className="icon-button"
              title="Remove wavelength"
              disabled={system.wavelengths.length <= 1}
              onClick={() => set({
                wavelengths: system.wavelengths.filter((_, j) => j !== i),
                primaryWavelength: system.primaryWavelength > i ? system.primaryWavelength - 1 : Math.min(system.primaryWavelength, system.wavelengths.length - 2),
              })}
            ><i className="codicon codicon-close" /></button>
          </div>
        ))}
      </Section>
      <Section title="Object">
        <div className="prop-row">
          <label className="check"><input type="checkbox" checked={system.objectDistance === null} onChange={event => set({ objectDistance: event.target.checked ? null : 1000 })} /> Object at infinity</label>
        </div>
        {system.objectDistance !== null && (
          <div className="prop-row">
            <label>Object Distance</label>
            <NumberField ariaLabel="Object distance" value={system.objectDistance} min={0} allowInfinity onCommit={v => v !== system.objectDistance && set({ objectDistance: v })} />
            <span className="unit">mm</span>
          </div>
        )}
      </Section>
      <Section title="Ray Aiming">
        <div className="prop-row">
          <label className="check"><input type="checkbox" checked={system.rayAiming} onChange={event => set({ rayAiming: event.target.checked })} /> Aim real rays at the stop</label>
        </div>
      </Section>
    </>
  )
}

function Samples({ samples, onOpen }: { samples: Sample[]; onOpen: (sample: Sample) => void }) {
  return (
    <Section title="Built-in">
      {samples.map(sample => (
        <button key={sample.id} className="list-item" onClick={() => onOpen(sample)} title={`Open ${sample.system.name}`}>
          <i className="codicon codicon-circle-large-outline" />
          <span className="list-item-label">{sample.system.name}</span>
          <span className="list-item-detail">{sample.system.surfaces.length} surf</span>
        </button>
      ))}
    </Section>
  )
}

function Windows({ onOpen }: { onOpen: (id: string) => void }) {
  const groups = [...new Set(WINDOWS.map(window => window.group))].filter(group => group !== 'Help')
  return (
    <>
      {groups.map(group => (
        <Section key={group} title={group}>
          {WINDOWS.filter(window => window.group === group).map(window => (
            <button key={window.id} className="list-item" onClick={() => onOpen(window.id)}>
              <i className={`codicon codicon-${window.icon}`} />
              <span className="list-item-label">{window.title}</span>
            </button>
          ))}
        </Section>
      ))}
    </>
  )
}

export function Sidebar({ view, samples, onOpenSample, onOpenWindow }: {
  view: SidebarView; samples: Sample[]; onOpenSample: (sample: Sample) => void; onOpenWindow: (id: string) => void
}) {
  const title = SIDEBAR_VIEWS.find(v => v.id === view)!.title
  return (
    <aside className="sidebar">
      <div className="sidebar-title">{title}</div>
      <div className="sidebar-body">
        {view === 'system' && <SystemExplorer />}
        {view === 'samples' && <Samples samples={samples} onOpen={onOpenSample} />}
        {view === 'windows' && <Windows onOpen={onOpenWindow} />}
      </div>
    </aside>
  )
}
