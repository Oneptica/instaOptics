// TypeScript mirror of the Rust lens model (crates/optics-core/src/system.rs) and its results.
// Lengths in mm, wavelengths in µm, angles in degrees.

export interface Surface {
  radius: number // 0 means flat
  thickness: number
  material: string // '' / 'AIR', a catalog glass, or a model glass "nd/vd"
  semiDiameter?: number // fixed clear semi-aperture; absent means automatic
  conic?: number
  aspheric?: number[]
  decenter?: [number, number]
  tilt?: [number, number]
  variable?: SurfaceVariables // parameters the optimizer may change
  /** Makes the surface a coordinate break: decenter (mm), then tilt about x, y, z (degrees) for all later surfaces. */
  coordinateBreak?: { decenter: [number, number]; tilt: [number, number, number] }
}

export type ApertureType = 'entrancePupilDiameter' | 'imageFNumber' | 'workingFNumber' | 'objectNa' | 'floatByStop'
export type FieldType = 'angle' | 'objectHeight' | 'imageHeight'

export interface SurfaceVariables { radius?: boolean; thickness?: boolean; conic?: boolean; aspheric?: boolean[] }

export interface OptimizationSettings {
  objective?: 'spot' | 'wavefront'
  rings?: number
  maxTotalTrack?: number
  minBackFocus?: number
  maxChiefRayAngle?: number
  minGlassCenter?: number
  minGlassEdge?: number
  minAir?: number
}

export interface LensSystem {
  name: string
  objectDistance: number | null // null means infinity
  surfaces: Surface[]
  stopIndex: number
  entrancePupilDiameter: number
  apertureType?: ApertureType
  apertureValue?: number // F/#, NA or stop semi-diameter for the other aperture types
  fields: number[]
  fieldType?: FieldType
  wavelengths: number[]
  primaryWavelength: number
  rayAiming: boolean
  targetEfl?: number
  optimization?: OptimizationSettings
  glasses?: GlassDef[] // definitions of the catalog glasses the surfaces use
}

// Non-finite numbers arrive as null.
export interface ParaxialData {
  efl: number | null
  bfl: number | null
  fNumber: number | null
  workingFNumber: number | null
  entrancePupilZ: number | null
  imageDistance: number | null
  imageHeight: number | null
  totalTrack: number
  exitPupilZ: number | null
  stopSemiDiameter: number | null
}

export type Point = [number, number] // (z, y)

export interface LayoutRay {
  field: number
  points: Point[]
  failure: 'miss' | 'tir' | 'clip' | 'backward' | null
}

export interface Layout {
  surfaces: Point[][]
  elements: Point[][]
  mirrors: number[]
  rays: LayoutRay[]
  stopZ: number
  stopSemiDiameter: number
  startZ: number
  imageZ: number
}

export interface Overview {
  paraxial: ParaxialData
  entrancePupilDiameter: number // after converting the aperture type
  fieldAngles: number[] // degrees, after converting the field type
  automaticSemiDiameters: number[]
  semiDiameters: number[]
  layout: Layout
}

export interface GlassInfo { name: string; nd: number; vd: number; catalog?: string }

/** A glass from an OpticStudio catalog: AGF dispersion formula 1–13 with its coefficients. */
export interface GlassDef { name: string; formula: number; coefficients: number[]; nd: number; vd: number; range?: [number, number]; catalog?: string }

export interface Sample { id: string; system: LensSystem }

export function newSystem(): LensSystem {
  return {
    name: 'Untitled',
    objectDistance: null,
    surfaces: [
      { radius: 0, thickness: 10, material: 'AIR' },
    ],
    stopIndex: 0,
    entrancePupilDiameter: 10,
    fields: [0],
    wavelengths: [0.5875618],
    primaryWavelength: 0,
    rayAiming: false,
  }
}

/** Loose structural check for files and messages; the Rust side does the real validation. */
export function isLensSystem(value: unknown): value is LensSystem {
  const v = value as LensSystem
  return !!v && typeof v === 'object' && typeof v.name === 'string' && Array.isArray(v.surfaces) && v.surfaces.length > 0
    && v.surfaces.every(s => typeof s.radius === 'number' && typeof s.thickness === 'number')
    && typeof v.stopIndex === 'number' && typeof v.entrancePupilDiameter === 'number'
    && Array.isArray(v.fields) && Array.isArray(v.wavelengths) && typeof v.primaryWavelength === 'number'
}
