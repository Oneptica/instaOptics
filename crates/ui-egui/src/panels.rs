//! The dockable windows, their menu groups and shortcuts.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Panel {
    SystemExplorer,
    LensData,
    SystemData,
    Configs,
    Layout,
    Layout3d,
    Spot,
    RayFan,
    Mtf,
    Psf,
    Wavefront,
    ThroughFocus,
    MtfVsField,
    Footprint,
    FieldCurves,
    Seidel,
    ChromaticFocalShift,
    Illumination,
    Coating,
    Polarization,
    Transmission,
    Beam,
    ImageSim,
    Optimize,
    Tolerance,
    Glasses,
}

/// Menu groups, in menu order.
pub const SETUP: &[Panel] = &[Panel::SystemExplorer, Panel::LensData, Panel::SystemData, Panel::Configs, Panel::Layout, Panel::Layout3d];
pub const IMAGE_QUALITY: &[Panel] = &[Panel::Spot, Panel::RayFan, Panel::Mtf, Panel::Psf, Panel::Wavefront, Panel::ThroughFocus, Panel::MtfVsField, Panel::Footprint];
pub const ABERRATIONS: &[Panel] = &[Panel::FieldCurves, Panel::Seidel, Panel::ChromaticFocalShift, Panel::Illumination];
pub const POLARIZATION: &[Panel] = &[Panel::Coating, Panel::Polarization, Panel::Transmission];
pub const PHYSICAL: &[Panel] = &[Panel::Beam, Panel::ImageSim];

impl Panel {
    pub const ALL: [Panel; 26] = [
        Panel::SystemExplorer,
        Panel::LensData,
        Panel::SystemData,
        Panel::Configs,
        Panel::Layout,
        Panel::Layout3d,
        Panel::Spot,
        Panel::RayFan,
        Panel::Mtf,
        Panel::Psf,
        Panel::Wavefront,
        Panel::ThroughFocus,
        Panel::MtfVsField,
        Panel::Footprint,
        Panel::FieldCurves,
        Panel::Seidel,
        Panel::ChromaticFocalShift,
        Panel::Illumination,
        Panel::Coating,
        Panel::Polarization,
        Panel::Transmission,
        Panel::Beam,
        Panel::ImageSim,
        Panel::Optimize,
        Panel::Tolerance,
        Panel::Glasses,
    ];

    pub fn id(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    pub fn from_id(id: &str) -> Option<Panel> {
        serde_json::from_value(serde_json::Value::String(id.to_string())).ok()
    }

    pub fn title(self) -> &'static str {
        crate::i18n::t(self.title_en())
    }

    /// The English title: the translation key and the export file name.
    pub fn title_en(self) -> &'static str {
        match self {
            Panel::SystemExplorer => "System Explorer",
            Panel::LensData => "Lens Data",
            Panel::SystemData => "System Data",
            Panel::Configs => "Multi-Configuration",
            Panel::Layout => "2D Layout",
            Panel::Layout3d => "3D Layout",
            Panel::Spot => "Spot Diagram",
            Panel::RayFan => "Ray Fan",
            Panel::Mtf => "FFT MTF",
            Panel::Psf => "FFT PSF",
            Panel::Wavefront => "Wavefront Map",
            Panel::ThroughFocus => "Through-Focus MTF",
            Panel::MtfVsField => "MTF vs Field",
            Panel::Footprint => "Footprint Diagram",
            Panel::FieldCurves => "Field Curvature / Distortion",
            Panel::Seidel => "Seidel Diagram",
            Panel::ChromaticFocalShift => "Chromatic Focal Shift",
            Panel::Illumination => "Relative Illumination",
            Panel::Coating => "Coating",
            Panel::Polarization => "Polarization Maps",
            Panel::Transmission => "Transmission vs Field",
            Panel::Beam => "Beam Propagation",
            Panel::ImageSim => "Image Simulation",
            Panel::Optimize => "Optimization",
            Panel::Tolerance => "Tolerancing",
            Panel::Glasses => "Glass Catalogs",
        }
    }

    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Panel::LensData => Some("Ctrl+L"),
            Panel::Layout => Some("Ctrl+Shift+L"),
            Panel::Spot => Some("Ctrl+Shift+P"),
            Panel::Mtf => Some("Ctrl+Shift+M"),
            Panel::Optimize => Some("Ctrl+F12"),
            _ => None,
        }
    }

    /// The analysis request kind of an analysis window.
    pub fn analysis_kind(self) -> Option<&'static str> {
        Some(match self {
            Panel::Spot => "spot",
            Panel::RayFan => "rayFan",
            Panel::Mtf => "mtf",
            Panel::Psf => "psf",
            Panel::Wavefront => "wavefront",
            Panel::ThroughFocus => "throughFocus",
            Panel::MtfVsField => "mtfVsField",
            Panel::Footprint => "footprint",
            Panel::FieldCurves => "fieldCurves",
            Panel::Seidel => "seidel",
            Panel::ChromaticFocalShift => "chromaticFocalShift",
            Panel::Illumination => "illumination",
            Panel::Coating => "coating",
            Panel::Polarization => "polarization",
            Panel::Transmission => "transmissionByField",
            _ => return None,
        })
    }

    /// File name stem for exports.
    pub fn file_stem(self) -> String {
        self.title_en().to_lowercase().replace([' ', '/'], "-").replace("--", "-")
    }
}
