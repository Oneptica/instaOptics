//! Every user-visible action is a command: an id, a label, its menu, a shortcut, a parameter description and a JSON
//! `run`. Menus, shortcuts, the CLI, the control channel and MCP all reach these same commands. Commands never open
//! dialogs; the desktop asks for a missing path first and then runs the command with it.
//!
//! Surfaces are numbered as in the Lens Data editor: surface 1 is the first surface after the object.

use std::path::PathBuf;

use instaoptics_core::configs::{ConfigParameter, ConfigValue};
use instaoptics_core::tolerance::ToleranceSettings;
use instaoptics_core::{AnalysisRequest, LensSystem};
use serde_json::{Map, Value, json};

use crate::edit::{self, VariableKey};
use crate::session::Session;

pub type Run = fn(&mut Session, &Value) -> Result<Value, String>;

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Top-level menu, or "" for commands reached only from panels, scripts and agents.
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub run: Run,
}

macro_rules! cmd {
    ($id:literal, $label:literal, $menu:literal, $shortcut:expr, $params:literal, $run:expr) => {
        CommandSpec { id: $id, label: $label, menu: $menu, shortcut: $shortcut, params: $params, run: $run }
    };
}

pub static COMMANDS: &[CommandSpec] = &[
    cmd!("new", "New System", "File", Some("Ctrl+N"), "{}", |s, _| {
        s.load(instaoptics_core::system::new_system(), None);
        Ok(Value::Null)
    }),
    cmd!("open", "Open…", "File", Some("Ctrl+O"), "{path}: an instaOptics lens (.iol) or a bare lens JSON", open),
    cmd!("save", "Save", "File", Some("Ctrl+S"), "{path?}: saves to the current file without a path", save),
    cmd!("saveas", "Save As…", "File", Some("Ctrl+Shift+S"), "{path}", save),
    cmd!("import.zmx", "Import Zemax (.zmx)…", "File", None, "{path}; returns the import notes", import_zmx),
    cmd!("export.zmx", "Export Zemax (.zmx)…", "File", None, "{path}", |s, p| {
        write(&path(p)?, instaoptics_io::zmx::export_zmx(s.system()).as_bytes())?;
        Ok(Value::Null)
    }),
    cmd!("sample", "Open Sample", "", None, "{id}: one of `samples`", |s, p| {
        let id = text(p, "id")?;
        let (_, system) = instaoptics_core::system::samples().into_iter().find(|(name, _)| *name == id).ok_or_else(|| format!("No sample {id}"))?;
        s.load(system, None);
        Ok(Value::Null)
    }),
    cmd!("samples", "Sample Lenses", "", None, "{}: ids and names of the built-in lenses", |_, _| {
        Ok(Value::Array(instaoptics_core::system::samples().into_iter().map(|(id, sys)| json!({ "id": id, "name": sys.name, "surfaces": sys.surfaces.len() })).collect()))
    }),
    cmd!("undo", "Undo", "Edit", Some("Ctrl+Z"), "{}", |s, _| Ok(json!(s.undo()))),
    cmd!("redo", "Redo", "Edit", Some("Ctrl+Y"), "{}", |s, _| Ok(json!(s.redo()))),
    cmd!("inspect", "Inspect", "", None, "{}: the lens, its paraxial data, file and history state", inspect),
    cmd!(
        "system.set",
        "System Settings",
        "",
        None,
        "{name?, objectDistance? (null: infinity), apertureType? (entrancePupilDiameter|imageFNumber|workingFNumber|objectNa|floatByStop), apertureValue?, entrancePupilDiameter?, fields?, fieldType? (angle|objectHeight|imageHeight), wavelengths? (µm), primaryWavelength? (0-based), rayAiming?, targetEfl?}",
        system_set
    ),
    cmd!(
        "surface.set",
        "Set Surface",
        "",
        None,
        "{surface, radius? (0 or \"Infinity\": flat), thickness?, material? (catalog name, nd/vd, MIRROR, blank: air), conic?, semiDiameter? (null: automatic), coating?, solve? (\"M [height]\" | \"Pr|t|k<n> [scale] [offset]\" | \"\"), a4?, a6?, a8?, a10?, coordinateBreak? {decenter:[x,y], tilt:[x,y,z]}}. A parameter in the configuration table changes the active configuration.",
        surface_set
    ),
    cmd!("surface.insert", "Insert Surface", "", None, "{at?}: new flat surface before surface `at`; appends without it", |s, p| {
        let n = s.system().surfaces.len();
        let at = opt_surface(p, "at", n + 1)?.unwrap_or(n);
        s.edit(|sys| {
            edit::insert_surface(sys, at);
            true
        });
        Ok(json!({ "surface": at + 1 }))
    }),
    cmd!("surface.delete", "Delete Surface", "", None, "{surface}", |s, p| {
        let i = surface(s, p, "surface")?;
        Ok(json!(s.edit(|sys| edit::delete_surface(sys, i))))
    }),
    cmd!("stop", "Make Stop", "", None, "{surface}: makes it the aperture stop", |s, p| {
        let i = surface(s, p, "surface")?;
        Ok(json!(s.edit(|sys| std::mem::replace(&mut sys.stop_index, i) != i)))
    }),
    cmd!("variable", "Variable", "", None, "{surface, parameter (radius|thickness|conic|a4|a6|a8|a10), on?}: toggles without `on`", |s, p| {
        let i = surface(s, p, "surface")?;
        let key = VariableKey::parse(&text(p, "parameter")?).ok_or("parameter is radius, thickness, conic, a4, a6, a8 or a10")?;
        let on = match p.get("on").and_then(Value::as_bool) {
            Some(on) => on,
            None => !edit::is_variable(&s.system().surfaces[i], key),
        };
        Ok(json!(s.edit(|sys| edit::set_variable(sys, i, key, on))))
    }),
    cmd!("variables.clear", "Remove All Variables", "Optimize", None, "{}", |s, _| Ok(json!(s.edit(edit::clear_variables)))),
    cmd!("coordbreak", "Coordinate Break", "", None, "{surface}: turns the surface into a coordinate break, or back", |s, p| {
        let i = surface(s, p, "surface")?;
        Ok(json!(s.edit(|sys| edit::toggle_coordinate_break(sys, i))))
    }),
    cmd!("config.add", "Add Configuration", "", None, "{}: creates the table with two configurations the first time", |s, _| {
        s.edit(|sys| {
            edit::add_configuration(sys);
            true
        });
        Ok(Value::Null)
    }),
    cmd!("config.remove", "Remove Configuration", "", None, "{index? (1-based, default the active one)}", |s, p| {
        let active = edit::active_configuration(s.system());
        let index = p.get("index").and_then(Value::as_u64).map_or(active, |i| (i as usize).saturating_sub(1));
        Ok(json!(s.edit(|sys| edit::remove_configuration(sys, index))))
    }),
    cmd!("config.active", "Active Configuration", "", None, "{index (1-based)}", |s, p| {
        let index = (number(p, "index")? as usize).saturating_sub(1);
        Ok(json!(s.edit(|sys| sys.configs.as_mut().is_some_and(|c| index < c.names.len() && std::mem::replace(&mut c.active, index) != index))))
    }),
    cmd!("config.rename", "Rename Configuration", "", None, "{index (1-based), name}", |s, p| {
        let (index, name) = ((number(p, "index")? as usize).saturating_sub(1), text(p, "name")?);
        Ok(json!(s.edit(|sys| sys.configs.as_mut().and_then(|c| c.names.get_mut(index)).is_some_and(|n| std::mem::replace(n, name.clone()) != name))))
    }),
    cmd!("config.row.add", "Add Configuration Row", "", None, "{surface, parameter (radius|thickness|conic|material|decenterX|decenterY|tiltX|tiltY)}", |s, p| {
        let i = surface(s, p, "surface")?;
        let parameter: ConfigParameter = serde_json::from_value(p.get("parameter").cloned().unwrap_or(Value::Null)).map_err(|e| format!("parameter: {e}"))?;
        Ok(json!(s.edit(|sys| edit::add_config_row(sys, i, parameter))))
    }),
    cmd!("config.row.remove", "Remove Configuration Row", "", None, "{row (1-based)}", |s, p| {
        let row = (number(p, "row")? as usize).saturating_sub(1);
        Ok(json!(s.edit(|sys| sys.configs.as_mut().is_some_and(|c| row < c.rows.len() && { c.rows.remove(row); true }))))
    }),
    cmd!("config.value", "Configuration Value", "", None, "{row (1-based), config (1-based), value (number, or text for material)}", |s, p| {
        let (row, config) = ((number(p, "row")? as usize).saturating_sub(1), (number(p, "config")? as usize).saturating_sub(1));
        let value = match p.get("value") {
            Some(Value::String(t)) => ConfigValue::Text(t.trim().to_uppercase()),
            Some(v) => ConfigValue::Number(v.as_f64().ok_or("value is a number or text")?),
            None => return Err("value is required".into()),
        };
        Ok(json!(s.edit(|sys| edit::set_config_value(sys, row, config, value))))
    }),
    cmd!("optimization.set", "Merit Function Settings", "", None, "{objective? (spot|wavefront|none), rings?, maxTotalTrack?, minBackFocus?, maxChiefRayAngle?, minGlassCenter?, minGlassEdge?, minAir?, operands?}; null clears a value", |s, p| {
        let mut value = serde_json::to_value(&s.system().optimization).map_err(|e| e.to_string())?;
        merge(&mut value, p);
        let settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
        Ok(json!(s.edit(|sys| std::mem::replace(&mut sys.optimization, settings) != sys.optimization)))
    }),
    cmd!("operand.add", "Add Operand", "", None, "{kind (efl|totalTrack|backFocus|fNumber|imageHeight|chiefRayAngle|distortion|spotRadius|thickness|radius), target, relation? (equal|atMost|atLeast), weight?, surface? (1-based), field? (1-based), config? (1-based)}", |s, p| {
        let operand = operand(p)?;
        s.edit(|sys| {
            sys.optimization.operands.push(operand);
            true
        });
        Ok(json!({ "index": s.system().optimization.operands.len() }))
    }),
    cmd!("operand.set", "Set Operand", "", None, "{index (1-based), …fields of operand.add}", |s, p| {
        let index = (number(p, "index")? as usize).saturating_sub(1);
        let current = s.system().optimization.operands.get(index).ok_or_else(|| format!("No operand {}", index + 1))?;
        let mut value = operand_json(current);
        merge(&mut value, p);
        let next = operand(&value)?;
        Ok(json!(s.edit(|sys| std::mem::replace(&mut sys.optimization.operands[index], next.clone()) != next)))
    }),
    cmd!("operand.remove", "Remove Operand", "", None, "{index (1-based)}", |s, p| {
        let index = (number(p, "index")? as usize).saturating_sub(1);
        Ok(json!(s.edit(|sys| index < sys.optimization.operands.len() && { sys.optimization.operands.remove(index); true })))
    }),
    cmd!("optimize", "Optimize", "Optimize", None, "{cycles? (default 50)}: damped least squares on the variables; undo restores the starting lens", |s, p| {
        let cycles = p.get("cycles").and_then(Value::as_u64).unwrap_or(50).clamp(1, 10_000) as usize;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let base = s.system().clone();
        let outcome = crate::jobs::optimize(&base, cycles, &cancel, |_, _, _| {})?;
        s.preview(outcome.system.clone());
        s.commit(base);
        Ok(json!({ "initialMerit": outcome.initial_merit, "finalMerit": outcome.final_merit, "cycles": outcome.cycles, "variables": outcome.variables }))
    }),
    cmd!("overview", "Paraxial Data", "", None, "{}: paraxial data, pupils, apertures and resolved surfaces", |s, _| {
        let overview = instaoptics_core::overview(s.system(), 7).map_err(|e| e.0)?;
        to_json(&overview)
    }),
    cmd!("analyze", "Analyze", "", None, "{request}: an analysis request, e.g. {\"kind\":\"spot\",\"rings\":6}; see `analyses`", |s, p| {
        let request = request(p)?;
        to_json(&instaoptics_core::analyze(s.system(), &request).map_err(|e| e.0)?)
    }),
    cmd!("analyses", "Analysis Kinds", "", None, "{}: the analysis requests `analyze` and `render` accept, with default parameters", |_, _| Ok(analyses())),
    cmd!("render", "Render", "", None, "{view (\"layout\" or an analysis request), width? (1200), height? (800), theme? (dark|light), path?}: a PNG; returns pngBase64 without a path", render),
    cmd!("tolerance.sensitivity", "Sensitivity", "Tolerance", None, "{radius? (%), thickness? (mm), decenter? (mm), tilt? (arcmin), index?, abbe? (%), compensator? (focus|none)}", |s, p| {
        let settings = tolerance_settings(p)?;
        s.system().validate().map_err(|e| e.0)?;
        to_json(&instaoptics_core::tolerance::sensitivity(s.system(), &settings))
    }),
    cmd!("tolerance.montecarlo", "Monte Carlo", "Tolerance", None, "{…sensitivity settings, trials? (500), seed? (1)}: RMS spot radius of each trial (mm)", |s, p| {
        let settings = tolerance_settings(p)?;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        to_json(&crate::jobs::monte_carlo(s.system(), &settings, &cancel, |_| {})?)
    }),
    cmd!("catalog.add", "Add Glass Catalog…", "Tools", None, "{path}: an OpticStudio .agf catalog", |s, p| Ok(json!(s.add_catalog(&path(p)?)?))),
    cmd!("catalog.remove", "Remove Glass Catalog", "", None, "{name}", |s, p| Ok(json!(s.remove_catalog(&text(p, "name")?)))),
    cmd!("glasses", "Glasses", "", None, "{filter?, catalog?, limit? (200)}: glasses whose name contains `filter`", |s, p| {
        let filter = p.get("filter").and_then(Value::as_str).unwrap_or("").to_uppercase();
        let catalog = p.get("catalog").and_then(Value::as_str);
        let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(200) as usize;
        let list: Vec<_> = s.glasses().into_iter().filter(|g| g.name.to_uppercase().contains(&filter) && catalog.is_none_or(|c| g.catalog == c)).take(limit).collect();
        to_json(&list)
    }),
];

pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Runs a command by id with JSON parameters (an object, or null for none).
pub fn execute(session: &mut Session, id: &str, params: &Value) -> Result<Value, String> {
    let spec = find(id).ok_or_else(|| format!("Unknown command {id}; `commands` lists them"))?;
    let empty = Value::Object(Map::new());
    let params = if params.is_null() { &empty } else { params };
    if !params.is_object() {
        return Err(format!("{id}: parameters must be a JSON object"));
    }
    (spec.run)(session, params)
}

/// The command catalog as JSON.
pub fn catalog() -> Value {
    Value::Array(COMMANDS.iter().map(|c| json!({ "id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut, "params": c.params })).collect())
}

// ---------- parameters ----------

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn text(p: &Value, key: &str) -> Result<String, String> {
    p.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("{key} is required"))
}

fn number(p: &Value, key: &str) -> Result<f64, String> {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| format!("{key} must be a number"))
}

fn path(p: &Value) -> Result<PathBuf, String> {
    text(p, "path").map(PathBuf::from)
}

fn opt_surface(p: &Value, key: &str, count: usize) -> Result<Option<usize>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| format!("{key} is a surface number"))? as usize;
            if n == 0 || n > count {
                return Err(format!("{key}: no surface {n}"));
            }
            Ok(Some(n - 1))
        }
    }
}

fn surface(s: &Session, p: &Value, key: &str) -> Result<usize, String> {
    opt_surface(p, key, s.system().surfaces.len())?.ok_or_else(|| format!("{key} is required"))
}

/// Shallow merge of `patch`'s keys into `target`; null removes a key.
fn merge(target: &mut Value, patch: &Value) {
    let (Some(t), Some(p)) = (target.as_object_mut(), patch.as_object()) else { return };
    for (k, v) in p {
        if v.is_null() {
            t.remove(k);
        } else {
            t.insert(k.clone(), v.clone());
        }
    }
}

fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn request(p: &Value) -> Result<AnalysisRequest, String> {
    let value = p.get("request").cloned().ok_or("request is required")?;
    serde_json::from_value(fill_request(value)).map_err(|e| format!("request: {e}"))
}

/// Default parameters for each analysis kind, so `{"kind":"mtf"}` is enough.
pub fn default_request(kind: &str) -> Option<Value> {
    Some(match kind {
        "spot" => json!({ "kind": "spot", "rings": 6 }),
        "rayFan" => json!({ "kind": "rayFan", "samples": 121 }),
        "seidel" => json!({ "kind": "seidel" }),
        "fieldCurves" => json!({ "kind": "fieldCurves", "samples": 121 }),
        "wavefront" => json!({ "kind": "wavefront", "field": 0, "wavelength": 0, "size": 192 }),
        "mtf" => json!({ "kind": "mtf", "size": 128, "points": 201, "maxFrequency": null }),
        "illumination" => json!({ "kind": "illumination", "samples": 91 }),
        "psf" => json!({ "kind": "psf", "field": 0, "samples": 128, "padding": 4, "crop": 128 }),
        "throughFocus" => json!({ "kind": "throughFocus", "frequency": 50.0, "range": 0.2, "steps": 61 }),
        "mtfVsField" => json!({ "kind": "mtfVsField", "frequencies": [10.0, 30.0, 50.0], "samples": 41 }),
        "chromaticFocalShift" => json!({ "kind": "chromaticFocalShift", "samples": 81 }),
        "footprint" => json!({ "kind": "footprint", "surface": 0, "rings": 8 }),
        "coating" => json!({ "kind": "coating", "surface": 0, "wavelength": 0, "maxAngle": 80.0, "points": 81 }),
        "polarization" => json!({ "kind": "polarization", "field": 0, "wavelength": 0, "grid": 129, "input": { "kind": "unpolarized" } }),
        "transmissionByField" => json!({ "kind": "transmissionByField", "wavelength": 0, "grid": 33, "input": { "kind": "unpolarized" } }),
        "meritFunction" => json!({ "kind": "meritFunction" }),
        "pop" => json!({ "kind": "pop", "settings": { "source": { "kind": "gaussian", "radius": 1.0, "waist": 0.0 }, "samples": 256 } }),
        _ => return None,
    })
}

fn fill_request(value: Value) -> Value {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else { return value };
    let Some(mut full) = default_request(kind) else { return value };
    merge(&mut full, &value);
    full
}

fn analyses() -> Value {
    let kinds = ["spot", "rayFan", "mtf", "psf", "wavefront", "throughFocus", "mtfVsField", "footprint", "fieldCurves", "seidel", "chromaticFocalShift", "illumination", "coating", "polarization", "transmissionByField", "pop", "meritFunction"];
    Value::Array(kinds.iter().filter_map(|k| default_request(k)).collect())
}

fn tolerance_settings(p: &Value) -> Result<ToleranceSettings, String> {
    let mut value = json!({ "radius": 0.2, "thickness": 0.05, "decenter": 0.02, "tilt": 1.0, "index": 0.0005, "abbe": 0.5, "compensator": "focus", "trials": 500, "seed": 1 });
    merge(&mut value, p);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn operand(p: &Value) -> Result<instaoptics_core::system::Operand, String> {
    // Surface, field and configuration numbers are 1-based in commands and 0-based in the lens model.
    let mut value = p.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.remove("index");
        for key in ["surface", "field", "config"] {
            if let Some(n) = obj.get(key).and_then(Value::as_u64) {
                obj.insert(key.into(), json!(n.saturating_sub(1)));
            }
        }
    }
    serde_json::from_value(value).map_err(|e| format!("operand: {e}"))
}

fn operand_json(o: &instaoptics_core::system::Operand) -> Value {
    let mut value = serde_json::to_value(o).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        for key in ["surface", "field", "config"] {
            if let Some(n) = obj.get(key).and_then(Value::as_u64) {
                obj.insert(key.into(), json!(n + 1));
            }
        }
    }
    value
}

// ---------- command bodies ----------

fn open(s: &mut Session, p: &Value) -> Result<Value, String> {
    let path = path(p)?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let system = instaoptics_io::parse_document(&text).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    s.load(system, Some(path));
    Ok(Value::Null)
}

fn save(s: &mut Session, p: &Value) -> Result<Value, String> {
    let path = match p.get("path").and_then(Value::as_str) {
        Some(path) => PathBuf::from(path),
        None => s.path.clone().ok_or("The lens has no file yet; give a path")?,
    };
    let text = instaoptics_io::serialize_document(s.system()).map_err(|e| e.to_string())?;
    write(&path, text.as_bytes())?;
    s.mark_saved(path.clone());
    Ok(json!(path.to_string_lossy()))
}

fn import_zmx(s: &mut Session, p: &Value) -> Result<Value, String> {
    let path = path(p)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let imported = instaoptics_io::zmx::import_zmx(&instaoptics_io::decode_text(&bytes), &|name| s.known_glass(name)).map_err(|e| format!("Could not import {}: {e}", path.display()))?;
    s.load(imported.system, None);
    Ok(json!({ "warnings": imported.warnings }))
}

fn inspect(s: &mut Session, _: &Value) -> Result<Value, String> {
    let overview = instaoptics_core::overview(s.system(), 3).ok();
    Ok(json!({
        "path": s.path.as_ref().map(|p| p.to_string_lossy()),
        "dirty": s.is_dirty(),
        "canUndo": s.can_undo(),
        "canRedo": s.can_redo(),
        "system": s.system(),
        "paraxial": overview.as_ref().map(|o| &o.paraxial),
        "error": instaoptics_core::overview(s.system(), 1).err().map(|e| e.0),
    }))
}

fn system_set(s: &mut Session, p: &Value) -> Result<Value, String> {
    let mut value = serde_json::to_value(s.system()).map_err(|e| e.to_string())?;
    let patch = p.as_object().cloned().unwrap_or_default();
    for (k, v) in &patch {
        if matches!(k.as_str(), "surfaces" | "stopIndex" | "configs" | "optimization" | "glasses") {
            return Err(format!("{k} is changed with its own commands"));
        }
        // objectDistance: null means an object at infinity, so it is kept rather than removed.
        if v.is_null() && k != "objectDistance" {
            value.as_object_mut().map(|o| o.remove(k));
        } else if let Some(o) = value.as_object_mut() {
            o.insert(k.clone(), v.clone());
        }
    }
    let next: LensSystem = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if next.wavelengths.is_empty() || next.fields.is_empty() {
        return Err("A system needs at least one field and one wavelength".into());
    }
    let primary = next.primary_wavelength.min(next.wavelengths.len() - 1);
    Ok(json!(s.edit(|sys| {
        let mut next = next;
        next.primary_wavelength = primary;
        std::mem::replace(sys, next) != *sys
    })))
}

fn parse_radius(v: &Value) -> Result<f64, String> {
    match v {
        Value::String(t) if ["inf", "infinity", "∞", "-inf", "-infinity"].contains(&t.trim().to_lowercase().as_str()) => Ok(0.0),
        _ => v.as_f64().filter(|r| r.is_finite()).ok_or_else(|| "radius must be a number or Infinity".into()),
    }
}

fn surface_set(s: &mut Session, p: &Value) -> Result<Value, String> {
    let i = surface(s, p, "surface")?;
    let n = s.system().surfaces.len();
    let obj = p.as_object().cloned().unwrap_or_default();
    let num = |k: &str| -> Result<Option<f64>, String> {
        match obj.get(k) {
            None => Ok(None),
            Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or_else(|| format!("{k} must be a number")),
        }
    };
    let radius = obj.get("radius").map(parse_radius).transpose()?;
    let thickness = num("thickness")?;
    let conic = num("conic")?;
    let material = obj.get("material").map(|v| v.as_str().map(|t| if t.trim().is_empty() { "AIR".to_string() } else { t.trim().to_uppercase() }).ok_or("material is text")).transpose()?;
    if let Some(m) = &material
        && !s.is_known_material(m)
    {
        return Err(format!("Unknown material {m}"));
    }
    let solve = obj.get("solve").map(|v| edit::parse_solve(v.as_str().unwrap_or(""), n)).transpose()?;
    if let Some(Some(instaoptics_core::configs::Solve::Pickup { surface: t, .. })) = &solve
        && *t == i
    {
        return Err("A pickup cannot copy its own surface".into());
    }
    let semi = match obj.get("semiDiameter") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(v.as_f64().filter(|x| *x > 0.0 && x.is_finite()).ok_or("semiDiameter must be positive")?)),
    };
    let coating = obj.get("coating").map(|v| v.as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string));
    let aspheric: Vec<(usize, f64)> = ["a4", "a6", "a8", "a10"].iter().enumerate().filter_map(|(t, k)| num(k).transpose().map(|v| v.map(|v| (t, v)))).collect::<Result<_, _>>()?;
    let cb: Option<instaoptics_core::system::CoordinateBreak> = obj.get("coordinateBreak").map(|v| serde_json::from_value(v.clone()).map_err(|e| format!("coordinateBreak: {e}"))).transpose()?;
    let changed = s.edit(|sys| {
        // Parameters in the configuration table are edited in the active configuration.
        let active = edit::active_configuration(sys);
        let routed = |sys: &mut LensSystem, parameter: ConfigParameter, value: ConfigValue| -> bool {
            match edit::config_row(sys, i, parameter) {
                Some(row) => {
                    edit::set_config_value(sys, row, active, value);
                    true
                }
                None => false,
            }
        };
        if let Some(r) = radius
            && !routed(sys, ConfigParameter::Radius, ConfigValue::Number(r))
        {
            sys.surfaces[i].radius = r;
        }
        if let Some(t) = thickness
            && !routed(sys, ConfigParameter::Thickness, ConfigValue::Number(t))
        {
            sys.surfaces[i].thickness = t;
        }
        if let Some(k) = conic
            && !routed(sys, ConfigParameter::Conic, ConfigValue::Number(k))
        {
            sys.surfaces[i].conic = k;
        }
        if let Some(m) = &material
            && !routed(sys, ConfigParameter::Material, ConfigValue::Text(m.clone()))
        {
            sys.surfaces[i].material = m.clone();
        }
        let surface = &mut sys.surfaces[i];
        if let Some(solve) = solve {
            surface.solve = solve;
        }
        if let Some(semi) = semi {
            surface.semi_diameter = semi;
        }
        if let Some(coating) = coating {
            surface.coating = coating;
        }
        for (t, v) in &aspheric {
            edit::set_aspheric(surface, *t, *v);
        }
        if let Some(cb) = cb
            && surface.coordinate_break.is_some()
        {
            surface.coordinate_break = Some(cb);
        }
        true
    });
    Ok(json!(changed))
}

fn render(s: &mut Session, p: &Value) -> Result<Value, String> {
    use instaoptics_render::{figures, raster};
    let width = p.get("width").and_then(Value::as_u64).unwrap_or(1200).clamp(64, 8192) as u32;
    let height = p.get("height").and_then(Value::as_u64).unwrap_or(800).clamp(64, 8192) as u32;
    let theme = if p.get("theme").and_then(Value::as_str) == Some("light") { raster::Theme::LIGHT } else { raster::Theme::DARK };
    let view = p.get("view").cloned().unwrap_or(json!("layout"));
    let overview = instaoptics_core::overview(s.system(), 7).map_err(|e| e.0)?;
    let figure = if view.as_str() == Some("layout") {
        figures::layout_figure(&overview.layout, None)
    } else {
        let view = if let Some(kind) = view.as_str() { json!({ "kind": kind }) } else { view };
        let request: AnalysisRequest = serde_json::from_value(fill_request(view)).map_err(|e| format!("view: {e}"))?;
        let result = instaoptics_core::analyze(s.system(), &request).map_err(|e| e.0)?;
        figures::figure(&result, s.system(), Some(overview.paraxial.working_f_number), &figures::Options::default())
    };
    let png = raster::render_png(&figure, width, height, theme)?;
    match p.get("path").and_then(Value::as_str) {
        Some(path) => {
            write(std::path::Path::new(path), &png)?;
            Ok(json!({ "path": path, "width": width, "height": height }))
        }
        None => Ok(json!({ "pngBase64": base64(&png), "width": width, "height": height })),
    }
}

/// Standard base64 (RFC 4648) for images in JSON replies.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
