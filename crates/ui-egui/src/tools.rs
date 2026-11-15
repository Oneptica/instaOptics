//! Tool windows: Optimization, Tolerancing, Beam Propagation, Image Simulation, Multi-Configuration, Glass Catalogs.

use crate::i18n::t;
use std::collections::HashMap;

use egui::{RichText, Ui};
use instaoptics_core::configs::{ConfigParameter, ConfigValue};
use instaoptics_core::system::{Objective, OperandKind, Relation};
use instaoptics_core::tolerance::{Compensator, Sensitivity, ToleranceSettings};
use instaoptics_core::{AnalysisResult, LensSystem};
use instaoptics_engine::jobs::{MonteCarloOutcome, OptimizeOutcome};
use instaoptics_render::Figure;
use instaoptics_render::figures::{self, BeamView, short};
use serde_json::{Value, json};

use crate::analysis::select;
use crate::app::Workbench;
use crate::explorer::number_field;
use crate::worker::{JobKey, Output, tag};

pub struct State {
    drafts: HashMap<String, String>,
    // optimization
    cycles: usize,
    opt_base: Option<LensSystem>,
    opt_history: Vec<f64>,
    opt_cycle: usize,
    opt_last_preview: f64,
    opt_result: Option<OptimizeOutcome>,
    opt_error: Option<String>,
    // tolerancing
    tol: ToleranceSettings,
    tol_monte_carlo: bool,
    sensitivity: Option<Sensitivity>,
    monte_carlo: Option<MonteCarloOutcome>,
    mc_figure: Option<Figure>,
    tol_error: Option<String>,
    tol_ms: Option<f64>,
    // beam propagation
    beam: BeamSettings,
    beam_result: Option<(AnalysisResult, u64)>,
    beam_figure: Option<Figure>,
    beam_options: figures::Options,
    beam_error: Option<String>,
    beam_ms: Option<f64>,
    // image simulation
    image: Option<(Vec<u8>, usize, usize, String)>,
    sim_settings: instaoptics_core::image_sim::SimulationSettings,
    sim_output: Option<(egui::TextureHandle, f64)>,
    sim_input: Option<egui::TextureHandle>,
    sim_error: Option<String>,
    // glasses
    glass_query: String,
    glass_catalog: String,
    glass_picked: String,
    // multi-configuration
    config_surface: usize,
    config_parameter: ConfigParameter,
}

#[derive(Clone, PartialEq)]
struct BeamSettings {
    kind: u8,
    radius: Option<f64>,
    waist: f64,
    order: f64,
    curvature: Option<f64>,
    wavelength: usize,
    samples: usize,
    aberrations: bool,
    apertures: bool,
    field: Option<usize>,
    fiber: Option<f64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            drafts: HashMap::new(),
            cycles: 50,
            opt_base: None,
            opt_history: Vec::new(),
            opt_cycle: 0,
            opt_last_preview: 0.0,
            opt_result: None,
            opt_error: None,
            tol: ToleranceSettings { radius: 0.2, thickness: 0.05, decenter: 0.02, tilt: 1.0, index: 0.0005, abbe: 0.5, compensator: Compensator::Focus, trials: 500, seed: 1 },
            tol_monte_carlo: false,
            sensitivity: None,
            monte_carlo: None,
            mc_figure: None,
            tol_error: None,
            tol_ms: None,
            beam: BeamSettings { kind: 0, radius: None, waist: 0.0, order: 4.0, curvature: None, wavelength: 0, samples: 256, aberrations: true, apertures: true, field: None, fiber: None },
            beam_result: None,
            beam_figure: None,
            beam_options: figures::Options::default(),
            beam_error: None,
            beam_ms: None,
            image: None,
            sim_settings: serde_json::from_value(json!({})).unwrap_or(instaoptics_core::image_sim::SimulationSettings { pixel_pitch: 0.0, rings: 6, field_samples: 32, diffraction: true, gamma: 2.2 }),
            sim_output: None,
            sim_input: None,
            sim_error: None,
            glass_query: String::new(),
            glass_catalog: String::new(),
            glass_picked: String::new(),
            config_surface: 0,
            config_parameter: ConfigParameter::Thickness,
        }
    }
}

/// A labelled number row in a form grid; returns the committed value.
fn row(ui: &mut Ui, drafts: &mut HashMap<String, String>, id: &str, label: &str, value: Option<f64>, unit: &str) -> Option<Option<f64>> {
    ui.label(label);
    let r = number_field(ui, drafts, id, value, 90.0);
    ui.label(RichText::new(unit).small());
    ui.end_row();
    r
}

fn busy_label(ui: &mut Ui, ms: Option<f64>, busy: bool) {
    if busy {
        ui.spinner();
    } else if let Some(ms) = ms {
        ui.label(RichText::new(if ms > 2000.0 { format!("{:.1} s", ms / 1000.0) } else { format!("{ms:.0} ms") }).small());
    }
}

pub fn receive(wb: &mut Workbench, key: JobKey, tag: u64, ms: f64, output: Output) {
    let ctx = wb.worker_ctx();
    let st = &mut wb.tools;
    match output {
        Output::OptimizeStep(cycle, merit, system) => {
            st.opt_cycle = cycle;
            st.opt_history.push(merit);
            // Show progress live a few times per second; history is recorded once at the end.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            if now - st.opt_last_preview > 0.2 {
                st.opt_last_preview = now;
                wb.session.preview(system);
            }
        }
        Output::Optimize(result) => {
            let base = st.opt_base.take();
            match result {
                Ok(outcome) => {
                    wb.session.preview(outcome.system.clone());
                    if let Some(base) = base {
                        wb.session.commit(base);
                    }
                    st.opt_history = outcome.history.clone();
                    st.opt_result = Some(outcome);
                }
                Err(e) => {
                    if let Some(base) = base {
                        wb.session.preview(base);
                    }
                    st.opt_error = Some(e);
                }
            }
        }
        Output::Sensitivity(r) => {
            st.tol_ms = Some(ms);
            match r {
                Ok(s) => st.sensitivity = Some(s),
                Err(e) => st.tol_error = Some(e),
            }
        }
        Output::MonteCarlo(r, done) => {
            if done {
                st.tol_ms = Some(ms);
            }
            match r {
                Ok(mc) => {
                    st.mc_figure = Some(figures::monte_carlo_figure(&mc.values, mc.nominal));
                    st.monte_carlo = Some(mc);
                }
                Err(e) => st.tol_error = Some(e),
            }
        }
        Output::Analysis(r) if key == JobKey::Analysis(crate::panels::Panel::Beam) => {
            st.beam_ms = Some(ms);
            match r {
                Ok(result) => {
                    st.beam_figure = Some(figures::figure(&result, wb.session.system(), None, &st.beam_options));
                    st.beam_result = Some((result, tag));
                    st.beam_error = None;
                }
                Err(e) => st.beam_error = Some(e),
            }
        }
        Output::ImageSim(r) => match r {
            Ok((data, w, h, pitch)) => {
                let pixels: Vec<egui::Color32> = data
                    .chunks_exact(3)
                    .map(|c| {
                        let g = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / st.sim_settings.gamma as f32) * 255.0).round() as u8;
                        egui::Color32::from_rgb(g(c[0]), g(c[1]), g(c[2]))
                    })
                    .collect();
                if pixels.len() == w * h {
                    let tex = ctx.load_texture("image-sim-output", egui::ColorImage::new([w, h], pixels), egui::TextureOptions::LINEAR);
                    st.sim_output = Some((tex, pitch));
                    st.sim_error = None;
                }
            }
            Err(e) => st.sim_error = Some(e),
        },
        _ => {}
    }
}

// ---------- optimization ----------

const OPERANDS: [(OperandKind, &str, &str, u8); 10] = [
    (OperandKind::Efl, "EFL", "mm", 0),
    (OperandKind::TotalTrack, "Total track", "mm", 0),
    (OperandKind::BackFocus, "Back focus", "mm", 0),
    (OperandKind::FNumber, "F/#", "", 0),
    (OperandKind::ImageHeight, "Image height", "mm", 0),
    (OperandKind::ChiefRayAngle, "Chief ray angle", "deg", 2),
    (OperandKind::Distortion, "Distortion", "%", 2),
    (OperandKind::SpotRadius, "RMS spot radius", "mm", 2),
    (OperandKind::Thickness, "Thickness", "mm", 1),
    (OperandKind::Radius, "Radius", "mm", 1),
];

pub fn optimize(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system().clone();
    let running = wb.worker.is_running(JobKey::Optimize);
    let variables = instaoptics_core::optimize::list_variables(&system);
    let mut commands: Vec<(&'static str, Value)> = Vec::new();
    let st = &mut wb.tools;
    ui.horizontal(|ui| {
        if running {
            if ui.button(t("Stop")).clicked() {
                wb.worker.cancel(JobKey::Optimize);
            }
        } else if ui.add_enabled(!variables.is_empty(), egui::Button::new(t("Optimize"))).on_hover_text(if variables.is_empty() { "Mark variables in the Lens Data editor first" } else { "Damped least squares" }).clicked() {
            st.opt_base = Some(system.clone());
            st.opt_history.clear();
            st.opt_result = None;
            st.opt_error = None;
            st.opt_cycle = 0;
            let (sys, cycles) = (system.clone(), st.cycles);
            wb.worker.spawn(JobKey::Optimize, tag(&[&wb.system_json, "optimize"]), move |progress, cancel| {
                Output::Optimize(instaoptics_engine::jobs::optimize(&sys, cycles, cancel, |c, m, s| progress.send(Output::OptimizeStep(c, m, s.clone()))))
            });
        }
        select(ui, "cycles", t("Cycles"), &mut st.cycles, &[10, 25, 50, 100, 200].map(|v| (v, v.to_string())));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut info = String::new();
            if running {
                info = format!("Cycle {} · ", st.opt_cycle);
            }
            if let Some(m) = st.opt_history.last() {
                info.push_str(&format!("Merit {m:.4e}"));
            }
            if let Some(r) = &st.opt_result {
                info.push_str(&format!(" · {} cycles{}", r.cycles, if r.stopped { " (stopped)" } else { "" }));
            }
            ui.label(RichText::new(info).monospace().small());
        });
    });
    ui.separator();
    let settings = system.optimization.clone();
    let report = instaoptics_core::optimize::merit_report(&system);
    ui.columns(2, |cols| {
        egui::ScrollArea::vertical().id_salt("opt-form").show(&mut cols[0], |ui| {
            ui.strong(t("Merit Function"));
            egui::Grid::new("merit").num_columns(3).show(ui, |ui| {
                ui.label(t("Criterion"));
                let mut objective = settings.objective;
                let opts = [(Objective::Spot, t("RMS spot radius (centroid)").to_string()), (Objective::Wavefront, "RMS wavefront error".into()), (Objective::None, "None (operands only)".into())];
                if select(ui, "objective", "", &mut objective, &opts) {
                    commands.push(("optimization.set", json!({ "objective": objective })));
                }
                ui.end_row();
                ui.label(t("Pupil rings"));
                let mut rings = settings.rings.unwrap_or(3);
                if select(ui, "rings", "", &mut rings, &[2, 3, 4, 5, 6, 8].map(|v| (v, v.to_string()))) {
                    commands.push(("optimization.set", json!({ "rings": rings })));
                }
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.strong(t("Targets and Constraints"));
            egui::Grid::new("constraints").num_columns(3).show(ui, |ui| {
                if let Some(v) = row(ui, &mut st.drafts, "efl", t("Effective focal length"), system.target_efl, "mm") {
                    commands.push(("system.set", json!({ "targetEfl": v })));
                }
                let fields: [(&str, &str, Option<f64>, &str); 7] = [
                    ("maxTotalTrack", t("Max total track"), settings.max_total_track, "mm"),
                    ("minBackFocus", t("Min back focus"), settings.min_back_focus, "mm"),
                    ("maxChiefRayAngle", t("Max chief ray angle"), settings.max_chief_ray_angle, "deg"),
                    ("minGlassCenter", t("Min glass centre"), settings.min_glass_center, "mm"),
                    ("minGlassEdge", t("Min glass edge"), settings.min_glass_edge, "mm"),
                    ("minAir", t("Min air gap"), settings.min_air, "mm"),
                    ("", "", None, ""),
                ];
                for (key, label, value, unit) in fields.iter().filter(|f| !f.0.is_empty()) {
                    if let Some(v) = row(ui, &mut st.drafts, key, label, *value, unit) {
                        commands.push(("optimization.set", json!({ *key: v })));
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.strong(format!("Operands ({})", settings.operands.len()));
                if ui.small_button(t("Add")).clicked() {
                    let efl = wb.overview.as_ref().map_or(0.0, |o| o.paraxial.efl);
                    commands.push(("operand.add", json!({ "kind": "efl", "target": if efl.is_finite() { efl } else { 0.0 } })));
                }
            });
            egui::Grid::new("operands").striped(true).show(ui, |ui| {
                for h in ["#", t("Type"), t("Surf / Field"), t("Rel"), t("Target"), t("Weight"), t("Value"), ""] {
                    ui.label(RichText::new(h).small());
                }
                ui.end_row();
                for (i, o) in settings.operands.iter().enumerate() {
                    let index = i + 1;
                    ui.label(index.to_string());
                    let mut kind = o.kind;
                    if select(ui, ("op-kind", i), "", &mut kind, &OPERANDS.map(|(k, l, ..)| (k, t(l).to_string()))) {
                        commands.push(("operand.set", json!({ "index": index, "kind": kind, "surface": null, "field": null })));
                    }
                    let needs = OPERANDS.iter().find(|x| x.0 == o.kind).map_or(0, |x| x.3);
                    match needs {
                        1 => {
                            let mut s = o.surface.unwrap_or(0) + 1;
                            if select(ui, ("op-s", i), "", &mut s, &(1..=system.surfaces.len()).map(|k| (k, k.to_string())).collect::<Vec<_>>()) {
                                commands.push(("operand.set", json!({ "index": index, "surface": s })));
                            }
                        }
                        2 => {
                            let mut f = o.field.map_or(0, |f| f + 1);
                            let mut opts = vec![(0, t("All").to_string())];
                            opts.extend((1..=system.fields.len()).map(|k| (k, k.to_string())));
                            if select(ui, ("op-f", i), "", &mut f, &opts) {
                                commands.push(("operand.set", json!({ "index": index, "field": if f == 0 { Value::Null } else { json!(f) } })));
                            }
                        }
                        _ => {
                            ui.label("");
                        }
                    }
                    let mut rel = o.relation;
                    if select(ui, ("op-r", i), "", &mut rel, &[(Relation::Equal, "=".to_string()), (Relation::AtMost, "≤".into()), (Relation::AtLeast, "≥".into())]) {
                        commands.push(("operand.set", json!({ "index": index, "relation": rel })));
                    }
                    if let Some(Some(v)) = number_field(ui, &mut st.drafts, &format!("op-t{i}"), Some(o.target), 70.0) {
                        commands.push(("operand.set", json!({ "index": index, "target": v })));
                    }
                    if let Some(Some(v)) = number_field(ui, &mut st.drafts, &format!("op-w{i}"), Some(o.weight), 50.0) {
                        commands.push(("operand.set", json!({ "index": index, "weight": v })));
                    }
                    let unit = OPERANDS.iter().find(|x| x.0 == o.kind).map_or("", |x| x.2);
                    ui.label(RichText::new(report.values.get(i).copied().flatten().map_or("—".into(), |v| format!("{v:.4} {unit}"))).monospace());
                    if ui.small_button("×").clicked() {
                        commands.push(("operand.remove", json!({ "index": index })));
                    }
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.strong(format!("Variables ({})", variables.len()));
            egui::Grid::new("variables").striped(true).show(ui, |ui| {
                for v in &variables {
                    let (s, name, value) = match *v {
                        instaoptics_core::optimize::Variable::Radius { surface } => (surface, t("Radius").to_string(), short(system.surfaces[surface].radius)),
                        instaoptics_core::optimize::Variable::Thickness { surface } => (surface, t("Thickness").into(), short(system.surfaces[surface].thickness)),
                        instaoptics_core::optimize::Variable::Conic { surface } => (surface, t("Conic").into(), short(system.surfaces[surface].conic)),
                        instaoptics_core::optimize::Variable::Aspheric { surface, term } => (surface, format!("A{}", 2 * term + 4), format!("{:.6e}", system.surfaces[surface].aspheric.get(term).copied().unwrap_or(0.0))),
                    };
                    ui.label((s + 1).to_string());
                    ui.label(name);
                    ui.label(RichText::new(value).monospace());
                    ui.end_row();
                }
            });
        });
        let ui = &mut cols[1];
        if let Some(e) = &st.opt_error {
            ui.label(RichText::new(e).color(wb.tokens.error));
        }
        if st.opt_history.len() > 1 {
            let figure = figures::merit_history_figure(&st.opt_history);
            if let Some(r) = &st.opt_result {
                ui.label(RichText::new(format!("Initial {:.4e} → final {:.4e} · undo restores the starting lens", r.initial_merit, r.final_merit)).small().monospace());
            }
            crate::plot::show(ui, egui::Id::unique("merit-history"), &figure, &wb.tokens, &mut wb.textures);
        } else {
            ui.label(RichText::new(format!("Merit {}", if report.merit.is_finite() { format!("{:.4e}", report.merit) } else { "—".into() })).monospace());
        }
    });
    for (id, params) in commands {
        wb.run(id, params);
    }
}

// ---------- tolerancing ----------

pub fn tolerance(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system().clone();
    let busy = wb.worker.is_running(JobKey::Sensitivity) || wb.worker.is_running(JobKey::MonteCarlo);
    let st = &mut wb.tools;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!busy, |ui| {
            if ui.button(t("Sensitivity")).clicked() {
                st.tol_monte_carlo = false;
                st.tol_error = None;
                let (sys, settings) = (system.clone(), st.tol.clone());
                wb.worker.spawn(JobKey::Sensitivity, tag(&[&wb.system_json, "sens"]), move |_, _| {
                    Output::Sensitivity(sys.validate().map_err(|e| e.0).map(|_| instaoptics_core::tolerance::sensitivity(&sys, &settings)))
                });
            }
            if ui.button(t("Monte Carlo")).clicked() {
                st.tol_monte_carlo = true;
                st.tol_error = None;
                st.monte_carlo = None;
                st.mc_figure = None;
                let (sys, settings) = (system.clone(), st.tol.clone());
                wb.worker.spawn(JobKey::MonteCarlo, tag(&[&wb.system_json, "mc"]), move |progress, cancel| {
                    Output::MonteCarlo(instaoptics_engine::jobs::monte_carlo(&sys, &settings, cancel, |mc| progress.send(Output::MonteCarlo(Ok(mc.clone()), false))), true)
                });
            }
        });
        if busy && ui.button(t("Stop")).clicked() {
            wb.worker.cancel(JobKey::MonteCarlo);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            busy_label(ui, st.tol_ms, busy);
            if let (true, Some(mc)) = (busy, &st.monte_carlo) {
                ui.label(RichText::new(format!("{} / {} trials", mc.values.len(), mc.total)).small());
            }
        });
    });
    ui.separator();
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.strong(t("Tolerances"));
        egui::Grid::new("tol").num_columns(3).show(ui, |ui| {
            let tol = &mut st.tol;
            for (id, label, value, unit) in [("r", t("Radius"), &mut tol.radius, "± %"), ("t", t("Thickness"), &mut tol.thickness, "± mm"), ("d", t("Surface decenter"), &mut tol.decenter, "± mm"), ("a", t("Surface tilt"), &mut tol.tilt, "± ′"), ("n", t("Index nd"), &mut tol.index, "±"), ("v", t("Abbe vd"), &mut tol.abbe, "± %")] {
                if let Some(Some(v)) = row(ui, &mut st.drafts, &format!("tol-{id}"), label, Some(*value), unit)
                    && v >= 0.0
                {
                    *value = v;
                }
            }
        });
        ui.add_space(6.0);
        ui.strong(t("Analysis"));
        egui::Grid::new("tol-analysis").num_columns(2).show(ui, |ui| {
            ui.label(t("Compensator"));
            select(ui, "comp", "", &mut st.tol.compensator, &[(Compensator::Focus, t("Back focus").to_string()), (Compensator::None, "None".into())]);
            ui.end_row();
            ui.label(t("Monte Carlo trials"));
            select(ui, "trials", "", &mut st.tol.trials, &[100, 200, 500, 1000, 2000, 5000, 10000].map(|v| (v, v.to_string())));
            ui.end_row();
            ui.label(t("Random seed"));
            if let Some(Some(v)) = number_field(ui, &mut st.drafts, "seed", Some(st.tol.seed as f64), 70.0) {
                st.tol.seed = v.max(1.0) as u32;
            }
            ui.end_row();
        });
        ui.label(RichText::new(t("Criterion: polychromatic RMS spot radius about each field's centroid. Monte Carlo draws each parameter from a normal distribution with 2σ = tolerance.")).small().color(wb.tokens.muted));
        let ui = &mut cols[1];
        if let Some(e) = &st.tol_error {
            ui.label(RichText::new(e).color(wb.tokens.error));
        }
        if st.tol_monte_carlo {
            if let Some(f) = &st.mc_figure {
                crate::plot::show(ui, egui::Id::unique("mc"), f, &wb.tokens, &mut wb.textures);
            }
        } else if let Some(s) = &st.sensitivity {
            let um = |mm: f64| if mm.is_finite() { format!("{:.3}", mm * 1000.0) } else { "—".into() };
            ui.label(RichText::new(format!("Nominal {} µm · estimated as built (RSS) {} µm · {} parameters", um(s.nominal), um(s.estimated), s.rows.len())).monospace().small());
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("sens").striped(true).show(ui, |ui| {
                    for h in [t("Surf"), t("Parameter"), t("±Tol"), t("Δ at + (µm)"), t("Δ at − (µm)"), t("Worst (µm)")] {
                        ui.label(RichText::new(h).small());
                    }
                    ui.end_row();
                    for r in &s.rows {
                        ui.label((r.parameter.surface + 1).to_string());
                        ui.label(serde_json::to_value(r.parameter.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default());
                        ui.label(short(r.parameter.tolerance));
                        ui.label(RichText::new(um(r.plus)).monospace());
                        ui.label(RichText::new(um(r.minus)).monospace());
                        ui.label(RichText::new(um(r.worst)).monospace());
                        ui.end_row();
                    }
                });
            });
        }
    });
}

// ---------- beam propagation ----------

fn beam_request(b: &BeamSettings, system: &LensSystem) -> Value {
    let radius = b.radius.unwrap_or(if b.kind == 0 { system.entrance_pupil_diameter / 4.0 } else { system.entrance_pupil_diameter / 2.0 });
    let source = match b.kind {
        0 => json!({ "kind": "gaussian", "radius": radius, "waist": b.waist }),
        1 => json!({ "kind": "superGaussian", "radius": radius, "order": b.order, "curvatureRadius": b.curvature }),
        _ => json!({ "kind": "topHat", "radius": radius, "curvatureRadius": b.curvature }),
    };
    json!({ "kind": "pop", "settings": { "source": source, "wavelength": b.wavelength.min(system.wavelengths.len() - 1), "samples": b.samples, "aberrations": b.aberrations, "apertures": b.apertures, "field": b.field.filter(|f| *f < system.fields.len()), "fiberRadius": b.fiber } })
}

pub fn beam(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system().clone();
    let key = JobKey::Analysis(crate::panels::Panel::Beam);
    let busy = wb.worker.is_running(key);
    let st = &mut wb.tools;
    let request = beam_request(&st.beam, &system);
    let input = tag(&[&wb.system_json, &request.to_string()]);
    let mut rebuild = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, egui::Button::new(if busy { "Running…" } else { "Run" })).on_hover_text(t("Propagate the beam through the system")).clicked() {
            let sys = system.clone();
            if let Ok(req) = serde_json::from_value::<instaoptics_core::AnalysisRequest>(request.clone()) {
                wb.worker.spawn(key, input, move |_, _| Output::Analysis(instaoptics_core::analyze(&sys, &req).map_err(|e| e.0)));
            }
        }
        let views = [(BeamView::SideX, t("Side view x–z")), (BeamView::SideY, t("Side view y–z")), (BeamView::Radius, t("Beam radius")), (BeamView::Section, t("Cross section")), (BeamView::Table, t("Table"))].map(|(v, l)| (v, l.to_string()));
        rebuild |= select(ui, "beam-view", t("View"), &mut st.beam_options.beam, &views);
        if st.beam_options.beam == BeamView::Section {
            let planes: Vec<(usize, String)> = match &st.beam_result {
                Some((AnalysisResult::Pop(p), _)) => p.planes.iter().map(|pl| (pl.surface, if pl.surface >= system.surfaces.len() { "Image".into() } else { format!("After surface {}", pl.surface + 1) })).collect(),
                _ => Vec::new(),
            };
            rebuild |= select(ui, "beam-plane", t("Plane"), &mut st.beam_options.beam_plane, &planes);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| busy_label(ui, st.beam_ms, busy));
    });
    if rebuild && let Some((r, _)) = &st.beam_result {
        st.beam_figure = Some(figures::figure(r, &system, None, &st.beam_options));
    }
    ui.separator();
    egui::Panel::left("beam-form").resizable(true).default_size(260.0).show(ui, |ui| {
        let b = &mut st.beam;
        egui::Grid::new("beam").num_columns(3).show(ui, |ui| {
            ui.label(t("Beam"));
            select(ui, "beam-kind", "", &mut b.kind, &[(0u8, t("Gaussian").to_string()), (1, "Super-Gaussian".into()), (2, "Flat top (circular)".into())]);
            ui.end_row();
            ui.label(t("Field"));
            let mut f = b.field.map_or(0, |f| f + 1);
            let mut opts = vec![(0usize, t("On axis").to_string())];
            opts.extend(system.fields.iter().enumerate().map(|(i, v)| (i + 1, format!("{}: {}", i + 1, figures::field_label(*v, system.field_type)))));
            if select(ui, "beam-field", "", &mut f, &opts) {
                b.field = f.checked_sub(1);
            }
            ui.end_row();
            ui.label(t("Wavelength"));
            select(ui, "beam-wl", "", &mut b.wavelength, &system.wavelengths.iter().enumerate().map(|(i, w)| (i, format!("{w:.4} µm"))).collect::<Vec<_>>());
            ui.end_row();
            if let Some(v) = row(ui, &mut st.drafts, "beam-r", if b.kind == 0 { t("Waist radius (1/e²)") } else { t("Radius") }, b.radius, "mm") {
                b.radius = v.filter(|r| *r > 0.0);
            }
            if b.kind == 0 {
                if let Some(Some(v)) = row(ui, &mut st.drafts, "beam-w", t("Waist position"), Some(b.waist), "mm") {
                    b.waist = v;
                }
            } else {
                if b.kind == 1
                    && let Some(Some(v)) = row(ui, &mut st.drafts, "beam-o", t("Order"), Some(b.order), "")
                {
                    b.order = v.max(1.0);
                }
                if let Some(v) = row(ui, &mut st.drafts, "beam-c", t("Curvature radius"), b.curvature, "mm") {
                    b.curvature = v;
                }
            }
            ui.label(t("Grid samples"));
            select(ui, "beam-n", "", &mut b.samples, &[(128, "128 × 128".to_string()), (256, "256 × 256".into()), (512, "512 × 512".into())]);
            ui.end_row();
            if let Some(v) = row(ui, &mut st.drafts, "beam-fib", t("Fibre mode radius"), b.fiber, "mm") {
                b.fiber = v.filter(|r| *r > 0.0);
            }
        });
        ui.checkbox(&mut b.aberrations, t("Include wavefront aberration"));
        ui.checkbox(&mut b.apertures, t("Clip at fixed surface apertures"));
        if let Some((_, ran)) = &st.beam_result
            && *ran != input
        {
            ui.label(RichText::new(t("The lens or the settings changed since this result.")).small().color(wb.tokens.warning));
        }
    });
    if let Some(e) = &st.beam_error {
        ui.label(RichText::new(e).color(wb.tokens.error));
    }
    match &st.beam_figure {
        Some(f) => crate::plot::show(ui, egui::Id::unique("beam"), f, &wb.tokens, &mut wb.textures),
        None => {
            ui.centered_and_justified(|ui| ui.label(RichText::new(if busy { t("Propagating…") } else { t("Set up a source and press Run.") }).color(wb.tokens.muted)));
        }
    }
}

// ---------- image simulation ----------

/// A test chart: bars of decreasing period on a grid, so blur and distortion are easy to see.
fn test_chart(size: usize) -> Vec<u8> {
    let mut rgba = vec![255u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (x as f64 / size as f64, y as f64 / size as f64);
            let grid = (x % (size / 16)) < 2 || (y % (size / 16)) < 2;
            let period = 4.0 + 28.0 * v;
            let bars = u > 0.35 && u < 0.65 && ((x as f64 / period).floor() as i64) % 2 == 0;
            let dark = grid || bars;
            let i = (y * size + x) * 4;
            let c = if dark { 20 } else { 235 };
            rgba[i] = c;
            rgba[i + 1] = c;
            rgba[i + 2] = c;
        }
    }
    rgba
}

pub fn image_sim(wb: &mut Workbench, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let system = wb.session.system().clone();
    let busy = wb.worker.is_running(JobKey::ImageSim);
    let st = &mut wb.tools;
    if st.image.is_none() {
        st.image = Some((test_chart(512), 512, 512, t("Test chart").into()));
        st.sim_input = None;
    }
    ui.horizontal(|ui| {
        if ui.button(t("Load Image…")).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "bmp", "tif", "tiff"]).pick_file()
        {
            match image::open(&path) {
                Ok(img) => {
                    let img = img.to_rgba8();
                    let (w, h) = (img.width() as usize, img.height() as usize);
                    st.image = Some((img.into_raw(), w, h, path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())));
                    st.sim_input = None;
                    st.sim_output = None;
                }
                Err(e) => st.sim_error = Some(e.to_string()),
            }
        }
        if ui.small_button(t("Test Chart")).clicked() {
            st.image = None;
            st.sim_output = None;
        }
        if ui.add_enabled(!busy && st.image.is_some(), egui::Button::new(if busy { t("Simulating…") } else { t("Simulate") })).clicked()
            && let Some((rgba, w, h, _)) = st.image.clone()
        {
            let (sys, settings) = (system.clone(), st.sim_settings.clone());
            wb.worker.spawn(JobKey::ImageSim, tag(&[&wb.system_json, &format!("{settings:?}{w}{h}")]), move |_, _| {
                Output::ImageSim(instaoptics_core::image_sim::simulate(&sys, &settings, &rgba, w, h).map(|r| (r.data, w, h, r.pitch)).map_err(|e| e.0))
            });
        }
        let s = &mut st.sim_settings;
        select(ui, "sim-rings", t("Rings"), &mut s.rings, &[3, 4, 6, 8, 10].map(|v| (v, v.to_string())));
        select(ui, "sim-fields", t("Field samples"), &mut s.field_samples, &[8, 16, 32, 64].map(|v| (v, v.to_string())));
        ui.checkbox(&mut s.diffraction, t("Diffraction"));
        ui.label(t("Pixel pitch (µm, blank: fit)"));
        if let Some(v) = number_field(ui, &mut st.drafts, "sim-pitch", Some(s.pixel_pitch).filter(|p| *p > 0.0), 60.0) {
            s.pixel_pitch = v.filter(|p| *p > 0.0).unwrap_or(0.0);
        }
    });
    ui.separator();
    if let Some(e) = &st.sim_error {
        ui.label(RichText::new(e).color(wb.tokens.error));
    }
    if st.sim_input.is_none()
        && let Some((rgba, w, h, _)) = &st.image
    {
        st.sim_input = Some(ctx.load_texture("image-sim-input", egui::ColorImage::from_rgba_unmultiplied([*w, *h], rgba), egui::TextureOptions::LINEAR));
    }
    ui.columns(2, |cols| {
        let show = |ui: &mut Ui, tex: &egui::TextureHandle, caption: String| {
            ui.label(RichText::new(caption).small());
            let size = tex.size_vec2();
            let avail = ui.available_size();
            let scale = (avail.x / size.x).min((avail.y - 4.0) / size.y).max(0.05);
            ui.image((tex.id(), size * scale));
        };
        if let Some(t) = &st.sim_input {
            show(&mut cols[0], t, format!("Input · {}", st.image.as_ref().map_or("", |i| i.3.as_str())));
        }
        if let Some((t, pitch)) = &st.sim_output {
            show(&mut cols[1], t, format!("Simulated · pixel pitch {pitch:.3} µm"));
        }
    });
}

// ---------- multi-configuration ----------

const CONFIG_PARAMETERS: [(ConfigParameter, &str); 8] = [
    (ConfigParameter::Radius, "Radius"),
    (ConfigParameter::Thickness, "Thickness"),
    (ConfigParameter::Conic, "Conic"),
    (ConfigParameter::Material, "Material"),
    (ConfigParameter::DecenterX, "Decenter X"),
    (ConfigParameter::DecenterY, "Decenter Y"),
    (ConfigParameter::TiltX, "Tilt X"),
    (ConfigParameter::TiltY, "Tilt Y"),
];

pub fn configs(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system().clone();
    let mut commands: Vec<(&'static str, Value)> = Vec::new();
    let st = &mut wb.tools;
    let Some(configs) = &system.configs else {
        ui.horizontal(|ui| {
            if ui.button(t("Add Configurations")).on_hover_text(t("Surface parameters take a different value in each configuration: zoom positions, scan angles, tolerance cases")).clicked() {
                commands.push(("config.add", json!({})));
            }
        });
        for (id, params) in commands {
            wb.run(id, params);
        }
        return;
    };
    let active = instaoptics_engine::edit::active_configuration(&system);
    ui.horizontal(|ui| {
        if ui.small_button(t("Add Configuration")).clicked() {
            commands.push(("config.add", json!({})));
        }
        if ui.add_enabled(configs.names.len() > 1, egui::Button::new(t("Remove Configuration")).small()).on_hover_text(t("Remove the active configuration")).clicked() {
            commands.push(("config.remove", json!({ "index": active + 1 })));
        }
        ui.separator();
        st.config_surface = st.config_surface.min(system.surfaces.len() - 1);
        select(ui, "cfg-s", t("Surface"), &mut st.config_surface, &(0..system.surfaces.len()).map(|i| (i, (i + 1).to_string())).collect::<Vec<_>>());
        select(ui, "cfg-p", "", &mut st.config_parameter, &CONFIG_PARAMETERS.map(|(p, l)| (p, t(l).to_string())));
        if ui.small_button(t("Add Row")).clicked() {
            commands.push(("config.row.add", json!({ "surface": st.config_surface + 1, "parameter": st.config_parameter })));
        }
    });
    ui.separator();
    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("mce").striped(true).show(ui, |ui| {
            ui.label(RichText::new(t("Surf")).small());
            ui.label(RichText::new(t("Parameter")).small());
            for (k, name) in configs.names.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.radio(k == active, "").on_hover_text(t("Make this the active configuration")).clicked() {
                        commands.push(("config.active", json!({ "index": k + 1 })));
                    }
                    if let Some(text) = text_field(ui, &mut st.drafts, &format!("cfg-name{k}"), name, 80.0) {
                        commands.push(("config.rename", json!({ "index": k + 1, "name": text })));
                    }
                });
            }
            ui.label("");
            ui.end_row();
            for (r, cr) in configs.rows.iter().enumerate() {
                ui.label((cr.surface + 1).to_string());
                ui.label(CONFIG_PARAMETERS.iter().find(|p| p.0 == cr.parameter).map_or("", |p| t(p.1)));
                for k in 0..configs.names.len() {
                    let id = format!("cfg{r}-{k}");
                    match cr.values.get(k) {
                        Some(ConfigValue::Text(t)) => {
                            if let Some(text) = text_field(ui, &mut st.drafts, &id, t, 80.0) {
                                commands.push(("config.value", json!({ "row": r + 1, "config": k + 1, "value": text })));
                            }
                        }
                        other => {
                            let v = if let Some(ConfigValue::Number(v)) = other { *v } else { 0.0 };
                            if let Some(Some(v)) = number_field(ui, &mut st.drafts, &id, Some(v), 80.0) {
                                commands.push(("config.value", json!({ "row": r + 1, "config": k + 1, "value": v })));
                            }
                        }
                    }
                }
                if ui.small_button("×").on_hover_text(t("Remove row")).clicked() {
                    commands.push(("config.row.remove", json!({ "row": r + 1 })));
                }
                ui.end_row();
            }
        });
    });
    for (id, params) in commands {
        wb.run(id, params);
    }
}

fn text_field(ui: &mut Ui, drafts: &mut HashMap<String, String>, id: &str, value: &str, width: f32) -> Option<String> {
    let text = drafts.entry(id.to_string()).or_insert_with(|| value.to_string());
    let r = ui.add(egui::TextEdit::singleline(text).desired_width(width));
    if r.lost_focus() {
        let t = drafts.remove(id).unwrap_or_default();
        return (t.trim() != value && !t.trim().is_empty()).then(|| t.trim().to_string());
    }
    if !r.has_focus() && text != value {
        *text = value.to_string();
    }
    None
}

// ---------- glass catalogs ----------

pub fn glasses(wb: &mut Workbench, ui: &mut Ui) {
    let all = wb.session.glasses();
    let mut sources: Vec<String> = vec![instaoptics_engine::session::BUILT_IN.to_string()];
    sources.extend(wb.session.catalogs.iter().map(|c| c.name.clone()));
    let color = |s: &str| instaoptics_render::field_color(sources.iter().position(|x| x == s).unwrap_or(0));
    let target = wb.selected_surface;
    let mut apply: Option<String> = None;
    let mut remove: Option<String> = None;
    let mut add = false;
    let st = &mut wb.tools;
    ui.horizontal(|ui| {
        add = ui.button(t("Add Catalog…")).on_hover_text(t("Add OpticStudio glass catalogs (.agf)")).clicked();
        let mut opts = vec![(String::new(), t("All").to_string())];
        opts.extend(sources.iter().map(|s| (s.clone(), s.clone())));
        select(ui, "glass-src", t("Source"), &mut st.glass_catalog, &opts);
        ui.add(egui::TextEdit::singleline(&mut st.glass_query).hint_text("Search glass name").desired_width(160.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = target.map_or(t("Select a surface").to_string(), |s| format!("Use for surface {}", s + 1));
            if ui.add_enabled(!st.glass_picked.is_empty() && target.is_some(), egui::Button::new(label)).on_hover_text(t("Use the selected glass for the surface selected in the Lens Data editor")).clicked() {
                apply = Some(st.glass_picked.clone());
            }
        });
    });
    ui.separator();
    let needle = st.glass_query.trim().to_uppercase();
    let shown: Vec<_> = all.iter().filter(|g| (st.glass_catalog.is_empty() || g.catalog == st.glass_catalog) && (needle.is_empty() || g.name.to_uppercase().contains(&needle))).collect();
    egui::Panel::left("glass-side").resizable(true).default_size(360.0).show(ui, |ui| {
        ui.strong(t("Catalogs"));
        for s in &sources {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, wb.tokens.color(color(s)));
                ui.label(s);
                ui.label(RichText::new(all.iter().filter(|g| &g.catalog == s).count().to_string()).color(wb.tokens.muted));
                if s != instaoptics_engine::session::BUILT_IN && ui.small_button("×").on_hover_text(t("Remove this catalog")).clicked() {
                    remove = Some(s.clone());
                }
            });
        }
        let points: Vec<(String, f64, f64, instaoptics_render::Color)> = shown.iter().map(|g| (g.name.clone(), g.nd, g.vd, color(&g.catalog))).collect();
        let figure = figures::abbe_figure(&points, Some(&st.glass_picked));
        crate::plot::show(ui, egui::Id::unique("abbe"), &figure, &wb.tokens, &mut wb.textures);
    });
    const LIMIT: usize = 500;
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("glass-table").striped(true).show(ui, |ui| {
            for h in [t("Name"), t("Catalog"), "nd", "vd", t("Formula"), t("Range (µm)")] {
                ui.label(RichText::new(h).small());
            }
            ui.end_row();
            for g in shown.iter().take(LIMIT) {
                let r = ui.selectable_label(g.name == st.glass_picked, &g.name);
                if r.clicked() {
                    st.glass_picked = g.name.clone();
                }
                if r.double_clicked() {
                    apply = Some(g.name.clone());
                }
                ui.label(&g.catalog);
                ui.label(RichText::new(if g.nd > 0.0 { format!("{:.5}", g.nd) } else { "—".into() }).monospace());
                ui.label(RichText::new(if g.vd > 0.0 { format!("{:.2}", g.vd) } else { "—".into() }).monospace());
                ui.label(instaoptics_core::glass::FORMULA_NAMES.get(usize::from(g.formula).saturating_sub(1)).copied().unwrap_or(""));
                ui.label(g.range.map_or("—".into(), |r| format!("{:.2}–{:.2}", r[0], r[1])));
                ui.end_row();
            }
        });
        if shown.len() > LIMIT {
            ui.label(RichText::new(format!("Showing {LIMIT} of {}; narrow the search to see the rest.", shown.len())).small());
        }
    });
    if add {
        wb.act(crate::app::Action::AddCatalog);
    }
    if let Some(name) = remove {
        wb.run("catalog.remove", json!({ "name": name }));
    }
    if let (Some(name), Some(s)) = (apply, target) {
        wb.run("surface.set", json!({ "surface": s + 1, "material": name }));
    }
}
