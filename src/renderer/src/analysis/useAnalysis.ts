import { useEffect, useState } from 'react'
import { useWorkbench } from '../document'

// Result shapes of crates/optics-core/src/analysis.rs. Non-finite numbers arrive as null.
type N = number | null

export interface SpotField { field: number; points: Array<[number, number, number]>; reference: [number, number]; rmsRadius: number; geoRadius: number; traced: number; total: number }
export interface FanCurve { wavelength: number; pupil: number[]; error: number[] }
export interface RayFanField { field: number; tangential: FanCurve[]; sagittal: FanCurve[] }
export interface SeidelTerms { S1: number; S2: number; S3: number; S4: number; S5: number; CL: number; CT: number }
export interface SeidelResult { surfaces: SeidelTerms[]; total: SeidelTerms }
export interface FieldCurves { angles: number[]; curves: Array<{ wavelength: number; tangential: N[]; sagittal: N[] }>; distortion: N[] }
export interface WavefrontMap { field: number; wavelength: number; size: number; values: N[]; pv: N; rms: N }
export interface MtfResult { frequencies: number[]; cutoff: number; fields: Array<{ field: number; tangential: number[]; sagittal: number[] }>; diffraction: number[] }
export interface Illumination { angles: number[]; relative: number[]; unvignetted: number[]; cos4: number[] }
export interface PsfResult { field: number; size: number; spacing: number; data: number[]; strehl: number; radius: number[]; energy: number[]; diffraction: number[] }

export interface ThroughFocus { frequency: number; shifts: number[]; fields: Array<{ field: number; tangential: number[]; sagittal: number[] }> }
export interface MtfVsField { angles: number[]; frequencies: number[]; curves: Array<{ field: number; tangential: number[]; sagittal: number[] }> }
export interface ChromaticFocalShift { wavelengths: number[]; shift: number[]; range: number }
export interface Footprint { surface: number; fields: Array<Array<[number, number]>>; semiDiameter: number }

export interface CoatingCurves { surface: number; description: string; angles: number[]; rS: number[]; rP: number[]; tS: number[]; tP: number[]; wavelengths: number[]; rWavelength: number[]; tWavelength: number[] }
export interface PolarizationMap {
  fieldAngle: number; size: number; transmission: N[]; diattenuation: N[]; retardance: N[]; orientation: N[]; ellipticity: N[]
  meanTransmission: N; minTransmission: N; maxDiattenuation: number; rmsRetardance: N; surfaceTransmission: number[]
}
export interface FieldTransmission { field: number; mean: N; min: N; maxDiattenuation: number; rmsRetardance: N }
export interface MeritReport { merit: N; values: N[] }
export type PolInput = { kind: 'linear'; angle: number } | { kind: 'rightCircular' } | { kind: 'leftCircular' } | { kind: 'unpolarized' }

export type AnalysisRequest =
  | { kind: 'spot'; rings: number }
  | { kind: 'rayFan'; samples: number }
  | { kind: 'seidel' }
  | { kind: 'fieldCurves'; samples: number }
  | { kind: 'wavefront'; field: number; wavelength: number; size: number }
  | { kind: 'mtf'; size: number; points: number; maxFrequency: number | null }
  | { kind: 'illumination'; samples: number }
  | { kind: 'psf'; field: number; samples: number; padding: number; crop: number }
  | { kind: 'layout3d'; ring: number }
  | { kind: 'throughFocus'; frequency: number; range: number; steps: number }
  | { kind: 'mtfVsField'; frequencies: number[]; samples: number }
  | { kind: 'chromaticFocalShift'; samples: number }
  | { kind: 'footprint'; surface: number; rings: number }
  | { kind: 'meritFunction' }
  | { kind: 'coating'; surface: number; wavelength: number; maxAngle: number; points: number }
  | { kind: 'polarization'; field: number; wavelength: number; grid: number; input: PolInput }
  | { kind: 'transmissionByField'; wavelength: number; grid: number; input: PolInput }
  | { kind: 'gaussianBeam'; wavelength: number; radius: number; waist: number }

export interface AnalysisState<T> { result: T | null; error: string | null; ms: number | null; busy: boolean }

/** Runs an analysis in the engine whenever the system or the request changes; keeps the last result while busy. */
export function useAnalysis<T>(request: AnalysisRequest): AnalysisState<T> {
  const { doc } = useWorkbench()
  const key = JSON.stringify(request)
  const [state, setState] = useState<AnalysisState<T>>({ result: null, error: null, ms: null, busy: true })
  useEffect(() => {
    let cancelled = false
    const start = performance.now()
    // Marking busy happens in a microtask so the effect body itself does not set state synchronously.
    void Promise.resolve().then(() => { if (!cancelled) setState(s => s.busy ? s : { ...s, busy: true }) })
    window.instaOptics.engine.request('analyze', { system: JSON.stringify(doc.system), analysis: key }).then(
      response => { if (!cancelled) setState({ result: JSON.parse(response.json) as T, error: null, ms: performance.now() - start, busy: false }) },
      (error: Error) => { if (!cancelled) setState(s => ({ ...s, error: error.message, ms: null, busy: false })) },
    )
    return () => { cancelled = true }
  }, [doc.system, key])
  return state
}
