//! System Explorer (aperture, fields, wavelengths, object, ray aiming) and the System Data report.

use crate::i18n::t;
use egui::{RichText, Ui};
use instaoptics_core::system::{ApertureType, FieldType};
use instaoptics_render::figures::short;
use serde_json::{Value, json};

use crate::app::Workbench;

#[derive(Default)]
pub struct State {
    /// Text being typed into a number field, by field id, until Enter or focus loss commits it.
    pub drafts: std::collections::HashMap<String, String>,
}

const FDC: [f64; 3] = [0.4861327, 0.5875618, 0.6562725];

/// A number field that commits on Enter or when it loses focus; returns the new value.
pub fn number_field(ui: &mut Ui, drafts: &mut std::collections::HashMap<String, String>, id: &str, value: Option<f64>, width: f32) -> Option<Option<f64>> {
    let shown = value.map_or(String::new(), short);
    let text = drafts.entry(id.to_string()).or_insert_with(|| shown.clone());
    let response = ui.add(egui::TextEdit::singleline(text).desired_width(width).font(egui::TextStyle::Monospace));
    if response.lost_focus() {
        let entered = drafts.remove(id).unwrap_or_default();
        let t = entered.trim().to_lowercase();
        if t.is_empty() {
            return Some(None);
        }
        let parsed = if ["inf", "infinity", "∞"].contains(&t.as_str()) { Some(f64::INFINITY) } else { t.parse::<f64>().ok() };
        if let Some(v) = parsed
            && Some(v) != value
        {
            return Some(Some(v));
        }
    } else if !response.has_focus() && *text != shown {
        *text = shown;
    }
    None
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(true).show(ui, add);
}

pub fn ui(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system().clone();
    let tk = wb.tokens;
    let mut set: Option<Value> = None;
    let drafts = &mut wb.explorer.drafts;
    egui::ScrollArea::vertical().show(ui, |ui| {
        section(ui, t("Aperture"), |ui| {
            let types = [
                (ApertureType::EntrancePupilDiameter, t("Entrance pupil diameter"), "mm"),
                (ApertureType::ImageFNumber, t("Image space F/#"), ""),
                (ApertureType::WorkingFNumber, t("Paraxial working F/#"), ""),
                (ApertureType::ObjectNa, t("Object space NA"), ""),
                (ApertureType::FloatByStop, t("Float by stop size"), "mm"),
            ];
            let (_, label, unit) = types.iter().find(|(a, ..)| *a == system.aperture_type).copied().unwrap_or(types[0]);
            egui::ComboBox::from_id_salt("aperture-type").selected_text(label).width(220.0).show_ui(ui, |ui| {
                for (a, l, _) in types {
                    if ui.selectable_label(a == system.aperture_type, l).clicked() && a != system.aperture_type {
                        // Switching type starts from the value the current lens already has, so it does not jump.
                        let o = wb.overview.as_ref();
                        let epd = o.map_or(system.entrance_pupil_diameter, |o| o.entrance_pupil_diameter);
                        let value = match a {
                            ApertureType::EntrancePupilDiameter => None,
                            ApertureType::ImageFNumber => o.map(|o| o.paraxial.f_number),
                            ApertureType::WorkingFNumber => o.map(|o| o.paraxial.working_f_number),
                            ApertureType::ObjectNa => Some(match (system.object_distance, o) {
                                (Some(d), Some(o)) => (epd / 2.0 / (d + o.paraxial.entrance_pupil_z)).atan().sin(),
                                _ => 0.1,
                            }),
                            ApertureType::FloatByStop => o.map(|o| o.paraxial.stop_semi_diameter),
                        };
                        let value = value.filter(|v| v.is_finite()).map(|v| format!("{v:.6e}").parse::<f64>().unwrap_or(v));
                        set = Some(json!({ "apertureType": a, "entrancePupilDiameter": epd, "apertureValue": value }));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(label);
                let (current, key) = if system.aperture_type == ApertureType::EntrancePupilDiameter { (Some(system.entrance_pupil_diameter), "entrancePupilDiameter") } else { (system.aperture_value, "apertureValue") };
                if let Some(Some(v)) = number_field(ui, drafts, "aperture", current, 90.0)
                    && v > 0.0
                    && v.is_finite()
                {
                    set = Some(json!({ key: v }));
                }
                ui.label(unit);
            });
            if system.aperture_type != ApertureType::EntrancePupilDiameter
                && let Some(o) = &wb.overview
            {
                ui.label(RichText::new(format!("Entrance pupil diameter {:.4} mm", o.entrance_pupil_diameter)).small().color(tk.muted));
            }
        });
        section(ui, t("Fields"), |ui| {
            let types = [(FieldType::Angle, t("Angle (deg)")), (FieldType::ObjectHeight, t("Object height (mm)")), (FieldType::ImageHeight, t("Paraxial image height (mm)"))];
            let label = types.iter().find(|(f, _)| *f == system.field_type).map_or("", |(_, l)| *l);
            egui::ComboBox::from_id_salt("field-type").selected_text(label).width(220.0).show_ui(ui, |ui| {
                for (f, l) in types {
                    if ui.selectable_label(f == system.field_type, l).clicked() {
                        set = Some(json!({ "fieldType": f }));
                    }
                }
            });
            let mut fields = system.fields.clone();
            let mut changed = false;
            let mut remove = None;
            for (i, f) in system.fields.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:>2}", i + 1)).monospace().color(tk.color(instaoptics_render::field_color(i))));
                    if let Some(Some(v)) = number_field(ui, drafts, &format!("field{i}"), Some(*f), 90.0)
                        && v.is_finite()
                    {
                        fields[i] = v;
                        changed = true;
                    }
                    if ui.add_enabled(system.fields.len() > 1, egui::Button::new("×").small()).on_hover_text(t("Remove field")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                fields.remove(i);
                changed = true;
            }
            if ui.small_button(t("Add field")).clicked() {
                fields.push(fields.last().copied().unwrap_or(0.0));
                changed = true;
            }
            if changed {
                set = Some(json!({ "fields": fields }));
            }
        });
        section(ui, t("Wavelengths"), |ui| {
            ui.label(RichText::new(t("Wavelength (µm) · primary")).small().color(tk.muted));
            let mut wl = system.wavelengths.clone();
            let mut primary = system.primary_wavelength;
            let mut changed = false;
            let mut remove = None;
            for (i, w) in system.wavelengths.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{:>2}", i + 1)).monospace());
                    if let Some(Some(v)) = number_field(ui, drafts, &format!("wl{i}"), Some(*w), 90.0)
                        && v > 0.0
                        && v.is_finite()
                    {
                        wl[i] = v;
                        changed = true;
                    }
                    if ui.radio(i == primary, "").on_hover_text(t("Primary wavelength")).clicked() && i != primary {
                        primary = i;
                        changed = true;
                    }
                    if ui.add_enabled(system.wavelengths.len() > 1, egui::Button::new("×").small()).on_hover_text(t("Remove wavelength")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                wl.remove(i);
                if primary > i || primary >= wl.len() {
                    primary = primary.saturating_sub(1);
                }
                changed = true;
            }
            ui.horizontal(|ui| {
                if ui.small_button(t("Add wavelength")).clicked() {
                    wl.push(0.55);
                    changed = true;
                }
                if ui.small_button("F, d, C").on_hover_text(t("Use the F, d and C lines")).clicked() {
                    wl = FDC.to_vec();
                    primary = 1;
                    changed = true;
                }
            });
            if changed {
                set = Some(json!({ "wavelengths": wl, "primaryWavelength": primary }));
            }
        });
        section(ui, t("Object"), |ui| {
            let mut infinite = system.object_distance.is_none();
            if ui.checkbox(&mut infinite, t("Object at infinity")).changed() {
                set = Some(json!({ "objectDistance": if infinite { Value::Null } else { json!(1000.0) } }));
            }
            if let Some(d) = system.object_distance {
                ui.horizontal(|ui| {
                    ui.label(t("Object distance"));
                    if let Some(v) = number_field(ui, drafts, "object", Some(d), 90.0) {
                        set = Some(json!({ "objectDistance": v.filter(|v| v.is_finite() && *v > 0.0) }));
                    }
                    ui.label("mm");
                });
            }
        });
        section(ui, t("Ray Aiming"), |ui| {
            let mut aiming = system.ray_aiming;
            if ui.checkbox(&mut aiming, t("Aim real rays at the stop")).changed() {
                set = Some(json!({ "rayAiming": aiming }));
            }
        });
        section(ui, t("Title"), |ui| {
            let id = "name".to_string();
            let text = drafts.entry(id.clone()).or_insert_with(|| system.name.clone());
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(220.0));
            if r.lost_focus() {
                let name = drafts.remove(&id).unwrap_or_default();
                if !name.trim().is_empty() && name != system.name {
                    set = Some(json!({ "name": name.trim() }));
                }
            } else if !r.has_focus() && *text != system.name {
                *text = system.name.clone();
            }
        });
    });
    if let Some(patch) = set {
        wb.run("system.set", patch);
    }
}

fn fixed(v: f64, digits: usize) -> String {
    if v.is_finite() { format!("{v:.digits$}") } else { "Infinity".into() }
}

pub fn system_data(wb: &mut Workbench, ui: &mut Ui) {
    let system = wb.session.system();
    let tk = wb.tokens;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let grid = |ui: &mut Ui, id: &str, rows: Vec<(String, String, &str)>| {
            egui::Grid::new(id).num_columns(3).spacing([24.0, 3.0]).striped(true).show(ui, |ui| {
                for (k, v, u) in rows {
                    ui.label(k);
                    ui.label(RichText::new(v).monospace());
                    ui.label(RichText::new(u).color(tk.muted));
                    ui.end_row();
                }
            });
        };
        ui.heading(t("General"));
        match &wb.overview {
            Some(o) => {
                let p = &o.paraxial;
                grid(ui, "general", vec![
                    ("Effective focal length".into(), fixed(p.efl, 4), "mm"),
                    ("Back focal length".into(), fixed(p.bfl, 4), "mm"),
                    ("Image space F/#".into(), fixed(p.f_number, 4), ""),
                    ("Working F/#".into(), fixed(p.working_f_number, 4), ""),
                    ("Entrance pupil diameter".into(), fixed(o.entrance_pupil_diameter, 4), "mm"),
                    ("Entrance pupil position".into(), fixed(p.entrance_pupil_z, 4), "mm"),
                    ("Exit pupil position".into(), fixed(p.exit_pupil_z, 4), "mm"),
                    ("Stop semi-diameter (paraxial)".into(), fixed(p.stop_semi_diameter, 4), "mm"),
                    ("Paraxial image height".into(), fixed(p.image_height, 4), "mm"),
                    ("Paraxial image distance".into(), fixed(p.image_distance, 4), "mm"),
                    ("Total track".into(), fixed(p.total_track, 4), "mm"),
                ]);
            }
            None => {
                ui.label(RichText::new(wb.overview_error.clone().unwrap_or_else(|| t("Computing…").into())).color(tk.muted));
            }
        }
        ui.add_space(8.0);
        ui.heading(t("Object"));
        grid(ui, "object", vec![
            ("Object distance".into(), system.object_distance.map_or("Infinity".into(), |d| fixed(d, 4)), "mm"),
            ("Ray aiming".into(), if system.ray_aiming { t("Real").into() } else { t("Paraxial").into() }, ""),
        ]);
        ui.add_space(8.0);
        ui.heading(t("Fields"));
        let unit = if system.field_type == FieldType::Angle { "deg" } else { "mm" };
        grid(ui, "fields", system.fields.iter().enumerate().map(|(i, f)| (format!("Field {}", i + 1), short(*f), unit)).collect());
        ui.add_space(8.0);
        ui.heading(t("Wavelengths"));
        grid(
            ui,
            "wavelengths",
            system.wavelengths.iter().enumerate().map(|(i, w)| (format!("Wavelength {}{}", i + 1, if i == system.primary_wavelength { " (primary)" } else { "" }), format!("{w:.7}"), "µm")).collect(),
        );
    });
}
