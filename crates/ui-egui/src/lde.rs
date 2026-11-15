//! Lens Data editor: one row per surface, as in OpticStudio. Arrows move, Enter or F2 or typing edits, Tab moves
//! across, Insert adds a surface (Ctrl+Insert after), Delete removes one, Ctrl+T toggles a variable.

use crate::i18n::t;
use egui::{Align, Color32, Key, Layout, Modifiers, RichText, Sense, Ui, Vec2};
use instaoptics_core::configs::{ConfigParameter, Parameter};
use instaoptics_engine::edit::{self, VariableKey};
use serde_json::{Value, json};

use crate::app::Workbench;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Radius,
    Thickness,
    Material,
    Coating,
    Solve,
    Semi,
    Conic,
    A(usize),
}

const COLUMNS: [(Col, &str, f32); 11] = [
    (Col::Radius, "Radius", 96.0),
    (Col::Thickness, "Thickness", 96.0),
    (Col::Material, "Material", 92.0),
    (Col::Coating, "Coating", 80.0),
    (Col::Solve, "Solve", 64.0),
    (Col::Semi, "Clear Semi-Dia", 96.0),
    (Col::Conic, "Conic", 84.0),
    (Col::A(0), "A4", 104.0),
    (Col::A(1), "A6", 104.0),
    (Col::A(2), "A8", 104.0),
    (Col::A(3), "A10", 104.0),
];

/// On a coordinate break the parameter columns hold its decenters and tilts.
fn cb_label(col: Col) -> Option<&'static str> {
    Some(match col {
        Col::Conic => t("Decenter X"),
        Col::A(0) => t("Decenter Y"),
        Col::A(1) => t("Tilt X"),
        Col::A(2) => t("Tilt Y"),
        Col::A(3) => t("Tilt Z"),
        _ => return None,
    })
}

fn cb_index(col: Col) -> Option<usize> {
    match col {
        Col::Conic => Some(0),
        Col::A(i) => Some(i + 1),
        _ => None,
    }
}

fn variable_key(col: Col) -> Option<VariableKey> {
    match col {
        Col::Radius => Some(VariableKey::Radius),
        Col::Thickness => Some(VariableKey::Thickness),
        Col::Conic => Some(VariableKey::Conic),
        Col::A(i) => Some(VariableKey::Aspheric(i)),
        _ => None,
    }
}

pub struct State {
    row: usize,
    col: usize,
    editing: Option<(usize, usize, String, bool)>,
    focused: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { row: 1, col: 0, editing: None, focused: false }
    }
}

struct Cell {
    text: String,
    editable: bool,
    muted: bool,
    invalid: bool,
    note: Option<&'static str>,
    variable: Option<bool>,
}

fn fixed(v: f64) -> String {
    if !v.is_finite() {
        return "Infinity".into();
    }
    let s = format!("{v:.4}");
    if s == "-0.0000" { "0.0000".into() } else { s }
}

fn cell(wb: &Workbench, row: usize, col: Col) -> Cell {
    let system = wb.session.system();
    let n = system.surfaces.len();
    let plain = |text: String, editable: bool| Cell { text, editable, muted: false, invalid: false, note: None, variable: None };
    let muted = |text: &str| Cell { text: text.into(), editable: false, muted: true, invalid: false, note: None, variable: None };
    if row == 0 {
        return match col {
            Col::Radius => muted("Infinity"),
            Col::Thickness => plain(system.object_distance.map_or("Infinity".into(), fixed), true),
            _ => muted(""),
        };
    }
    if row == n + 1 {
        return match col {
            Col::Radius => muted("Infinity"),
            Col::Semi => muted(&wb.overview.as_ref().map_or(String::new(), |o| fixed(o.paraxial.image_height.abs()))),
            _ => muted(""),
        };
    }
    let i = row - 1;
    let s = &system.surfaces[i];
    if let Some(cb) = s.coordinate_break {
        return match (col, cb_index(col)) {
            (Col::Thickness, _) => plain(fixed(s.thickness), true),
            (_, Some(k)) => plain(fixed([cb.decenter[0], cb.decenter[1], cb.tilt[0], cb.tilt[1], cb.tilt[2]][k]), true),
            _ => muted(""),
        };
    }
    // Values after the active configuration and solves, as the engine sees them.
    let resolved = wb.overview.as_ref().and_then(|o| o.resolved_surfaces.get(i).filter(|_| o.resolved_surfaces.len() == n));
    let (radius, thickness, conic, material) = resolved.map_or((s.radius, s.thickness, s.conic, s.material.clone()), |r| (r.radius, r.thickness, r.conic, r.material.clone()));
    let solved = edit::solved_parameter(s.solve.as_ref());
    let config = |p: ConfigParameter| edit::config_row(system, i, p).is_some();
    let note = |p: Parameter, c: ConfigParameter| if solved == Some(p) { Some("S") } else if config(c) { Some("M") } else { None };
    let var = |c: Col, locked: bool| variable_key(c).map(|k| edit::is_variable(s, k) && !locked);
    match col {
        Col::Radius => {
            let n = note(Parameter::Radius, ConfigParameter::Radius);
            Cell { text: if radius == 0.0 { "Infinity".into() } else { fixed(radius) }, editable: solved != Some(Parameter::Radius), muted: false, invalid: false, note: n, variable: var(col, n.is_some()) }
        }
        Col::Thickness => {
            let n = note(Parameter::Thickness, ConfigParameter::Thickness);
            Cell { text: fixed(thickness), editable: solved != Some(Parameter::Thickness), muted: false, invalid: false, note: n, variable: var(col, n.is_some()) }
        }
        Col::Conic => {
            let n = note(Parameter::Conic, ConfigParameter::Conic);
            Cell { text: fixed(conic), editable: solved != Some(Parameter::Conic), muted: false, invalid: false, note: n, variable: var(col, n.is_some()) }
        }
        Col::Material => {
            let name = if instaoptics_core::glass::is_air(&material) { String::new() } else { material.clone() };
            Cell { text: name, editable: true, muted: false, invalid: !wb.session.is_known_material(&material), note: if config(ConfigParameter::Material) { Some("M") } else { None }, variable: None }
        }
        Col::Coating => plain(s.coating.clone().unwrap_or_default(), true),
        Col::Solve => plain(edit::format_solve(s.solve.as_ref()), true),
        Col::Semi => match s.semi_diameter {
            Some(v) => Cell { text: fixed(v), editable: true, muted: false, invalid: false, note: Some("U"), variable: None },
            None => plain(wb.overview.as_ref().and_then(|o| o.automatic_semi_diameters.get(i)).map_or(String::new(), |v| fixed(*v)), true),
        },
        Col::A(t) => {
            let v = s.aspheric.get(t).copied().unwrap_or(0.0);
            let variable = var(col, false);
            Cell { text: if v == 0.0 { "0".into() } else { format!("{v:.6e}") }, editable: true, muted: v == 0.0 && variable != Some(true), invalid: false, note: None, variable }
        }
    }
}

/// Text shown when editing starts: full precision, blank for an automatic aperture.
fn edit_text(wb: &Workbench, row: usize, col: Col) -> String {
    let system = wb.session.system();
    if row == 0 {
        return system.object_distance.map_or("Infinity".into(), |d| d.to_string());
    }
    let s = &system.surfaces[row - 1];
    if let (Some(cb), Some(k)) = (s.coordinate_break, cb_index(col)) {
        return [cb.decenter[0], cb.decenter[1], cb.tilt[0], cb.tilt[1], cb.tilt[2]][k].to_string();
    }
    match col {
        Col::Radius => if s.radius == 0.0 { "Infinity".into() } else { s.radius.to_string() },
        Col::Thickness => s.thickness.to_string(),
        Col::Conic => s.conic.to_string(),
        Col::Semi => s.semi_diameter.map_or(String::new(), |v| v.to_string()),
        Col::A(t) => s.aspheric.get(t).copied().unwrap_or(0.0).to_string(),
        _ => cell(wb, row, col).text,
    }
}

/// Applies an edited cell through the engine; false when the text is not a valid value.
fn apply(wb: &mut Workbench, row: usize, col: Col, text: &str) -> bool {
    let n = wb.session.system().surfaces.len();
    let number = || -> Option<Value> {
        let t = text.trim().to_lowercase();
        if ["inf", "infinity", "∞"].contains(&t.as_str()) { Some(json!("Infinity")) } else { t.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| json!(v)) }
    };
    if row == 0 {
        let distance = match number() {
            Some(Value::String(_)) => Value::Null,
            Some(v) if v.as_f64().is_some_and(|d| d > 0.0) => v,
            _ => return false,
        };
        return wb.run("system.set", json!({ "objectDistance": distance })).is_some();
    }
    if row > n {
        return false;
    }
    let s = &wb.session.system().surfaces[row - 1];
    let mut params = json!({ "surface": row });
    if let (Some(cb), Some(k)) = (s.coordinate_break, cb_index(col)) {
        let Some(v) = number().and_then(|v| v.as_f64()) else { return false };
        let mut values = [cb.decenter[0], cb.decenter[1], cb.tilt[0], cb.tilt[1], cb.tilt[2]];
        values[k] = v;
        params["coordinateBreak"] = json!({ "decenter": [values[0], values[1]], "tilt": [values[2], values[3], values[4]] });
    } else {
        let key = match col {
            Col::Radius => "radius",
            Col::Thickness => "thickness",
            Col::Conic => "conic",
            Col::Material => "material",
            Col::Coating => "coating",
            Col::Solve => "solve",
            Col::Semi => "semiDiameter",
            Col::A(t) => ["a4", "a6", "a8", "a10"][t],
        };
        params[key] = match col {
            Col::Material | Col::Coating | Col::Solve => json!(text.trim()),
            Col::Semi if text.trim().is_empty() => Value::Null,
            Col::Radius => match number() {
                Some(v) => v,
                None => return false,
            },
            _ => match number().and_then(|v| v.as_f64()) {
                Some(v) => json!(v),
                None => return false,
            },
        };
    }
    match instaoptics_engine::execute(&mut wb.session, "surface.set", &params) {
        Ok(_) => true,
        Err(e) => {
            wb.set_status(e, true);
            false
        }
    }
}

pub fn ui(wb: &mut Workbench, ui: &mut Ui) {
    let tk = wb.tokens;
    let n = wb.session.system().surfaces.len();
    let rows = n + 2;
    let st = &mut wb.lde;
    st.row = st.row.min(rows - 1);
    let (row, col) = (st.row, st.col);
    let surface = (row >= 1 && row <= n).then(|| row - 1);
    wb.selected_surface = surface;
    let mut command: Option<(&str, Value)> = None;
    ui.horizontal(|ui| {
        let at = surface.map_or(if row == 0 { 1 } else { n + 1 }, |s| s + 1);
        if ui.small_button(t("Insert")).on_hover_text(t("Insert a surface before (Insert)")).clicked() {
            command = Some(("surface.insert", json!({ "at": at })));
        }
        if ui.small_button(t("Insert After")).on_hover_text(t("Insert a surface after (Ctrl+Insert)")).clicked() {
            command = Some(("surface.insert", json!({ "at": (at + 1).min(n + 1) })));
        }
        if ui.add_enabled(surface.is_some() && n > 1, egui::Button::new(t("Delete")).small()).on_hover_text(t("Delete the surface (Delete)")).clicked() {
            command = Some(("surface.delete", json!({ "surface": at })));
        }
        ui.separator();
        let stop = wb.session.system().stop_index;
        if ui.add_enabled(surface.is_some_and(|s| s != stop), egui::Button::new(t("Make Stop")).small()).clicked() {
            command = Some(("stop", json!({ "surface": at })));
        }
        let key = variable_key(COLUMNS[col].0).filter(|_| surface.is_some());
        if ui.add_enabled(key.is_some(), egui::Button::new(t("Variable")).small()).on_hover_text(t("Toggle the selected cell as an optimization variable (Ctrl+T)")).clicked() {
            command = Some(("variable", json!({ "surface": at, "parameter": param_name(COLUMNS[col].0) })));
        }
        if ui.add_enabled(surface.is_some(), egui::Button::new(t("Coordinate Break")).small()).on_hover_text(t("Turn the surface into a coordinate break, or back")).clicked() {
            command = Some(("coordbreak", json!({ "surface": at })));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let system = wb.session.system();
            let mut info = match surface {
                None if row == 0 => t("Object").to_string(),
                None => t("Image").to_string(),
                Some(s) => format!("{} {}{}", t("Surface"), s + 1, if s == system.stop_index { format!(" ({})", t("stop")) } else { String::new() }),
            };
            if let Some(s) = surface
                && let Some(g) = instaoptics_core::glass::find(&system.surfaces[s].material)
            {
                info = format!("{info} · {} · nd {:.5} · vd {:.2}", g.name, g.nd, g.vd);
            }
            ui.label(RichText::new(info).small().color(tk.muted));
        });
    });
    ui.separator();
    let grid_rect = ui.available_rect_before_wrap();
    if ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !grid_rect.contains(p))) {
        wb.lde.focused = false;
    }
    let cells: Vec<Vec<Cell>> = (0..rows).map(|r| COLUMNS.iter().map(|(c, ..)| cell(wb, r, *c)).collect()).collect();
    let system = wb.session.system().clone();
    let mut clicked: Option<(usize, usize, bool)> = None;
    let mut toggle: Option<(usize, usize)> = None;
    let mut finished: Option<(bool, (i32, i32))> = None;
    let st = &mut wb.lde;
    let row_h = 20.0;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        let header = |ui: &mut Ui, text: &str, w: f32| {
            ui.add_sized([w, row_h], egui::Label::new(RichText::new(text).small().color(tk.muted)));
        };
        ui.horizontal(|ui| {
            header(ui, t("Surf"), 44.0);
            header(ui, t("Type"), 96.0);
            let cb = surface.is_some_and(|s| system.surfaces[s].coordinate_break.is_some());
            for (c, label, w) in COLUMNS {
                header(ui, if cb { cb_label(c).unwrap_or(t(label)) } else { t(label) }, w);
            }
        });
        for (r, row_cells) in cells.iter().enumerate() {
            let is_stop = r >= 1 && r - 1 == system.stop_index;
            let s = (r >= 1 && r <= n).then(|| &system.surfaces[r - 1]);
            let label = if r == 0 { "OBJ".to_string() } else if r == n + 1 { "IMA".into() } else if is_stop { "STO".into() } else { r.to_string() };
            let kind = s.map_or(t("Standard"), |s| if s.coordinate_break.is_some() { t("Coord Break") } else if s.aspheric.iter().any(|a| *a != 0.0) { t("Even Asphere") } else { t("Standard") });
            ui.horizontal(|ui| {
                let bg = if r == row { tk.selection.gamma_multiply(0.5) } else if r % 2 == 1 { tk.header.gamma_multiply(0.5) } else { Color32::TRANSPARENT };
                let start = ui.cursor().min;
                let width = 44.0 + 96.0 + COLUMNS.iter().map(|c| c.2).sum::<f32>();
                ui.painter().rect_filled(egui::Rect::from_min_size(start, Vec2::new(width, row_h)), 0.0, bg);
                let head = ui.add_sized([44.0, row_h], egui::Label::new(RichText::new(label).monospace().strong().color(if is_stop { tk.accent } else { tk.text })).sense(Sense::click()));
                if head.clicked() {
                    clicked = Some((r, col, false));
                }
                ui.add_sized([96.0, row_h], egui::Label::new(RichText::new(kind).small().color(tk.muted)));
                for (c, (cv, (_, _, w))) in row_cells.iter().zip(COLUMNS.iter()).enumerate() {
                    let size = Vec2::new(*w, row_h);
                    if let Some((er, ec, text, invalid)) = &mut st.editing
                        && *er == r
                        && *ec == c
                    {
                        let edit = egui::TextEdit::singleline(text).desired_width(*w - 4.0).font(egui::TextStyle::Monospace).text_color(if *invalid { tk.error } else { tk.text });
                        let resp = ui.add_sized(size, edit);
                        if !resp.has_focus() && !resp.lost_focus() {
                            resp.request_focus();
                        }
                        if resp.lost_focus() {
                            let (enter, tab, esc, shift) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Tab), i.key_pressed(Key::Escape), i.modifiers.shift));
                            finished = Some((esc, if enter { (1, 0) } else if tab { (0, if shift { -1 } else { 1 }) } else { (0, 0) }));
                        }
                        continue;
                    }
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let active = r == row && c == col;
                    if active && st.focused {
                        ui.painter().rect_stroke(rect.shrink(0.5), 0.0, egui::Stroke::new(1.0, tk.accent), egui::StrokeKind::Inside);
                    }
                    let color = if cv.invalid { tk.error } else if cv.muted { tk.muted } else { tk.text };
                    let numeric = !matches!(COLUMNS[c].0, Col::Material | Col::Coating | Col::Solve);
                    let text_rect = rect.shrink2(Vec2::new(14.0, 0.0));
                    let align = if numeric { egui::Align2::RIGHT_CENTER } else { egui::Align2::LEFT_CENTER };
                    let anchor = if numeric { text_rect.right_center() } else { rect.left_center() + Vec2::new(4.0, 0.0) };
                    ui.painter().text(anchor, align, &cv.text, egui::FontId::monospace(12.0), color);
                    if let Some(note) = cv.note {
                        ui.painter().text(rect.right_top() + Vec2::new(-2.0, 1.0), egui::Align2::RIGHT_TOP, note, egui::FontId::proportional(9.0), tk.accent);
                    }
                    if let Some(on) = cv.variable {
                        let vr = egui::Rect::from_min_size(rect.right_bottom() - Vec2::new(11.0, 11.0), Vec2::splat(10.0));
                        if on || resp.hovered() {
                            ui.painter().text(vr.center(), egui::Align2::CENTER_CENTER, "V", egui::FontId::proportional(9.0), if on { tk.variable } else { tk.muted });
                        }
                        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| vr.expand(2.0).contains(p)) {
                            toggle = Some((r, c));
                            continue;
                        }
                    }
                    if resp.double_clicked() && cv.editable {
                        clicked = Some((r, c, true));
                    } else if resp.clicked() {
                        clicked = Some((r, c, false));
                    }
                    if cv.invalid {
                        resp.on_hover_text(t("Unknown material"));
                    }
                }
            });
        }
    });
    // Mouse
    if let Some((r, c, edit)) = clicked {
        let st = &mut wb.lde;
        st.row = r;
        st.col = c;
        st.focused = true;
        if edit {
            let text = edit_text(wb, r, COLUMNS[c].0);
            wb.lde.editing = Some((r, c, text, false));
        }
    }
    if let Some((r, c)) = toggle {
        command = Some(("variable", json!({ "surface": r, "parameter": param_name(COLUMNS[c].0) })));
    }
    // Finishing an edit
    if let Some((cancel, (dr, dc))) = finished
        && let Some((r, c, text, _)) = wb.lde.editing.clone()
    {
        if cancel {
            wb.lde.editing = None;
        } else if apply(wb, r, COLUMNS[c].0, &text) {
            wb.lde.editing = None;
            move_by(&mut wb.lde, dr, dc, rows);
        } else if let Some(e) = &mut wb.lde.editing {
            e.3 = true;
        }
    }
    // Keyboard
    if wb.lde.focused && wb.lde.editing.is_none() && !ui.ctx().egui_wants_keyboard_input() {
        let (keys, typed, modifiers) = ui.input(|i| {
            let keys: Vec<Key> = [Key::ArrowUp, Key::ArrowDown, Key::ArrowLeft, Key::ArrowRight, Key::Home, Key::End, Key::PageUp, Key::PageDown, Key::Tab, Key::Enter, Key::F2, Key::Insert, Key::Delete, Key::T].into_iter().filter(|k| i.key_pressed(*k)).collect();
            let typed: Option<String> = i.events.iter().find_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None });
            (keys, typed, i.modifiers)
        });
        let st = &mut wb.lde;
        let at = st.row.clamp(1, n + 1);
        for key in keys {
            match key {
                Key::ArrowUp => move_by(st, -1, 0, rows),
                Key::ArrowDown => move_by(st, 1, 0, rows),
                Key::ArrowLeft => move_by(st, 0, -1, rows),
                Key::ArrowRight => move_by(st, 0, 1, rows),
                Key::Home => st.col = 0,
                Key::End => st.col = COLUMNS.len() - 1,
                Key::PageUp => st.row = 0,
                Key::PageDown => st.row = rows - 1,
                Key::Tab => move_by(st, 0, if modifiers.shift { -1 } else { 1 }, rows),
                Key::Enter | Key::F2 => {
                    if cells[st.row][st.col].editable {
                        let (r, c) = (st.row, st.col);
                        wb.lde.editing = Some((r, c, edit_text(wb, r, COLUMNS[c].0), false));
                        return;
                    }
                }
                Key::Insert => {
                    let at = if modifiers.command { (at + 1).min(n + 1) } else { at };
                    command = Some(("surface.insert", json!({ "at": at })));
                }
                Key::Delete if st.row >= 1 && st.row <= n => command = Some(("surface.delete", json!({ "surface": st.row }))),
                Key::T if modifiers.command && st.row >= 1 && st.row <= n => command = Some(("variable", json!({ "surface": st.row, "parameter": param_name(COLUMNS[st.col].0) }))),
                _ => {}
            }
        }
        if command.is_none()
            && let Some(text) = typed.filter(|t| !modifiers.command && !t.trim().is_empty())
            && cells[wb.lde.row][wb.lde.col].editable
        {
            wb.lde.editing = Some((wb.lde.row, wb.lde.col, text, false));
        }
    }
    if let Some((id, params)) = command {
        let insert = id == "surface.insert";
        if let Some(reply) = wb.run(id, params)
            && insert
            && let Some(s) = reply["surface"].as_u64()
        {
            wb.lde.row = s as usize;
        }
    }
    let _ = Modifiers::NONE;
}

fn param_name(col: Col) -> &'static str {
    match col {
        Col::Radius => "radius",
        Col::Thickness => "thickness",
        Col::Conic => "conic",
        Col::A(0) => "a4",
        Col::A(1) => "a6",
        Col::A(2) => "a8",
        Col::A(_) => "a10",
        _ => "",
    }
}

fn move_by(st: &mut State, dr: i32, dc: i32, rows: usize) {
    st.row = (st.row as i32 + dr).clamp(0, rows as i32 - 1) as usize;
    st.col = (st.col as i32 + dc).clamp(0, COLUMNS.len() as i32 - 1) as usize;
}
