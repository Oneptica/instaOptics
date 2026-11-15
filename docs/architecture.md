# Architecture

instaOptics is one Rust workspace. Lower layers never depend on higher ones, and nothing below the front ends depends on egui, eframe, winit, wgpu or rfd, so the interface can be replaced without touching the optics.

| Layer | Crate | What it does |
|---|---|---|
| L0 | `core` | Lens model, glasses and AGF dispersion formulas, paraxial optics, real ray tracing, every analysis (spot, fans, MTF, PSF, wavefront, Seidel, field curves, illumination, footprint), coatings and polarization, physical optics propagation, optimization, tolerancing, image simulation, 2D and 3D layout geometry |
| L1 | `io` | `.iol` lens documents, Zemax `.zmx` import and export, OpticStudio `.agf` catalogs, CSV of analysis results |
| L1 | `render` | Analysis results as figures (charts, legend, table, footer), the shared figure layout, and a CPU rasteriser (tiny-skia) for PNG |
| L2 | `engine` | `Session` (the open lens, undo, saved state, loaded catalogs) and the command registry |
| L3 | `ui-egui` | The desktop front end: menus, docked windows (craft-ui docking), Lens Data editor, figure painter, background work, settings (craft-settings), control channel |
| L3 | `mcp` | MCP server over stdio (craft-mcp, craft-jsonrpc), headless or bridged to the app |
| app | `instaoptics` | Desktop app (eframe, wgpu) |
| app | `instaoptics-cli` | `info`, `convert`, `analyze`, `render`, `run`, `commands`, `mcp` |

## Commands

Every user-visible action is a `CommandSpec` in `crates/engine/src/commands.rs`: an id, a label, its menu, a shortcut, a parameter description and a JSON `run`. Menus, shortcuts, the CLI, the control channel and MCP all reach these commands. A command never opens a dialog; the desktop asks for a missing path and then runs the command with it. Surfaces are numbered from 1 as in the Lens Data editor.

## Figures

An analysis result becomes a `render::Figure` once. The egui painter (`ui-egui/src/plot.rs`) draws it with zoom, pan and readout; `render::raster` draws the same figure into a PNG for export, the CLI and MCP. Both use `render::frame` for placement, so an exported image looks like the window.

## Background work

The interface thread never computes optics. `ui-egui/src/worker.rs` runs each analysis, the overview, optimization and Monte Carlo on worker threads (the core uses rayon inside). A result carries the tag of the lens and settings it was computed for; a stale result stays on screen as the last valid one until the new one arrives.

## Files and settings

Lens files are JSON (`.iol`). Settings are `config.toml` in the platform settings folder (craft-settings). Glass catalogs added from the app or the CLI are copied into the data folder's `catalogs/`, and the glasses a lens uses are stored inside the lens file.
