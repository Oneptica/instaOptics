//! Paraxial (first-order) optics: y–u trace, focal lengths, pupils and image position.

use serde::Serialize;

use crate::system::{LensError, LensSystem};

/// Heights at each surface and the angle after it.
pub struct ParaxialRay {
    pub heights: Vec<f64>,
    pub angles: Vec<f64>,
}

/// Paraxial y–u trace starting just before surface 1.
pub fn trace(system: &LensSystem, n: &[f64], y0: f64, u0: f64) -> ParaxialRay {
    let mut heights = Vec::with_capacity(system.surfaces.len());
    let mut angles = Vec::with_capacity(system.surfaces.len());
    let (mut y, mut u) = (y0, u0);
    for (i, surface) in system.surfaces.iter().enumerate() {
        heights.push(y);
        u = (n[i] * u - y * surface.curvature() * (n[i + 1] - n[i])) / n[i + 1];
        angles.push(u);
        y += u * surface.thickness;
    }
    ParaxialRay { heights, angles }
}

/// Non-finite values (e.g. a telecentric exit pupil) serialize as null.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParaxialData {
    pub efl: f64,
    pub bfl: f64,
    pub f_number: f64,
    pub working_f_number: f64,
    /// Relative to surface 1.
    pub entrance_pupil_z: f64,
    /// Paraxial image distance from the last surface for the current object.
    pub image_distance: f64,
    /// Paraxial chief ray height at the image plane for the largest field.
    pub image_height: f64,
    pub total_track: f64,
    /// Global z of the paraxial exit pupil; infinite when image space is telecentric.
    pub exit_pupil_z: f64,
    /// Paraxial marginal ray height at the stop.
    pub stop_semi_diameter: f64,
}

pub fn paraxial_data(system: &LensSystem) -> Result<ParaxialData, LensError> {
    let n = system.medium_indices(system.primary_wavelength())?;
    let last = system.last();
    let last_thickness = system.surfaces[last].thickness;
    let stop = system.stop();

    let parallel = trace(system, &n, 1.0, 0.0);
    let tilted = trace(system, &n, 0.0, 1.0);
    let u_out = parallel.angles[last];
    let efl = -1.0 / u_out;
    let bfl = -parallel.heights[last] / u_out;
    // The chief ray crosses the stop centre; in object space it appears to come from the entrance pupil.
    let entrance_pupil_z = if parallel.heights[stop] == 0.0 { 0.0 } else { tilted.heights[stop] / parallel.heights[stop] };

    let semi_pupil = system.entrance_pupil_diameter / 2.0;
    let marginal = match system.object_distance {
        None => trace(system, &n, semi_pupil, 0.0),
        Some(distance) => {
            let u = semi_pupil / (distance + entrance_pupil_z);
            trace(system, &n, u * distance, u)
        }
    };
    let marginal_out = marginal.angles[last];
    let image_distance = -marginal.heights[last] / marginal_out;

    let slope = system.max_field().to_radians().tan();
    let chief = trace(system, &n, -entrance_pupil_z * slope, slope);
    let image_height = chief.heights[last] + chief.angles[last] * last_thickness;
    let unit_chief = trace(system, &n, -entrance_pupil_z, 1.0);
    let last_vertex_z = system.image_z() - last_thickness;
    let exit_pupil_z = if unit_chief.angles[last].abs() < 1e-12 {
        f64::NEG_INFINITY
    } else {
        last_vertex_z - unit_chief.heights[last] / unit_chief.angles[last]
    };

    Ok(ParaxialData {
        efl,
        bfl,
        f_number: efl.abs() / system.entrance_pupil_diameter,
        working_f_number: 1.0 / (2.0 * (n[last + 1] * marginal_out).abs()),
        entrance_pupil_z,
        image_distance,
        image_height,
        total_track: system.image_z(),
        exit_pupil_z,
        stop_semi_diameter: marginal.heights[stop].abs(),
    })
}
