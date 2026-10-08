import { useMemo, useState } from 'react'
import { useWorkbench } from '../document'
import { fieldColor, formatShort } from '../format'
import { Heatmap } from '../plots/Heatmap'
import { Legend, LinePlot, type Series, niceTicks, useSize } from '../plots/LinePlot'
import { AnalysisFrame, Select, wavelengthColor } from './AnalysisFrame'
import {
  type FieldCurves, type Illumination, type MtfResult, type PsfResult, type RayFanField, type SeidelResult, type SpotField,
  type WavefrontMap, useAnalysis,
} from './useAnalysis'

const um = (mm: number | null | undefined) => mm === null || mm === undefined || !Number.isFinite(mm) ? '—' : (mm * 1000).toFixed(3)

function useFieldOptions() {
  const { doc } = useWorkbench()
  return doc.system.fields.map((field, i) => ({ value: i, label: `${i + 1}: ${formatShort(field)}°` }))
}

function useWavelengthOptions() {
  const { doc } = useWorkbench()
  return doc.system.wavelengths.map((wavelength, i) => ({ value: i, label: `${i + 1}: ${wavelength.toFixed(4)} µm` }))
}

function WavelengthLegend() {
  const { doc } = useWorkbench()
  return <Legend items={doc.system.wavelengths.map(w => ({ label: `${w.toFixed(4)} µm`, color: wavelengthColor(w) }))} />
}

// ---------- spot diagram ----------

function SpotCell({ spot, half, airy, colors }: { spot: SpotField; half: number; airy: number; colors: string[] }) {
  const [ref, { width }] = useSize<HTMLDivElement>()
  const side = Math.max(60, Math.min(width, 320))
  const scale = side / 2 / half
  return (
    <div className="spot-cell" ref={ref}>
      <svg width={side} height={side} className="spot-plot">
        <rect x={0.5} y={0.5} width={side - 1} height={side - 1} className="frame" />
        <line x1={side / 2} x2={side / 2} y1={0} y2={side} className="cross" />
        <line x1={0} x2={side} y1={side / 2} y2={side / 2} className="cross" />
        {airy > 0 && airy * scale > 1 && <circle cx={side / 2} cy={side / 2} r={airy * scale} className="airy" />}
        {spot.points.map(([x, y, w], i) => (
          <rect key={i} x={side / 2 + x * scale - 1} y={side / 2 - y * scale - 1} width={2} height={2} fill={colors[w]} />
        ))}
      </svg>
      <div className="spot-caption mono">
        <div>Field {formatShort(spot.field)}° · y = {spot.reference[1].toFixed(4)} mm</div>
        <div>RMS {um(spot.rmsRadius)} µm · GEO {um(spot.geoRadius)} µm</div>
        {spot.traced < spot.total && <div className="warn">{spot.total - spot.traced} of {spot.total} rays vignetted</div>}
      </div>
    </div>
  )
}

export function SpotPanel() {
  const { doc, engine } = useWorkbench()
  const [rings, setRings] = useState(6)
  const state = useAnalysis<SpotField[]>({ kind: 'spot', rings })
  const working = engine.overview?.paraxial.workingFNumber ?? null
  const primary = doc.system.wavelengths[doc.system.primaryWavelength] ?? 0.55
  const airy = working ? 1.22 * primary * 1e-3 * working : 0
  const colors = doc.system.wavelengths.map(wavelengthColor)
  return (
    <AnalysisFrame
      state={state}
      controls={<Select label="Rings" value={rings} options={[3, 4, 6, 8, 10, 12, 16].map(v => ({ value: v, label: String(v) }))} onChange={setRings} />}
      legend={<WavelengthLegend />}
    >
      {spots => {
        const largest = Math.max(...spots.map(s => s.geoRadius), airy, 1e-6)
        const box = niceTicks(0, largest * 1.05, 2).find(t => t >= largest * 1.05) ?? largest * 1.2
        return (
          <div className="spot-grid-host">
            <div className="spot-grid">{spots.map((spot, i) => <SpotCell key={i} spot={spot} half={box} airy={airy} colors={colors} />)}</div>
            <div className="analysis-footer mono">Box width {um(2 * box)} µm · Airy radius {um(airy)} µm · reference: primary chief ray</div>
          </div>
        )
      }}
    </AnalysisFrame>
  )
}

// ---------- ray fan ----------

export function RayFanPanel() {
  const { doc } = useWorkbench()
  const state = useAnalysis<RayFanField[]>({ kind: 'rayFan', samples: 61 })
  return (
    <AnalysisFrame state={state} legend={<WavelengthLegend />}>
      {fans => {
        const all = fans.flatMap(f => [...f.tangential, ...f.sagittal].flatMap(c => c.error)).map(Math.abs)
        const limit = Math.max(...all, 1e-6) * 1000 * 1.05
        const series = (curves: RayFanField['tangential']): Series[] => curves.map(c => ({
          points: c.pupil.map((p, i) => [p, c.error[i] * 1000] as [number, number]),
          color: wavelengthColor(c.wavelength),
        }))
        return (
          <div className="fan-grid">
            {fans.map((fan, i) => (
              <div className="fan-row" key={i}>
                <div className="fan-label mono">{formatShort(doc.system.fields[i] ?? fan.field)}°</div>
                <LinePlot title="Tangential" series={series(fan.tangential)} xLabel="Py" yLabel="ey (µm)" xDomain={[-1, 1]} yDomain={[-limit, limit]} />
                <LinePlot title="Sagittal" series={series(fan.sagittal)} xLabel="Px" yLabel="ex (µm)" xDomain={[-1, 1]} yDomain={[-limit, limit]} />
              </div>
            ))}
          </div>
        )
      }}
    </AnalysisFrame>
  )
}

// ---------- MTF ----------

export function MtfPanel() {
  const { doc } = useWorkbench()
  const [maxFrequency, setMaxFrequency] = useState<number>(0)
  const state = useAnalysis<MtfResult>({ kind: 'mtf', size: 64, points: 101, maxFrequency: maxFrequency || null })
  const fields = doc.system.fields
  return (
    <AnalysisFrame
      state={state}
      controls={<Select label="Max freq." value={maxFrequency} options={[0, 10, 20, 50, 100, 200, 500].map(v => ({ value: v, label: v ? `${v} lp/mm` : 'Cutoff' }))} onChange={setMaxFrequency} />}
      legend={<Legend items={[
        ...fields.map((f, i) => ({ label: `${formatShort(f)}°`, color: fieldColor(i) })),
        { label: 'T', color: 'var(--text-muted)' }, { label: 'S', color: 'var(--text-muted)', dash: '4 3' },
        { label: 'Diffraction', color: 'var(--text-strong)', dash: '1 3' },
      ]} />}
    >
      {result => (
        <div className="analysis-fill">
          <LinePlot
            xLabel="Spatial frequency (cycles/mm)"
            yLabel="Modulus of the OTF"
            xDomain={[0, result.frequencies[result.frequencies.length - 1]]}
            yDomain={[0, 1]}
            series={[
              { points: result.frequencies.map((f, i) => [f, result.diffraction[i]]), color: 'var(--text-strong)', dash: '1 3', width: 1.2 },
              ...result.fields.flatMap((field, i) => [
                { points: result.frequencies.map((f, k) => [f, field.tangential[k]] as [number, number]), color: fieldColor(i) },
                { points: result.frequencies.map((f, k) => [f, field.sagittal[k]] as [number, number]), color: fieldColor(i), dash: '5 3' },
              ]),
            ]}
          />
          <div className="analysis-footer mono">Polychromatic diffraction MTF · cutoff {result.cutoff.toFixed(1)} cycles/mm</div>
        </div>
      )}
    </AnalysisFrame>
  )
}

// ---------- PSF ----------

export function PsfPanel() {
  const fieldOptions = useFieldOptions()
  const [field, setField] = useState(0)
  const [scale, setScale] = useState<'linear' | 'log'>('linear')
  const state = useAnalysis<PsfResult>({ kind: 'psf', field: Math.min(field, fieldOptions.length - 1), samples: 64, padding: 4, crop: 64 })
  const log = scale === 'log'
  const transform = useMemo(() => log ? (v: number) => Math.log10(Math.max(v, 1e-5)) : undefined, [log])
  return (
    <AnalysisFrame
      state={state}
      controls={<>
        <Select label="Field" value={Math.min(field, fieldOptions.length - 1)} options={fieldOptions} onChange={setField} />
        <Select label="Scale" value={scale} options={[{ value: 'linear', label: 'Linear' }, { value: 'log', label: 'Log' }]} onChange={setScale} />
      </>}
    >
      {psf => (
        <div className="split-row">
          <div className="split-cell">
            <Heatmap
              values={psf.data}
              size={psf.size}
              min={log ? -5 : 0}
              max={log ? 0 : 1}
              colormap="inferno"
              unit={log ? 'log₁₀ I' : 'I / Iₘₐₓ'}
              transform={transform}
              caption={`${um(psf.size * psf.spacing)} µm wide · Strehl ${psf.strehl.toFixed(3)}`}
            />
          </div>
          <div className="split-cell">
            <LinePlot
              title="Encircled energy"
              xLabel="Radius (µm)"
              yLabel="Fraction of energy"
              yDomain={[0, 1]}
              xDomain={[0, psf.radius[Math.min(psf.radius.length - 1, Math.round(psf.radius.length / 4))] * 1000]}
              series={[
                { points: psf.radius.map((r, i) => [r * 1000, psf.diffraction[i]]), color: 'var(--text-strong)', dash: '1 3' },
                { points: psf.radius.map((r, i) => [r * 1000, psf.energy[i]]), color: 'var(--field-1)' },
              ]}
            />
          </div>
        </div>
      )}
    </AnalysisFrame>
  )
}

// ---------- wavefront ----------

export function WavefrontPanel() {
  const fieldOptions = useFieldOptions()
  const wavelengthOptions = useWavelengthOptions()
  const { doc } = useWorkbench()
  const [field, setField] = useState(0)
  const [wavelength, setWavelength] = useState(doc.system.primaryWavelength)
  const f = Math.min(field, fieldOptions.length - 1), w = Math.min(wavelength, wavelengthOptions.length - 1)
  const state = useAnalysis<WavefrontMap>({ kind: 'wavefront', field: f, wavelength: w, size: 96 })
  return (
    <AnalysisFrame
      state={state}
      controls={<>
        <Select label="Field" value={f} options={fieldOptions} onChange={setField} />
        <Select label="Wavelength" value={w} options={wavelengthOptions} onChange={setWavelength} />
      </>}
    >
      {map => {
        const finite = map.values.filter((v): v is number => v !== null && Number.isFinite(v))
        const min = finite.length ? Math.min(...finite) : 0, max = finite.length ? Math.max(...finite) : 1
        return (
          <div className="analysis-fill">
            <Heatmap values={map.values} size={map.size} min={min} max={max === min ? min + 1 : max} colormap="jet" unit="waves" caption="Exit pupil" />
            <div className="analysis-footer mono">P-V {map.pv?.toFixed(4) ?? '—'} waves · RMS {map.rms?.toFixed(4) ?? '—'} waves · reference sphere through the exit pupil</div>
          </div>
        )
      }}
    </AnalysisFrame>
  )
}

// ---------- field curvature and distortion ----------

export function FieldCurvesPanel() {
  const state = useAnalysis<FieldCurves>({ kind: 'fieldCurves', samples: 41 })
  return (
    <AnalysisFrame state={state} legend={<><WavelengthLegend /><Legend items={[{ label: 'T', color: 'var(--text-muted)' }, { label: 'S', color: 'var(--text-muted)', dash: '5 3' }]} /></>}>
      {result => {
        const top = result.angles[result.angles.length - 1] || 1
        return (
          <div className="split-row">
            <div className="split-cell">
              <LinePlot
                title="Field curvature"
                vertical
                xLabel="Field angle (deg)"
                yLabel="Focus shift (mm)"
                xDomain={[0, top]}
                symmetricY
                series={result.curves.flatMap(curve => [
                  { points: result.angles.map((a, i) => [a, curve.tangential[i]] as [number, number | null]), color: wavelengthColor(curve.wavelength) },
                  { points: result.angles.map((a, i) => [a, curve.sagittal[i]] as [number, number | null]), color: wavelengthColor(curve.wavelength), dash: '5 3' },
                ])}
              />
            </div>
            <div className="split-cell">
              <LinePlot
                title="Distortion"
                vertical
                xLabel="Field angle (deg)"
                yLabel="Distortion (%)"
                xDomain={[0, top]}
                symmetricY
                series={[{ points: result.angles.map((a, i) => [a, result.distortion[i]]), color: 'var(--field-1)' }]}
              />
            </div>
          </div>
        )
      }}
    </AnalysisFrame>
  )
}

// ---------- Seidel ----------

const SEIDEL_KEYS = ['S1', 'S2', 'S3', 'S4', 'S5', 'CL', 'CT'] as const
const SEIDEL_NAMES = ['Spherical', 'Coma', 'Astigmatism', 'Field curvature', 'Distortion', 'Axial colour', 'Lateral colour']
const SEIDEL_COLORS = ['#4f8ff7', '#3fb950', '#f85149', '#d29922', '#bc8cff', '#39c5cf', '#ff7b72']

function SeidelBars({ result }: { result: SeidelResult }) {
  const [ref, { width, height }] = useSize<HTMLDivElement>()
  const groups = [...result.surfaces.map((_, i) => String(i + 1)), 'SUM']
  const values = [...result.surfaces, result.total]
  const peak = Math.max(...values.flatMap(t => SEIDEL_KEYS.map(k => Math.abs(t[k]))), 1e-9)
  const ticks = niceTicks(-peak * 1.05, peak * 1.05, 6)
  const m = peak * 1.05
  const left = 56, bottom = 22, top = 8
  const plotW = Math.max(1, width - left - 12), plotH = Math.max(1, height - bottom - top)
  const groupW = plotW / groups.length, barW = Math.max(1, groupW * 0.8 / SEIDEL_KEYS.length)
  const sy = (v: number) => top + plotH / 2 - v / m * plotH / 2
  return (
    <div className="line-plot" ref={ref}>
      {width > 0 && (
        <svg width={width} height={height}>
          <g className="grid-lines">{ticks.map(t => <line key={t} x1={left} x2={left + plotW} y1={sy(t)} y2={sy(t)} />)}</g>
          <line className="zero-line" x1={left} x2={left + plotW} y1={sy(0)} y2={sy(0)} />
          <rect className="frame" x={left} y={top} width={plotW} height={plotH} />
          <g className="tick-labels">{ticks.map(t => <text key={t} x={left - 6} y={sy(t) + 4} textAnchor="end">{Math.abs(m) < 0.01 ? t.toExponential(1) : t.toFixed(3)}</text>)}</g>
          {values.map((terms, g) => (
            <g key={g}>
              {SEIDEL_KEYS.map((key, k) => {
                const v = terms[key]
                const x = left + g * groupW + groupW * 0.1 + k * barW
                return <rect key={key} x={x} width={barW * 0.9} y={Math.min(sy(v), sy(0))} height={Math.abs(sy(v) - sy(0))} fill={SEIDEL_COLORS[k]} />
              })}
              <text className="tick-label-x" x={left + (g + 0.5) * groupW} y={top + plotH + 15} textAnchor="middle">{groups[g]}</text>
            </g>
          ))}
        </svg>
      )}
    </div>
  )
}

export function SeidelPanel() {
  const state = useAnalysis<SeidelResult>({ kind: 'seidel' })
  return (
    <AnalysisFrame state={state} legend={<Legend items={SEIDEL_KEYS.map((k, i) => ({ label: k, color: SEIDEL_COLORS[i] }))} />}>
      {result => (
        <div className="seidel">
          <div className="seidel-chart"><SeidelBars result={result} /></div>
          <div className="seidel-table">
            <table className="data-table mono">
              <thead><tr><th>Surf</th>{SEIDEL_KEYS.map((k, i) => <th key={k} title={SEIDEL_NAMES[i]}>{k}</th>)}</tr></thead>
              <tbody>
                {[...result.surfaces, result.total].map((terms, i) => (
                  <tr key={i} className={i === result.surfaces.length ? 'total' : ''}>
                    <td>{i === result.surfaces.length ? 'SUM' : i + 1}</td>
                    {SEIDEL_KEYS.map(k => <td key={k}>{terms[k].toFixed(6)}</td>)}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </AnalysisFrame>
  )
}

// ---------- relative illumination ----------

export function IlluminationPanel() {
  const state = useAnalysis<Illumination>({ kind: 'illumination', samples: 31 })
  return (
    <AnalysisFrame
      state={state}
      legend={<Legend items={[
        { label: 'Relative illumination', color: 'var(--field-1)' },
        { label: 'cos⁴', color: 'var(--text-muted)', dash: '5 3' },
        { label: 'Unvignetted pupil', color: 'var(--field-4)', dash: '1 3' },
      ]} />}
    >
      {result => (
        <div className="analysis-fill">
          <LinePlot
            xLabel="Field angle (deg)"
            yLabel="Relative illumination"
            xDomain={[0, result.angles[result.angles.length - 1] || 1]}
            yDomain={[0, Math.max(1, ...result.relative) * 1.02]}
            series={[
              { points: result.angles.map((a, i) => [a, result.cos4[i]]), color: 'var(--text-muted)', dash: '5 3' },
              { points: result.angles.map((a, i) => [a, result.unvignetted[i]]), color: 'var(--field-4)', dash: '1 3', width: 1.6 },
              { points: result.angles.map((a, i) => [a, result.relative[i]]), color: 'var(--field-1)', width: 1.6 },
            ]}
          />
          <div className="analysis-footer mono">Edge of field: {(100 * result.relative[result.relative.length - 1]).toFixed(1)}% · primary wavelength</div>
        </div>
      )}
    </AnalysisFrame>
  )
}
