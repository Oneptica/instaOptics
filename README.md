<img src="build/icon.png" width="72" alt="">

# instaOptics

Free and open-source optical simulation software.

[![CI](https://github.com/Oneptica/instaOptics/actions/workflows/ci.yml/badge.svg)](https://github.com/Oneptica/instaOptics/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Oneptica/instaOptics)](https://github.com/Oneptica/instaOptics/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Oneptica/instaOptics/total)](https://github.com/Oneptica/instaOptics/releases)
![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)

[中文](README.zh-CN.md)

![](docs/screenshots/analysis.png)

## Features

- Sequential ray tracing: spherical, conic and even-asphere surfaces, real ray aiming, decenter and tilt
- Glasses: Schott catalog, fused silica, CaF₂, `nd/vd` model glasses
- Spot diagram, ray fan, FFT MTF, FFT PSF, wavefront map, field curvature and distortion, Seidel aberrations, relative illumination
- Image simulation on a test chart or any image
- Tolerancing: sensitivity and Monte Carlo
- Optimization (damped least squares)
- 2D and 3D layout
- Zemax `.zmx` import and export

The engine is written in Rust and runs on all CPU cores in a separate process; results update when the system changes.

| | |
| --- | --- |
| ![](docs/screenshots/imagesim.png) | ![](docs/screenshots/tolerance.png) |
| ![](docs/screenshots/workspace.png) | ![](docs/screenshots/3d.png) |

## Build

Requires Node.js 24 and Rust.

```bash
npm install
npm run dev      # run
npm test         # Rust tests
npm run dist     # installer for the current platform
```

On Linux, `npm run dev` runs Electron with `--noSandbox` unless `chrome-sandbox` is setuid root. If WebGL is not available (remote desktop, VM), turn on View → Software 3D Rendering.

## Structure

| Path | |
| --- | --- |
| `crates/optics-core` | Rust optics engine |
| `crates/optics-node` | Node-API bindings |
| `src/main` | Electron main process |
| `src/compute` | Process that runs the engine |
| `src/renderer` | Interface (React) |

Files are saved as `.iol` (JSON). Units: mm, µm, degrees.
