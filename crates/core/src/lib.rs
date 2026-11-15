//! instaOptics sequential optics core: lens model, glass catalog, paraxial optics and real ray tracing.

pub mod analysis;
pub mod coating;
pub mod configs;
pub mod glass;
pub mod image_sim;
pub mod layout;
pub mod optimize;
pub mod paraxial;
pub mod polarization;
pub mod pop;
pub mod surface;
pub mod system;
pub mod tolerance;
pub mod trace;
pub mod vec3;

use serde::{Deserialize, Serialize};

pub use paraxial::{ParaxialData, paraxial_data, resolved};
pub use system::{LensError, LensSystem, Surface};
pub use trace::{RealRay, TraceContext};

/// Everything the editor shows for a system after an edit.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub paraxial: ParaxialData,
    /// Entrance pupil diameter and field angles (degrees) after converting the aperture and field types.
    pub entrance_pupil_diameter: f64,
    pub field_angles: Vec<f64>,
    /// Automatic clear semi-apertures, before any fixed values are applied.
    pub automatic_semi_diameters: Vec<f64>,
    pub semi_diameters: Vec<f64>,
    pub layout: layout::Layout,
    /// Surface values after the active configuration, solves and pickups.
    pub resolved_surfaces: Vec<ResolvedSurface>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSurface {
    pub radius: f64,
    pub thickness: f64,
    pub conic: f64,
    pub material: String,
}

pub fn overview(system: &LensSystem, rays_per_field: usize) -> Result<Overview, LensError> {
    system.validate()?;
    let system = &resolved(system)?;
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    let automatic_semi_diameters = trace::automatic_semi_diameters(&context)?;
    let semi_diameters = trace::effective_semi_diameters(system, &automatic_semi_diameters);
    let layout = layout::layout(&context, &semi_diameters, rays_per_field)?;
    let resolved_surfaces = system.surfaces.iter().map(|s| ResolvedSurface { radius: s.radius, thickness: s.thickness, conic: s.conic, material: s.material.clone() }).collect();
    Ok(Overview { resolved_surfaces, paraxial, entrance_pupil_diameter: system.entrance_pupil_diameter, field_angles: system.fields.clone(), automatic_semi_diameters, semi_diameters, layout })
}

/// One analysis window's request. Field and wavelength are indices into the system's lists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    ThroughFocus { frequency: f64, range: f64, steps: usize },
    MtfVsField { frequencies: Vec<f64>, samples: usize },
    ChromaticFocalShift { samples: usize },
    Footprint { surface: usize, rings: usize },
    /// Analytic Gaussian beam (complex q parameter).
    GaussianBeam { wavelength: usize, radius: f64, waist: f64 },
    /// Physical optics propagation of a beam through the system.
    Pop { settings: pop::PopSettings },
    /// Current merit and the value of every operand.
    MeritFunction,
    /// Reflectance and transmittance of one surface's coating.
    Coating { surface: usize, wavelength: usize, max_angle: f64, points: usize },
    /// Transmission, diattenuation and retardance over the pupil for one field.
    Polarization { field: usize, wavelength: usize, grid: usize, input: polarization::PolInput },
    /// Mean and minimum transmission for every field.
    TransmissionByField { wavelength: usize, grid: usize, input: polarization::PolInput },
}

/// The typed result of one analysis. Serializes as the bare result, so the JSON form is the same for every caller.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum AnalysisResult {
    Spot(Vec<analysis::SpotField>),
    RayFan(Vec<analysis::RayFanField>),
    Seidel(analysis::SeidelResult),
    FieldCurves(analysis::FieldCurves),
    Wavefront(analysis::WavefrontMap),
    Mtf(analysis::MtfResult),
    Illumination(analysis::Illumination),
    Psf(analysis::Psf),
    Layout3d(layout::Layout3d),
    ThroughFocus(analysis::ThroughFocus),
    MtfVsField(analysis::MtfVsField),
    ChromaticFocalShift(analysis::ChromaticFocalShift),
    Footprint(analysis::Footprint),
    GaussianBeam(pop::BeamTrace),
    Pop(pop::PopResult),
    MeritFunction(optimize::MeritReport),
    Coating(polarization::CoatingCurves),
    Polarization(polarization::PolarizationMap),
    TransmissionByField(Vec<polarization::FieldTransmission>),
}

/// Runs one analysis.
pub fn analyze(system: &LensSystem, request: &AnalysisRequest) -> Result<AnalysisResult, LensError> {
    use AnalysisResult as R;
    system.validate()?;
    let system = &resolved(system)?;
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    let pick = |list: &[f64], index: usize, what: &str| list.get(index).copied().ok_or_else(|| LensError(format!("No {what} {}", index + 1)));
    Ok(match request.clone() {
        AnalysisRequest::Spot { rings } => R::Spot(analysis::spot_diagram(&context, rings.clamp(1, 40))?),
        AnalysisRequest::RayFan { samples } => R::RayFan(analysis::ray_fan(&context, samples.clamp(3, 501))?),
        AnalysisRequest::Seidel => R::Seidel(analysis::seidel(system, &paraxial)?),
        AnalysisRequest::FieldCurves { samples } => R::FieldCurves(analysis::field_curves(&context, samples.clamp(2, 201))?),
        AnalysisRequest::Wavefront { field, wavelength, size } => {
            let (field, wavelength) = (pick(&system.fields, field, "field")?, pick(&system.wavelengths, wavelength, "wavelength")?);
            R::Wavefront(analysis::wavefront(&context, &paraxial, field, wavelength, size.clamp(8, 512))?)
        }
        AnalysisRequest::Mtf { size, points, max_frequency } => {
            R::Mtf(analysis::mtf(&context, &paraxial, size.clamp(16, 256).next_power_of_two(), points.clamp(2, 1001), max_frequency)?)
        }
        AnalysisRequest::Illumination { samples } => R::Illumination(analysis::relative_illumination(&context, &paraxial, samples.clamp(2, 201), 14, 48)?),
        AnalysisRequest::ThroughFocus { frequency, range, steps } => {
            if !(frequency > 0.0 && range > 0.0) {
                return Err(LensError("Frequency and focus range must be positive".into()));
            }
            R::ThroughFocus(analysis::through_focus_mtf(system, frequency, range, steps.clamp(3, 101), 64)?)
        }
        AnalysisRequest::MtfVsField { frequencies, samples } => R::MtfVsField(analysis::mtf_vs_field(&context, &paraxial, &frequencies, samples.clamp(2, 51), 64)?),
        AnalysisRequest::ChromaticFocalShift { samples } => R::ChromaticFocalShift(analysis::chromatic_focal_shift(system, samples.clamp(3, 401))?),
        AnalysisRequest::Footprint { surface, rings } => {
            let index = surface.min(system.last());
            let automatic = trace::automatic_semi_diameters(&context)?;
            let semi = trace::effective_semi_diameters(system, &automatic);
            R::Footprint(analysis::footprint(&context, index, rings.clamp(2, 20), semi[index])?)
        }
        AnalysisRequest::GaussianBeam { wavelength, radius, waist } => R::GaussianBeam(pop::gaussian_beam(system, wavelength, radius, waist, 60)?),
        AnalysisRequest::MeritFunction => R::MeritFunction(optimize::merit_report(system)),
        AnalysisRequest::Coating { surface, wavelength, max_angle, points } => {
            let lambda = pick(&system.wavelengths, wavelength, "wavelength")?;
            R::Coating(polarization::coating_curves(system, surface, lambda, max_angle.clamp(1.0, 89.0), points.clamp(2, 400))?)
        }
        AnalysisRequest::Polarization { field, wavelength, grid, input } => {
            let lambda = pick(&system.wavelengths, wavelength, "wavelength")?;
            let angle = pick(&system.fields, field, "field")?;
            R::Polarization(polarization::polarization_map(&context, angle, lambda, grid.clamp(5, 129), input)?)
        }
        AnalysisRequest::TransmissionByField { wavelength, grid, input } => {
            let lambda = pick(&system.wavelengths, wavelength, "wavelength")?;
            R::TransmissionByField(polarization::transmission_by_field(&context, lambda, grid.clamp(5, 65), input)?)
        }
        AnalysisRequest::Pop { settings } => R::Pop(pop::simulate(system, &settings)?),
        AnalysisRequest::Layout3d { ring } => {
            let automatic = trace::automatic_semi_diameters(&context)?;
            let semi = trace::effective_semi_diameters(system, &automatic);
            R::Layout3d(layout::layout_3d(&context, &semi, ring.clamp(4, 64))?)
        }
        AnalysisRequest::Psf { field, samples, padding, crop } => {
            let field = pick(&system.fields, field, "field")?;
            R::Psf(analysis::psf(&context, &paraxial, field, samples.clamp(16, 256).next_power_of_two(), padding.clamp(2, 8).next_power_of_two(), crop.max(16))?)
        }
    })
}
