//! Colours. The plot colours come from the renderer's theme so the window and an exported PNG match.

use egui::{Color32, Visuals};
use instaoptics_render::raster::Theme;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    System,
    Dark,
    #[default]
    Light,
}

#[derive(Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    pub plot: Theme,
    /// The window behind the cards.
    pub backdrop: Color32,
    pub panel: Color32,
    pub header: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub variable: Color32,
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

impl Tokens {
    pub fn new(dark: bool) -> Self {
        let plot = if dark { Theme::DARK } else { Theme::LIGHT };
        if dark {
            Tokens {
                dark,
                plot,
                backdrop: Color32::from_rgb(18, 18, 20),
                panel: rgb(plot.background),
                header: Color32::from_rgb(37, 37, 38),
                text: rgb(plot.text),
                muted: rgb(plot.muted),
                border: Color32::from_rgb(60, 60, 62),
                accent: Color32::from_rgb(79, 143, 247),
                selection: Color32::from_rgb(38, 79, 120),
                error: Color32::from_rgb(241, 76, 76),
                warning: Color32::from_rgb(210, 153, 34),
                variable: Color32::from_rgb(63, 185, 80),
            }
        } else {
            Tokens {
                dark,
                plot,
                backdrop: Color32::from_rgb(233, 236, 241),
                panel: rgb(plot.background),
                header: Color32::from_rgb(243, 245, 248),
                text: rgb(plot.text),
                muted: rgb(plot.muted),
                border: Color32::from_rgb(210, 210, 214),
                accent: Color32::from_rgb(0, 95, 184),
                selection: Color32::from_rgb(204, 228, 247),
                error: Color32::from_rgb(205, 49, 49),
                warning: Color32::from_rgb(160, 110, 0),
                variable: Color32::from_rgb(22, 130, 50),
            }
        }
    }

    pub fn color(&self, c: instaoptics_render::Color) -> Color32 {
        rgb(self.plot.rgb(c))
    }

    pub fn apply(&self, style: &mut egui::Style) {
        use egui::{CornerRadius, FontFamily, FontId, TextStyle};
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = self.backdrop;
        v.window_fill = self.panel;
        v.window_stroke = egui::Stroke::new(1.0, self.border);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(6);
        v.extreme_bg_color = if self.dark { Color32::from_rgb(24, 24, 24) } else { Color32::WHITE };
        v.selection.bg_fill = self.selection;
        v.selection.stroke = egui::Stroke::new(1.0, self.accent);
        v.hyperlink_color = self.accent;
        let hover = if self.dark { Color32::from_rgb(52, 54, 60) } else { Color32::from_rgb(226, 231, 240) };
        let idle = if self.dark { Color32::from_rgb(40, 42, 46) } else { self.header };
        let radius = CornerRadius::same(5);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = radius;
        }
        v.widgets.inactive.weak_bg_fill = idle;
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.border);
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.border);
        v.widgets.active.weak_bg_fill = self.selection;
        v.widgets.open.weak_bg_fill = hover;
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 26.0;
        let sizes = [(TextStyle::Small, 12.0), (TextStyle::Body, 14.0), (TextStyle::Button, 14.0), (TextStyle::Heading, 17.0)];
        for (ts, size) in sizes {
            style.text_styles.insert(ts, FontId::new(size, FontFamily::Proportional));
        }
        style.text_styles.insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
    }
}
