<img src="build/icon.png" width="72" alt="">

# instaOptics

Free and open-source optical simulation software.

Try it in the browser: **[instaoptics.oneptica.com](https://instaoptics.oneptica.com)**

[![CI](https://img.shields.io/github/actions/workflow/status/Oneptica/instaOptics/ci.yml?branch=main&style=flat-square&logo=githubactions&logoColor=white&label=CI)](https://github.com/Oneptica/instaOptics/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Oneptica/instaOptics?style=flat-square&logo=github&label=release)](https://github.com/Oneptica/instaOptics/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
![Windows](https://img.shields.io/badge/Windows-0078D4?style=flat-square&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=flat-square&logo=linux&logoColor=black)
![Rust](https://img.shields.io/badge/engine-Rust-B7410E?style=flat-square&logo=rust&logoColor=white)

[中文](README.zh-CN.md)

![](docs/screenshots/workspace.png)

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

## Build

Requires Node.js 24 and Rust.

```bash
npm install
npm run dev      # run
npm test         # Rust tests
npm run dist     # installer for the current platform
```
