import { useWorkbench } from '../document'
import { formatFixed, formatShort } from '../format'

export function SystemData() {
  const { engine, doc } = useWorkbench()
  const system = doc.system
  const p = engine.overview?.paraxial
  const rows: [string, string, string][] = p ? [
    ['Effective focal length', formatFixed(p.efl), 'mm'],
    ['Back focal length', formatFixed(p.bfl), 'mm'],
    ['Image space F/#', formatFixed(p.fNumber), ''],
    ['Working F/#', formatFixed(p.workingFNumber), ''],
    ['Entrance pupil diameter', formatFixed(system.entrancePupilDiameter), 'mm'],
    ['Entrance pupil position', formatFixed(p.entrancePupilZ), 'mm'],
    ['Exit pupil position', formatFixed(p.exitPupilZ), 'mm'],
    ['Stop semi-diameter (paraxial)', formatFixed(p.stopSemiDiameter), 'mm'],
    ['Paraxial image height', formatFixed(p.imageHeight), 'mm'],
    ['Paraxial image distance', formatFixed(p.imageDistance), 'mm'],
    ['Total track', formatFixed(p.totalTrack), 'mm'],
  ] : []

  return (
    <div className="panel report">
      <div className="report-body">
        <h3>General</h3>
        {p ? (
          <table className="kv">
            <tbody>
              {rows.map(([label, value, unit]) => <tr key={label}><th>{label}</th><td className="mono">{value}</td><td className="unit">{unit}</td></tr>)}
            </tbody>
          </table>
        ) : <p className="muted">{engine.error ?? 'Waiting for the engine…'}</p>}
        <h3>Object</h3>
        <table className="kv"><tbody>
          <tr><th>Object distance</th><td className="mono">{formatFixed(system.objectDistance)}</td><td className="unit">mm</td></tr>
          <tr><th>Ray aiming</th><td className="mono">{system.rayAiming ? 'Real' : 'Paraxial'}</td><td /></tr>
        </tbody></table>
        <h3>Fields</h3>
        <table className="kv"><tbody>
          {system.fields.map((field, i) => <tr key={i}><th>Field {i + 1}</th><td className="mono">{formatShort(field)}</td><td className="unit">deg</td></tr>)}
        </tbody></table>
        <h3>Wavelengths</h3>
        <table className="kv"><tbody>
          {system.wavelengths.map((wavelength, i) => (
            <tr key={i}><th>Wavelength {i + 1}{i === system.primaryWavelength ? ' (primary)' : ''}</th><td className="mono">{wavelength.toFixed(7)}</td><td className="unit">µm</td></tr>
          ))}
        </tbody></table>
      </div>
    </div>
  )
}
