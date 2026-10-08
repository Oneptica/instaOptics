//! Paraxial (first-order) optics: y–u trace, focal lengths, pupils and image position.

use serde::Serialize;

use crate::system::{ApertureType, FieldType, LensError, LensSystem};

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
    // n' u' is the reduced angle; with mirrors the image-space index is negative.
    let efl = -1.0 / (n[last + 1] * u_out);
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

/// Returns the system with the aperture expressed as an entrance pupil diameter and the fields as angles in
/// degrees, which is what the tracer works with. Paraxial optics is linear in the pupil size and in tan(field), so
/// each conversion is one rescaling of a trial value.
pub fn resolved(system: &LensSystem) -> Result<LensSystem, LensError> {
    if system.aperture_type == ApertureType::EntrancePupilDiameter && system.field_type == FieldType::Angle {
        return Ok(system.clone());
    }
    let mut out = system.clone();
    if !(out.entrance_pupil_diameter > 0.0 && out.entrance_pupil_diameter.is_finite()) {
        out.entrance_pupil_diameter = 10.0;
    }
    if out.aperture_type != ApertureType::EntrancePupilDiameter {
        let value = out.aperture_value.filter(|v| *v > 0.0 && v.is_finite())
            .ok_or_else(|| LensError("Set a positive aperture value".into()))?;
        let trial = paraxial_data(&out)?;
        let epd = out.entrance_pupil_diameter;
        out.entrance_pupil_diameter = match out.aperture_type {
            ApertureType::EntrancePupilDiameter => epd,
            ApertureType::ImageFNumber => trial.efl.abs() / value,
            ApertureType::WorkingFNumber => epd * trial.working_f_number / value,
            ApertureType::FloatByStop => epd * value / trial.stop_semi_diameter,
            ApertureType::ObjectNa => {
                let distance = out.object_distance.ok_or_else(|| LensError("Object NA needs a finite object distance".into()))?;
                if value >= 1.0 {
                    return Err(LensError("Object NA must be below 1".into()));
                }
                2.0 * (distance + trial.entrance_pupil_z) * value.asin().tan()
            }
        };
        if !(out.entrance_pupil_diameter > 0.0 && out.entrance_pupil_diameter.is_finite()) {
            return Err(LensError("The aperture cannot be reached with this system".into()));
        }
        out.aperture_type = ApertureType::EntrancePupilDiameter;
        out.aperture_value = None;
    }
    if out.field_type != FieldType::Angle {
        let entrance_pupil_z = paraxial_data(&out)?.entrance_pupil_z;
        let scale = match out.field_type {
            FieldType::Angle => 1.0,
            // tan θ = h / (object distance + pupil distance)
            FieldType::ObjectHeight => out.object_distance.ok_or_else(|| LensError("Object height fields need a finite object distance".into()))? + entrance_pupil_z,
            FieldType::ImageHeight => {
                let n = out.medium_indices(out.primary_wavelength())?;
                let last = out.last();
                let chief = trace(&out, &n, -entrance_pupil_z, 1.0);
                chief.heights[last] + chief.angles[last] * out.surfaces[last].thickness
            }
        };
        if scale == 0.0 || !scale.is_finite() {
            return Err(LensError("Cannot convert the fields to angles for this system".into()));
        }
        out.fields = out.fields.iter().map(|h| (h / scale).atan().to_degrees()).collect();
        out.field_type = FieldType::Angle;
    }
    Ok(out)
}
