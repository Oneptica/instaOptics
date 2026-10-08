//! instaOptics sequential optics core: lens model, glass catalog, paraxial optics and real ray tracing.

pub mod analysis;
pub mod glass;
pub mod image_sim;
pub mod layout;
pub mod optimize;
pub mod paraxial;
pub mod surface;
pub mod system;
pub mod tolerance;
pub mod trace;
pub mod vec3;

use serde::{Deserialize, Serialize};

pub use paraxial::{ParaxialData, paraxial_data};
pub use system::{LensError, LensSystem, Surface};
pub use trace::{RealRay, TraceContext};

/// Everything the editor shows for a system after an edit.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub paraxial: ParaxialData,
    /// Automatic clear semi-apertures, before any fixed values are applied.
    pub automatic_semi_diameters: Vec<f64>,
    pub semi_diameters: Vec<f64>,
    pub layout: layout::Layout,
}

pub fn overview(system: &LensSystem, rays_per_field: usize) -> Result<Overview, LensError> {
    system.validate()?;
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    let automatic_semi_diameters = trace::automatic_semi_diameters(&context)?;
    let semi_diameters = trace::effective_semi_diameters(system, &automatic_semi_diameters);
    let layout = layout::layout(&context, &semi_diameters, rays_per_field)?;
    Ok(Overview { paraxial, automatic_semi_diameters, semi_diameters, layout })
}

/// One analysis window's request. Field and wavelength are indices into the system's lists.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AnalysisRequest {
    Spot { rings: usize },
    RayFan { samples: usize },
    Seidel,
    FieldCurves { samples: usize },
    Wavefront { field: usize, wavelength: usize, size: usize },
    Mtf { size: usize, points: usize, max_frequency: Option<f64> },
    Illumination { samples: usize },
    Psf { field: usize, samples: usize, padding: usize, crop: usize },
    Layout3d { ring: usize },
}

fn to_value<T: Serialize>(value: T) -> serde_json::Value {
    serde_json::to_value(value).expect("analysis results serialize")
}

/// Runs one analysis and returns its result as JSON (non-finite numbers become null).
pub fn analyze(system: &LensSystem, request: &AnalysisRequest) -> Result<serde_json::Value, LensError> {
    system.validate()?;
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    let pick = |list: &[f64], index: usize, what: &str| list.get(index).copied().ok_or_else(|| LensError(format!("No {what} {}", index + 1)));
    Ok(match *request {
        AnalysisRequest::Spot { rings } => to_value(analysis::spot_diagram(&context, rings.clamp(1, 40))?),
        AnalysisRequest::RayFan { samples } => to_value(analysis::ray_fan(&context, samples.clamp(3, 501))?),
        AnalysisRequest::Seidel => to_value(analysis::seidel(system, &paraxial)?),
        AnalysisRequest::FieldCurves { samples } => to_value(analysis::field_curves(&context, samples.clamp(2, 201))?),
        AnalysisRequest::Wavefront { field, wavelength, size } => {
            let (field, wavelength) = (pick(&system.fields, field, "field")?, pick(&system.wavelengths, wavelength, "wavelength")?);
            to_value(analysis::wavefront(&context, &paraxial, field, wavelength, size.clamp(8, 512))?)
        }
        AnalysisRequest::Mtf { size, points, max_frequency } => {
            to_value(analysis::mtf(&context, &paraxial, size.clamp(16, 256).next_power_of_two(), points.clamp(2, 1001), max_frequency)?)
        }
        AnalysisRequest::Illumination { samples } => to_value(analysis::relative_illumination(&context, &paraxial, samples.clamp(2, 201), 14, 48)?),
        AnalysisRequest::Layout3d { ring } => {
            let automatic = trace::automatic_semi_diameters(&context)?;
            let semi = trace::effective_semi_diameters(system, &automatic);
            to_value(layout::layout_3d(&context, &semi, ring.clamp(4, 64))?)
        }
        AnalysisRequest::Psf { field, samples, padding, crop } => {
            let field = pick(&system.fields, field, "field")?;
            to_value(analysis::psf(&context, &paraxial, field, samples.clamp(16, 256).next_power_of_two(), padding.clamp(2, 8).next_power_of_two(), crop.max(16))?)
        }
    })
}
