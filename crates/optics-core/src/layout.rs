//! Meridional (y–z) cross-section geometry for the 2D layout view.

use serde::Serialize;

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
    /// Indices into `surfaces` of reflecting surfaces.
    pub mirrors: Vec<usize>,
    pub rays: Vec<LayoutRay>,
    pub stop_z: f64,
    pub stop_semi_diameter: f64,
    pub start_z: f64,
    pub image_z: f64,
}

const STEPS: usize = 32;

/// Ray points to draw: coordinate-break crossings are bookkeeping, not real vertices of the path.
fn drawn(system: &crate::system::LensSystem, world: &[[f64; 3]]) -> Vec<[f64; 3]> {
    world.iter().enumerate()
        .filter(|&(k, _)| k == 0 || system.surfaces.get(k - 1).is_none_or(|s| s.coordinate_break.is_none()))
        .map(|(_, p)| *p)
        .collect()
}

/// Profile of surface `i` at vertex `z0`, out to `edge`, with the sag frozen beyond the clear aperture `h`.
fn profile(context: &TraceContext, i: usize, h: f64, edge: f64) -> Vec<[f64; 2]> {
    let surface = &context.system.surfaces[i];
    let clear = h.min(surface_limit(surface));
    let frame = &context.frames[i];
    (0..=2 * STEPS)
        .map(|k| {
            let y = edge * (k as f64 / STEPS as f64 - 1.0);
            let sag = if surface.coordinate_break.is_some() { 0.0 } else { sag(surface, y.abs().min(clear)) };
            let g = frame.to_global([0.0, y, sag]);
            [g[2], g[1]]
        })
        .collect()
}

pub fn layout(context: &TraceContext, semi_diameters: &[f64], rays_per_field: usize) -> Result<Layout, LensError> {
    let system = context.system;
    // Coordinate breaks are not drawn.
    let surfaces = (0..system.surfaces.len())
        .map(|i| if system.surfaces[i].coordinate_break.is_some() { Vec::new() } else { profile(context, i, semi_diameters[i], semi_diameters[i]) })
        .collect();

    let mut elements = Vec::new();
    for i in 0..system.surfaces.len().saturating_sub(1) {
        if !system.surfaces[i].is_glass() {
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
                points: drawn(system, &ray.world).iter().map(|p| [p[2], p[1]]).collect(),
                failure: ray.failure.map(|failure| failure.reason),
            });
        }
    }

    let stop = system.stop();
    Ok(Layout {
        surfaces,
        elements,
        mirrors: (0..system.surfaces.len()).filter(|&i| system.surfaces[i].is_mirror()).collect(),
        rays,
        stop_z: context.frames[stop].origin[2],
        stop_semi_diameter: semi_diameters[stop],
        start_z: context.start_z,
        image_z: context.image_frame.origin[2],
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ray3d {
    pub field: usize,
    pub points: Vec<[f64; 3]>,
}

/// A body of revolution: the (r, z) outline in its own frame, and the frame in global coordinates
/// (global = origin + rotation · local, rotation row-major).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Body3d {
    pub outline: Vec<[f64; 2]>,
    pub origin: [f64; 3],
    pub rotation: [f64; 9],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame3d {
    pub origin: [f64; 3],
    pub rotation: [f64; 9],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout3d {
    /// Glass elements as bodies of revolution about their own axes.
    pub elements: Vec<Body3d>,
    /// Profiles of surfaces that are not glass interfaces (e.g. mirrors and dummy surfaces).
    pub surfaces: Vec<Body3d>,
    /// Rays through the pupil centre and a ring at its rim, for every field (primary wavelength), in global coordinates.
    pub rays: Vec<Ray3d>,
    pub stop: Frame3d,
    pub stop_semi_diameter: f64,
    pub image: Frame3d,
    pub start_z: f64,
    pub image_z: f64,
    pub image_semi_height: f64,
}

fn frame3d(frame: &crate::trace::Frame) -> Frame3d {
    Frame3d { origin: frame.origin, rotation: frame.rotation }
}

/// Half profile (y ≥ 0) of surface `i` in the coordinates of surface `reference`: (r, z) pairs.
fn half_profile(context: &TraceContext, i: usize, reference: usize, h: f64, edge: f64) -> Vec<[f64; 2]> {
    let surface = &context.system.surfaces[i];
    let clear = h.min(surface_limit(surface));
    let (frame, base) = (&context.frames[i], &context.frames[reference]);
    (0..=STEPS)
        .map(|k| {
            let y = edge * k as f64 / STEPS as f64;
            let sag = if surface.coordinate_break.is_some() { 0.0 } else { sag(surface, y.min(clear)) };
            let local = base.to_local(frame.to_global([0.0, y, sag]));
            [local[1], local[2]]
        })
        .collect()
}

/// Geometry for the 3D view: bodies of revolution with their frames, and real rays in 3D.
pub fn layout_3d(context: &TraceContext, semi_diameters: &[f64], ring: usize) -> Result<Layout3d, LensError> {
    let system = context.system;
    let mut elements = Vec::new();
    for i in 0..system.surfaces.len().saturating_sub(1) {
        if !system.surfaces[i].is_glass() {
            continue;
        }
        let edge = semi_diameters[i].max(semi_diameters[i + 1]);
        let mut outline = half_profile(context, i, i, semi_diameters[i], edge);
        let mut back = half_profile(context, i + 1, i, semi_diameters[i + 1], edge);
        back.reverse();
        outline.extend(back);
        let frame = &context.frames[i];
        elements.push(Body3d { outline, origin: frame.origin, rotation: frame.rotation });
    }
    let surfaces = (0..system.surfaces.len())
        .filter(|&i| system.surfaces[i].coordinate_break.is_none() && !system.surfaces[i].is_glass() && (i == 0 || !system.surfaces[i - 1].is_glass()) && i != system.stop())
        .map(|i| Body3d { outline: half_profile(context, i, i, semi_diameters[i], semi_diameters[i]), origin: context.frames[i].origin, rotation: context.frames[i].rotation })
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
            rays.push(Ray3d { field: field_index, points: drawn(system, &ray.world) });
        }
    }
    let stop = system.stop();
    Ok(Layout3d {
        elements,
        surfaces,
        rays,
        stop: frame3d(&context.frames[stop]),
        stop_semi_diameter: semi_diameters[stop],
        image: frame3d(&context.image_frame),
        start_z: context.start_z,
        image_z: context.image_frame.origin[2],
        image_semi_height,
    })
}
