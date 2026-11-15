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

/// One refraction or reflection, in global coordinates, for polarization and coating calculations.
#[derive(Clone, Copy, Debug)]
pub struct Bounce {
    pub surface: usize,
    pub k_in: Vec3,
    pub k_out: Vec3,
    /// Unit surface normal.
    pub normal: Vec3,
    /// Refractive indices before and after (positive).
    pub n_in: f64,
    pub n_out: f64,
    pub mirror: bool,
}

#[derive(Clone, Debug)]
pub struct RealRay {
    /// Start point, one point per surface reached, then the image plane point when the ray survives, in image
    /// coordinates: the image plane is z = image_z with its axes, so analyses work unchanged after folds. Without
    /// coordinate breaks these are global coordinates.
    pub points: Vec<Vec3>,
    /// The same points in global coordinates, for drawing.
    pub world: Vec<Vec3>,
    /// (x, y) of each surface hit in that surface's own coordinates.
    pub local: Vec<[f64; 2]>,
    pub failure: Option<Failure>,
    /// Final direction after the last surface reached.
    pub direction: Vec3,
    /// Optical path from the object (or an incident plane wavefront) to the last surface reached.
    pub opl: f64,
    /// Every refraction or reflection the ray went through.
    pub bounces: Vec<Bounce>,
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

/// A surface's coordinate system: global = origin + rotation · local (rotation columns are the local axes).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: Vec3,
    pub rotation: Matrix3,
}

impl Frame {
    pub const IDENTITY: Frame = Frame { origin: [0.0, 0.0, 0.0], rotation: IDENTITY };

    pub fn to_global(&self, p: Vec3) -> Vec3 {
        add(rotate(&self.rotation, p), self.origin)
    }

    pub fn to_local(&self, p: Vec3) -> Vec3 {
        rotate_back(&self.rotation, [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]])
    }

    pub fn dir_to_global(&self, d: Vec3) -> Vec3 {
        rotate(&self.rotation, d)
    }

    pub fn dir_to_local(&self, d: Vec3) -> Vec3 {
        rotate_back(&self.rotation, d)
    }
}

fn multiply(a: &Matrix3, b: &Matrix3) -> Matrix3 {
    let mut m = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    m
}

/// Vertex frame of every surface and the image plane frame. Coordinate breaks decenter, then rotate about x, y and z.
pub fn surface_frames(system: &LensSystem) -> (Vec<Frame>, Frame) {
    let mut frame = Frame::IDENTITY;
    let mut frames = Vec::with_capacity(system.surfaces.len());
    for surface in &system.surfaces {
        frames.push(frame);
        if let Some(cb) = surface.coordinate_break {
            frame.origin = frame.to_global([cb.decenter[0], cb.decenter[1], 0.0]);
            let [ax, ay, az] = cb.tilt.map(f64::to_radians);
            let (sx, cx) = ax.sin_cos();
            let (sy, cy) = ay.sin_cos();
            let (sz, cz) = az.sin_cos();
            let rx = [1.0, 0.0, 0.0, 0.0, cx, -sx, 0.0, sx, cx];
            let ry = [cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy];
            let rz = [cz, -sz, 0.0, sz, cz, 0.0, 0.0, 0.0, 1.0];
            frame.rotation = multiply(&multiply(&multiply(&frame.rotation, &rx), &ry), &rz);
        }
        frame.origin = frame.to_global([0.0, 0.0, surface.thickness]);
    }
    (frames, frame)
}

/// Per-system data shared by every ray: vertex positions, pupil, clipping and the ray-aiming cache.
pub struct TraceContext<'a> {
    pub system: &'a LensSystem,
    /// Unfolded vertex positions (cumulative thickness), as used by paraxial optics.
    pub z: Vec<f64>,
    pub frames: Vec<Frame>,
    pub image_frame: Frame,
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
        let z = system.surface_z();
        let span = z.iter().fold(image_z.abs(), |m, v| m.max(v.abs())).max(10.0);
        let ParaxialData { entrance_pupil_z, stop_semi_diameter, .. } = *paraxial;
        let (frames, image_frame) = surface_frames(system);
        TraceContext {
            system,
            z,
            frames,
            image_frame,
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
            frames: self.frames.clone(),
            image_frame: self.image_frame,
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

    /// Global → image coordinates: the image plane becomes z = image_z (identity without coordinate breaks).
    fn to_image(&self, p: Vec3) -> Vec3 {
        let q = self.image_frame.to_local(p);
        [q[0], q[1], q[2] + self.image_z]
    }

    /// Propagates through surfaces 0..=last; when `last` is before the final surface the ray stops there unrefracted.
    fn propagate(&self, start: Vec3, start_direction: Vec3, n: &[f64], last: usize, clip: bool) -> RealRay {
        let surfaces = &self.system.surfaces;
        let mut position = start;
        let mut direction = start_direction;
        let mut world = vec![position];
        let mut local_hits = Vec::new();
        let mut bounces: Vec<Bounce> = Vec::new();
        // For a collimated beam the optical path is measured from the plane wavefront through the origin.
        let mut opl = if self.system.object_distance.is_none() { dot(direction, position) } else { 0.0 };
        let finish = |world: Vec<Vec3>, local: Vec<[f64; 2]>, bounces: Vec<Bounce>, failure: Option<Failure>, direction: Vec3, opl: f64| RealRay {
            points: world.iter().map(|&p| self.to_image(p)).collect(),
            world,
            local,
            failure,
            direction: self.image_frame.dir_to_local(direction),
            opl,
            bounces,
        };
        let fail = |surface: usize, reason: FailureReason| Some(Failure { surface, reason });
        for i in 0..=last {
            let surface = &surfaces[i];
            let vertex = &self.frames[i];
            let mut local = vertex.to_local(position);
            let mut local_direction = vertex.dir_to_local(direction);
            if surface.coordinate_break.is_some() {
                // No interaction. Record where the ray line crosses the break's vertex plane (for drawing and
                // apertures) but keep the ray where it is: the next surface may lie before that plane, e.g. a
                // 45° fold mirror at the same vertex.
                if local_direction[2].abs() < 1e-15 {
                    return finish(world, local_hits, bounces, fail(i, FailureReason::Miss), direction, opl);
                }
                let t = -local[2] / local_direction[2];
                world.push(add(position, [t * direction[0], t * direction[1], t * direction[2]]));
                local_hits.push([local[0] + t * local_direction[0], local[1] + t * local_direction[1]]);
                if i == last && last < surfaces.len() - 1 {
                    return finish(world, local_hits, bounces, None, direction, opl);
                }
                continue;
            }
            let frame = surface_frame(surface);
            let (offset, rotation) = frame.unwrap_or(([0.0, 0.0], IDENTITY));
            if frame.is_some() {
                local = rotate(&rotation, [local[0] - offset[0], local[1] - offset[1], local[2]]);
                local_direction = rotate(&rotation, local_direction);
            }
            let Some(hit) = intersect(surface, local, local_direction) else {
                return finish(world, local_hits, bounces, fail(i, FailureReason::Miss), direction, opl);
            };
            if hit.t < -1e-9 && i > 0 {
                return finish(world, local_hits, bounces, fail(i, FailureReason::Backward), direction, opl);
            }
            let local_hit = [local[0] + hit.t * local_direction[0], local[1] + hit.t * local_direction[1], local[2] + hit.t * local_direction[2]];
            let vertex_hit = if frame.is_some() { add(rotate_back(&rotation, local_hit), [offset[0], offset[1], 0.0]) } else { local_hit };
            position = vertex.to_global(vertex_hit);
            opl += n[i].abs() * hit.t;
            world.push(position);
            local_hits.push([local_hit[0], local_hit[1]]);
            if clip {
                if let Some(limit) = self.clip[i] {
                    if local_hit[0].hypot(local_hit[1]) > limit + 1e-9 {
                        return finish(world, local_hits, bounces, fail(i, FailureReason::Clip), direction, opl);
                    }
                }
            }
            if i == last && last < surfaces.len() - 1 {
                return finish(world, local_hits, bounces, None, direction, opl);
            }

            let mut normal = hit.normal;
            let mut cos_i = dot(local_direction, normal);
            if cos_i < 0.0 {
                normal = [-normal[0], -normal[1], -normal[2]];
                cos_i = -cos_i;
            }
            let new_direction = if surface.is_mirror() {
                normalize([
                    local_direction[0] - 2.0 * cos_i * normal[0],
                    local_direction[1] - 2.0 * cos_i * normal[1],
                    local_direction[2] - 2.0 * cos_i * normal[2],
                ])
            } else {
                let mu = n[i].abs() / n[i + 1].abs();
                let k = 1.0 - mu * mu * (1.0 - cos_i * cos_i);
                if k < 0.0 {
                    return finish(world, local_hits, bounces, fail(i, FailureReason::Tir), direction, opl);
                }
                let g = k.sqrt() - mu * cos_i;
                normalize([
                    mu * local_direction[0] + g * normal[0],
                    mu * local_direction[1] + g * normal[1],
                    mu * local_direction[2] + g * normal[2],
                ])
            };
            let in_vertex = if frame.is_some() { rotate_back(&rotation, new_direction) } else { new_direction };
            let new_global = vertex.dir_to_global(in_vertex);
            let normal_vertex = if frame.is_some() { rotate_back(&rotation, normal) } else { normal };
            bounces.push(Bounce {
                surface: i,
                k_in: direction,
                k_out: new_global,
                normal: vertex.dir_to_global(normal_vertex),
                n_in: n[i].abs(),
                n_out: n[i + 1].abs(),
                mirror: surface.is_mirror(),
            });
            direction = new_global;
        }
        // Image plane: z = 0 in the image frame.
        let local = self.image_frame.to_local(position);
        let local_direction = self.image_frame.dir_to_local(direction);
        let t = if local_direction[2].abs() < 1e-15 { -1.0 } else { -local[2] / local_direction[2] };
        if t < -1e-9 {
            return finish(world, local_hits, bounces, fail(surfaces.len(), FailureReason::Backward), direction, opl);
        }
        world.push(add(position, [t * direction[0], t * direction[1], t * direction[2]]));
        finish(world, local_hits, bounces, None, direction, opl)
    }

    fn stop_hit(&self, field_angle: f64, ax: f64, ay: f64, n: &[f64]) -> Option<[f64; 2]> {
        let (position, direction) = self.launch(field_angle, ax, ay);
        let ray = self.propagate(position, direction, n, self.system.stop(), false);
        // Stop coordinates are measured in the stop surface's own frame.
        if ray.ok() { ray.local.last().copied() } else { None }
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
            for (i, hit) in ray.local.iter().take(system.surfaces.len()).enumerate() {
                heights[i] = heights[i].max(hit[0].hypot(hit[1]));
            }
        }
    }
    Ok(heights.into_iter().map(|h| ((h * 1.02 + 1e-4) * 1000.0).round() / 1000.0).map(|h| h.max(0.1)).collect())
}

pub fn effective_semi_diameters(system: &LensSystem, automatic: &[f64]) -> Vec<f64> {
    system.surfaces.iter().zip(automatic).map(|(surface, &auto)| surface.semi_diameter.unwrap_or(auto)).collect()
}
