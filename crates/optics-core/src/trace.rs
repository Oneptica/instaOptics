//! Real ray tracing with optional ray aiming at the stop.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::paraxial::ParaxialData;
use crate::surface::intersect;
use crate::system::{LensError, LensSystem, Surface};
use crate::vec3::{Matrix3, Vec3, add, dot, normalize, rotate, rotate_back};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureReason {
    Miss,
    Tir,
    Clip,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub surface: usize,
    pub reason: FailureReason,
}

#[derive(Clone, Debug)]
pub struct RealRay {
    /// Start point, one point per surface reached, then the image plane point when the ray survives.
    pub points: Vec<Vec3>,
    pub failure: Option<Failure>,
    /// Final direction after the last surface reached.
    pub direction: Vec3,
    /// Optical path from the object (or an incident plane wavefront) to the last surface reached.
    pub opl: f64,
}

impl RealRay {
    pub fn ok(&self) -> bool {
        self.failure.is_none()
    }

    pub fn end(&self) -> Vec3 {
        *self.points.last().expect("a ray always has a start point")
    }
}

#[derive(Clone, Copy)]
struct AimModel {
    base: [f64; 2],
    inverse: [f64; 4],
}

/// Per-system data shared by every ray: vertex positions, pupil, clipping and the ray-aiming cache.
pub struct TraceContext<'a> {
    pub system: &'a LensSystem,
    pub z: Vec<f64>,
    pub image_z: f64,
    pub entrance_pupil_z: f64,
    pub stop_semi_diameter: f64,
    pub start_z: f64,
    pub clip: Vec<Option<f64>>,
    pub aiming: bool,
    aim_cache: Mutex<HashMap<(u64, u64), Option<AimModel>>>,
}

impl<'a> TraceContext<'a> {
    pub fn new(system: &'a LensSystem, paraxial: &ParaxialData) -> Self {
        let image_z = system.image_z();
        let span = image_z.max(10.0);
        let ParaxialData { entrance_pupil_z, stop_semi_diameter, .. } = *paraxial;
        TraceContext {
            system,
            z: system.surface_z(),
            image_z,
            entrance_pupil_z,
            stop_semi_diameter,
            start_z: match system.object_distance {
                None => entrance_pupil_z.min(0.0) - 0.18 * span,
                Some(distance) => -distance,
            },
            clip: system.surfaces.iter().map(|surface| surface.semi_diameter).collect(),
            aiming: system.ray_aiming && stop_semi_diameter > 0.0 && stop_semi_diameter.is_finite(),
            aim_cache: Mutex::new(HashMap::new()),
        }
    }

    /// The same context without clipping apertures.
    pub fn unclipped(&self) -> Self {
        TraceContext {
            system: self.system,
            z: self.z.clone(),
            image_z: self.image_z,
            entrance_pupil_z: self.entrance_pupil_z,
            stop_semi_diameter: self.stop_semi_diameter,
            start_z: self.start_z,
            clip: vec![None; self.clip.len()],
            aiming: self.aiming,
            aim_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Object-space ray for a field angle that passes the entrance pupil plane at (ax, ay).
    fn launch(&self, field_angle: f64, ax: f64, ay: f64) -> (Vec3, Vec3) {
        let theta = field_angle.to_radians();
        let pupil = [ax, ay, self.entrance_pupil_z];
        match self.system.object_distance {
            None => {
                let direction = [0.0, theta.sin(), theta.cos()];
                let t = (self.start_z - pupil[2]) / direction[2];
                ([pupil[0], pupil[1] + t * direction[1], self.start_z], direction)
            }
            Some(distance) => {
                let position = [0.0, -(distance + self.entrance_pupil_z) * theta.tan(), -distance];
                (position, normalize([pupil[0] - position[0], pupil[1] - position[1], pupil[2] - position[2]]))
            }
        }
    }

    /// Propagates through surfaces 0..=last; when `last` is before the final surface the ray stops there unrefracted.
    fn propagate(&self, start: Vec3, start_direction: Vec3, n: &[f64], last: usize, clip: bool) -> RealRay {
        let surfaces = &self.system.surfaces;
        let mut position = start;
        let mut direction = start_direction;
        let mut points = vec![position];
        // For a collimated beam the optical path is measured from the plane wavefront through the origin.
        let mut opl = if self.system.object_distance.is_none() { dot(direction, position) } else { 0.0 };
        let fail = |points: Vec<Vec3>, surface: usize, reason: FailureReason, direction: Vec3, opl: f64| RealRay {
            points,
            failure: Some(Failure { surface, reason }),
            direction,
            opl,
        };
        for i in 0..=last {
            let surface = &surfaces[i];
            let frame = surface_frame(surface);
            let (offset, rotation) = frame.unwrap_or(([0.0, 0.0], IDENTITY));
            let mut local = [position[0] - offset[0], position[1] - offset[1], position[2] - self.z[i]];
            let mut local_direction = direction;
            if frame.is_some() {
                local = rotate(&rotation, local);
                local_direction = rotate(&rotation, direction);
            }
            let Some(hit) = intersect(surface, local, local_direction) else {
                return fail(points, i, FailureReason::Miss, direction, opl);
            };
            if hit.t < -1e-9 && i > 0 {
                return fail(points, i, FailureReason::Backward, direction, opl);
            }
            let local_hit = [local[0] + hit.t * local_direction[0], local[1] + hit.t * local_direction[1], local[2] + hit.t * local_direction[2]];
            position = if frame.is_some() {
                add(rotate_back(&rotation, local_hit), [offset[0], offset[1], self.z[i]])
            } else {
                [local_hit[0], local_hit[1], local_hit[2] + self.z[i]]
            };
            opl += n[i] * hit.t;
            points.push(position);
            if clip {
                if let Some(limit) = self.clip[i] {
                    if local_hit[0].hypot(local_hit[1]) > limit + 1e-9 {
                        return fail(points, i, FailureReason::Clip, direction, opl);
                    }
                }
            }
            if i == last && last < surfaces.len() - 1 {
                return RealRay { points, failure: None, direction, opl };
            }

            let mut normal = hit.normal;
            let mut cos_i = dot(local_direction, normal);
            if cos_i < 0.0 {
                normal = [-normal[0], -normal[1], -normal[2]];
                cos_i = -cos_i;
            }
            let mu = n[i] / n[i + 1];
            let k = 1.0 - mu * mu * (1.0 - cos_i * cos_i);
            if k < 0.0 {
                return fail(points, i, FailureReason::Tir, direction, opl);
            }
            let g = k.sqrt() - mu * cos_i;
            let refracted = normalize([
                mu * local_direction[0] + g * normal[0],
                mu * local_direction[1] + g * normal[1],
                mu * local_direction[2] + g * normal[2],
            ]);
            direction = if frame.is_some() { rotate_back(&rotation, refracted) } else { refracted };
        }
        if direction[2] <= 0.0 {
            return fail(points, surfaces.len(), FailureReason::Backward, direction, opl);
        }
        let t = (self.image_z - position[2]) / direction[2];
        points.push([position[0] + t * direction[0], position[1] + t * direction[1], self.image_z]);
        RealRay { points, failure: None, direction, opl }
    }

    fn stop_hit(&self, field_angle: f64, ax: f64, ay: f64, n: &[f64]) -> Option<[f64; 2]> {
        let (position, direction) = self.launch(field_angle, ax, ay);
        let ray = self.propagate(position, direction, n, self.system.stop(), false);
        let hit = ray.end();
        ray.ok().then_some([hit[0], hit[1]])
    }

    /// Linearizes stop coordinates around the real chief ray with finite-difference Newton steps.
    fn aim_model(&self, field_angle: f64, n: &[f64]) -> Option<AimModel> {
        let h = self.system.entrance_pupil_diameter.max(1e-6) * 1e-5;
        let mut base = [0.0, 0.0];
        for _ in 0..20 {
            let centre = self.stop_hit(field_angle, base[0], base[1], n)?;
            let dx = self.stop_hit(field_angle, base[0] + h, base[1], n)?;
            let dy = self.stop_hit(field_angle, base[0], base[1] + h, n)?;
            let j = [(dx[0] - centre[0]) / h, (dy[0] - centre[0]) / h, (dx[1] - centre[1]) / h, (dy[1] - centre[1]) / h];
            let det = j[0] * j[3] - j[1] * j[2];
            if det.abs() < 1e-15 {
                return None;
            }
            let inverse = [j[3] / det, -j[1] / det, -j[2] / det, j[0] / det];
            if centre[0].hypot(centre[1]) < 1e-12 * self.stop_semi_diameter.max(1.0) {
                return Some(AimModel { base, inverse });
            }
            base = [
                base[0] - (inverse[0] * centre[0] + inverse[1] * centre[1]),
                base[1] - (inverse[2] * centre[0] + inverse[3] * centre[1]),
            ];
        }
        None
    }

    /// Entrance pupil plane coordinates whose real ray lands on (tx, ty) at the stop.
    fn aim_at_stop(&self, field_angle: f64, wavelength: f64, tx: f64, ty: f64, n: &[f64]) -> Option<[f64; 2]> {
        let key = (field_angle.to_bits(), wavelength.to_bits());
        let cached = self.aim_cache.lock().unwrap().get(&key).copied();
        let model = match cached {
            Some(model) => model,
            None => {
                let model = self.aim_model(field_angle, n);
                self.aim_cache.lock().unwrap().insert(key, model);
                model
            }
        }?;
        let AimModel { base, inverse } = model;
        let mut a = [base[0] + inverse[0] * tx + inverse[1] * ty, base[1] + inverse[2] * tx + inverse[3] * ty];
        for _ in 0..25 {
            let hit = self.stop_hit(field_angle, a[0], a[1], n)?;
            let (ex, ey) = (hit[0] - tx, hit[1] - ty);
            if ex.hypot(ey) < 1e-10 * self.stop_semi_diameter.max(1.0) {
                return Some(a);
            }
            a = [a[0] - (inverse[0] * ex + inverse[1] * ey), a[1] - (inverse[2] * ex + inverse[3] * ey)];
        }
        Some(a)
    }

    /// Traces one real ray for a field angle (degrees) and normalized pupil coordinates.
    /// Without ray aiming the ray is aimed paraxially at the entrance pupil; with it, (px, py) are real stop coordinates.
    pub fn trace_ray(&self, field_angle: f64, px: f64, py: f64, wavelength: f64, n: &[f64]) -> RealRay {
        let semi_pupil = self.system.entrance_pupil_diameter / 2.0;
        let mut aim = [px * semi_pupil, py * semi_pupil];
        if self.aiming {
            let stop = self.stop_semi_diameter;
            if let Some(aimed) = self.aim_at_stop(field_angle, wavelength, px * stop, py * stop, n) {
                aim = aimed;
            }
        }
        let (position, direction) = self.launch(field_angle, aim[0], aim[1]);
        self.propagate(position, direction, n, self.system.last(), true)
    }
}

const IDENTITY: Matrix3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Rotation into a decentered / tilted surface's frame, or None for a centred surface.
fn surface_frame(surface: &Surface) -> Option<([f64; 2], Matrix3)> {
    let [dx, dy] = surface.decenter.unwrap_or([0.0, 0.0]);
    let [ax, ay] = surface.tilt.unwrap_or([0.0, 0.0]).map(f64::to_radians);
    if dx == 0.0 && dy == 0.0 && ax == 0.0 && ay == 0.0 {
        return None;
    }
    let (sx, cx) = ax.sin_cos();
    let (sy, cy) = ay.sin_cos();
    // World → local: undo the surface rotation Ry(ay)·Rx(ax), i.e. apply Rx(−ax)·Ry(−ay).
    Some(([dx, dy], [cy, 0.0, -sy, sx * sy, cx, sx * cy, cx * sy, -sx, cx * cy]))
}

/// Automatic clear semi-apertures: the largest marginal ray height on each surface over all fields at the primary wavelength.
pub fn automatic_semi_diameters(context: &TraceContext) -> Result<Vec<f64>, LensError> {
    let system = context.system;
    let wavelength = system.primary_wavelength();
    let n = system.medium_indices(wavelength)?;
    let open = context.unclipped();
    let mut heights = vec![0.0_f64; system.surfaces.len()];
    for &field in &system.fields {
        for (px, py) in [(0.0, 1.0), (0.0, -1.0), (1.0, 0.0)] {
            let ray = open.trace_ray(field, px, py, wavelength, &n);
            for (i, point) in ray.points.iter().skip(1).take(system.surfaces.len()).enumerate() {
                heights[i] = heights[i].max(point[0].hypot(point[1]));
            }
        }
    }
    Ok(heights.into_iter().map(|h| ((h * 1.02 + 1e-4) * 1000.0).round() / 1000.0).map(|h| h.max(0.1)).collect())
}

pub fn effective_semi_diameters(system: &LensSystem, automatic: &[f64]) -> Vec<f64> {
    system.surfaces.iter().zip(automatic).map(|(surface, &auto)| surface.semi_diameter.unwrap_or(auto)).collect()
}
