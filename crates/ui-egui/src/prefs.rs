//! User settings in config.toml through craft-settings (platform folder, atomic saves, recovery).

use serde::{Deserialize, Serialize};

use crate::theme::ThemeChoice;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeChoice,
    /// Most recent first.
    pub recent: Vec<String>,
    /// The docked window layout as JSON (craft-ui docking workspace).
    pub workspace: String,
}

impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "instaoptics";
    const SCHEMA_VERSION: u32 = 1;
}

impl Prefs {
    pub fn push_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(10);
    }
}
