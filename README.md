<div align="center">

<img src="build/icon.png" width="112" alt="instaOptics icon">

# instaOptics

**Optical design, from the first ray to a finished lens.**

A desktop lens design workbench with a multi-threaded Rust ray-tracing engine and a fast, modern interface.<br>
Edit a surface and every analysis updates in milliseconds.

![Windows](https://img.shields.io/badge/Windows-0078D4?style=flat-square&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)
![Rust engine](https://img.shields.io/badge/engine-Rust-B7410E?style=flat-square&logo=rust&logoColor=white)

[中文说明](README.zh-CN.md)

</div>

![instaOptics workspace: lens data editor, 2D layout and spot diagram](docs/screenshots/workspace.png)

## Why instaOptics

- **Live.** Paraxial data, layout and every open analysis are recomputed on each edit — a Cooke triplet's full update takes under a millisecond for first-order data and layout, and under 0.2 s for a full polychromatic MTF.
- **Fast where it matters.** The optics engine is Rust, runs in its own process, and spreads ray tracing, FFTs, optimization Jacobians and Monte Carlo trials over every CPU core. The interface never freezes.
- **Built for engineers.** A spreadsheet-style lens data editor driven from the keyboard, dockable analysis windows you can split and arrange, dark and light themes.
- **Open.** Lenses are plain JSON (`.iol`); Zemax `.zmx` files import and export.

## Feature tour

<table>
<tr>
<td width="50%"><img src="docs/screenshots/analysis.png" alt="MTF, PSF, field curvature and wavefront analyses"></td>
<td width="50%"><img src="docs/screenshots/aberrations.png" alt="Seidel diagram, ray fans and relative illumination"></td>
</tr>
<tr>
<td><b>Image quality</b> — polychromatic FFT MTF against the diffraction limit, FFT PSF with Strehl ratio and encircled energy, wavefront maps, field curvature and distortion.</td>
<td><b>Aberrations</b> — Seidel contributions per surface, tangential and sagittal ray fans for every field and wavelength, relative illumination and vignetting.</td>
</tr>
<tr>
<td><img src="docs/screenshots/optimize.png" alt="Damped least squares optimization with live merit history"></td>
<td><img src="docs/screenshots/tolerance.png" alt="Monte Carlo tolerance analysis"></td>
</tr>
<tr>
<td><b>Optimization</b> — mark radii, thicknesses, conics or aspheric terms variable with one click and run damped least squares on RMS spot or wavefront error, with focal length, track, back focus, chief ray angle and edge thickness constraints. The lens updates live; one undo restores the start.</td>
<td><b>Tolerancing</b> — rank every radius, thickness, decenter, tilt and glass tolerance by its effect, then run thousands of Monte Carlo builds with a back-focus compensator to predict yield.</td>
</tr>
<tr>
<td><img src="docs/screenshots/imagesim.png" alt="Image simulation with ISP controls"></td>
<td><img src="docs/screenshots/3d.png" alt="3D layout with real rays"></td>
</tr>
<tr>
<td><b>Image simulation</b> — render a test chart or your own photo through the lens: blur, distortion, lateral colour, vignetting and cos⁴ fall-off, then adjust exposure, gamma, white balance and saturation.</td>
<td><b>2D and 3D layout</b> — cross-sections with real traced rays, and an orbitable 3D model of the assembled lens.</td>
</tr>
</table>

### Everything in the box

| Area | What you get |
| --- | --- |
| Lens model | Spherical, conic and even-asphere surfaces (A4–A10); Schott, fused silica and CaF₂ catalog glasses and `nd/vd` model glasses; infinite or finite object; real ray aiming at the stop |
| Analyses | Spot diagram, ray fan, FFT MTF, FFT PSF and encircled energy, wavefront map, field curvature and distortion, Seidel diagram, relative illumination, system data report |
| Design | Damped least squares optimization with live progress and stop; sensitivity and Monte Carlo tolerancing |
| Visualization | 2D layout with zoom and pan, 3D layout, image simulation with ISP |
| Files | `.iol` lens documents, Zemax `.zmx` import and export, undo and redo for every edit |

<p align="center"><img src="docs/screenshots/welcome.png" width="85%" alt="Welcome page with sample lenses"></p>

## Getting started

Installers will be published on the Releases page. Until then, build from source:

```bash
git clone https://github.com/Oneptica/instaOptics.git
cd instaOptics
npm install
npm run dev
```

You need Node.js 24 and a Rust toolchain ([rustup](https://rustup.rs)), plus the platform's C toolchain (MSVC Build Tools on Windows, Xcode command line tools on macOS, `build-essential` on Linux). The welcome page opens with five sample lenses — start with the Cooke triplet.

## Roadmap

- AGF glass catalogs (Schott, Ohara, CDGM, Hoya) and more dispersion formulas
- F/#, NA and image-space apertures; object and image height fields; field and wavelength weights
- Mirrors, coordinate breaks, odd aspheres, Zernike and XY polynomial surfaces
- Merit function editor with operands, multi-configuration, solves and pickups
- Global optimization (hammer and multi-start)
- Through-focus MTF, MTF versus field, grid distortion, footprints

## Architecture

```
renderer (React + TypeScript)        lens data editor, docking windows, plots, 3D (three.js)
   │  MessagePort — direct to the engine, binary images included
compute (Electron utility process)   loads native/optics.node, streams progress, can be stopped
   │  Node-API (napi-rs), work on the libuv thread pool
optics-core (Rust + rayon)           glass, paraxial optics, real ray tracing, analyses,
                                     optimization, tolerancing, image simulation
```

| Path | Contents |
| --- | --- |
| `crates/optics-core` | Pure Rust optics engine, no Node or UI dependencies |
| `crates/optics-node` | Node-API bindings used by the compute process |
| `src/main` | Electron main process: windows, menus, files, compute process lifecycle |
| `src/compute` | Utility process that serves engine requests |
| `src/preload` | The `window.instaOptics` bridge exposed to the renderer |
| `src/renderer` | The interface |
| `src/shared` | Types shared by all processes (the lens model mirrors `optics-core`) |

## Development

```bash
npm run dev         # build the engine (optimized, incremental) and start Electron with hot reload
npm test            # Rust unit tests and the cross-check against reference values
npm run build       # typecheck, release engine build, bundle
npm run dist        # installers for the current platform in dist/ (NSIS, DMG, AppImage + deb)
```

CI builds, tests and packages Windows, macOS and Linux on every push.

On Linux, `npm run dev` adds `--noSandbox` unless Electron's `chrome-sandbox` helper is setuid root
(`sudo chown root node_modules/electron/dist/chrome-sandbox && sudo chmod 4755 node_modules/electron/dist/chrome-sandbox`).
If the 3D view reports that WebGL is unavailable (remote desktops, some virtual machines), use the button it shows or
View → Software 3D Rendering; the choice is stored in `startup.json` in the app's user data folder.

Lenses are saved as `.iol` files: JSON with `{ "format": "instaoptics-lens", "version": 1, "system": … }`.
Lengths are in mm, wavelengths in µm and field angles in degrees.
