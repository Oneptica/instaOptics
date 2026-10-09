//! Physical optics propagation (POP): a scalar complex field is propagated through the sequential system.
//!
//! The field is stored as a smooth residual A on a square grid together with a reference sphere (curvature 1/R):
//! U = A · exp(i k n r² / 2R). Free-space steps use Collins' scaling of the angular-spectrum method, so the pixel pitch
//! follows the beam as it focuses or expands. Surfaces change the reference curvature paraxially and clip with their
//! fixed apertures. Aberrations come from real ray tracing: the wavefront error of the system relative to the paraxial
//! focus sphere is applied as a phase screen on the last surface. Everything is scalar and paraxial in the propagation,
//! so accuracy drops for beams faster than about NA 0.1.

use std::f64::consts::PI;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{OpdEvaluator, fft2};
use crate::paraxial::{ParaxialData, paraxial_data, resolved};
use crate::system::{LensError, LensSystem};
use crate::trace::TraceContext;

// ---------- settings and results ----------

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Source {
    /// Fundamental Gaussian. `radius` is the 1/e² intensity radius at the waist; `waist` is the distance from the first
    /// surface to the waist (mm, positive when the waist lies beyond the surface).
    Gaussian { radius: f64, waist: f64 },
    /// Intensity exp(−2 (r/radius)^(2·order)); order 1 is a Gaussian and large orders approach a flat top.
    SuperGaussian { radius: f64, order: f64, curvature_radius: Option<f64> },
    /// Uniform circular beam. `curvature_radius` is positive for a diverging beam; None is collimated.
    TopHat { radius: f64, curvature_radius: Option<f64> },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopSettings {
    pub source: Source,
    /// Index into the system's wavelengths.
    #[serde(default)]
    pub wavelength: usize,
    #[serde(default = "default_samples")]
    pub samples: usize,
    /// Apply the real-ray wavefront aberration of the system.
    #[serde(default = "default_true")]
    pub aberrations: bool,
    /// Clip with surfaces that have a fixed semi-diameter.
    #[serde(default = "default_true")]
    pub apertures: bool,
    /// Field index: the beam travels along that field's chief ray. None is on axis.
    #[serde(default)]
    pub field: Option<usize>,
    /// Mode field radius (1/e² intensity) of a single-mode fibre; enables the coupling efficiency.
    #[serde(default)]
    pub fiber_radius: Option<f64>,
}

fn default_samples() -> usize {
    256
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    /// Distance along the optical path from the first surface, mm.
    pub z: f64,
    /// Pixel pitch of the cuts, mm.
    pub pitch: f64,
    /// Second-moment (D4σ / 2) radii, mm.
    pub w_x: f64,
    pub w_y: f64,
    pub power: f64,
    /// Peak irradiance, power per mm².
    pub peak: f64,
    /// Central row and column of the irradiance, normalized to the slice peak.
    pub x_cut: Vec<f32>,
    pub y_cut: Vec<f32>,
    /// Set when the slice is the plane just after a surface.
    pub surface: Option<usize>,
    /// Position of the beam centre (the chief ray) in the field's y direction, mm.
    pub center: f64,
    /// Power coupling into the fibre mode centred on the optical axis, when a fibre radius was given.
    pub coupling: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaneMap {
    /// Surface index, or the number of surfaces for the image plane.
    pub surface: usize,
    pub z: f64,
    pub half_width: f64,
    pub size: usize,
    /// Irradiance normalized to the plane peak, row 0 at +y.
    pub intensity: Vec<f32>,
    /// Phase of the residual field relative to its best-fit sphere, radians; null where there is no light.
    pub phase: Vec<f32>,
    pub w_x: f64,
    pub w_y: f64,
    pub power: f64,
    pub peak: f64,
    pub center: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingSummary {
    /// Coupling at the image plane.
    pub at_image: f64,
    /// Best coupling found in image space and where it occurs (path position).
    pub best: f64,
    pub best_z: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AberrationInfo {
    pub rms_waves: f64,
    pub pv_waves: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopResult {
    pub wavelength: f64,
    pub samples: usize,
    pub slices: Vec<Slice>,
    pub planes: Vec<PlaneMap>,
    /// Path position of each surface vertex, and of the image plane.
    pub surface_z: Vec<f64>,
    pub image_z: f64,
    pub z_start: f64,
    /// Analytic Gaussian beam radius (z, w) for Gaussian sources, to compare with the simulated radius.
    pub analytic: Option<Vec<[f64; 2]>>,
    pub aberration: Option<AberrationInfo>,
    pub coupling: Option<CouplingSummary>,
    /// Field angle the beam follows, degrees.
    pub field_angle: f64,
    pub warnings: Vec<String>,
}

// ---------- analytic Gaussian beam (complex q parameter) ----------

#[derive(Clone, Copy, Debug)]
struct Cx(f64, f64);

impl Cx {
    fn add(self, o: Cx) -> Cx {
        Cx(self.0 + o.0, self.1 + o.1)
    }
    fn inv(self) -> Cx {
        let d = self.0 * self.0 + self.1 * self.1;
        Cx(self.0 / d, -self.1 / d)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceBeam {
    pub surface: usize,
    pub z: f64,
    /// Beam radius at the vertex plane (after the surface), mm.
    pub w: f64,
    /// Radius of curvature of the wavefront, mm; null for a plane wave front.
    pub radius_of_curvature: Option<f64>,
    /// Waist after this surface, if it lies in the following gap: distance from the surface and its radius.
    pub waist_distance: Option<f64>,
    pub waist_radius: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeamTrace {
    pub wavelength: f64,
    pub z_start: f64,
    pub profile: Vec<[f64; 2]>,
    pub surfaces: Vec<SurfaceBeam>,
    pub image_z: f64,
    pub surface_z: Vec<f64>,
    /// Waist radius and divergence half-angle of the beam in image space.
    pub image_waist: Option<f64>,
    pub image_waist_distance: Option<f64>,
    pub divergence: Option<f64>,
}

fn path_positions(system: &LensSystem) -> (Vec<f64>, f64) {
    let mut z = 0.0;
    let mut out = Vec::with_capacity(system.surfaces.len());
    for surface in &system.surfaces {
        out.push(z);
        z += surface.thickness.abs();
    }
    (out, z)
}

pub fn check_supported(system: &LensSystem) -> Result<(), LensError> {
    for (i, surface) in system.surfaces.iter().enumerate() {
        if surface.decenter.is_some() || surface.tilt.is_some() {
            return Err(LensError(format!("Physical optics does not support surface decenter or tilt (surface {})", i + 1)));
        }
        if let Some(cb) = surface.coordinate_break {
            if cb.decenter != [0.0, 0.0] {
                return Err(LensError(format!("Physical optics does not support decentered coordinate breaks (surface {})", i + 1)));
            }
        }
    }
    Ok(())
}

/// Coordinate breaks only rotate the frame the beam travels in; the unfolded path is what is simulated.
fn has_tilted_break(system: &LensSystem) -> bool {
    system.surfaces.iter().any(|s| s.coordinate_break.is_some_and(|cb| cb.tilt != [0.0, 0.0, 0.0]))
}

/// Gaussian beam through the system with the complex q parameter (paraxial, no aberrations or apertures).
pub fn gaussian_beam(system: &LensSystem, wavelength: usize, radius: f64, waist: f64, per_gap: usize) -> Result<BeamTrace, LensError> {
    check_supported(system)?;
    if !(radius > 0.0 && radius.is_finite()) {
        return Err(LensError("The beam radius must be positive".into()));
    }
    let lambda = *system.wavelengths.get(wavelength).ok_or_else(|| LensError(format!("No wavelength {}", wavelength + 1)))?;
    let l0 = lambda * 1e-3;
    let n = system.medium_indices(lambda)?;
    let (surface_z, image_z) = path_positions(system);
    let zr0 = PI * radius * radius / l0;
    // q at the first surface: distance past the waist is −waist.
    let mut q = Cx(-waist, zr0);
    let width = |q: Cx, index: f64| (-l0 / (PI * index * q.inv().1)).max(0.0).sqrt();
    let mut profile = Vec::new();
    let z_start = if waist < 0.0 { waist } else { 0.0 };
    if waist < 0.0 {
        // Sample from the waist to the first surface.
        let q_at = |z: f64| Cx(z - waist, zr0);
        for k in 0..=per_gap {
            let z = waist * (1.0 - k as f64 / per_gap as f64);
            profile.push([z, width(q_at(z), 1.0)]);
        }
    }
    let mut surfaces = Vec::new();
    let mut image_waist = None;
    let mut image_waist_distance = None;
    let mut divergence = None;
    for (i, surface) in system.surfaces.iter().enumerate() {
        let (before, after) = (n[i], n[i + 1]);
        let s = before.signum();
        let c = surface.curvature();
        // Surface: n'/q' = n/q − s (n'−n) c for refraction; 1/q' = 1/q + 2 s c for a mirror.
        let inv = q.inv();
        q = if surface.is_mirror() {
            Cx(inv.0 + 2.0 * s * c, inv.1).inv()
        } else {
            let (nb, na) = (before.abs(), after.abs());
            Cx((nb * inv.0 - s * (na - nb) * c) / na, nb * inv.1 / na).inv()
        };
        let index = after.abs();
        let w = width(q, index);
        let r = if q.inv().0.abs() > 1e-12 { Some(1.0 / q.inv().0) } else { None };
        // A waist lies where Re(q) = 0; within the following gap if −Re(q) is between 0 and the gap length.
        let gap = surface.thickness.abs();
        let to_waist = -q.0;
        let in_gap = to_waist >= 0.0 && to_waist <= gap;
        let waist_w = (l0 * q.1 / (PI * index)).max(0.0).sqrt();
        surfaces.push(SurfaceBeam {
            surface: i,
            z: surface_z[i],
            w,
            radius_of_curvature: r,
            waist_distance: in_gap.then_some(to_waist),
            waist_radius: in_gap.then_some(waist_w),
        });
        profile.push([surface_z[i], w]);
        for k in 1..=per_gap {
            let d = gap * k as f64 / per_gap as f64;
            profile.push([surface_z[i] + d, width(q.add(Cx(d, 0.0)), index)]);
        }
        q = q.add(Cx(gap, 0.0));
        if i == system.last() {
            let to_waist = -q.0;
            image_waist = Some((l0 * q.1 / (PI * index)).max(0.0).sqrt());
            image_waist_distance = Some(to_waist);
            divergence = Some(l0 / (PI * index * image_waist.unwrap().max(1e-12)));
        }
    }
    Ok(BeamTrace { wavelength: lambda, z_start, profile, surfaces, image_z, surface_z, image_waist, image_waist_distance, divergence })
}

// ---------- the wave field ----------

/// Where the chief ray runs in the gap being propagated: centre(s) = y + u · s along the path.
#[derive(Clone, Copy)]
struct ChiefGap {
    y: f64,
    u: f64,
    start: f64,
}

struct Field {
    n: usize,
    re: Vec<f64>,
    im: Vec<f64>,
    pitch: f64,
    /// Curvature 1/R of the reference sphere, mm⁻¹ (positive for a diverging wave).
    inv_r: f64,
}

struct Moments {
    power: f64,
    x2: f64,
    y2: f64,
    peak: f64,
    /// Change of 1/R that removes the quadratic phase of A.
    dcurv: f64,
}

impl Field {
    fn zeros(n: usize, pitch: f64) -> Field {
        Field { n, re: vec![0.0; n * n], im: vec![0.0; n * n], pitch, inv_r: 0.0 }
    }

    fn half(&self) -> f64 {
        (self.n / 2) as f64
    }

    fn measure(&self, k: f64) -> Moments {
        let n = self.n;
        let (d, h) = (self.pitch, self.half());
        let (re, im) = (&self.re, &self.im);
        let sums = (0..n).into_par_iter().map(|i| {
            let y = (i as f64 - h) * d;
            let (mut s0, mut sx, mut sy, mut peak, mut cross) = (0.0, 0.0, 0.0, 0.0_f64, 0.0);
            for j in 0..n {
                let x = (j as f64 - h) * d;
                let at = i * n + j;
                let p = re[at] * re[at] + im[at] * im[at];
                s0 += p;
                sx += x * x * p;
                sy += y * y * p;
                peak = peak.max(p);
                if i > 0 && i + 1 < n && j > 0 && j + 1 < n {
                    let (dre_x, dim_x) = ((re[at + 1] - re[at - 1]) / (2.0 * d), (im[at + 1] - im[at - 1]) / (2.0 * d));
                    let (dre_y, dim_y) = ((re[at + n] - re[at - n]) / (2.0 * d), (im[at + n] - im[at - n]) / (2.0 * d));
                    cross += x * (re[at] * dim_x - im[at] * dre_x) + y * (re[at] * dim_y - im[at] * dre_y);
                }
            }
            (s0, sx, sy, peak, cross)
        }).reduce(|| (0.0, 0.0, 0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3.max(b.3), a.4 + b.4));
        let (s0, sx, sy, peak, cross) = sums;
        if s0 <= 0.0 {
            return Moments { power: 0.0, x2: 0.0, y2: 0.0, peak: 0.0, dcurv: 0.0 };
        }
        let r2w = sx + sy;
        Moments { power: s0 * d * d, x2: sx / s0, y2: sy / s0, peak, dcurv: if r2w > 0.0 { cross / (k * r2w) } else { 0.0 } }
    }

    /// Moves the best-fit quadratic phase of A into the reference sphere (the physical field is unchanged).
    fn rereference(&mut self, k: f64) {
        let m = self.measure(k);
        if m.power <= 0.0 || !m.dcurv.is_finite() {
            return;
        }
        self.multiply_phase(|r2| -0.5 * k * m.dcurv * r2);
        self.inv_r += m.dcurv;
    }

    fn multiply_phase(&mut self, phase_of_r2: impl Fn(f64) -> f64 + Sync) {
        let n = self.n;
        let (d, h) = (self.pitch, self.half());
        let (re, im) = (&mut self.re, &mut self.im);
        re.par_chunks_mut(n).zip(im.par_chunks_mut(n)).enumerate().for_each(|(i, (rr, ii))| {
            let y = (i as f64 - h) * d;
            for j in 0..n {
                let x = (j as f64 - h) * d;
                let (s, c) = phase_of_r2(x * x + y * y).sin_cos();
                let (a, b) = (rr[j], ii[j]);
                rr[j] = a * c - b * s;
                ii[j] = a * s + b * c;
            }
        });
    }

    /// Circular aperture of `radius` about the optical axis; the grid is centred `center` mm off the axis in y.
    fn apply_aperture(&mut self, radius: f64, center: f64) {
        let n = self.n;
        let (d, h) = (self.pitch, self.half());
        let (re, im) = (&mut self.re, &mut self.im);
        re.par_chunks_mut(n).zip(im.par_chunks_mut(n)).enumerate().for_each(|(i, (rr, ii))| {
            let y = (i as f64 - h) * d + center;
            for j in 0..n {
                let x = (j as f64 - h) * d;
                // Anti-aliased edge: coverage of the pixel by the disc.
                let t = ((radius - x.hypot(y)) / d + 0.5).clamp(0.0, 1.0);
                rr[j] *= t;
                ii[j] *= t;
            }
        });
    }

    /// Free-space step of `distance` in a medium with wavenumber `k`, using Collins' scaling about the reference sphere.
    fn propagate(&mut self, distance: f64, k: f64) {
        let m = 1.0 + distance * self.inv_r;
        let d_eff = distance / m;
        self.angular_spectrum(d_eff, k);
        let scale = 1.0 / m;
        self.re.par_iter_mut().for_each(|v| *v *= scale);
        self.im.par_iter_mut().for_each(|v| *v *= scale);
        self.pitch *= m;
        self.inv_r /= m;
    }

    fn angular_spectrum(&mut self, distance: f64, k: f64) {
        let n = self.n;
        let big = 2 * n;
        let offset = n / 2;
        let mut re = vec![0.0; big * big];
        let mut im = vec![0.0; big * big];
        for i in 0..n {
            let at = (i + offset) * big + offset;
            re[at..at + n].copy_from_slice(&self.re[i * n..(i + 1) * n]);
            im[at..at + n].copy_from_slice(&self.im[i * n..(i + 1) * n]);
        }
        fft2(&mut re, &mut im, big, -1.0);
        let wavelength = 2.0 * PI / k;
        let df = 1.0 / (big as f64 * self.pitch);
        re.par_chunks_mut(big).zip(im.par_chunks_mut(big)).enumerate().for_each(|(i, (rr, ii))| {
            let fy = (if i < big / 2 { i as f64 } else { i as f64 - big as f64 }) * df;
            for j in 0..big {
                let fx = (if j < big / 2 { j as f64 } else { j as f64 - big as f64 }) * df;
                let s2 = wavelength * wavelength * (fx * fx + fy * fy);
                if s2 < 1.0 {
                    let (s, c) = (k * distance * ((1.0 - s2).sqrt() - 1.0)).sin_cos();
                    let (a, b) = (rr[j], ii[j]);
                    rr[j] = a * c - b * s;
                    ii[j] = a * s + b * c;
                } else {
                    rr[j] = 0.0;
                    ii[j] = 0.0;
                }
            }
        });
        fft2(&mut re, &mut im, big, 1.0);
        let norm = 1.0 / (big * big) as f64;
        for i in 0..n {
            let at = (i + offset) * big + offset;
            for j in 0..n {
                self.re[i * n + j] = re[at + j] * norm;
                self.im[i * n + j] = im[at + j] * norm;
            }
        }
    }

    /// Catmull–Rom sample of the complex field at fractional pixel coordinates.
    fn sample(&self, u: f64, v: f64) -> (f64, f64) {
        let n = self.n as isize;
        let (u0, v0) = (u.floor(), v.floor());
        let (fu, fv) = (u - u0, v - v0);
        let weights = |t: f64| [(-0.5 * t * t * t + t * t - 0.5 * t), (1.5 * t * t * t - 2.5 * t * t + 1.0), (-1.5 * t * t * t + 2.0 * t * t + 0.5 * t), (0.5 * t * t * t - 0.5 * t * t)];
        let (wu, wv) = (weights(fu), weights(fv));
        let (mut a, mut b) = (0.0, 0.0);
        for (dj, wy) in wv.iter().enumerate() {
            for (di, wx) in wu.iter().enumerate() {
                let (x, y) = (u0 as isize + di as isize - 1, v0 as isize + dj as isize - 1);
                if x < 0 || y < 0 || x >= n || y >= n {
                    continue;
                }
                let at = (y * n + x) as usize;
                a += wx * wy * self.re[at];
                b += wx * wy * self.im[at];
            }
        }
        (a, b)
    }

    /// Resamples the field onto a grid with another pitch (same centre).
    fn regrid(&mut self, new_pitch: f64) {
        let n = self.n;
        let h = self.half();
        let ratio = new_pitch / self.pitch;
        let mut out = Field::zeros(n, new_pitch);
        out.inv_r = self.inv_r;
        {
            let src = &*self;
            out.re.par_chunks_mut(n).zip(out.im.par_chunks_mut(n)).enumerate().for_each(|(i, (rr, ii))| {
                for j in 0..n {
                    let (a, b) = src.sample((j as f64 - h) * ratio + h, (i as f64 - h) * ratio + h);
                    rr[j] = a;
                    ii[j] = b;
                }
            });
        }
        *self = out;
    }
}

// ---------- running the simulation ----------

struct Run<'a> {
    field: Field,
    k0: f64,
    lambda0: f64,
    /// Beam radius over half window the source started with; the grid is rescaled when the beam leaves 0.4×–2.5× of it.
    target_ratio: f64,
    z: f64,
    slices: Vec<Slice>,
    planes: Vec<PlaneMap>,
    max_step: f64,
    warnings: &'a mut Vec<String>,
    steps: usize,
    chief: Option<ChiefGap>,
    center: f64,
    fiber: Option<f64>,
}

/// Power coupling into a Gaussian mode of radius `mode` centred on the grid: |∫ U ψ|² / (∫|U|² ∫ψ²).
fn coupling_efficiency(field: &Field, k: f64, mode: f64) -> f64 {
    let n = field.n;
    let (d, h) = (field.pitch, field.half());
    let (re, im) = (&field.re, &field.im);
    let (or, oi, power) = (0..n).into_par_iter().map(|i| {
        let y = (i as f64 - h) * d;
        let (mut or, mut oi, mut p) = (0.0, 0.0, 0.0);
        for j in 0..n {
            let x = (j as f64 - h) * d;
            let r2 = x * x + y * y;
            let at = i * n + j;
            // The reference sphere is part of the physical field; the fibre mode is at its waist (flat phase).
            let (s, c) = (0.5 * k * field.inv_r * r2).sin_cos();
            let psi = (-r2 / (mode * mode)).exp();
            or += psi * (re[at] * c - im[at] * s);
            oi += psi * (re[at] * s + im[at] * c);
            p += re[at] * re[at] + im[at] * im[at];
        }
        (or, oi, p)
    }).reduce(|| (0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
    let mode_power = PI * mode * mode / 2.0;
    if power <= 0.0 { 0.0 } else { (or * or + oi * oi) * d * d / (power * mode_power) }
}

fn round4(v: f32) -> f32 {
    if v.is_finite() { (v * 1e4).round() / 1e4 } else { v }
}

impl Run<'_> {
    fn snapshot(&mut self, n: f64, surface: Option<usize>, plane: Option<usize>) {
        let k = self.k0 * n;
        self.field.rereference(k);
        let m = self.field.measure(k);
        let f = &self.field;
        let nn = f.n;
        let mid = nn / 2;
        let peak = m.peak.max(1e-300);
        let x_cut: Vec<f32> = (0..nn).map(|j| round4((f.re[mid * nn + j].powi(2) + f.im[mid * nn + j].powi(2)) as f32 / peak as f32)).collect();
        let y_cut: Vec<f32> = (0..nn).map(|i| round4((f.re[i * nn + mid].powi(2) + f.im[i * nn + mid].powi(2)) as f32 / peak as f32)).collect();
        let (w_x, w_y) = (2.0 * m.x2.sqrt(), 2.0 * m.y2.sqrt());
        let coupling = self.fiber.map(|mode| coupling_efficiency(f, k, mode));
        self.slices.push(Slice { z: self.z, pitch: f.pitch, w_x, w_y, power: m.power, peak, x_cut, y_cut, surface, center: self.center, coupling });
        if let Some(index) = plane {
            let half_width = (2.4 * w_x.max(w_y)).clamp(f.pitch * 8.0, f.pitch * f.half());
            let size = 96;
            let mut intensity = Vec::with_capacity(size * size);
            let mut phase = Vec::with_capacity(size * size);
            for i in 0..size {
                for j in 0..size {
                    let x = ((j as f64 + 0.5) / size as f64 * 2.0 - 1.0) * half_width;
                    let y = (1.0 - (i as f64 + 0.5) / size as f64 * 2.0) * half_width;
                    let (a, b) = f.sample(x / f.pitch + f.half(), y / f.pitch + f.half());
                    let p = ((a * a + b * b) / peak).min(1.0);
                    intensity.push(round4(p as f32));
                    phase.push(if p > 1e-3 { round4(b.atan2(a) as f32) } else { f32::NAN });
                }
            }
            self.planes.push(PlaneMap { surface: index, z: self.z, half_width, size, intensity, phase, w_x, w_y, power: m.power, peak, center: self.center });
        }
    }

    /// Rescales the grid when the beam fills too much or too little of it.
    fn keep_beam_on_grid(&mut self, n: f64) {
        let k = self.k0 * n;
        let m = self.field.measure(k);
        if m.power <= 0.0 {
            return;
        }
        let w = (2.0 * (m.x2 + m.y2)).sqrt();
        let half = self.field.half() * self.field.pitch;
        let ratio = w / half;
        if ratio > self.target_ratio * 2.5 || ratio < self.target_ratio * 0.4 {
            let new_pitch = w / self.target_ratio / self.field.half();
            self.field.regrid(new_pitch);
        }
    }

    fn propagate(&mut self, distance: f64, n: f64) {
        let k = self.k0 * n;
        let mut remaining = distance;
        while remaining > 1e-9 {
            self.field.rereference(k);
            self.keep_beam_on_grid(n);
            let m = self.field.measure(k);
            let w = (2.0 * (m.x2 + m.y2)).sqrt().max(1e-9);
            let rayleigh = PI * n * w * w / self.lambda0;
            let inv_r = self.field.inv_r;
            let mut d = remaining.min(self.max_step);
            // Largest step over which the beam radius changes by at most ~12 % and the scaling stays well conditioned.
            while d > 1e-7 {
                let mm = 1.0 + d * inv_r;
                let ratio = (mm * mm + (d / rayleigh).powi(2)).sqrt();
                if mm >= 0.6 && ratio.ln().abs() <= 0.12 {
                    break;
                }
                d *= 0.7;
            }
            // Finish in one go when what is left is only slightly more than a step.
            if remaining - d < 0.15 * d {
                d = remaining;
            }
            let angle = w * inv_r.abs();
            if angle > 0.15 && !self.warnings.iter().any(|s| s.starts_with("The beam")) {
                self.warnings.push(format!("The beam converges or diverges at about NA {angle:.2}; paraxial propagation is less accurate for fast beams"));
            }
            self.field.propagate(d, k);
            self.z += d;
            remaining -= d;
            self.steps += 1;
            if let Some(gap) = self.chief {
                self.center = gap.y + gap.u * (self.z - gap.start);
            }
            self.snapshot(n, None, None);
        }
    }
}

/// Builds the source field and returns it with the path coordinate it starts at.
fn make_source(source: &Source, samples: usize, lambda0: f64) -> Result<(Field, f64, f64), LensError> {
    let positive = |v: f64, what: &str| if v > 0.0 && v.is_finite() { Ok(v) } else { Err(LensError(format!("The {what} must be positive"))) };
    let (radius, half_factor) = match *source {
        Source::Gaussian { radius, .. } => (positive(radius, "beam radius")?, 3.5),
        Source::SuperGaussian { radius, .. } => (positive(radius, "beam radius")?, 1.9),
        Source::TopHat { radius, .. } => (positive(radius, "beam radius")?, 2.2),
    };
    let mut start = 0.0;
    let mut width = radius;
    let mut inv_r = 0.0;
    if let Source::Gaussian { radius, waist } = *source {
        let zr = PI * radius * radius / lambda0;
        if waist < 0.0 {
            // The beam starts at its waist and travels to the first surface.
            start = waist;
        } else if waist > 0.0 {
            width = radius * (1.0 + (waist / zr).powi(2)).sqrt();
            inv_r = -1.0 / (waist * (1.0 + (zr / waist).powi(2)));
        }
    }
    if let Source::SuperGaussian { curvature_radius: Some(r), .. } | Source::TopHat { curvature_radius: Some(r), .. } = *source {
        if r != 0.0 && r.is_finite() {
            inv_r = 1.0 / r;
        }
    }
    let half = half_factor * width;
    let mut field = Field::zeros(samples, 2.0 * half / samples as f64);
    field.inv_r = inv_r;
    let (d, h) = (field.pitch, field.half());
    let n = samples;
    for i in 0..n {
        for j in 0..n {
            let (x, y) = ((j as f64 - h) * d, (i as f64 - h) * d);
            let r = x.hypot(y);
            field.re[i * n + j] = match *source {
                Source::Gaussian { .. } => (-(r / width).powi(2)).exp(),
                Source::SuperGaussian { order, .. } => (-(r / width).powf(2.0 * order.max(0.5))).exp(),
                Source::TopHat { .. } => ((width - r) / d + 0.5).clamp(0.0, 1.0),
            };
        }
    }
    // Unit power.
    let power: f64 = field.re.iter().map(|v| v * v).sum::<f64>() * d * d;
    let scale = 1.0 / power.sqrt();
    field.re.iter_mut().for_each(|v| *v *= scale);
    Ok((field, start, width / half))
}

/// Wavefront error of the real system relative to the sphere about the paraxial focus, in waves on a unit-pupil grid.
fn aberration_map(context: &TraceContext, paraxial: &ParaxialData, field: f64, wavelength: f64, size: usize) -> Result<Option<(Vec<f64>, AberrationInfo)>, LensError> {
    let system = context.system;
    let shift = paraxial.image_distance - system.surfaces[system.last()].thickness;
    let Some(opd) = OpdEvaluator::with_focus_shift(context, paraxial, field, wavelength, shift)? else { return Ok(None) };
    let mut values = vec![f64::NAN; size * size];
    for i in 0..size {
        for j in 0..size {
            let (px, py) = ((j as f64 + 0.5) / size as f64 * 2.0 - 1.0, (i as f64 + 0.5) / size as f64 * 2.0 - 1.0);
            if px * px + py * py <= 1.0 {
                values[i * size + j] = opd.opd(px, py);
            }
        }
    }
    let valid: Vec<f64> = values.iter().cloned().filter(|v| v.is_finite()).collect();
    if valid.len() < 8 {
        return Ok(None);
    }
    let mean = valid.iter().sum::<f64>() / valid.len() as f64;
    let rms = (valid.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / valid.len() as f64).sqrt();
    let pv = valid.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - valid.iter().cloned().fold(f64::INFINITY, f64::min);
    // Fill unavailable cells with the mean so interpolation near the edge stays finite.
    for v in values.iter_mut().filter(|v| !v.is_finite()) {
        *v = mean;
    }
    for v in values.iter_mut() {
        *v -= mean;
    }
    Ok(Some((values, AberrationInfo { rms_waves: rms, pv_waves: pv })))
}

fn apply_aberration(field: &mut Field, map: &[f64], size: usize, pupil_radius: f64) {
    let n = field.n;
    let (d, h) = (field.pitch, field.half());
    let inv = 1.0 / pupil_radius; // signed: an inverted pupil maps x to −x
    let (re, im) = (&mut field.re, &mut field.im);
    re.par_chunks_mut(n).zip(im.par_chunks_mut(n)).enumerate().for_each(|(i, (rr, ii))| {
        let y = (i as f64 - h) * d * inv;
        for j in 0..n {
            let x = (j as f64 - h) * d * inv;
            // Outside the pupil keep the edge value.
            let rho = x.hypot(y);
            let (ux, uy) = if rho > 1.0 { (x / rho, y / rho) } else { (x, y) };
            let (u, v) = ((ux + 1.0) / 2.0 * size as f64 - 0.5, (uy + 1.0) / 2.0 * size as f64 - 0.5);
            let (u0, v0) = (u.floor().clamp(0.0, size as f64 - 2.0), v.floor().clamp(0.0, size as f64 - 2.0));
            let (fu, fv) = ((u - u0).clamp(0.0, 1.0), (v - v0).clamp(0.0, 1.0));
            let at = |a: f64, b: f64| map[b as usize * size + a as usize];
            let w = at(u0, v0) * (1.0 - fu) * (1.0 - fv) + at(u0 + 1.0, v0) * fu * (1.0 - fv) + at(u0, v0 + 1.0) * (1.0 - fu) * fv + at(u0 + 1.0, v0 + 1.0) * fu * fv;
            // Longer optical path means later phase: a ray with OPD (reference − path) < 0 lags.
            let (s, c) = (-2.0 * PI * w).sin_cos();
            let (a, b) = (rr[j], ii[j]);
            rr[j] = a * c - b * s;
            ii[j] = a * s + b * c;
        }
    });
}

pub fn simulate(system: &LensSystem, settings: &PopSettings) -> Result<PopResult, LensError> {
    system.validate()?;
    check_supported(system)?;
    let mut resolved_system = resolved(system)?;
    // Paraxial data, pupils and the reference sphere of the aberration all refer to the simulated wavelength.
    resolved_system.primary_wavelength = settings.wavelength.min(resolved_system.wavelengths.len().saturating_sub(1));
    let system = &resolved_system;
    let lambda = *system.wavelengths.get(settings.wavelength).ok_or_else(|| LensError(format!("No wavelength {}", settings.wavelength + 1)))?;
    let lambda0 = lambda * 1e-3;
    let samples = settings.samples.clamp(64, 1024).next_power_of_two();
    let n_all = system.medium_indices(lambda)?;
    let (surface_z, image_z) = path_positions(system);
    let (field, start, ratio) = make_source(&settings.source, samples, lambda0)?;
    let mut warnings = Vec::new();

    let field_angle = match settings.field {
        Some(i) => *system.fields.get(i).ok_or_else(|| LensError(format!("No field {}", i + 1)))?,
        None => 0.0,
    };
    let tan = field_angle.to_radians().tan();
    let paraxial = paraxial_data(system)?;
    let context = TraceContext::new(system, &paraxial);
    // Paraxial chief ray: at surface 1 it is at −EPz·tan θ with slope tan θ; the beam grid follows it.
    let chief = (tan != 0.0).then(|| crate::paraxial::trace(system, &n_all, -paraxial.entrance_pupil_z * tan, tan));

    let folded = has_tilted_break(system);
    if folded {
        warnings.push("The system is folded by coordinate breaks: the unfolded path is simulated, tilted curved surfaces are treated as on-axis, and wavefront aberration is not applied".into());
    }
    let mut aberration = None;
    let mut aberration_map_data = None;
    if settings.aberrations && !folded {
        match aberration_map(&context, &paraxial, field_angle, lambda, 64)? {
            Some((map, info)) => {
                aberration_map_data = Some(map);
                aberration = Some(info);
            }
            None => warnings.push("The wavefront aberration could not be traced; the system is treated as ideal".into()),
        }
    }
    // Height of the paraxial marginal ray on the last surface (signed): the pupil radius seen there.
    let marginal_height = {
        let semi = system.entrance_pupil_diameter / 2.0;
        let ray = match system.object_distance {
            None => crate::paraxial::trace(system, &n_all, semi, 0.0),
            Some(d) => {
                let u = semi / (d + paraxial.entrance_pupil_z);
                crate::paraxial::trace(system, &n_all, u * d, u)
            }
        };
        ray.heights[system.last()]
    };

    let total = image_z + start.abs();
    let mut run = Run {
        field,
        k0: 2.0 * PI / lambda0,
        lambda0,
        target_ratio: ratio,
        z: start,
        slices: Vec::new(),
        planes: Vec::new(),
        max_step: (total / 110.0).max(1e-3),
        warnings: &mut warnings,
        steps: 0,
        chief: None,
        center: 0.0,
        fiber: settings.fiber_radius.filter(|r| *r > 0.0),
    };
    // Before the first surface the chief ray is y0 + tan θ · z with z ≤ 0.
    let y0 = -paraxial.entrance_pupil_z * tan;
    run.chief = chief.as_ref().map(|_| ChiefGap { y: y0, u: tan, start: 0.0 });
    run.center = y0 + tan * start;
    run.snapshot(1.0, None, None);
    if start < 0.0 {
        run.propagate(-start, 1.0);
        run.z = 0.0;
    }
    for (i, surface) in system.surfaces.iter().enumerate() {
        let (before, after) = (n_all[i], n_all[i + 1]);
        let s = before.signum();
        let c = surface.curvature();
        let chief_here = chief.as_ref().map_or(0.0, |ray| ray.heights[i]);
        run.center = chief_here;
        // Paraxial effect of the surface on the reference sphere.
        let inv_r = run.field.inv_r;
        run.field.inv_r = if surface.is_mirror() { inv_r + 2.0 * s * c } else { (before.abs() * inv_r - s * (after.abs() - before.abs()) * c) / after.abs() };
        if settings.apertures {
            if let Some(a) = surface.semi_diameter {
                run.field.apply_aperture(a, chief_here);
            }
        }
        if i == system.last() {
            if let Some(map) = &aberration_map_data {
                if marginal_height.abs() > 1e-9 {
                    // Applied in the plane of the last surface; a negative height means the pupil is inverted there.
                    apply_aberration(&mut run.field, map, 64, marginal_height);
                }
            }
        }
        run.z = surface_z[i];
        run.snapshot(after.abs(), Some(i), Some(i));
        run.chief = chief.as_ref().map(|ray| ChiefGap { y: ray.heights[i], u: ray.angles[i] * surface.thickness.signum(), start: surface_z[i] });
        run.propagate(surface.thickness.abs(), after.abs());
    }
    run.z = image_z;
    run.snapshot(n_all[system.surfaces.len()].abs(), None, Some(system.surfaces.len()));

    let analytic = if let (Source::Gaussian { radius, waist }, None) = (&settings.source, settings.field) {
        gaussian_beam(system, settings.wavelength, *radius, *waist, 60).ok().map(|t| t.profile)
    } else {
        None
    };
    let power_in = run.slices.first().map_or(1.0, |s| s.power);
    if run.slices.last().is_some_and(|s| s.power < 0.97 * power_in) {
        run.warnings.push("Power was lost at apertures or at the edge of the grid".into());
    }
    // Coupling at the image plane and the best value in image space, refined with a parabola through the top samples.
    let coupling = run.fiber.and_then(|_| {
        let last_surface_z = surface_z[system.last()];
        let at_image = run.slices.last()?.coupling?;
        let samples: Vec<(f64, f64)> = run.slices.iter().filter(|s| s.z >= last_surface_z).filter_map(|s| s.coupling.map(|c| (s.z, c))).collect();
        let k = (0..samples.len()).max_by(|&a, &b| samples[a].1.total_cmp(&samples[b].1))?;
        let (mut best_z, mut best) = samples[k];
        if k > 0 && k + 1 < samples.len() {
            let ((x0, y0), (x1, y1), (x2, y2)) = (samples[k - 1], samples[k], samples[k + 1]);
            // Vertex of the parabola through three points with uneven spacing.
            let denominator = (x0 - x1) * (x0 - x2) * (x1 - x2);
            let a2 = (x2 * (y1 - y0) + x1 * (y0 - y2) + x0 * (y2 - y1)) / denominator;
            let b2 = (x2 * x2 * (y0 - y1) + x1 * x1 * (y2 - y0) + x0 * x0 * (y1 - y2)) / denominator;
            if a2 < 0.0 {
                let vertex = -b2 / (2.0 * a2);
                if vertex >= x0 && vertex <= x2 {
                    best_z = vertex;
                    best = a2 * vertex * vertex + b2 * vertex + (y1 - a2 * x1 * x1 - b2 * x1);
                }
            }
        }
        Some(CouplingSummary { at_image, best: best.min(1.0), best_z })
    });
    let (slices, planes) = (run.slices, run.planes);
    Ok(PopResult { wavelength: lambda, samples, slices, planes, surface_z, image_z, z_start: start, analytic, aberration, coupling, field_angle, warnings })
}
