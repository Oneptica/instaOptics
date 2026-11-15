//! instaOptics desktop front end in egui. It is thin: windows read the session and act through engine commands,
//! so the menus, shortcuts, the control channel and MCP all do the same thing.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod analysis;
mod app;
mod control;
mod explorer;
pub mod i18n;
mod lde;
pub mod panels;
mod plot;
mod prefs;
mod theme;
mod tools;
mod updates;
mod view3d;
mod worker;

pub use app::Workbench;
