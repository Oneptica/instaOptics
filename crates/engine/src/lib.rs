//! instaOptics engine: the session (open lens, undo, catalogs) and the command registry. Menus, shortcuts, the CLI,
//! the control channel and MCP all run the same commands; nothing here depends on a user interface.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod commands;
pub mod edit;
pub mod jobs;
pub mod session;

pub use commands::{COMMANDS, CommandSpec, execute};
pub use session::Session;

/// Where the desktop app writes the per-session token of its control channel: an owner-only file in the user's
/// runtime folder. Loopback TCP cannot tell local users apart, so every control request must carry this token.
pub fn control_token_path(port: u16) -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("instaoptics-control-{port}.token"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn edits_through_commands_and_undoes_them() {
        let mut s = Session::new(None);
        execute(&mut s, "sample", &json!({ "id": "cooke" })).unwrap();
        let before = s.system().surfaces[0].radius;
        execute(&mut s, "surface.set", &json!({ "surface": 1, "radius": 25.0, "material": "n-bk7" })).unwrap();
        assert_eq!(s.system().surfaces[0].radius, 25.0);
        assert_eq!(s.system().surfaces[0].material, "N-BK7");
        assert!(s.is_dirty());
        execute(&mut s, "undo", &json!({})).unwrap();
        assert_eq!(s.system().surfaces[0].radius, before);
        assert!(execute(&mut s, "surface.set", &json!({ "surface": 1, "material": "NOPE" })).is_err());
    }

    #[test]
    fn analyzes_and_renders_with_default_parameters() {
        let mut s = Session::new(None);
        execute(&mut s, "sample", &json!({ "id": "cooke" })).unwrap();
        let mtf = execute(&mut s, "analyze", &json!({ "request": { "kind": "mtf" } })).unwrap();
        assert!(mtf["cutoff"].as_f64().unwrap() > 0.0);
        let png = execute(&mut s, "render", &json!({ "view": "spot", "width": 400, "height": 300 })).unwrap();
        assert!(png["pngBase64"].as_str().unwrap().starts_with("iVBOR"));
    }

    #[test]
    fn optimizes_a_variable_and_keeps_undo() {
        let mut s = Session::new(None);
        execute(&mut s, "sample", &json!({ "id": "cooke" })).unwrap();
        execute(&mut s, "variable", &json!({ "surface": 6, "parameter": "thickness" })).unwrap();
        let r = execute(&mut s, "optimize", &json!({ "cycles": 5 })).unwrap();
        assert!(r["finalMerit"].as_f64().unwrap() <= r["initialMerit"].as_f64().unwrap());
        assert!(s.can_undo());
    }
}
