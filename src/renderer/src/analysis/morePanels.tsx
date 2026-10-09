import { useState } from 'react'
import { useWorkbench } from '../document'
import { fieldColor, fieldLabel, formatShort } from '../format'
import { Legend, LinePlot, useSize } from '../plots/LinePlot'
import { useZoomPan } from '../plots/useZoomPan'
import { AnalysisFrame, Select, toCsv } from './AnalysisFrame'
import { type ChromaticFocalShift, type Footprint, type MtfVsField, type ThroughFocus, useAnalysis } from './useAnalysis'

const TS = [{ label: 'T', color: 'var(--text-muted)' }, { label: 'S', color: 'var(--text-muted)', dash: '5 3' }]

export function ThroughFocusPanel() {
  const { doc } = useWorkbench()
  const [frequency, setFrequency] = useState(50)
  const [range, setRange] = useState(0.2)
  const state = useAnalysis<ThroughFocus>({ kind: 'throughFocus', frequency, range, steps: 61 })
  return (
    <AnalysisFrame
      state={state}
      name="through-focus-mtf"
      csv={r => toCsv(['shift_mm', ...r.fields.flatMap(f => [`T_${f.field}`, `S_${f.field}`])], r.shifts.map((z, i) => [z, ...r.fields.flatMap(f => [f.tangential[i], f.sagittal[i]])]))}
      controls={<>
        <Select label="Frequency" value={frequency} options={[10, 20, 30, 50, 80, 100, 150, 200].map(v => ({ value: v, label: `${v} lp/mm` }))} onChange={setFrequency} />
        <Select label="Range" value={range} options={[0.05, 0.1, 0.2, 0.5, 1].map(v => ({ value: v, label: `±${v} mm` }))} onChange={setRange} />
      </>}
      legend={<Legend items={[...doc.system.fields.map((f, i) => ({ label: fieldLabel(f, doc.system.fieldType), color: fieldColor(i) })), ...TS]} />}
    >
      {r => (
        <div className="analysis-fill">
          <LinePlot
            xLabel="Focus shift (mm)"
            yLabel={`MTF at ${formatShort(r.frequency)} lp/mm`}
            xDomain={[r.shifts[0], r.shifts[r.shifts.length - 1]]}
            yDomain={[0, 1]}
            series={r.fields.flatMap((f, i) => [
              { points: r.shifts.map((z, k) => [z, f.tangential[k]] as [number, number]), color: fieldColor(i) },
              { points: r.shifts.map((z, k) => [z, f.sagittal[k]] as [number, number]), color: fieldColor(i), dash: '5 3' },
            ])}
          />
        </div>
      )}
    </AnalysisFrame>
  )
}

const FREQUENCY_SETS: Record<string, number[]> = { '10, 30, 50': [10, 30, 50], '20, 40, 80': [20, 40, 80], '50, 100, 150': [50, 100, 150] }
const FREQUENCY_COLORS = ['var(--field-1)', 'var(--field-2)', 'var(--field-3)', 'var(--field-4)']

export function MtfVsFieldPanel() {
  const [set, setSet] = useState('10, 30, 50')
  const frequencies = FREQUENCY_SETS[set]
  const state = useAnalysis<MtfVsField>({ kind: 'mtfVsField', frequencies, samples: 41 })
  return (
    <AnalysisFrame
      state={state}
      name="mtf-vs-field"
      csv={r => toCsv(['field_deg', ...r.curves.flatMap(c => [`T_${c.field}`, `S_${c.field}`])], r.angles.map((a, i) => [a, ...r.curves.flatMap(c => [c.tangential[i], c.sagittal[i]])]))}
      controls={<Select label="Frequencies" value={set} options={Object.keys(FREQUENCY_SETS).map(k => ({ value: k, label: `${k} lp/mm` }))} onChange={setSet} />}
      legend={<Legend items={[...frequencies.map((f, i) => ({ label: `${f} lp/mm`, color: FREQUENCY_COLORS[i] })), ...TS]} />}
    >
      {r => (
        <div className="analysis-fill">
          <LinePlot
            xLabel="Field angle (deg)"
            yLabel="Modulus of the OTF"
            xDomain={[0, r.angles[r.angles.length - 1] || 1]}
            yDomain={[0, 1]}
            series={r.curves.flatMap((c, i) => [
              { points: r.angles.map((a, k) => [a, c.tangential[k]] as [number, number]), color: FREQUENCY_COLORS[i] },
              { points: r.angles.map((a, k) => [a, c.sagittal[k]] as [number, number]), color: FREQUENCY_COLORS[i], dash: '5 3' },
            ])}
          />
        </div>
      )}
    </AnalysisFrame>
  )
}

export function ChromaticFocalShiftPanel() {
  const state = useAnalysis<ChromaticFocalShift>({ kind: 'chromaticFocalShift', samples: 81 })
  return (
    <AnalysisFrame
      state={state}
      name="chromatic-focal-shift"
      csv={r => toCsv(['wavelength_um', 'shift_um'], r.wavelengths.map((w, i) => [w, r.shift[i]]))}
    >
      {r => (
        <div className="analysis-fill">
          <LinePlot
            vertical
            xLabel="Wavelength (µm)"
            yLabel="Focal shift (µm)"
            xDomain={[r.wavelengths[0], r.wavelengths[r.wavelengths.length - 1]]}
            symmetricY
            series={[{ points: r.wavelengths.map((w, i) => [w, r.shift[i]]), color: 'var(--field-1)', width: 1.6 }]}
          />
          <div className="analysis-footer mono">Paraxial focus relative to the primary wavelength · maximum shift range {r.range.toFixed(3)} µm</div>
        </div>
      )}
    </AnalysisFrame>
  )
}

function FootprintPlot({ footprint }: { footprint: Footprint }) {
  const [ref, { width, height }] = useSize<HTMLDivElement>()
  const side = Math.max(60, Math.min(width, height) - 24)
  const half = Math.max(footprint.semiDiameter, ...footprint.fields.flat().map(p => Math.max(Math.abs(p[0]), Math.abs(p[1])))) * 1.08 || 1
  const { ref: svgRef, view, handlers } = useZoomPan<SVGSVGElement>()
  const s = side / 2 / half * view.k
  const cx = side / 2 + view.ox, cy = side / 2 + view.oy
  return (
    <div className="footprint" ref={ref}>
      {width > 0 && (
        <svg width={side} height={side} ref={svgRef} {...handlers} style={{ touchAction: 'none', cursor: view.k > 1 ? 'grab' : 'crosshair' }}>
          <svg width={side} height={side} overflow="hidden">
            <line x1={cx} x2={cx} y1={0} y2={side} className="cross" />
            <line x1={0} x2={side} y1={cy} y2={cy} className="cross" />
            <circle cx={cx} cy={cy} r={footprint.semiDiameter * s} className="aperture" />
            {footprint.fields.map((points, f) => points.map(([x, y], i) => (
              <circle key={`${f}-${i}`} cx={cx + x * s} cy={cy - y * s} r={1.6} fill={fieldColor(f)} />
            )))}
          </svg>
          <rect x={0.5} y={0.5} width={side - 1} height={side - 1} className="frame" />
        </svg>
      )}
    </div>
  )
}

export function FootprintPanel() {
  const { doc } = useWorkbench()
  const [surface, setSurface] = useState(0)
  const n = doc.system.surfaces.length
  const index = Math.min(surface, n - 1)
  const state = useAnalysis<Footprint>({ kind: 'footprint', surface: index, rings: 8 })
  return (
    <AnalysisFrame
      state={state}
      name="footprint"
      csv={r => toCsv(['field', 'x_mm', 'y_mm'], r.fields.flatMap((points, f) => points.map(([x, y]) => [doc.system.fields[f], x, y])))}
      controls={<Select label="Surface" value={index} options={doc.system.surfaces.map((_, i) => ({ value: i, label: i === doc.system.stopIndex ? `${i + 1} (stop)` : String(i + 1) }))} onChange={setSurface} />}
      legend={<Legend items={doc.system.fields.map((f, i) => ({ label: fieldLabel(f, doc.system.fieldType), color: fieldColor(i) }))} />}
    >
      {r => (
        <div className="analysis-fill">
          <FootprintPlot footprint={r} />
          <div className="analysis-footer mono">Surface {r.surface + 1} · clear semi-diameter {r.semiDiameter.toFixed(3)} mm (circle) · primary wavelength</div>
        </div>
      )}
    </AnalysisFrame>
  )
}
