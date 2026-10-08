// Zemax OpticStudio .zmx import / export for the sequential subset this app models:
// standard and even-asphere surfaces, catalog or model glasses, entrance pupil diameter, angle fields.
import type { GlassInfo, LensSystem, Surface } from '../../shared/lens'

const isAir = (material: string) => { const name = material.trim().toUpperCase(); return name === '' || name === 'AIR' }
const curvature = (radius: number) => radius === 0 ? 0 : 1 / radius
function parseModelGlass(material: string): { nd: number; vd: number } | null {
  const match = material.trim().match(/^(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)$/)
  return match ? { nd: Number(match[1]), vd: Number(match[2]) } : null
}
const number = (value: number) => Number.isFinite(value) ? Number(value.toPrecision(12)).toString() : '0'

export function exportZmx(system: LensSystem): string {
  const lines = [
    'VERS 190513 80 123457 L123457',
    'MODE SEQ',
    `NAME ${system.name}`,
    'UNIT MM X W X CM MR CPMM',
    ...apertureLines(system),
    'GCAT SCHOTT',
    `FTYP ${FIELD_CODES[system.fieldType ?? 'angle']} ${APERTURE_CODES[system.apertureType ?? 'entrancePupilDiameter']} ${system.fields.length} ${system.wavelengths.length} 0 0 0`,
    `XFLN ${system.fields.map(() => '0').join(' ')}`,
    `YFLN ${system.fields.map(number).join(' ')}`,
    `FWGN ${system.fields.map(() => '1').join(' ')}`,
    ...system.wavelengths.map((wavelength, i) => `WAVM ${i + 1} ${number(wavelength)} 1`),
    `PWAV ${system.primaryWavelength + 1}`,
    'SURF 0',
    '  TYPE STANDARD',
    '  CURV 0.0',
    `  DISZ ${system.objectDistance === null ? 'INFINITY' : number(system.objectDistance)}`,
  ]
  system.surfaces.forEach((surface, i) => {
    const asphere = surface.aspheric?.some(Boolean)
    lines.push(`SURF ${i + 1}`)
    if (i === system.stopIndex) lines.push('  STOP')
    if (surface.coordinateBreak) {
      const { decenter, tilt } = surface.coordinateBreak
      lines.push('  TYPE COORDBRK', '  CURV 0.0', `  DISZ ${number(surface.thickness)}`)
      ;[decenter[0], decenter[1], tilt[0], tilt[1], tilt[2], 0].forEach((value, j) => lines.push(`  PARM ${j + 1} ${number(value)}`))
      return
    }
    lines.push(`  TYPE ${asphere ? 'EVENASPH' : 'STANDARD'}`)
    lines.push(`  CURV ${number(curvature(surface.radius))}`)
    lines.push(`  DISZ ${number(surface.thickness)}`)
    if (surface.material.trim().toUpperCase() === 'MIRROR') lines.push('  GLAS MIRROR 0 0')
    else if (!isAir(surface.material)) {
      const model = parseModelGlass(surface.material)
      lines.push(model ? `  GLAS ___BLANK 1 0 ${number(model.nd)} ${number(model.vd)} 0 0 0 0 0 0` : `  GLAS ${surface.material} 0 0`)
    }
    if (surface.conic) lines.push(`  CONI ${number(surface.conic)}`)
    if (asphere) {
      lines.push('  PARM 1 0')
      surface.aspheric!.forEach((coefficient, j) => lines.push(`  PARM ${j + 2} ${number(coefficient)}`))
    }
    if (surface.semiDiameter !== undefined) {
      lines.push(`  DIAM ${number(surface.semiDiameter)} 1 0 0 1 ""`)
      lines.push(`  CLAP 0 ${number(surface.semiDiameter)} 0`)
    }
  })
  lines.push(`SURF ${system.surfaces.length + 1}`, '  TYPE STANDARD', '  CURV 0.0', '  DISZ 0')
  return lines.join('\r\n') + '\r\n'
}

// OpticStudio's FTYP codes: field type first, aperture type second.
const FIELD_CODES = { angle: 0, objectHeight: 1, imageHeight: 2 } as const
const APERTURE_CODES = { entrancePupilDiameter: 0, imageFNumber: 1, objectNa: 2, floatByStop: 3, workingFNumber: 4 } as const

function apertureLines(system: LensSystem): string[] {
  const value = number(system.apertureValue ?? 0)
  switch (system.apertureType ?? 'entrancePupilDiameter') {
    case 'entrancePupilDiameter': return [`ENPD ${number(system.entrancePupilDiameter)}`]
    case 'imageFNumber': case 'workingFNumber': return [`FNUM ${value} 0`]
    case 'objectNa': return [`OBNA ${value} 0`]
    case 'floatByStop': return ['FLOA']
  }
}

/** Decodes a .zmx file, which OpticStudio usually writes as UTF-16 LE with a byte order mark. */
export function decodeZmx(bytes: Uint8Array): string {
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return new TextDecoder('utf-16le').decode(bytes.subarray(2))
  if (bytes[0] === 0xfe && bytes[1] === 0xff) return new TextDecoder('utf-16be').decode(bytes.subarray(2))
  // UTF-16 without a BOM shows up as every second byte being zero.
  if (bytes.length > 3 && bytes[1] === 0 && bytes[3] === 0) return new TextDecoder('utf-16le').decode(bytes)
  return new TextDecoder('utf-8').decode(bytes)
}

interface RawSurface { curv: number; disz: number; glass: string; conic: number; parms: number[]; stop: boolean; type: string; aperture?: number; diam?: number }

export function importZmx(text: string, glasses: GlassInfo[]): { system: LensSystem; warnings: string[] } {
  const catalogNames = new Map(glasses.map(glass => [glass.name.toUpperCase(), glass.name]))
  const warnings: string[] = []
  const raw: RawSurface[] = []
  let name = 'Imported lens', epd: number | null = null, fieldType = 0, apertureType = 0, fnum: number | null = null, obna: number | null = null
  let fields: number[] = [], wavelengths: number[] = [], primary = 1
  let current: RawSurface | null = null
  for (const line of text.split(/\r?\n/)) {
    const parts = line.trim().split(/\s+/)
    const key = parts[0]
    const value = (i: number) => Number(parts[i])
    if (key === 'SURF') { current = { curv: 0, disz: 0, glass: '', conic: 0, parms: [], stop: false, type: 'STANDARD' }; raw.push(current); continue }
    if (current) {
      if (key === 'TYPE') current.type = parts[1]
      else if (key === 'CURV') current.curv = value(1)
      else if (key === 'DISZ') current.disz = parts[1] === 'INFINITY' ? Infinity : value(1)
      else if (key === 'STOP') current.stop = true
      else if (key === 'CONI') current.conic = value(1)
      else if (key === 'PARM') current.parms[value(1)] = value(2)
      else if (key === 'CLAP' || key === 'FLAP') current.aperture = value(2)
      else if (key === 'DIAM') current.diam = value(1)
      else if (key === 'GLAS') {
        const catalog = catalogNames.get(parts[1].toUpperCase())
        if (parts[1].toUpperCase() === 'MIRROR') current.glass = 'MIRROR'
        else if (catalog) current.glass = catalog
        else if (Number.isFinite(value(4)) && Number.isFinite(value(5)) && value(4) > 1) {
          current.glass = `${number(value(4))}/${number(value(5))}`
          if (parts[1] !== '___BLANK') warnings.push(`${parts[1]} is not in the catalog; using model glass nd ${number(value(4))}, vd ${number(value(5))}`)
        } else throw new Error(`Unknown glass ${parts[1]}`)
      }
    }
    if (key === 'NAME' && parts.length > 1) name = line.trim().slice(5)
    else if (key === 'ENPD') epd = value(1)
    else if (key === 'FNUM') fnum = value(1)
    else if (key === 'OBNA') obna = value(1)
    else if (key === 'FTYP') { fieldType = value(1); apertureType = value(2) }
    else if (key === 'YFLN') fields = parts.slice(1).map(Number)
    else if (key === 'WAVM') { const i = value(1) - 1; if (value(2) > 0) wavelengths[i] = value(2) }
    else if (key === 'WAVL') wavelengths = parts.slice(1).map(Number)
    else if (key === 'PWAV') primary = value(1)
  }
  if (raw.length < 3) throw new Error('No sequential surfaces found')
  const unsupported = raw.find(surface => !['STANDARD', 'EVENASPH', 'COORDBRK'].includes(surface.type))
  if (unsupported) throw new Error(`Surface type ${unsupported.type} is not supported`)
  const fieldTypes = ['angle', 'objectHeight', 'imageHeight', 'imageHeight'] as const
  if (fieldType === 3) warnings.push('Real image height fields were read as paraxial image heights')
  if (fieldType > 3) warnings.push('This field type is not supported; field values were read as angles')
  const apertureTypes = ['entrancePupilDiameter', 'imageFNumber', 'objectNa', 'floatByStop', 'workingFNumber'] as const
  const apertureKind = apertureTypes[apertureType] ?? 'entrancePupilDiameter'
  if (apertureType > 4) warnings.push('This aperture type is not supported; using an entrance pupil diameter')

  const object = raw[0], image = raw[raw.length - 1]
  const lens = raw.slice(1, -1)
  if (image.curv !== 0) warnings.push('A curved image surface was flattened')
  const fieldCount = fields.length
  const surfaces: Surface[] = lens.map((surface, i) => {
    if (surface.type === 'COORDBRK') {
      const p = (k: number) => surface.parms[k] ?? 0
      if (p(6)) warnings.push(`Coordinate break on surface ${i + 1} uses order 1 (tilt then decenter); it was read as order 0`)
      return { radius: 0, thickness: surface.disz, material: lens[i - 1]?.glass === 'MIRROR' ? 'AIR' : (lens[i - 1]?.glass || 'AIR'), coordinateBreak: { decenter: [p(1), p(2)], tilt: [p(3), p(4), p(5)] } }
    }
    const result: Surface = { radius: surface.curv === 0 ? 0 : 1 / surface.curv, thickness: surface.disz, material: surface.glass || 'AIR' }
    if (surface.conic) result.conic = surface.conic
    if (surface.type === 'EVENASPH') {
      if (surface.parms[1]) warnings.push('The r² term of an even asphere was ignored')
      const aspheric = [2, 3, 4, 5].map(i => surface.parms[i] ?? 0)
      if (aspheric.some(Boolean)) result.aspheric = aspheric
    }
    if (surface.aperture !== undefined && surface.aperture > 0) result.semiDiameter = surface.aperture
    return result
  })
  const firstWavelengths = wavelengths.filter(w => w > 0)
  const stopIndex = Math.max(0, lens.findIndex(surface => surface.stop))
  return {
    system: {
      name,
      objectDistance: Number.isFinite(object.disz) && object.disz > 0 ? object.disz : null,
      surfaces,
      stopIndex,
      entrancePupilDiameter: epd && epd > 0 ? epd : 10,
      ...(apertureKind === 'entrancePupilDiameter' ? {} : {
        apertureType: apertureKind,
        apertureValue: apertureKind === 'objectNa' ? (obna ?? 0.1)
          : apertureKind === 'floatByStop' ? (lens.find(s => s.stop)?.diam ?? 5)
          : (fnum ?? 5),
      }),
      fields: (fieldCount ? fields : [0]).map(Math.abs),
      fieldType: fieldTypes[fieldType] ?? 'angle',
      wavelengths: firstWavelengths.length ? firstWavelengths : [0.5875618],
      primaryWavelength: Math.min(Math.max(primary - 1, 0), Math.max(firstWavelengths.length - 1, 0)),
      rayAiming: true,
    },
    warnings,
  }
}
