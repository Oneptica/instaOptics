//! The workbench: menu bar, docked windows, status bar, background work and the control channel.

use crate::i18n::t;
use std::collections::HashMap;
use std::path::PathBuf;

use craft_ui::docking::{Action as DockAction, DockArea, DockStyle, Layout, Node, Permissions, Workspace};
use craft_ui::layout::{SplitAxis, SplitSize};
use egui::{Context, Key, KeyboardShortcut, Modifiers, RichText, Ui};
use instaoptics_core::Overview;
use instaoptics_engine::{Session, commands};
use serde_json::{Value, json};

use crate::analysis::AnalysisWindow;
use crate::panels::{self, Panel};
use crate::plot::Textures;
use crate::prefs::Prefs;
use crate::theme::{ThemeChoice, Tokens};
use crate::worker::{JobKey, Output, Worker, tag};
use crate::{control, explorer, lde, tools, view3d};

/// Actions the menus, shortcuts and toolbars ask for. Engine commands run through the command registry.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Command(&'static str, Value),
    /// A file command that needs a path: shows the native dialog first.
    Open,
    SaveAs,
    ImportZmx,
    ExportZmx,
    AddCatalog,
    OpenRecent(String),
    Sample(&'static str),
    Window(Panel),
    Theme(ThemeChoice),
    Language(&'static str),
    ResetLayout,
    FullScreen,
    CheckUpdates,
    Quit,
}

/// An action held back until the user decides about unsaved changes.
struct Pending {
    action: Action,
}

pub struct Workbench {
    ctx: Context,
    pub session: Session,
    pub worker: Worker,
    pub tokens: Tokens,
    prefs: craft_settings::SettingsStore<Prefs>,
    workspace: Workspace<Panel>,
    pub overview: Option<Overview>,
    pub overview_error: Option<String>,
    overview_ms: Option<f64>,
    overview_tag: u64,
    /// The lens as JSON, refreshed when it changes; the input of every analysis tag.
    pub system_json: String,
    system_seen: instaoptics_core::LensSystem,
    pub analyses: HashMap<Panel, AnalysisWindow>,
    pub lde: lde::State,
    pub explorer: explorer::State,
    pub tools: tools::State,
    pub view3d: view3d::State,
    pub textures: Textures,
    pub selected_surface: Option<usize>,
    status: Option<(String, bool)>,
    pending: Option<Pending>,
    control: Option<control::Server>,
    quitting: bool,
    updates: crate::updates::Updates,
    actions: Vec<Action>,
}

fn default_layout() -> Layout<Panel> {
    let tabs = |panels: Vec<Panel>| Node::Tabs { panels, active: 0 };
    let split = |axis, size, first, second| Node::Split { axis, size, first: Box::new(first), second: Box::new(second) };
    let views = split(SplitAxis::Horizontal, SplitSize::Ratio(0.5), tabs(vec![Panel::Layout, Panel::Layout3d]), tabs(vec![Panel::Spot, Panel::Mtf, Panel::RayFan]));
    let right = split(SplitAxis::Vertical, SplitSize::Ratio(0.66), views, tabs(vec![Panel::LensData, Panel::SystemData, Panel::Configs]));
    Layout { root: Some(split(SplitAxis::Horizontal, SplitSize::FixedFirst(270.0), tabs(vec![Panel::SystemExplorer]), right)), floating: Vec::new() }
}

fn permissions(_: &Panel) -> Permissions {
    Permissions::default()
}

/// Where a window opens when it is not in the layout: next to the windows of its kind.
fn area(panel: Panel) -> u8 {
    match panel {
        Panel::SystemExplorer => 0,
        Panel::LensData | Panel::SystemData | Panel::Configs => 1,
        Panel::Layout | Panel::Layout3d => 2,
        _ => 3,
    }
}

pub fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+') {
        match part {
            "Ctrl" => modifiers = modifiers | Modifiers::COMMAND,
            "Shift" => modifiers = modifiers | Modifiers::SHIFT,
            "Alt" => modifiers = modifiers | Modifiers::ALT,
            k => key = Key::from_name(k),
        }
    }
    key.map(|k| KeyboardShortcut::new(modifiers, k))
}

impl Workbench {
    pub fn new(ctx: &Context, file: Option<PathBuf>, control_port: Option<u16>) -> Self {
        let (prefs, report) = craft_settings::SettingsStore::<Prefs>::open(craft_settings::Location::Auto);
        log::info!("settings: {report:?}");
        let catalog_dir = craft_settings::paths::data_dir("instaoptics").map(|d| d.join("catalogs"));
        let mut session = Session::new(catalog_dir);
        let mut status = None;
        match &file {
            Some(path) => {
                let zmx = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zmx"));
                if let Err(e) = commands::execute(&mut session, if zmx { "import.zmx" } else { "open" }, &json!({ "path": path })) {
                    status = Some((e, true));
                }
            }
            None => {
                let _ = commands::execute(&mut session, "sample", &json!({ "id": "cooke" }));
            }
        }
        let mut workspace = serde_json::from_str::<Workspace<Panel>>(&prefs.get().workspace).ok().filter(|w| w.validate().is_ok() && w.layout.root.is_some()).unwrap_or_default();
        if workspace.layout.root.is_none() {
            workspace = Workspace::new(default_layout());
        }
        let theme = prefs.get().theme;
        crate::i18n::set(&prefs.get().language);
        crate::i18n::install_fonts(ctx);
        let control = control_port.and_then(|port| match control::start(port, ctx.clone()) {
            Ok(server) => Some(server),
            Err(e) => {
                status = Some((e, true));
                None
            }
        });
        let mut wb = Self {
            ctx: ctx.clone(),
            worker: Worker::new(ctx.clone()),
            tokens: Tokens::new(true),
            prefs,
            workspace,
            overview: None,
            overview_error: None,
            overview_ms: None,
            overview_tag: 0,
            system_json: String::new(),
            system_seen: session.system().clone(),
            session,
            analyses: HashMap::new(),
            lde: lde::State::default(),
            explorer: explorer::State::default(),
            tools: tools::State::default(),
            view3d: view3d::State::default(),
            textures: Textures::default(),
            selected_surface: Some(0),
            status,
            pending: None,
            control,
            quitting: false,
            updates: crate::updates::Updates::default(),
            actions: Vec::new(),
        };
        wb.system_json = serde_json::to_string(wb.session.system()).unwrap_or_default();
        wb.apply_theme(ctx, theme);
        wb
    }

    fn apply_theme(&mut self, ctx: &Context, choice: ThemeChoice) {
        let dark = match choice {
            ThemeChoice::Dark => true,
            ThemeChoice::Light => false,
            ThemeChoice::System => ctx.system_theme().is_none_or(|t| t == egui::Theme::Dark),
        };
        self.tokens = Tokens::new(dark);
        ctx.set_visuals(self.tokens.visuals());
        self.prefs.update(|p| p.theme = choice);
    }

    pub fn worker_ctx(&self) -> Context {
        self.ctx.clone()
    }

    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some((text.into(), error));
    }

    /// Runs an engine command and reports its error in the status bar.
    pub fn run(&mut self, id: &str, params: Value) -> Option<Value> {
        match commands::execute(&mut self.session, id, &params) {
            Ok(v) => Some(v),
            Err(e) => {
                self.set_status(e, true);
                None
            }
        }
    }

    fn needs_confirmation(action: &Action) -> bool {
        matches!(action, Action::Command("new", _) | Action::Open | Action::ImportZmx | Action::OpenRecent(_) | Action::Sample(_) | Action::Quit)
    }

    fn perform(&mut self, ctx: &Context, action: Action) {
        if Self::needs_confirmation(&action) && self.session.is_dirty() && self.pending.is_none() {
            self.pending = Some(Pending { action });
            return;
        }
        let lens = ("instaOptics lens", &["iol", "json"][..]);
        match action {
            Action::Command(id, params) => {
                if id == "save" && self.session.path.is_none() {
                    return self.perform(ctx, Action::SaveAs);
                }
                if self.run(id, params).is_some() && (id == "save" || id == "saveas") {
                    self.remember_file();
                }
            }
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new().add_filter(lens.0, lens.1).pick_file() {
                    self.open_path(path);
                }
            }
            Action::OpenRecent(path) => self.open_path(PathBuf::from(path)),
            Action::SaveAs => {
                let name = format!("{}.iol", self.session.system().name);
                if let Some(path) = rfd::FileDialog::new().add_filter(lens.0, &["iol"]).set_file_name(name).save_file()
                    && self.run("saveas", json!({ "path": path })).is_some()
                {
                    self.remember_file();
                }
            }
            Action::ImportZmx => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Zemax lens", &["zmx"]).pick_file()
                    && let Some(reply) = self.run("import.zmx", json!({ "path": path }))
                {
                    let notes: Vec<String> = reply["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str().map(str::to_string)).collect();
                    self.set_status(if notes.is_empty() { format!("Imported {}", path.display()) } else { notes.join(" · ") }, !notes.is_empty());
                }
            }
            Action::ExportZmx => {
                let name = format!("{}.zmx", self.session.system().name);
                if let Some(path) = rfd::FileDialog::new().add_filter("Zemax lens", &["zmx"]).set_file_name(name).save_file() {
                    self.run("export.zmx", json!({ "path": path }));
                }
            }
            Action::AddCatalog => {
                if let Some(paths) = rfd::FileDialog::new().add_filter("OpticStudio glass catalog", &["agf"]).pick_files() {
                    for path in paths {
                        if let Some(name) = self.run("catalog.add", json!({ "path": path })) {
                            self.set_status(format!("Loaded catalog {}", name.as_str().unwrap_or_default()), false);
                        }
                    }
                }
            }
            Action::Sample(id) => {
                self.run("sample", json!({ "id": id }));
            }
            Action::Window(panel) => self.open_window(panel),
            Action::Theme(choice) => self.apply_theme(ctx, choice),
            Action::Language(code) => {
                crate::i18n::set(code);
                self.prefs.update(|p| p.language = code.to_string());
            }
            Action::ResetLayout => {
                self.workspace = Workspace::new(default_layout());
                self.save_workspace();
            }
            Action::FullScreen => {
                let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
            }
            Action::CheckUpdates => self.updates.check_now(ctx),
            Action::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        let zmx = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zmx"));
        if self.run(if zmx { "import.zmx" } else { "open" }, json!({ "path": path })).is_some() && !zmx {
            self.remember_file();
        }
    }

    fn remember_file(&mut self) {
        if let Some(path) = self.session.path.as_ref().map(|p| p.to_string_lossy().into_owned()) {
            self.prefs.update(|p| p.push_recent(&path));
        }
    }

    pub fn open_window(&mut self, panel: Panel) {
        let present = self.workspace.layout.root.as_ref().is_some_and(|r| contains(r, panel)) || self.workspace.layout.floating.iter().any(|f| f.panels().any(|p| *p == panel));
        let action = if present {
            DockAction::Activate { panel }
        } else {
            let anchor = Panel::ALL.iter().copied().find(|p| *p != panel && area(*p) == area(panel) && self.workspace.layout.root.as_ref().is_some_and(|r| contains(r, *p)));
            DockAction::Open { panel, anchor }
        };
        if let Err(e) = self.workspace.apply(action, permissions) {
            self.set_status(format!("{e:?}"), true);
        }
        self.save_workspace();
    }

    fn save_workspace(&mut self) {
        if let Ok(json) = serde_json::to_string(&self.workspace) {
            self.prefs.update(|p| p.workspace = json);
        }
    }

    pub fn open_windows(&self) -> Vec<Panel> {
        Panel::ALL.iter().copied().filter(|p| self.workspace.layout.root.as_ref().is_some_and(|r| contains(r, *p))).collect()
    }

    // ---------- frame ----------

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.collect_results();
        self.refresh_overview();
        self.handle_control(&ctx);
        self.shortcuts(&ctx);
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.session.is_dirty() && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending { action: Action::Quit });
        }
        let title = format!("{}{} — instaOptics", if self.session.is_dirty() { "● " } else { "" }, self.session.file_name());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").exact_size(22.0).show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let layout = self.workspace.layout.clone();
            let style = DockStyle::from_ui(ui);
            let output = DockArea::new(egui::Id::unique("instaoptics-dock")).show(ui, &layout, &style, |p| p.title().to_string(), permissions, |ui, panel| self.panel_ui(ui, *panel));
            if !output.actions.is_empty() {
                for action in output.actions {
                    if let Err(e) = self.workspace.apply(action, permissions) {
                        log::warn!("dock: {e:?}");
                    }
                }
                self.save_workspace();
            }
        });
        self.confirm_dialog(&ctx);
        self.updates.frame(&ctx);
        for action in std::mem::take(&mut self.actions) {
            self.perform(&ctx, action);
        }
        self.textures.end_frame();
        self.prefs.tick(std::time::Instant::now());
    }

    pub fn on_exit(&mut self) {
        let _ = self.prefs.flush();
        self.updates.on_exit();
    }

    fn panel_ui(&mut self, ui: &mut Ui, panel: Panel) {
        match panel {
            Panel::SystemExplorer => explorer::ui(self, ui),
            Panel::LensData => lde::ui(self, ui),
            Panel::SystemData => explorer::system_data(self, ui),
            Panel::Configs => tools::configs(self, ui),
            Panel::Layout => crate::analysis::layout_2d(self, ui),
            Panel::Layout3d => view3d::ui(self, ui),
            Panel::Beam => tools::beam(self, ui),
            Panel::ImageSim => tools::image_sim(self, ui),
            Panel::Optimize => tools::optimize(self, ui),
            Panel::Tolerance => tools::tolerance(self, ui),
            Panel::Glasses => tools::glasses(self, ui),
            _ => crate::analysis::ui(self, ui, panel),
        }
    }

    fn collect_results(&mut self) {
        for done in self.worker.poll() {
            match (done.key, done.output) {
                (JobKey::Overview, Output::Overview(result)) => {
                    self.overview_ms = Some(done.ms);
                    match result {
                        Ok(o) => {
                            self.overview = Some(o);
                            self.overview_error = None;
                        }
                        Err(e) => self.overview_error = Some(e),
                    }
                }
                (JobKey::Analysis(panel), Output::Analysis(result)) => {
                    let system = self.session.system().clone();
                    let working = self.overview.as_ref().map(|o| o.paraxial.working_f_number);
                    self.analyses.entry(panel).or_insert_with(|| AnalysisWindow::new(panel)).receive(done.tag, done.ms, result, &system, working);
                }
                (key, output) => tools::receive(self, key, done.tag, done.ms, output),
            }
        }
    }

    fn refresh_overview(&mut self) {
        if *self.session.system() != self.system_seen {
            self.system_seen = self.session.system().clone();
            self.system_json = serde_json::to_string(&self.system_seen).unwrap_or_default();
        }
        let t = tag(&[&self.system_json, "overview"]);
        if t != self.overview_tag && self.worker.running_tag(JobKey::Overview) != Some(t) {
            let system = self.session.system().clone();
            self.worker.spawn(JobKey::Overview, t, move |_, _| Output::Overview(instaoptics_core::overview(&system, 7).map_err(|e| e.0)));
        }
        if self.overview_ms.is_some() && self.worker.running_tag(JobKey::Overview).is_none() {
            self.overview_tag = t;
        }
    }

    fn shortcuts(&mut self, ctx: &Context) {
        // A focused text field keeps its own undo, copy and paste.
        let typing = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
        let mut fired = Vec::new();
        ctx.input_mut(|input| {
            for spec in commands::COMMANDS {
                if let Some(sc) = spec.shortcut.and_then(parse_shortcut)
                    && !(typing && matches!(spec.id, "undo" | "redo"))
                    && input.consume_shortcut(&sc)
                {
                    fired.push(match spec.id {
                        "open" => Action::Open,
                        "saveas" => Action::SaveAs,
                        id => Action::Command(id, json!({})),
                    });
                }
            }
            for panel in Panel::ALL {
                if let Some(sc) = panel.shortcut().and_then(parse_shortcut)
                    && input.consume_shortcut(&sc)
                {
                    fired.push(Action::Window(panel));
                }
            }
            if input.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) && !typing {
                fired.push(Action::Command("redo", json!({})));
            }
            if input.consume_key(Modifiers::NONE, Key::F11) {
                fired.push(Action::FullScreen);
            }
        });
        self.actions.extend(fired);
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            let item = |ui: &mut Ui, label: &str, shortcut: Option<&str>| {
                let mut button = egui::Button::new(label);
                if let Some(s) = shortcut {
                    button = button.shortcut_text(s);
                }
                ui.add(button).clicked()
            };
            let mut chosen: Option<Action> = None;
            let mut pick = |a: Action| chosen = Some(a);
            ui.menu_button(t("File"), |ui| {
                if item(ui, t("New System"), Some("Ctrl+N")) {
                    pick(Action::Command("new", json!({})));
                }
                if item(ui, t("Open…"), Some("Ctrl+O")) {
                    pick(Action::Open);
                }
                let recent = self.prefs.get().recent.clone();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button(t("Open Recent"), |ui| {
                        for path in recent {
                            if ui.button(&path).clicked() {
                                pick(Action::OpenRecent(path));
                            }
                        }
                    });
                });
                ui.menu_button(t("Samples"), |ui| {
                    for (id, system) in instaoptics_core::system::samples() {
                        if ui.button(&system.name).clicked() {
                            pick(Action::Sample(id));
                        }
                    }
                });
                ui.separator();
                if item(ui, t("Save"), Some("Ctrl+S")) {
                    pick(Action::Command("save", json!({})));
                }
                if item(ui, t("Save As…"), Some("Ctrl+Shift+S")) {
                    pick(Action::SaveAs);
                }
                ui.separator();
                if item(ui, t("Import Zemax (.zmx)…"), None) {
                    pick(Action::ImportZmx);
                }
                if item(ui, t("Export Zemax (.zmx)…"), None) {
                    pick(Action::ExportZmx);
                }
                ui.separator();
                if item(ui, t("Exit"), None) {
                    pick(Action::Quit);
                }
            });
            ui.menu_button(t("Edit"), |ui| {
                if ui.add_enabled(self.session.can_undo(), egui::Button::new(t("Undo")).shortcut_text("Ctrl+Z")).clicked() {
                    pick(Action::Command("undo", json!({})));
                }
                if ui.add_enabled(self.session.can_redo(), egui::Button::new(t("Redo")).shortcut_text("Ctrl+Y")).clicked() {
                    pick(Action::Command("redo", json!({})));
                }
            });
            let windows = |ui: &mut Ui, list: &[Panel], pick: &mut dyn FnMut(Action)| {
                for p in list {
                    if item(ui, p.title(), p.shortcut()) {
                        pick(Action::Window(*p));
                    }
                }
            };
            ui.menu_button(t("Setup"), |ui| windows(ui, panels::SETUP, &mut pick));
            ui.menu_button(t("Analyze"), |ui| {
                ui.menu_button(t("Image Quality"), |ui| windows(ui, panels::IMAGE_QUALITY, &mut pick));
                ui.menu_button(t("Aberrations"), |ui| windows(ui, panels::ABERRATIONS, &mut pick));
                ui.menu_button(t("Polarization"), |ui| windows(ui, panels::POLARIZATION, &mut pick));
                ui.menu_button(t("Physical Optics"), |ui| windows(ui, panels::PHYSICAL, &mut pick));
            });
            ui.menu_button(t("Optimize"), |ui| {
                windows(ui, &[Panel::Optimize], &mut pick);
                if item(ui, t("Remove All Variables"), None) {
                    pick(Action::Command("variables.clear", json!({})));
                }
            });
            ui.menu_button(t("Tolerance"), |ui| windows(ui, &[Panel::Tolerance], &mut pick));
            ui.menu_button(t("Libraries"), |ui| {
                windows(ui, &[Panel::Glasses], &mut pick);
                if item(ui, t("Add Glass Catalog…"), None) {
                    pick(Action::AddCatalog);
                }
            });
            ui.menu_button(t("View"), |ui| {
                let current = self.prefs.get().theme;
                for (label, choice) in [("System Theme", ThemeChoice::System), ("Dark Theme", ThemeChoice::Dark), ("Light Theme", ThemeChoice::Light)] {
                    if ui.radio(current == choice, label).clicked() {
                        pick(Action::Theme(choice));
                    }
                }
                ui.menu_button(t("Language"), |ui| {
                    let current = self.prefs.get().language.clone();
                    if ui.radio(current.is_empty() || current == "system", t("System Language")).clicked() {
                        pick(Action::Language("system"));
                    }
                    for (code, name, _) in crate::i18n::LANGUAGES {
                        if ui.radio(current == code, name).clicked() {
                            pick(Action::Language(code));
                        }
                    }
                });
                ui.separator();
                if item(ui, t("Reset Window Layout"), None) {
                    pick(Action::ResetLayout);
                }
                if item(ui, t("Full Screen"), Some("F11")) {
                    pick(Action::FullScreen);
                }
            });
            ui.menu_button(t("Help"), |ui| {
                if item(ui, t("Check for Updates…"), None) {
                    pick(Action::CheckUpdates);
                }
                ui.separator();
                ui.label(format!("instaOptics {}", env!("CARGO_PKG_VERSION")));
                ui.hyperlink_to(t("Source code"), "https://github.com/Oneptica/instaOptics");
            });
            if let Some(a) = chosen {
                self.act(a);
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let tk = self.tokens;
        ui.horizontal_centered(|ui| {
            if let Some(e) = &self.overview_error {
                ui.label(RichText::new(e).color(tk.error).small());
            } else if let Some(ms) = self.overview_ms {
                ui.label(RichText::new(format!("{ms:.1} ms")).color(tk.muted).small()).on_hover_text(t("Time for paraxial data, apertures and layout"));
            }
            if let Some((text, error, link)) = self.updates.status() {
                ui.separator();
                let label = RichText::new(text).color(if error { tk.error } else { tk.muted }).small();
                match link {
                    Some(url) => {
                        ui.hyperlink_to(label, url);
                    }
                    None => {
                        ui.label(label);
                    }
                }
            }
            if let Some((text, error)) = &self.status {
                ui.separator();
                ui.label(RichText::new(text).color(if *error { tk.error } else { tk.muted }).small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let system = self.session.system();
                ui.label(RichText::new(format!("{} {}", system.surfaces.len(), t("surfaces"))).small());
                if let Some(w) = system.wavelengths.get(system.primary_wavelength) {
                    ui.label(RichText::new(format!("λ {w:.4} µm")).monospace().small());
                }
                if let Some(o) = &self.overview {
                    let p = &o.paraxial;
                    ui.label(RichText::new(format!("TTL {:.3} mm", p.total_track)).monospace().small());
                    ui.label(RichText::new(format!("F/# {:.3}", p.f_number)).monospace().small());
                    ui.label(RichText::new(format!("EFL {:.4}", p.efl)).monospace().small());
                }
                if let Some(c) = &self.control {
                    ui.label(RichText::new(format!("control :{}", c.port)).color(tk.muted).small());
                }
            });
        });
    }

    fn confirm_dialog(&mut self, ctx: &Context) {
        let Some(pending) = &self.pending else { return };
        let action = pending.action.clone();
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("unsaved")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(format!("Save changes to {}?", self.session.file_name())).strong());
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(t("Save")).clicked() {
                    choice = Some(1);
                }
                if ui.button(t("Don't Save")).clicked() {
                    choice = Some(2);
                }
                if ui.button(t("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(0);
                }
            });
        });
        match choice {
            Some(1) => {
                self.pending = None;
                self.perform(ctx, Action::Command("save", json!({})));
                if !self.session.is_dirty() {
                    self.perform(ctx, action);
                }
            }
            Some(2) => {
                // Discarding: load the action without asking again.
                self.pending = Some(Pending { action: action.clone() });
                self.perform_unchecked(ctx, action);
                self.pending = None;
            }
            Some(_) => self.pending = None,
            None => {}
        }
    }

    fn perform_unchecked(&mut self, ctx: &Context, action: Action) {
        if action == Action::Quit {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.perform(ctx, action);
        }
    }

    // ---------- control channel ----------

    fn handle_control(&mut self, ctx: &Context) {
        let Some(server) = &self.control else { return };
        let requests: Vec<control::Request> = server.rx.try_iter().collect();
        for request in requests {
            let params = request.params;
            let reply = match request.method.as_str() {
                "engine.execute" => match params.get("command").and_then(Value::as_str) {
                    Some(command) => commands::execute(&mut self.session, command, params.get("params").unwrap_or(&Value::Null)),
                    None => Err("command is required".into()),
                },
                "engine.commands" => Ok(commands::catalog()),
                "ui.windows" => Ok(Value::Array(Panel::ALL.iter().map(|p| json!({ "id": p.id(), "title": p.title() })).collect())),
                "ui.open" => match params.get("window").and_then(Value::as_str).and_then(Panel::from_id) {
                    Some(panel) => {
                        self.open_window(panel);
                        Ok(Value::Null)
                    }
                    None => Err("window: one of ui.windows".into()),
                },
                "ui.state" => Ok(json!({
                    "theme": if self.tokens.dark { "dark" } else { "light" },
                    "windows": self.open_windows().iter().map(|p| p.id()).collect::<Vec<_>>(),
                    "selectedSurface": self.selected_surface.map(|s| s + 1),
                    "path": self.session.path.as_ref().map(|p| p.to_string_lossy()),
                    "dirty": self.session.is_dirty(),
                })),
                "ui.screenshot" => {
                    let path = params.get("path").and_then(Value::as_str).map(str::to_string);
                    let reply = request.reply;
                    ctx.request_screenshot(move |image| {
                        let _ = reply.send(encode_screenshot(&image, path.as_deref()));
                    });
                    ctx.request_repaint();
                    continue;
                }
                "ui.resize" => match (params.get("width").and_then(Value::as_f64), params.get("height").and_then(Value::as_f64)) {
                    (Some(w), Some(h)) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w as f32, h as f32)));
                        Ok(Value::Null)
                    }
                    _ => Err("width and height are required".into()),
                },
                "ui.language" => match params.get("language").and_then(Value::as_str) {
                    Some(code) => {
                        let code = crate::i18n::LANGUAGES.iter().map(|l| l.0).find(|c| *c == code).unwrap_or("system");
                        self.act(Action::Language(code));
                        Ok(Value::Null)
                    }
                    None => Err("language is required".into()),
                },
                "ping" => Ok(json!("pong")),
                "control.methods" => Ok(Value::Array(control::METHODS.iter().map(|(m, d)| json!({ "method": m, "description": d })).collect())),
                m => Err(format!("Unknown method {m}; control.methods lists them")),
            };
            let _ = request.reply.send(reply);
        }
    }
}

fn encode_screenshot(image: &egui::ColorImage, path: Option<&str>) -> Result<Value, String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("screenshot size")?;
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    match path {
        Some(p) => std::fs::write(p, &png).map(|_| json!({ "path": p, "width": w, "height": h })).map_err(|e| e.to_string()),
        None => Ok(json!({ "pngBase64": commands::base64(&png), "width": w, "height": h })),
    }
}

fn contains(node: &Node<Panel>, panel: Panel) -> bool {
    match node {
        Node::Split { first, second, .. } => contains(first, panel) || contains(second, panel),
        Node::Tabs { panels, .. } => panels.contains(&panel),
        Node::Stack { entries } => entries.iter().any(|e| e.panel == panel),
        Node::Dock { columns, .. } => columns.iter().any(|c| c.groups.iter().any(|g| g.panels.contains(&panel))),
        _ => false,
    }
}
