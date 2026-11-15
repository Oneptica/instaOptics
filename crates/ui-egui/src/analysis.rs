//! Analysis windows: a toolbar with the window's settings and exports, the figure, and its compute state.

use egui::{RichText, Ui};
use instaoptics_core::{AnalysisResult, LensSystem};
use instaoptics_render::Figure;
use instaoptics_render::figures::{self, Options, PolarizationView, field_label};
use serde_json::{Value, json};

use crate::app::Workbench;
use crate::panels::Panel;
use crate::worker::{JobKey, Output, tag};

pub struct AnalysisWindow {
    pub request: Value,
    pub options: Options,
    pub result: Option<AnalysisResult>,
    pub figure: Option<Figure>,
    pub error: Option<String>,
    pub ms: Option<f64>,
    /// Tag of the input the result was computed for.
    pub computed: u64,
}

impl AnalysisWindow {
    pub fn new(panel: Panel) -> Self {
        let request = panel.analysis_kind().and_then(instaoptics_engine::commands::default_request).unwrap_or(Value::Null);
        Self { request, options: Options::default(), result: None, figure: None, error: None, ms: None, computed: 0 }
    }

    pub fn receive(&mut self, tag: u64, ms: f64, result: Result<AnalysisResult, String>, system: &LensSystem, working: Option<f64>) {
        self.computed = tag;
        self.ms = Some(ms);
        match result {
            Ok(r) => {
                self.figure = Some(figures::figure(&r, system, working, &self.options));
                self.result = Some(r);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn rebuild(&mut self, system: &LensSystem, working: Option<f64>) {
        if let Some(r) = &self.result {
            self.figure = Some(figures::figure(r, system, working, &self.options));
        }
    }
}

/// A labelled combo box over (value, label) options; returns true when the value changed.
pub fn select<T: PartialEq + Clone>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, label: &str, value: &mut T, options: &[(T, String)]) -> bool {
    let current = options.iter().find(|(v, _)| v == value).map_or(String::new(), |(_, l)| l.clone());
    let mut changed = false;
    ui.label(RichText::new(label).small());
    egui::ComboBox::from_id_salt(id).selected_text(current).show_ui(ui, |ui| {
        for (v, l) in options {
            if ui.selectable_label(v == value, l).clicked() && v != value {
                *value = v.clone();
                changed = true;
            }
        }
    });
    changed
}

fn field_options(system: &LensSystem) -> Vec<(u64, String)> {
    system.fields.iter().enumerate().map(|(i, f)| (i as u64, format!("{}: {}", i + 1, field_label(*f, system.field_type)))).collect()
}

fn wavelength_options(system: &LensSystem) -> Vec<(u64, String)> {
    system.wavelengths.iter().enumerate().map(|(i, w)| (i as u64, format!("{}: {w:.4} µm", i + 1))).collect()
}

fn surface_options(system: &LensSystem) -> Vec<(u64, String)> {
    (0..system.surfaces.len()).map(|i| (i as u64, if i == system.stop_index { format!("{} (stop)", i + 1) } else { (i + 1).to_string() })).collect()
}

/// Index setting stored in the request JSON, clamped to the list it indexes.
fn index_select(ui: &mut Ui, id: (Panel, &str), label: &str, request: &mut Value, key: &str, options: &[(u64, String)]) {
    let mut v = request[key].as_u64().unwrap_or(0).min(options.len().saturating_sub(1) as u64);
    if select(ui, id, label, &mut v, options) || request[key].as_u64() != Some(v) {
        request[key] = json!(v);
    }
}

fn number_select(ui: &mut Ui, id: (Panel, &str), label: &str, request: &mut Value, key: &str, options: &[(f64, String)]) {
    let mut v = request[key].as_f64().unwrap_or(f64::NAN);
    if select(ui, id, label, &mut v, options) {
        request[key] = json!(v);
    }
}

fn controls(ui: &mut Ui, panel: Panel, w: &mut AnalysisWindow, system: &LensSystem) -> bool {
    let before = (w.request.clone(), w.options);
    let r = &mut w.request;
    match panel {
        Panel::Spot => number_select(ui, (panel, "rings"), "Rings", r, "rings", &[3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0].map(|v| (v, v.to_string()))),
        Panel::Mtf => {
            let mut v = r["maxFrequency"].as_f64().unwrap_or(0.0);
            let opts: Vec<(f64, String)> = [0.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0].iter().map(|f| (*f, if *f == 0.0 { "Cutoff".into() } else { format!("{f} lp/mm") })).collect();
            if select(ui, (panel, "max"), "Max freq.", &mut v, &opts) {
                r["maxFrequency"] = if v == 0.0 { Value::Null } else { json!(v) };
            }
        }
        Panel::Psf => {
            index_select(ui, (panel, "field"), "Field", r, "field", &field_options(system));
            select(ui, (panel, "scale"), "Scale", &mut w.options.psf_log, &[(false, "Linear".into()), (true, "Log".into())]);
        }
        Panel::Wavefront => {
            index_select(ui, (panel, "field"), "Field", r, "field", &field_options(system));
            index_select(ui, (panel, "wl"), "Wavelength", r, "wavelength", &wavelength_options(system));
        }
        Panel::ThroughFocus => {
            number_select(ui, (panel, "f"), "Frequency", r, "frequency", &[10.0, 20.0, 30.0, 50.0, 80.0, 100.0, 150.0, 200.0].map(|v| (v, format!("{v} lp/mm"))));
            number_select(ui, (panel, "r"), "Range", r, "range", &[0.05, 0.1, 0.2, 0.5, 1.0].map(|v| (v, format!("±{v} mm"))));
        }
        Panel::MtfVsField => {
            let sets = [[10.0, 30.0, 50.0], [20.0, 40.0, 80.0], [50.0, 100.0, 150.0]];
            let mut v = r["frequencies"].clone();
            let opts: Vec<(Value, String)> = sets.iter().map(|s| (json!(s), format!("{}, {}, {} lp/mm", s[0], s[1], s[2]))).collect();
            if select(ui, (panel, "set"), "Frequencies", &mut v, &opts) {
                r["frequencies"] = v;
            }
        }
        Panel::Footprint => index_select(ui, (panel, "s"), "Surface", r, "surface", &surface_options(system)),
        Panel::Coating => {
            index_select(ui, (panel, "s"), "Surface", r, "surface", &surface_options(system));
            index_select(ui, (panel, "wl"), "Wavelength", r, "wavelength", &wavelength_options(system));
        }
        Panel::Polarization | Panel::Transmission => {
            if panel == Panel::Polarization {
                index_select(ui, (panel, "field"), "Field", r, "field", &field_options(system));
            }
            index_select(ui, (panel, "wl"), "Wavelength", r, "wavelength", &wavelength_options(system));
            let inputs = [
                (json!({ "kind": "unpolarized" }), "Unpolarized"),
                (json!({ "kind": "linear", "angle": 0.0 }), "Linear X"),
                (json!({ "kind": "linear", "angle": 90.0 }), "Linear Y"),
                (json!({ "kind": "rightCircular" }), "Right circular"),
                (json!({ "kind": "leftCircular" }), "Left circular"),
            ]
            .map(|(v, l)| (v, l.to_string()));
            let mut input = r["input"].clone();
            if select(ui, (panel, "in"), "Input", &mut input, &inputs) {
                r["input"] = input;
            }
            if panel == Panel::Polarization {
                let maps = [(PolarizationView::Transmission, "Transmission"), (PolarizationView::Diattenuation, "Diattenuation"), (PolarizationView::Retardance, "Retardance")].map(|(v, l)| (v, l.to_string()));
                select(ui, (panel, "map"), "Map", &mut w.options.polarization_map, &maps);
            }
        }
        _ => {}
    }
    (w.request.clone(), w.options) != before
}

fn csv(panel: Panel, result: &AnalysisResult, system: &LensSystem) -> Option<String> {
    instaoptics_io::csv::analysis_csv(result, system).filter(|_| panel.analysis_kind().is_some())
}

pub fn ui(wb: &mut Workbench, ui: &mut Ui, panel: Panel) {
    let system = wb.session.system().clone();
    let working = wb.overview.as_ref().map(|o| o.paraxial.working_f_number);
    let t = wb.tokens;
    let w = wb.analyses.entry(panel).or_insert_with(|| AnalysisWindow::new(panel));
    let mut export_csv = None;
    let mut export_png = false;
    ui.horizontal(|ui| {
        if controls(ui, panel, w, &system) {
            w.rebuild(&system, working);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match w.ms {
                Some(ms) => ui.label(RichText::new(format!("{ms:.0} ms")).small().color(t.muted)),
                None => ui.spinner(),
            };
            if w.result.is_some() && ui.small_button("PNG").on_hover_text("Save as PNG image").clicked() {
                export_png = true;
            }
            if let Some(r) = &w.result
                && ui.small_button("CSV").on_hover_text("Export data as CSV").clicked()
            {
                export_csv = csv(panel, r, &system);
            }
        });
    });
    ui.separator();
    let input = tag(&[&wb.system_json, &w.request.to_string()]);
    if w.computed != input && wb.worker.running_tag(JobKey::Analysis(panel)) != Some(input) {
        match serde_json::from_value::<instaoptics_core::AnalysisRequest>(w.request.clone()) {
            Ok(request) => {
                let sys = system.clone();
                wb.worker.spawn(JobKey::Analysis(panel), input, move |_, _| Output::Analysis(instaoptics_core::analyze(&sys, &request).map_err(|e| e.0)));
            }
            Err(e) => w.error = Some(e.to_string()),
        }
    }
    let w = wb.analyses.get(&panel);
    match w.and_then(|w| w.figure.as_ref()) {
        Some(figure) => {
            if let Some(e) = w.and_then(|w| w.error.as_ref()) {
                ui.label(RichText::new(format!("{e} — showing the last valid result")).color(t.warning).small());
            }
            crate::plot::show(ui, egui::Id::unique(("figure", panel)), figure, &t, &mut wb.textures);
        }
        None => {
            ui.centered_and_justified(|ui| ui.label(RichText::new(w.and_then(|w| w.error.clone()).unwrap_or_else(|| "Computing…".into())).color(t.muted)));
        }
    }
    if let Some(text) = export_csv {
        save_text(wb, &format!("{}.csv", panel.file_stem()), "CSV", "csv", &text);
    }
    if export_png && let Some(figure) = wb.analyses.get(&panel).and_then(|w| w.figure.clone()) {
        save_png(wb, &panel.file_stem(), &figure);
    }
}

pub fn save_text(wb: &mut Workbench, name: &str, filter: &str, ext: &str, text: &str) {
    if let Some(path) = rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(name).save_file()
        && let Err(e) = std::fs::write(&path, text)
    {
        wb.set_status(format!("{}: {e}", path.display()), true);
    }
}

/// Saves a figure as PNG at a print-friendly size with the light plot theme.
pub fn save_png(wb: &mut Workbench, stem: &str, figure: &Figure) {
    let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(format!("{stem}.png")).save_file() else { return };
    let theme = if wb.tokens.dark { instaoptics_render::raster::Theme::DARK } else { instaoptics_render::raster::Theme::LIGHT };
    let result = instaoptics_render::raster::render_png(figure, 1600, 1000, theme).and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
    if let Err(e) = result {
        wb.set_status(format!("{}: {e}", path.display()), true);
    }
}

/// The 2D layout: built from the overview the workbench keeps current, with the selected surface highlighted.
pub fn layout_2d(wb: &mut Workbench, ui: &mut Ui) {
    let t = wb.tokens;
    let Some(overview) = &wb.overview else {
        ui.centered_and_justified(|ui| ui.label(RichText::new(wb.overview_error.clone().unwrap_or_else(|| "Computing…".into())).color(t.muted)));
        return;
    };
    let figure = figures::layout_figure(&overview.layout, wb.selected_surface);
    let mut export = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| export = ui.small_button("PNG").on_hover_text("Save as PNG image").clicked());
    });
    crate::plot::show(ui, egui::Id::unique("layout2d"), &figure, &t, &mut wb.textures);
    if export {
        save_png(wb, "layout", &figure);
    }
}
