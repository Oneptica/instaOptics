//! The open lens: the system, undo history, file path and saved state, and the loaded glass catalogs.

use std::path::{Path, PathBuf};

use instaoptics_core::LensSystem;
use instaoptics_core::glass::{self, GlassDef};

const HISTORY_LIMIT: usize = 200;

pub struct Catalog {
    pub name: String,
    pub glasses: Vec<GlassDef>,
}

/// One row of the glass list: built-in glasses have no formula number of their own (Sellmeier 1).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassInfo {
    pub name: String,
    pub nd: f64,
    pub vd: f64,
    pub catalog: String,
    pub formula: u8,
    pub range: Option<[f64; 2]>,
}

pub const BUILT_IN: &str = "Built-in (Schott)";

pub struct Session {
    system: LensSystem,
    saved: LensSystem,
    past: Vec<LensSystem>,
    future: Vec<LensSystem>,
    pub path: Option<PathBuf>,
    pub catalogs: Vec<Catalog>,
    /// Folder the .agf catalogs are copied into; None keeps them for this session only.
    pub catalog_dir: Option<PathBuf>,
}

impl Session {
    pub fn new(catalog_dir: Option<PathBuf>) -> Self {
        let system = instaoptics_core::system::new_system();
        let mut session = Self { saved: system.clone(), system, past: Vec::new(), future: Vec::new(), path: None, catalogs: Vec::new(), catalog_dir };
        session.reload_catalogs();
        session
    }

    pub fn system(&self) -> &LensSystem {
        &self.system
    }

    pub fn is_dirty(&self) -> bool {
        self.system != self.saved
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Applies an edit and records it for undo. The closure returns whether it changed anything.
    pub fn edit(&mut self, change: impl FnOnce(&mut LensSystem) -> bool) -> bool {
        let mut next = self.system.clone();
        if !change(&mut next) {
            return false;
        }
        self.attach_glasses(&mut next);
        if next == self.system {
            return false;
        }
        let previous = std::mem::replace(&mut self.system, next);
        self.past.push(previous);
        if self.past.len() > HISTORY_LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
        true
    }

    /// Shows a system without recording history (live optimizer progress).
    pub fn preview(&mut self, system: LensSystem) {
        self.system = system;
    }

    /// Records `base` as the undo point for the current, previewed system.
    pub fn commit(&mut self, base: LensSystem) {
        if base != self.system {
            self.past.push(base);
            self.future.clear();
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.past.pop() else { return false };
        self.future.push(std::mem::replace(&mut self.system, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else { return false };
        self.past.push(std::mem::replace(&mut self.system, next));
        true
    }

    pub fn load(&mut self, mut system: LensSystem, path: Option<PathBuf>) {
        self.attach_glasses(&mut system);
        self.saved = system.clone();
        self.system = system;
        self.past.clear();
        self.future.clear();
        self.path = path;
    }

    pub fn mark_saved(&mut self, path: PathBuf) {
        self.saved = self.system.clone();
        self.path = Some(path);
    }

    pub fn file_name(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).map_or_else(|| self.system.name.clone(), |n| n.to_string_lossy().into_owned())
    }

    // ---------- glasses ----------

    pub fn reload_catalogs(&mut self) {
        self.catalogs.clear();
        let Some(dir) = &self.catalog_dir else { return };
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("agf"))).collect();
        files.sort();
        for path in files {
            if let Ok(bytes) = std::fs::read(&path) {
                let name = path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                let glasses = instaoptics_io::agf::parse_agf(&instaoptics_io::decode_text(&bytes), &name);
                self.catalogs.push(Catalog { name, glasses });
            }
        }
    }

    /// Copies a catalog into the catalog folder (or keeps it in memory) and loads it.
    pub fn add_catalog(&mut self, path: &Path) -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().map_or_else(|| "catalog".to_string(), |s| s.to_string_lossy().into_owned());
        let glasses = instaoptics_io::agf::parse_agf(&instaoptics_io::decode_text(&bytes), &name);
        if glasses.is_empty() {
            return Err(format!("{} has no glasses", path.display()));
        }
        if let Some(dir) = &self.catalog_dir {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{name}.agf")), &bytes).map_err(|e| e.to_string())?;
        }
        self.catalogs.retain(|c| c.name != name);
        self.catalogs.push(Catalog { name: name.clone(), glasses });
        let mut system = self.system.clone();
        self.attach_glasses(&mut system);
        self.system = system;
        Ok(name)
    }

    pub fn remove_catalog(&mut self, name: &str) -> bool {
        let before = self.catalogs.len();
        self.catalogs.retain(|c| c.name != name);
        if let Some(dir) = &self.catalog_dir {
            let _ = std::fs::remove_file(dir.join(format!("{name}.agf")));
        }
        before != self.catalogs.len()
    }

    fn catalog_glass(&self, name: &str) -> Option<&GlassDef> {
        self.catalogs.iter().flat_map(|c| c.glasses.iter()).find(|g| g.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The catalog spelling of a glass name, built-in or loaded.
    pub fn known_glass(&self, name: &str) -> Option<String> {
        glass::find(name).map(|g| g.name.to_string()).or_else(|| self.catalog_glass(name).map(|g| g.name.clone()))
    }

    pub fn is_known_material(&self, material: &str) -> bool {
        glass::is_known_material(material) || self.system.glasses.iter().any(|g| g.name.eq_ignore_ascii_case(material.trim())) || self.catalog_glass(material).is_some()
    }

    pub fn glasses(&self) -> Vec<GlassInfo> {
        let built_in = glass::CATALOG.iter().map(|g| GlassInfo { name: g.name.to_string(), nd: g.nd, vd: g.vd, catalog: BUILT_IN.into(), formula: 2, range: None });
        let loaded = self.catalogs.iter().flat_map(|c| c.glasses.iter()).map(|g| GlassInfo { name: g.name.clone(), nd: g.nd, vd: g.vd, catalog: g.catalog.clone(), formula: g.formula, range: g.range });
        built_in.chain(loaded).collect()
    }

    /// Makes the system carry the definitions of the catalog glasses it uses and drop the ones it no longer uses, so a
    /// saved lens opens anywhere without the catalog.
    fn attach_glasses(&self, system: &mut LensSystem) {
        let mut wanted: Vec<GlassDef> = Vec::new();
        for s in &system.surfaces {
            let name = s.material.trim();
            if glass::is_air(name) || glass::is_mirror(name) || glass::find(name).is_some() || glass::parse_model_glass(name).is_some() {
                continue;
            }
            if wanted.iter().any(|g| g.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            let found = system.glasses.iter().find(|g| g.name.eq_ignore_ascii_case(name)).or_else(|| self.catalog_glass(name));
            if let Some(g) = found {
                wanted.push(g.clone());
            }
        }
        system.glasses = wanted;
    }
}
