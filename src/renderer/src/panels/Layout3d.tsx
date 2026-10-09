import { useEffect, useRef, useState } from 'react'
import * as THREE from 'three'
import { OrbitControls } from 'three/addons/controls/OrbitControls.js'
import { AnalysisFrame, Select } from '../analysis/AnalysisFrame'
import { useAnalysis } from '../analysis/useAnalysis'
import { useWorkbench } from '../document'
import { defaultRadius, usePopStore } from './popStore'

interface Layout3dResult {
  elements: Array<Array<[number, number]>> // (r, z)
  surfaces: Array<Array<[number, number]>>
  rays: Array<{ field: number; points: Array<[number, number, number]> }>
  stopZ: number
  stopSemiDiameter: number
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
    const lathe = (outline: Array<[number, number]>, material: THREE.Material, edges: boolean) => {
      const geometry = keep(new THREE.LatheGeometry(outline.map(([r, z]) => new THREE.Vector2(Math.max(r, 0), z)), 72))
      const mesh = new THREE.Mesh(geometry, material)
      mesh.rotation.z = -Math.PI / 2
      scene.add(mesh)
      if (edges) {
        const line = new THREE.LineSegments(keep(new THREE.EdgesGeometry(geometry, 30)), edgeMaterial)
        line.rotation.z = -Math.PI / 2
        scene.add(line)
      }
    }
    layout.elements.forEach(outline => lathe(outline, glass, true))
    const thin = keep(new THREE.MeshBasicMaterial({ color: token('--surface-stroke'), transparent: true, opacity: 0.15, side: THREE.DoubleSide, depthWrite: false }))
    layout.surfaces.forEach(profile => lathe(profile, thin, false))

    // Beam envelope as a translucent tube around the axis.
    if (beam && beam.profile.length > 1) {
      const tube = keep(new THREE.LatheGeometry(beam.profile.map(([z, w]) => new THREE.Vector2(w, z)), 48))
      const material = keep(new THREE.MeshBasicMaterial({ color: token('--field-3'), transparent: true, opacity: 0.22, side: THREE.DoubleSide, depthWrite: false }))
      const mesh = new THREE.Mesh(tube, material)
      mesh.rotation.z = -Math.PI / 2
      scene.add(mesh)
    }

    const stop = new THREE.Mesh(keep(new THREE.RingGeometry(layout.stopSemiDiameter, layout.stopSemiDiameter * 1.3, 72)), keep(new THREE.MeshBasicMaterial({ color: token('--stop'), side: THREE.DoubleSide })))
    stop.rotation.y = Math.PI / 2
    stop.position.x = layout.stopZ
    scene.add(stop)

    let extent = Math.max(layout.stopSemiDiameter, ...layout.elements.flatMap(o => o.map(p => p[0])), 1)
    const materials = new Map<number, THREE.LineBasicMaterial>()
    for (const ray of layout.rays) {
      let material = materials.get(ray.field)
      if (!material) {
        material = keep(new THREE.LineBasicMaterial({ color: token(`--field-${(ray.field % 6) + 1}`), transparent: true, opacity: 0.85 }))
        materials.set(ray.field, material)
      }
      const points = ray.points.map(toScene)
      for (const p of points) extent = Math.max(extent, Math.abs(p.y), Math.abs(p.z))
      scene.add(new THREE.Line(keep(new THREE.BufferGeometry().setFromPoints(points)), material))
    }

    const planeSize = Math.max(layout.imageSemiHeight * 2.4, extent * 0.6)
    const plane = new THREE.Mesh(keep(new THREE.PlaneGeometry(planeSize, planeSize)), keep(new THREE.MeshBasicMaterial({ color: token('--image-plane'), transparent: true, opacity: 0.18, side: THREE.DoubleSide })))
    plane.rotation.y = Math.PI / 2
    plane.position.x = layout.imageZ
    scene.add(plane)
    const axis = new THREE.Line(
      keep(new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(layout.startZ, 0, 0), new THREE.Vector3(layout.imageZ, 0, 0)])),
      keep(new THREE.LineDashedMaterial({ color: token('--plot-axis'), dashSize: 2, gapSize: 1.5 })),
    )
    axis.computeLineDistances()
    scene.add(axis)

    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    mount.appendChild(renderer.domElement)
    const centre = new THREE.Vector3((layout.startZ + layout.imageZ) / 2, 0, 0)
    const span = Math.max(layout.imageZ - layout.startZ, extent * 2)
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
