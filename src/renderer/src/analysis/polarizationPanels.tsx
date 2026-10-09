import { useState } from 'react'
import { useWorkbench } from '../document'
import { fieldColor, fieldLabel, formatShort } from '../format'
import { Legend, LinePlot } from '../plots/LinePlot'
import { Heatmap } from '../plots/Heatmap'
import { AnalysisFrame, Select, toCsv } from './AnalysisFrame'
import { useFieldOptions, useWavelengthOptions } from './panels'
import { type CoatingCurves, type FieldTransmission, type PolarizationMap, useAnalysis } from './useAnalysis'

type InputKey = 'unpolarized' | 'x' | 'y' | 'right' | 'left'
const INPUTS: Array<{ value: InputKey; label: string }> = [
  { value: 'unpolarized', label: 'Unpolarized' }, { value: 'x', label: 'Linear X' }, { value: 'y', label: 'Linear Y' },
  { value: 'right', label: 'Right circular' }, { value: 'left', label: 'Left circular' },
]
const inputRequest = (key: InputKey) => {
  switch (key) {
    case 'x': return { kind: 'linear' as const, angle: 0 }
    case 'y': return { kind: 'linear' as const, angle: 90 }
    case 'right': return { kind: 'rightCircular' as const }
    case 'left': return { kind: 'leftCircular' as const }
    default: return { kind: 'unpolarized' as const }
  }
}

export function CoatingPanel() {
  const { doc } = useWorkbench()
  const wavelengthOptions = useWavelengthOptions()
  const [surface, setSurface] = useState(0)
  const [wavelength, setWavelength] = useState(doc.system.primaryWavelength)
  const s = Math.min(surface, doc.system.surfaces.length - 1), w = Math.min(wavelength, wavelengthOptions.length - 1)
  const state = useAnalysis<CoatingCurves>({ kind: 'coating', surface: s, wavelength: w, maxAngle: 80, points: 81 })
  const surfaceOptions = doc.system.surfaces.map((_, i) => ({ value: i, label: `Surface ${i + 1}` }))
  return (
    <AnalysisFrame
      state={state}
      name="coating"
      csv={r => toCsv(['angle_deg', 'Rs', 'Rp', 'Ts', 'Tp'], r.angles.map((a, i) => [a, r.rS[i], r.rP[i], r.tS[i], r.tP[i]]))}
      controls={<>
        <Select label="Surface" value={s} options={surfaceOptions} onChange={setSurface} />
        <Select label="Wavelength" value={w} options={wavelengthOptions} onChange={setWavelength} />
      </>}
      legend={<Legend items={[{ label: 'Reflectance', color: 'var(--field-1)' }, { label: 'Transmittance', color: 'var(--field-2)' }, { label: 's', color: 'var(--text-muted)' }, { label: 'p', color: 'var(--text-muted)', dash: '5 3' }]} />}
    >
      {r => (
        <div className="analysis-fill">
          <LinePlot
            xLabel="Angle of incidence (deg)"
            yLabel="Power"
            xDomain={[0, r.angles[r.angles.length - 1]]}
            yDomain={[0, 1]}
            series={[
              { points: r.angles.map((a, i) => [a, r.rS[i]] as [number, number]), color: 'var(--field-1)' },
              { points: r.angles.map((a, i) => [a, r.rP[i]] as [number, number]), color: 'var(--field-1)', dash: '5 3' },
              { points: r.angles.map((a, i) => [a, r.tS[i]] as [number, number]), color: 'var(--field-2)' },
              { points: r.angles.map((a, i) => [a, r.tP[i]] as [number, number]), color: 'var(--field-2)', dash: '5 3' },
            ]}
          />
          <div className="analysis-footer mono">{r.description} · normal incidence R {formatShort(r.rS[0] * 100)}% T {formatShort(r.tS[0] * 100)}%</div>
        </div>
      )}
    </AnalysisFrame>
  )
}

type MapKey = 'transmission' | 'diattenuation' | 'retardance'
const MAPS: Array<{ value: MapKey; label: string; unit: string }> = [
  { value: 'transmission', label: 'Transmission', unit: 'power' },
  { value: 'diattenuation', label: 'Diattenuation', unit: '' },
  { value: 'retardance', label: 'Retardance', unit: 'deg' },
]

export function PolarizationPanel() {
  const fieldOptions = useFieldOptions()
  const wavelengthOptions = useWavelengthOptions()
  const { doc } = useWorkbench()
  const [field, setField] = useState(0)
  const [wavelength, setWavelength] = useState(doc.system.primaryWavelength)
  const [input, setInput] = useState<InputKey>('unpolarized')
  const [map, setMap] = useState<MapKey>('transmission')
  const f = Math.min(field, fieldOptions.length - 1), w = Math.min(wavelength, wavelengthOptions.length - 1)
  const state = useAnalysis<PolarizationMap>({ kind: 'polarization', field: f, wavelength: w, grid: 129, input: inputRequest(input) })
  return (
    <AnalysisFrame
      state={state}
      name="polarization"
      csv={r => toCsv(['row', ...Array.from({ length: r.size }, (_, c) => `x${c}`)], Array.from({ length: r.size }, (_, row) => [row, ...r[map].slice(row * r.size, (row + 1) * r.size)]))}
      controls={<>
        <Select label="Field" value={f} options={fieldOptions} onChange={setField} />
        <Select label="Wavelength" value={w} options={wavelengthOptions} onChange={setWavelength} />
        <Select label="Input" value={input} options={INPUTS} onChange={setInput} />
        <Select label="Map" value={map} options={MAPS} onChange={setMap} />
      </>}
    >
      {r => {
        const values = r[map]
        const finite = values.filter((v): v is number => v !== null && Number.isFinite(v))
        const min = finite.length ? Math.min(...finite) : 0, max = finite.length ? Math.max(...finite) : 1
        const info = MAPS.find(m => m.value === map)!
        return (
          <div className="analysis-fill">
            <Heatmap values={values} size={r.size} min={min} max={max === min ? min + 1 : max} colormap="magma" unit={info.unit} caption="Exit pupil" />
            <div className="analysis-footer mono">
              mean T {r.meanTransmission?.toFixed(4) ?? '—'} · min T {r.minTransmission?.toFixed(4) ?? '—'} · max D {r.maxDiattenuation?.toFixed(4) ?? '—'} · RMS retardance {r.rmsRetardance?.toFixed(3) ?? '—'}°
            </div>
          </div>
        )
      }}
    </AnalysisFrame>
  )
}

export function TransmissionPanel() {
  const wavelengthOptions = useWavelengthOptions()
  const { doc } = useWorkbench()
  const [wavelength, setWavelength] = useState(doc.system.primaryWavelength)
  const [input, setInput] = useState<InputKey>('unpolarized')
  const w = Math.min(wavelength, wavelengthOptions.length - 1)
  const state = useAnalysis<FieldTransmission[]>({ kind: 'transmissionByField', wavelength: w, grid: 33, input: inputRequest(input) })
  return (
    <AnalysisFrame
      state={state}
      name="transmission"
      csv={r => toCsv(['field', 'mean_T', 'min_T', 'max_diattenuation', 'rms_retardance_deg'], r.map(x => [x.field, x.mean, x.min, x.maxDiattenuation, x.rmsRetardance]))}
      controls={<>
        <Select label="Wavelength" value={w} options={wavelengthOptions} onChange={setWavelength} />
        <Select label="Input" value={input} options={INPUTS} onChange={setInput} />
      </>}
      legend={<Legend items={[{ label: 'Mean', color: 'var(--field-1)' }, { label: 'Minimum', color: 'var(--field-1)', dash: '5 3' }]} />}
    >
      {r => (
        <div className="analysis-fill">
          <LinePlot
            xLabel={`Field (${doc.system.fieldType === 'objectHeight' || doc.system.fieldType === 'imageHeight' ? 'mm' : 'deg'})`}
            yLabel="Transmission"
            xDomain={[Math.min(0, ...r.map(x => x.field)), Math.max(1e-9, ...r.map(x => x.field))]}
            yDomain={[0, 1]}
            series={[
              { points: r.map(x => [x.field, x.mean ?? 0] as [number, number]), color: fieldColor(0) },
              { points: r.map(x => [x.field, x.min ?? 0] as [number, number]), color: fieldColor(0), dash: '5 3' },
            ]}
          />
          <div className="analysis-footer mono">{r.map(x => `${fieldLabel(x.field, doc.system.fieldType)}: ${((x.mean ?? 0) * 100).toFixed(1)}%`).join(' · ')}</div>
        </div>
      )}
    </AnalysisFrame>
  )
}
