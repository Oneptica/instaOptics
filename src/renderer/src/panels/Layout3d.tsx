import { useEffect, useRef, useState } from 'react'
import * as THREE from 'three'
import { OrbitControls } from 'three/addons/controls/OrbitControls.js'
import { AnalysisFrame, Select } from '../analysis/AnalysisFrame'
import { useAnalysis } from '../analysis/useAnalysis'
import { useWorkbench } from '../document'
import { defaultRadius, usePopStore } from './popStore'

type Vec3 = [number, number, number]
interface Frame3d { origin: Vec3; rotation: number[] } // global = origin + R · local, R row-major
interface Body3d extends Frame3d { outline: Array<[number, number]> } // (r, z) in the body's own frame

interface Layout3dResult {
  elements: Body3d[]
  surfaces: Body3d[]
  rays: Array<{ field: number; points: Vec3[] }>
  stop: Frame3d
  stopSemiDiameter: number
  image: Frame3d
  startZ: number
  imageZ: number
  imageSemiHeight: number
}

const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#888'

/** Re-renders when the light/dark theme changes, since the scene colours come from CSS tokens. */
function useThemeVersion() {
  const [version, setVersion] = useState(0)
  useEffect(() => {
    const observer = new MutationObserver(() => setVersion(v => v + 1))
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
    return () => observer.disconnect()
  }, [])
  return version
}

// Optical z runs along three.js x; optical y stays y; optical x becomes three.js z.
const toScene = ([x, y, z]: [number, number, number]) => new THREE.Vector3(z, y, x)

const place = (frame: Frame3d, [x, y, z]: Vec3): Vec3 => {
  const r = frame.rotation
  return [frame.origin[0] + r[0] * x + r[1] * y + r[2] * z, frame.origin[1] + r[3] * x + r[4] * y + r[5] * z, frame.origin[2] + r[6] * x + r[7] * y + r[8] * z]
}

/** Surface of revolution of an (r, z) outline about its frame's z axis, placed in the scene. */
function revolve(body: Frame3d & { outline: Array<[number, number]> }, segments = 72): THREE.BufferGeometry {
  const rows = body.outline.length
  const positions = new Float32Array(rows * (segments + 1) * 3)
  const indices: number[] = []
  body.outline.forEach(([r, z], i) => {
    for (let k = 0; k <= segments; k++) {
      const phi = 2 * Math.PI * k / segments
      const p = toScene(place(body, [Math.max(r, 0) * Math.cos(phi), Math.max(r, 0) * Math.sin(phi), z]))
      positions.set([p.x, p.y, p.z], (i * (segments + 1) + k) * 3)
    }
  })
  for (let i = 0; i + 1 < rows; i++) for (let k = 0; k < segments; k++) {
    const a = i * (segments + 1) + k, b = a + 1, c = a + segments + 1, d = c + 1
    indices.push(a, b, c, b, d, c)
  }
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geometry.setIndex(indices)
  geometry.computeVertexNormals()
  return geometry
}

interface BeamProfile { profile: Array<[number, number]> }

function Scene({ layout, beam }: { layout: Layout3dResult; beam: BeamProfile | null }) {
  const mountRef = useRef<HTMLDivElement>(null)
  const cameraState = useRef<{ position: THREE.Vector3; target: THREE.Vector3 } | null>(null)
  const theme = useThemeVersion()
  const [failed, setFailed] = useState(false)

  useEffect(() => {
    const mount = mountRef.current
    if (!mount || failed) return
    let renderer: THREE.WebGLRenderer
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true })
    } catch {
      // Reported in a microtask so the effect does not set state synchronously.
      void Promise.resolve().then(() => setFailed(true))
      return
    }
    const scene = new THREE.Scene()
    scene.background = new THREE.Color(token('--plot-bg'))
    scene.add(new THREE.AmbientLight(0xffffff, 1.4))
    const light = new THREE.DirectionalLight(0xffffff, 1.6)
    light.position.set(-1, 2, 3)
    scene.add(light)
    const disposables: Array<{ dispose: () => void }> = []
    const keep = <T extends { dispose: () => void }>(item: T) => { disposables.push(item); return item }

    const glass = keep(new THREE.MeshPhysicalMaterial({ color: token('--glass-stroke'), transparent: true, opacity: 0.32, roughness: 0.15, side: THREE.DoubleSide, depthWrite: false }))
    const edgeMaterial = keep(new THREE.LineBasicMaterial({ color: token('--glass-stroke'), transparent: true, opacity: 0.55 }))
    const body = (item: Body3d, material: THREE.Material, edges: boolean) => {
      const geometry = keep(revolve(item))
      scene.add(new THREE.Mesh(geometry, material))
      if (edges) scene.add(new THREE.LineSegments(keep(new THREE.EdgesGeometry(geometry, 30)), edgeMaterial))
    }
    layout.elements.forEach(item => body(item, glass, true))
    const thin = keep(new THREE.MeshBasicMaterial({ color: token('--surface-stroke'), transparent: true, opacity: 0.15, side: THREE.DoubleSide, depthWrite: false }))
    layout.surfaces.forEach(item => body(item, thin, false))

    // Beam envelope as a translucent tube around the first surface's axis (only drawn for straight systems).
    if (beam && beam.profile.length > 1) {
      const tube = keep(revolve({ origin: [0, 0, 0], rotation: [1, 0, 0, 0, 1, 0, 0, 0, 1], outline: beam.profile.map(([z, w]) => [w, z] as [number, number]) }, 48))
      const material = keep(new THREE.MeshBasicMaterial({ color: token('--field-3'), transparent: true, opacity: 0.22, side: THREE.DoubleSide, depthWrite: false }))
      scene.add(new THREE.Mesh(tube, material))
    }

    // The stop is a ring in the stop surface's own plane.
    const frameMatrix = (frame: Frame3d) => {
      const [ex, ey, ez] = [toScene(place(frame, [1, 0, 0])), toScene(place(frame, [0, 1, 0])), toScene(place(frame, [0, 0, 1]))]
      const origin = toScene(frame.origin)
      return new THREE.Matrix4().makeBasis(ex.sub(origin), ey.sub(origin), ez.sub(origin)).setPosition(origin)
    }
    const stop = new THREE.Mesh(keep(new THREE.RingGeometry(layout.stopSemiDiameter, layout.stopSemiDiameter * 1.3, 72)), keep(new THREE.MeshBasicMaterial({ color: token('--stop'), side: THREE.DoubleSide })))
    stop.applyMatrix4(frameMatrix(layout.stop))
    scene.add(stop)

    const bounds = new THREE.Box3()
    const materials = new Map<number, THREE.LineBasicMaterial>()
    for (const ray of layout.rays) {
      let material = materials.get(ray.field)
      if (!material) {
        material = keep(new THREE.LineBasicMaterial({ color: token(`--field-${(ray.field % 6) + 1}`), transparent: true, opacity: 0.85 }))
        materials.set(ray.field, material)
      }
      const points = ray.points.map(toScene)
      points.forEach(p => bounds.expandByPoint(p))
      scene.add(new THREE.Line(keep(new THREE.BufferGeometry().setFromPoints(points)), material))
    }
    for (const item of [...layout.elements, ...layout.surfaces]) {
      const reach = Math.max(...item.outline.map(p => p[0]))
      for (const [x, y] of [[reach, 0], [-reach, 0], [0, reach], [0, -reach]]) for (const z of [item.outline[0][1], item.outline[item.outline.length - 1][1]]) bounds.expandByPoint(toScene(place(item, [x, y, z])))
    }
    const size = bounds.getSize(new THREE.Vector3())
    const extent = Math.max(size.x, size.y, size.z, 1)

    // Image plane, in the image frame.
    const planeSize = Math.max(layout.imageSemiHeight * 2.4, extent * 0.15)
    const plane = new THREE.Mesh(keep(new THREE.PlaneGeometry(planeSize, planeSize)), keep(new THREE.MeshBasicMaterial({ color: token('--image-plane'), transparent: true, opacity: 0.18, side: THREE.DoubleSide })))
    plane.applyMatrix4(frameMatrix(layout.image))
    scene.add(plane)
    // Axis through the surface origins (bent by folds).
    const axisPoints = [new THREE.Vector3(layout.startZ, 0, 0), ...layout.elements.map(e => toScene(e.origin)), toScene(layout.image.origin)]
    const axis = new THREE.Line(keep(new THREE.BufferGeometry().setFromPoints(axisPoints)), keep(new THREE.LineDashedMaterial({ color: token('--plot-axis'), dashSize: 2, gapSize: 1.5 })))
    axis.computeLineDistances()
    scene.add(axis)

    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    mount.appendChild(renderer.domElement)
    const centre = bounds.getCenter(new THREE.Vector3())
    const span = extent * 1.1
    const camera = new THREE.PerspectiveCamera(35, 1, span / 100, span * 20)
    const controls = new OrbitControls(camera, renderer.domElement)
    // Keep the user's viewpoint across edits; start from an oblique view.
    if (cameraState.current) {
      camera.position.copy(cameraState.current.position)
      controls.target.copy(cameraState.current.target)
    } else {
      camera.position.set(centre.x - span * 0.35, span * 0.45, span * 1.25)
      controls.target.copy(centre)
    }
    controls.enableDamping = true
    controls.update()

    const resize = () => {
      const width = mount.clientWidth || 1, height = mount.clientHeight || 1
      renderer.setSize(width, height, false)
      camera.aspect = width / height
      camera.updateProjectionMatrix()
    }
    const observer = new ResizeObserver(resize)
    observer.observe(mount)
    resize()
    let frame = 0
    const loop = () => { frame = requestAnimationFrame(loop); controls.update(); renderer.render(scene, camera) }
    loop()
    return () => {
      cameraState.current = { position: camera.position.clone(), target: controls.target.clone() }
      cancelAnimationFrame(frame)
      observer.disconnect()
      controls.dispose()
      disposables.forEach(item => item.dispose())
      renderer.dispose()
      mount.removeChild(renderer.domElement)
    }
  }, [layout, beam, theme, failed])

  if (failed) {
    return (
      <div className="placeholder">
        <div className="webgl-missing">
          <i className="codicon codicon-warning" />
          <p>This display has no usable hardware WebGL (common over remote desktop or in virtual machines).</p>
          <button className="tool labeled primary" onClick={() => window.instaOptics.setSoftwareRendering(true)}>Restart with software rendering</button>
          <p className="muted">You can switch back under View → Software 3D Rendering.</p>
        </div>
      </div>
    )
  }

  return (
    <div className="scene-3d" ref={mountRef}>
      <span className="scene-hint">Drag to orbit · scroll to zoom · right-drag to pan</span>
    </div>
  )
}

export default function Layout3dPanel() {
  const { doc } = useWorkbench()
  const system = doc.system
  const { ui } = usePopStore()
  const [ring, setRing] = useState(16)
  const [showBeam, setShowBeam] = useState(false)
  const state = useAnalysis<Layout3dResult>({ kind: 'layout3d', ring })
  // Beam radius along the system with the Beam Propagation window's source; paths are only straight without folds.
  const straight = system.surfaces.every(surface => !surface.coordinateBreak && surface.material.trim().toUpperCase() !== 'MIRROR')
  const radius = ui.radius ?? defaultRadius(ui, system)
  const beam = useAnalysis<BeamProfile>({ kind: 'gaussianBeam', wavelength: Math.min(ui.wavelength, system.wavelengths.length - 1), radius, waist: ui.waist })
  return (
    <AnalysisFrame
      state={state}
      controls={<>
        <Select label="Rays per ring" value={ring} options={[8, 12, 16, 24, 32].map(v => ({ value: v, label: String(v) }))} onChange={setRing} />
        <label className="tool-field" title={straight ? 'Gaussian beam envelope from the Beam Propagation source' : 'Not available for folded systems'}>
          <input type="checkbox" disabled={!straight} checked={showBeam && straight} onChange={e => setShowBeam(e.target.checked)} /> Beam
        </label>
      </>}
    >
      {layout => <Scene layout={layout} beam={showBeam && straight ? beam.result : null} />}
    </AnalysisFrame>
  )
}
