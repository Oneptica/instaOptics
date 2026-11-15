//! Colours. The plot colours come from the renderer's theme so the window and an exported PNG match.

use egui::{Color32, Visuals};
use instaoptics_render::raster::Theme;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Tokens {
    pub dark: bool,
    pub plot: Theme,
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
                panel: rgb(plot.background),
                header: Color32::from_rgb(243, 243, 243),
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

    pub fn visuals(&self) -> Visuals {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = self.panel;
        v.window_fill = self.panel;
        v.extreme_bg_color = if self.dark { Color32::from_rgb(24, 24, 24) } else { Color32::WHITE };
        v.selection.bg_fill = self.selection;
        v.hyperlink_color = self.accent;
        v
    }
}
