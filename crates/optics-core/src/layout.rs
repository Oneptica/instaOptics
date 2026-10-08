//! Meridional (y–z) cross-section geometry for the 2D layout view.

use serde::Serialize;

use crate::glass::is_air;
use crate::surface::{sag, surface_limit};
use crate::system::LensError;
use crate::trace::{FailureReason, TraceContext};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRay {
    pub field: usize,
    /// (z, y) points from the start of the ray to where it ends.
    pub points: Vec<[f64; 2]>,
    pub failure: Option<FailureReason>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// (z, y) profile of each surface across its clear aperture, bottom to top.
    pub surfaces: Vec<Vec<[f64; 2]>>,
    /// Closed (z, y) outlines of each glass element, flat flanges included.
    pub elements: Vec<Vec<[f64; 2]>>,
    pub rays: Vec<LayoutRay>,
    pub stop_z: f64,
    pub stop_semi_diameter: f64,
    pub start_z: f64,
    pub image_z: f64,
}

const STEPS: usize = 32;

/// Profile of surface `i` at vertex `z0`, out to `edge`, with the sag frozen beyond the clear aperture `h`.
fn profile(context: &TraceContext, i: usize, h: f64, edge: f64) -> Vec<[f64; 2]> {
    let surface = &context.system.surfaces[i];
    let clear = h.min(surface_limit(surface));
    (0..=2 * STEPS)
        .map(|k| {
            let y = edge * (k as f64 / STEPS as f64 - 1.0);
            [context.z[i] + sag(surface, y.abs().min(clear)), y]
        })
        .collect()
}

pub fn layout(context: &TraceContext, semi_diameters: &[f64], rays_per_field: usize) -> Result<Layout, LensError> {
    let system = context.system;
    let surfaces = (0..system.surfaces.len()).map(|i| profile(context, i, semi_diameters[i], semi_diameters[i])).collect();

    let mut elements = Vec::new();
    for i in 0..system.surfaces.len().saturating_sub(1) {
        if is_air(&system.surfaces[i].material) {
            continue;
        }
        let edge = semi_diameters[i].max(semi_diameters[i + 1]);
        let mut outline = profile(context, i, semi_diameters[i], edge);
        outline.extend(profile(context, i + 1, semi_diameters[i + 1], edge).into_iter().rev());
        elements.push(outline);
    }

    let wavelength = system.primary_wavelength();
    let n = system.medium_indices(wavelength)?;
    let count = rays_per_field.max(1);
    let mut rays = Vec::new();
    for (field_index, &field) in system.fields.iter().enumerate() {
        for k in 0..count {
            let py = if count == 1 { 0.0 } else { -1.0 + 2.0 * k as f64 / (count - 1) as f64 };
            let ray = context.trace_ray(field, 0.0, py, wavelength, &n);
            rays.push(LayoutRay {
                field: field_index,
                points: ray.points.iter().map(|p| [p[2], p[1]]).collect(),
                failure: ray.failure.map(|failure| failure.reason),
            });
        }
    }

    let stop = system.stop();
    Ok(Layout {
        surfaces,
        elements,
        rays,
        stop_z: context.z[stop],
        stop_semi_diameter: semi_diameters[stop],
        start_z: context.start_z,
        image_z: context.image_z,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ray3d {
    pub field: usize,
    pub points: Vec<[f64; 3]>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout3d {
    /// Closed (r, z) outlines of each glass element, revolved about the axis for display.
    pub elements: Vec<Vec<[f64; 2]>>,
    /// (r, z) profile of each air-to-air surface (e.g. a stop or dummy surface with a fixed aperture).
    pub surfaces: Vec<Vec<[f64; 2]>>,
    /// Rays through the pupil centre and a ring at its rim, for every field (primary wavelength).
    pub rays: Vec<Ray3d>,
    pub stop_z: f64,
    pub stop_semi_diameter: f64,
    pub start_z: f64,
    pub image_z: f64,
    pub image_semi_height: f64,
}

/// Geometry for the 3D view: element outlines in (r, z) and real rays in 3D.
pub fn layout_3d(context: &TraceContext, semi_diameters: &[f64], ring: usize) -> Result<Layout3d, LensError> {
    let system = context.system;
    let half = |points: Vec<[f64; 2]>| -> Vec<[f64; 2]> { points.into_iter().filter(|p| p[1] >= 0.0).map(|[z, y]| [y, z]).collect() };
    let mut elements = Vec::new();
    for i in 0..system.surfaces.len().saturating_sub(1) {
        if is_air(&system.surfaces[i].material) {
            continue;
        }
        let edge = semi_diameters[i].max(semi_diameters[i + 1]);
        let mut outline = half(profile(context, i, semi_diameters[i], edge));
        let mut back = half(profile(context, i + 1, semi_diameters[i + 1], edge));
        back.reverse();
        outline.extend(back);
        elements.push(outline);
    }
    let surfaces = (0..system.surfaces.len())
        .filter(|&i| is_air(&system.surfaces[i].material) && (i == 0 || is_air(&system.surfaces[i - 1].material)) && i != system.stop())
        .map(|i| half(profile(context, i, semi_diameters[i], semi_diameters[i])))
        .collect();
    let wavelength = system.primary_wavelength();
    let n = system.medium_indices(wavelength)?;
    let mut rays = Vec::new();
    let mut image_semi_height: f64 = 0.0;
    for (field_index, &field) in system.fields.iter().enumerate() {
        let mut pupil = vec![[0.0, 0.0]];
        pupil.extend((0..ring).map(|k| {
            let a = 2.0 * std::f64::consts::PI * k as f64 / ring as f64;
            [a.sin(), a.cos()]
        }));
        for [px, py] in pupil {
            let ray = context.trace_ray(field, px, py, wavelength, &n);
            if ray.ok() {
                image_semi_height = image_semi_height.max(ray.end()[0].hypot(ray.end()[1]));
            }
            rays.push(Ray3d { field: field_index, points: ray.points });
        }
    }
    let stop = system.stop();
    Ok(Layout3d {
        elements,
        surfaces,
        rays,
        stop_z: context.z[stop],
        stop_semi_diameter: semi_diameters[stop],
        start_z: context.start_z,
        image_z: context.image_z,
        image_semi_height,
    })
}
