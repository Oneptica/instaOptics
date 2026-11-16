//! The ribbon: a title row with the file menu and quick buttons, the tab row, and the grouped icon buttons under it.

use super::*;
use egui::{Align2, FontId, Frame, Margin, Sense, Stroke, vec2};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::*;

const TABS: [&str; 7] = ["Setup", "Analyze", "Optimize", "Tolerance", "Libraries", "View", "Help"];

fn icon(panel: Panel) -> MaterialIcon {
    match panel {
        Panel::SystemExplorer => ICON_TUNE,
        Panel::LensData => ICON_TABLE_ROWS,
        Panel::SystemData => ICON_INFO,
        Panel::Configs => ICON_LAYERS,
        Panel::Layout => ICON_SHOW_CHART,
        Panel::Layout3d => ICON_VIEW_IN_AR,
        Panel::Spot => ICON_SCATTER_PLOT,
        Panel::RayFan => ICON_SSID_CHART,
        Panel::Mtf => ICON_MONITORING,
        Panel::Psf => ICON_BLUR_ON,
        Panel::Wavefront => ICON_WAVES,
        Panel::ThroughFocus => ICON_CENTER_FOCUS_STRONG,
        Panel::MtfVsField => ICON_INSERT_CHART,
        Panel::Footprint => ICON_GRID_ON,
        Panel::FieldCurves => ICON_STACKED_LINE_CHART,
        Panel::Seidel => ICON_BAR_CHART,
        Panel::ChromaticFocalShift => ICON_PALETTE,
        Panel::Illumination => ICON_LIGHT_MODE,
        Panel::Coating => ICON_FILTER_NONE,
        Panel::Polarization => ICON_SYNC_ALT,
        Panel::Transmission => ICON_PERCENT,
        Panel::Beam => ICON_FLARE,
        Panel::ImageSim => ICON_IMAGE,
        Panel::Optimize => ICON_AUTO_FIX_HIGH,
        Panel::Tolerance => ICON_STRAIGHTEN,
        Panel::Glasses => ICON_DIAMOND,
    }
}

/// An icon over a label. Returns whether it was clicked.
fn big_button(ui: &mut Ui, tk: &Tokens, icon: MaterialIcon, label: &str) -> bool {
    let galley = ui.painter().layout(label.to_owned(), FontId::proportional(12.0), tk.text, 90.0);
    let width = (galley.size().x + 14.0).max(58.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 62.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 5, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    ui.painter().text(egui::pos2(rect.center().x, rect.top() + 22.0), Align2::CENTER_CENTER, icon.codepoint, FontId::new(24.0, icon.font_family()), tk.accent);
    ui.painter().galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 40.0), galley, tk.text);
    response.clicked()
}

/// A small icon button for the title row.
fn quick_button(ui: &mut Ui, tk: &Tokens, icon: MaterialIcon, tip: &str, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let response = response.on_hover_text(tip);
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 5, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let color = if enabled { tk.text } else { tk.muted.gamma_multiply(0.5) };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon.codepoint, FontId::new(18.0, icon.font_family()), color);
    enabled && response.clicked()
}

fn group(ui: &mut Ui, tk: &Tokens, label: Option<&str>, buttons: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            buttons(ui);
        });
        if let Some(label) = label {
            ui.vertical_centered(|ui| ui.label(RichText::new(label).small().color(tk.muted)));
        }
    });
    let x = ui.cursor().left() + 4.0;
    let rect = ui.min_rect();
    ui.painter().vline(x, rect.top() + 4.0..=rect.bottom() - 4.0, Stroke::new(1.0, tk.border));
    ui.add_space(10.0);
}

impl Workbench {
    pub(super) fn ribbon(&mut self, ui: &mut Ui) {
        let tk = self.tokens;
        let ctx = ui.ctx().clone();
        egui::Panel::top("title").exact_size(40.0).frame(Frame::new().fill(tk.panel).inner_margin(Margin::symmetric(10, 5))).show(ui, |ui| {
            // The bar moves the window; the controls added after it take the clicks that land on them.
            let bar = ui.interact(ui.max_rect(), ui.id().with("drag"), Sense::click_and_drag());
            if bar.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("instaOptics").strong().size(15.0));
                ui.add_space(6.0);
                ui.allocate_ui(vec2(48.0, 28.0), |ui| self.file_menu(ui));
                ui.add_space(6.0);
                if quick_button(ui, &tk, ICON_SAVE, t("Save"), true) {
                    self.act(Action::Command("save", json!({})));
                }
                if quick_button(ui, &tk, ICON_UNDO, t("Undo"), self.session.can_undo()) {
                    self.act(Action::Command("undo", json!({})));
                }
                if quick_button(ui, &tk, ICON_REDO, t("Redo"), self.session.can_redo()) {
                    self.act(Action::Command("redo", json!({})));
                }
                ui.add_space(10.0);
                let name = format!("{}{}", self.session.file_name(), if self.session.is_dirty() { " ●" } else { "" });
                Frame::new().fill(tk.backdrop).corner_radius(5).inner_margin(Margin::symmetric(10, 3)).show(ui, |ui| ui.label(name));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if quick_button(ui, &tk, ICON_CLOSE, "", true) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    if quick_button(ui, &tk, ICON_CROP_SQUARE, "", true) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if quick_button(ui, &tk, ICON_MINIMIZE, "", true) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    ui.add_space(8.0);
                    self.search_box(ui);
                });
            });
        });
        egui::Panel::top("ribbon_tabs").exact_size(30.0).frame(Frame::new().fill(tk.panel).inner_margin(Margin::symmetric(10, 0))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                for (i, name) in TABS.iter().enumerate() {
                    let active = self.ribbon_tab == i;
                    let text = RichText::new(t(name)).color(if active { tk.accent } else { tk.text });
                    let response = ui.add(egui::Button::new(if active { text.strong() } else { text }).frame(false));
                    if active {
                        let r = response.rect;
                        ui.painter().hline(r.x_range(), r.bottom() + 1.0, Stroke::new(2.0, tk.accent));
                    }
                    if response.clicked() {
                        self.ribbon_tab = i;
                    }
                }
            });
        });
        egui::Panel::top("ribbon_body").exact_size(92.0).frame(Frame::new().fill(tk.header).stroke(Stroke::new(1.0, tk.border)).inner_margin(Margin::symmetric(10, 6))).show(ui, |ui| {
            egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                ui.horizontal(|ui| self.ribbon_body(ui));
            });
        });
    }

    fn panel_group(&mut self, ui: &mut Ui, label: &str, list: &[Panel]) {
        let tk = self.tokens;
        group(ui, &tk, Some(label), |ui| {
            for p in list {
                if big_button(ui, &tk, icon(*p), p.title()) {
                    self.act(Action::Window(*p));
                }
            }
        });
    }

    fn ribbon_body(&mut self, ui: &mut Ui) {
        let tk = self.tokens;
        match self.ribbon_tab {
            0 => self.panel_group(ui, t("Setup"), panels::SETUP),
            1 => {
                self.panel_group(ui, t("Image Quality"), panels::IMAGE_QUALITY);
                self.panel_group(ui, t("Aberrations"), panels::ABERRATIONS);
                self.panel_group(ui, t("Polarization"), panels::POLARIZATION);
                self.panel_group(ui, t("Physical Optics"), panels::PHYSICAL);
            }
            2 => self.panel_group(ui, t("Optimize"), &[Panel::Optimize]),
            3 => self.panel_group(ui, t("Tolerance"), &[Panel::Tolerance]),
            4 => {
                self.panel_group(ui, t("Libraries"), &[Panel::Glasses]);
                group(ui, &tk, None, |ui| {
                    if big_button(ui, &tk, ICON_NOTE_ADD, t("Add Glass Catalog…")) {
                        self.act(Action::AddCatalog);
                    }
                });
            }
            5 => {
                let current = self.prefs.get().theme;
                group(ui, &tk, Some(t("View")), |ui| {
                    ui.add_space(14.0);
                    let choices = [("System Theme", ThemeChoice::System), ("Dark Theme", ThemeChoice::Dark), ("Light Theme", ThemeChoice::Light)];
                    let shown = choices.iter().find(|(_, c)| *c == current).map(|(l, _)| t(l)).unwrap_or_default();
                    egui::ComboBox::from_id_salt("theme").selected_text(shown).show_ui(ui, |ui| {
                        for (label, choice) in choices {
                            if ui.selectable_label(current == choice, t(label)).clicked() {
                                self.act(Action::Theme(choice));
                            }
                        }
                    });
                });
                let language = self.prefs.get().language.clone();
                group(ui, &tk, Some(t("Language")), |ui| {
                    let shown = crate::i18n::LANGUAGES.iter().find(|(code, _, _)| *code == language).map(|(_, name, _)| *name).unwrap_or(t("System Language"));
                    ui.add_space(14.0);
                    egui::ComboBox::from_id_salt("language").selected_text(shown).show_ui(ui, |ui| {
                        if ui.selectable_label(language.is_empty() || language == "system", t("System Language")).clicked() {
                            self.act(Action::Language("system"));
                        }
                        for (code, name, _) in crate::i18n::LANGUAGES {
                            if ui.selectable_label(language == code, name).clicked() {
                                self.act(Action::Language(code));
                            }
                        }
                    });
                });
                group(ui, &tk, None, |ui| {
                    if big_button(ui, &tk, ICON_RESTART_ALT, t("Reset Window Layout")) {
                        self.act(Action::ResetLayout);
                    }
                    if big_button(ui, &tk, ICON_FULLSCREEN, t("Full Screen")) {
                        self.act(Action::FullScreen);
                    }
                });
            }
            _ => group(ui, &tk, None, |ui| {
                if big_button(ui, &tk, ICON_SYSTEM_UPDATE_ALT, t("Check for Updates…")) {
                    self.act(Action::CheckUpdates);
                }
                if big_button(ui, &tk, ICON_CODE, t("Source code")) {
                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/Oneptica/instaOptics"));
                }
            }),
        }
    }
}

impl Workbench {
    /// Everything the search box can run: the windows and the file, view and library actions.
    fn search_items(&self) -> Vec<(&'static str, Action)> {
        let mut items: Vec<(&'static str, Action)> = Panel::ALL.iter().map(|p| (p.title(), Action::Window(*p))).collect();
        items.extend([
            (t("New System"), Action::Command("new", json!({}))),
            (t("Open…"), Action::Open),
            (t("Save"), Action::Command("save", json!({}))),
            (t("Save As…"), Action::SaveAs),
            (t("Import Zemax (.zmx)…"), Action::ImportZmx),
            (t("Export Zemax (.zmx)…"), Action::ExportZmx),
            (t("Add Glass Catalog…"), Action::AddCatalog),
            (t("System Theme"), Action::Theme(ThemeChoice::System)),
            (t("Dark Theme"), Action::Theme(ThemeChoice::Dark)),
            (t("Light Theme"), Action::Theme(ThemeChoice::Light)),
            (t("Reset Window Layout"), Action::ResetLayout),
            (t("Full Screen"), Action::FullScreen),
            (t("Check for Updates…"), Action::CheckUpdates),
        ]);
        items
    }

    fn search_box(&mut self, ui: &mut Ui) {
        let tk = self.tokens;
        let response = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(t("Search commands")).desired_width(220.0).margin(Margin::symmetric(8, 4)));
        if response.gained_focus() || response.changed() {
            self.search_open = true;
        }
        if !self.search_open || self.search.trim().is_empty() {
            return;
        }
        let needle = self.search.trim().to_lowercase();
        let matches: Vec<_> = self.search_items().into_iter().filter(|(label, _)| label.to_lowercase().contains(&needle)).take(10).collect();
        let mut chosen = None;
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            chosen = matches.first().map(|(_, a)| a.clone());
        }
        let area = egui::Area::new(ui.id().with("search_results")).order(egui::Order::Foreground).fixed_pos(response.rect.left_bottom() + vec2(0.0, 4.0)).show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(response.rect.width());
                if matches.is_empty() {
                    ui.label(RichText::new("—").color(tk.muted));
                }
                for (label, action) in &matches {
                    if ui.selectable_label(false, *label).clicked() {
                        chosen = Some(action.clone());
                    }
                }
            });
        });
        let pressed_outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !response.rect.contains(p)));
        if chosen.is_some() || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.search_open = false;
            if chosen.is_some() {
                self.search.clear();
            }
        }
        if let Some(action) = chosen {
            self.act(action);
        }
    }
}
