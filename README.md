# instaOptics

Desktop optical design software for sequential lens design. Electron hosts the interface; the optics engine is written
in Rust and runs in a separate compute process, so long calculations never block the UI.

```
renderer (React + TypeScript)        lens data editor, layout, reports, docking windows
   │  MessagePort (direct, no main-process hop)
compute (Electron utility process)   loads native/optics.node
   │  Node-API (napi-rs)
optics-core (Rust)                   glass catalog, paraxial optics, real ray tracing with ray aiming
```

## Features

- Lens data editor: spherical, conic and even-asphere (A4–A10) surfaces, catalog and model glasses, optimization variables
- System explorer: entrance pupil, angle fields, wavelengths, object distance, real ray aiming
- 2D and 3D layout
- Analyses: spot diagram, ray fan, FFT MTF, FFT PSF with encircled energy, wavefront map, field curvature and distortion,
  Seidel diagram, relative illumination
- Optimization: damped least squares on radii, thicknesses, conics and aspheric terms; RMS spot or wavefront merit,
  focal length target, track, back focus, chief ray angle and thickness constraints
- Tolerancing: sensitivity with RSS estimate, Monte Carlo with a back-focus compensator
- Image simulation: ray-traced blur, distortion, lateral colour and vignetting on a test chart or any image, with ISP
- Zemax .zmx import and export

Long computations (analyses, optimization, tolerancing, image simulation) run on worker threads in the compute
process and use every core; optimization and Monte Carlo report progress and can be stopped.

## Layout

| Path | Contents |
| --- | --- |
| `crates/optics-core` | Pure Rust optics engine, no Node or UI dependencies |
| `crates/optics-node` | Node-API bindings used by the compute process |
| `src/main` | Electron main process: windows, menus, files, compute process lifecycle |
| `src/compute` | Utility process that serves engine requests |
| `src/preload` | The `window.instaOptics` bridge exposed to the renderer |
| `src/renderer` | The interface |
| `src/shared` | Types shared by all processes (lens model mirrors `optics-core`) |

## Requirements

- Node.js 24 and npm 11
- Rust stable (`rustup`), with `cargo` on `PATH` or in `~/.cargo/bin`
- Platform toolchain for native builds: MSVC Build Tools on Windows, Xcode command line tools on macOS, `build-essential` on Linux

## Commands

```bash
npm install
npm run dev         # build the engine (optimized, incremental) and start Electron with hot reload
npm test            # Rust unit tests and the cross-check against the web engine's reference values
npm run build       # typecheck, release engine build, bundle
npm run dist        # installers for the current platform in dist/ (NSIS, DMG, AppImage + deb)
```

On Linux, `npm run dev` adds `--noSandbox` unless Electron's `chrome-sandbox` helper is setuid root
(`sudo chown root node_modules/electron/dist/chrome-sandbox && sudo chmod 4755 node_modules/electron/dist/chrome-sandbox`).

If the 3D view reports that WebGL is unavailable (remote desktops, some virtual machines), use the button it shows or
View → Software 3D Rendering; the choice is stored in `startup.json` in the app's user data folder.

The native module is built per platform and architecture; CI builds and packages Windows, macOS and Linux.

## Files

Lenses are saved as `.iol` files: JSON with `{ "format": "instaoptics-lens", "version": 1, "system": … }`.
Lengths are in mm, wavelengths in µm and field angles in degrees.
