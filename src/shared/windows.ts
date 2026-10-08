// Dockable windows, shared by the native Window menu and the renderer's window registry.
export interface WindowInfo { id: string; title: string; icon: string; group: string; accelerator?: string }

export const WINDOWS: WindowInfo[] = [
  { id: 'welcome', title: 'Welcome', icon: 'home', group: 'Help' },
  { id: 'lensData', title: 'Lens Data', icon: 'table', group: 'System', accelerator: 'CmdOrCtrl+L' },
  { id: 'systemData', title: 'System Data', icon: 'output', group: 'System' },
  { id: 'layout', title: '2D Layout', icon: 'layout-panel-left', group: 'System', accelerator: 'CmdOrCtrl+Shift+L' },
  { id: 'layout3d', title: '3D Layout', icon: 'symbol-namespace', group: 'System' },
  { id: 'spot', title: 'Spot Diagram', icon: 'debug-breakpoint-log-unverified', group: 'Image Quality', accelerator: 'CmdOrCtrl+Shift+P' },
  { id: 'rayFan', title: 'Ray Fan', icon: 'pulse', group: 'Image Quality' },
  { id: 'mtf', title: 'FFT MTF', icon: 'graph-line', group: 'Image Quality', accelerator: 'CmdOrCtrl+Shift+M' },
  { id: 'psf', title: 'FFT PSF', icon: 'target', group: 'Image Quality' },
  { id: 'wavefront', title: 'Wavefront Map', icon: 'symbol-color', group: 'Image Quality' },
  { id: 'fieldCurves', title: 'Field Curvature / Distortion', icon: 'graph-scatter', group: 'Aberrations' },
  { id: 'seidel', title: 'Seidel Diagram', icon: 'graph', group: 'Aberrations' },
  { id: 'illumination', title: 'Relative Illumination', icon: 'lightbulb', group: 'Aberrations' },
  { id: 'optimize', title: 'Optimization', icon: 'rocket', group: 'Optimize', accelerator: 'CmdOrCtrl+F12' },
  { id: 'tolerance', title: 'Tolerancing', icon: 'symbol-ruler', group: 'Tolerance' },
  { id: 'imageSim', title: 'Image Simulation', icon: 'file-media', group: 'Image Simulation' },
]
