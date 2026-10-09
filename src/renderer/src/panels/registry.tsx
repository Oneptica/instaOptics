import { Suspense, lazy, type ReactNode } from 'react'
import { WINDOWS } from '../../../shared/windows'
import { FieldCurvesPanel, IlluminationPanel, MtfPanel, PsfPanel, RayFanPanel, SeidelPanel, SpotPanel, WavefrontPanel } from '../analysis/panels'
import { LayoutView } from './LayoutView'
import { BeamPanel } from './BeamPanel'
import { ChromaticFocalShiftPanel, FootprintPanel, MtfVsFieldPanel, ThroughFocusPanel } from '../analysis/morePanels'
import { ImageSimPanel } from './ImageSimPanel'
import { LensDataEditor } from './LensDataEditor'
import { OptimizePanel } from './OptimizePanel'
import { TolerancePanel } from './TolerancePanel'
import { Welcome } from './Welcome'
import { SystemData } from './SystemData'

// three.js loads only when a 3D window opens.
const Layout3dPanel = lazy(() => import('./Layout3d'))

const RENDER: Record<string, () => ReactNode> = {
  welcome: () => <Welcome />,
  lensData: () => <LensDataEditor />,
  systemData: () => <SystemData />,
  layout: () => <LayoutView />,
  layout3d: () => <Suspense fallback={<div className="placeholder">Loading 3D view…</div>}><Layout3dPanel /></Suspense>,
  spot: () => <SpotPanel />,
  rayFan: () => <RayFanPanel />,
  mtf: () => <MtfPanel />,
  psf: () => <PsfPanel />,
  wavefront: () => <WavefrontPanel />,
  fieldCurves: () => <FieldCurvesPanel />,
  seidel: () => <SeidelPanel />,
  illumination: () => <IlluminationPanel />,
  throughFocus: () => <ThroughFocusPanel />,
  mtfVsField: () => <MtfVsFieldPanel />,
  chromaticFocalShift: () => <ChromaticFocalShiftPanel />,
  footprint: () => <FootprintPanel />,
  beam: () => <BeamPanel />,
  optimize: () => <OptimizePanel />,
  tolerance: () => <TolerancePanel />,
  imageSim: () => <ImageSimPanel />,
}

export const PANELS = WINDOWS.map(window => ({ ...window, render: RENDER[window.id] }))
export const panelTitle = (id: string) => WINDOWS.find(window => window.id === id)?.title ?? id
