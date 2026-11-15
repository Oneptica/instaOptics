# instaOptics — instructions for agents

instaOptics is free and open-source optical simulation software for sequential lens systems, aiming at what OpticStudio does for classical optics. It is pure Rust: optics core, egui desktop app, CLI and MCP share one engine. It follows the architecture of the storytold Craft apps (CADCraft, PhotoCraft): layered crates, everything is a command, the UI is thin, agents drive the same commands people use.

## From a fresh machine

1. Install Rust stable (`rustup`). Linux builds need no extra system packages; file dialogs go through the XDG desktop portal.
2. `cargo test --workspace`
3. `cargo run --release -p instaoptics` (the app) or `cargo run --release -p instaoptics-cli -- --help`

## Rules

- **Layering** ([docs/architecture.md](docs/architecture.md)): `core` → `io`, `render` → `engine` → `ui-egui`, `mcp` → apps. Nothing below the front ends uses egui, eframe, winit, wgpu or rfd.
- **Everything is a command.** A user-visible action is a `CommandSpec` in `crates/engine/src/commands.rs` with JSON parameters. The UI calls `commands::execute`; it does not edit the lens directly. Commands never open dialogs.
- **Figures are defined once** in `crates/render/src/figures.rs`. The egui painter and the PNG rasteriser both draw them.
- **The interface thread does no optics.** Computations go through `ui-egui/src/worker.rs`.
- **Never crash.** Production crate roots deny `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `unreachable!`; `unsafe` is forbidden. Return errors.
- **Rust only.** No JavaScript, TypeScript, Electron or webview.
- Units: lengths mm, wavelengths µm, angles degrees. Surfaces are numbered from 1 in commands and the UI, from 0 in the lens model.

## Looking at the app

Run `instaoptics --control 7391`, then send JSON lines with the token from `$XDG_RUNTIME_DIR/instaoptics-control-7391.token` ([docs/control-protocol.md](docs/control-protocol.md)): `ui.open {"window":"mtf"}`, `engine.execute {...}`, `ui.screenshot {"path":"/tmp/shot.png"}`, then read the PNG. Without a window, `instaoptics-cli render` draws the same figures.
