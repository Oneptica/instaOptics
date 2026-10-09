//! Polarization ray tracing: every refraction and reflection contributes a Jones matrix built from the Fresnel (or thin
//! film) coefficients in the local s and p directions; their 3×3 product maps the input field to the output field.
//! Reports transmission, diattenuation, retardance and the output polarization ellipse over the pupil.

use num_complex::Complex64 as C;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::coating::{self, Coating, coefficients};
use crate::system::{LensError, LensSystem};
use crate::trace::{Bounce, RealRay, TraceContext};
use crate::vec3::{Vec3, dot, normalize};

type Mat3 = [[C; 3]; 3];

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PolInput {
    /// Linear polarization at an angle (degrees) from the local x axis.
    Linear { angle: f64 },
    RightCircular,
    LeftCircular,
    Unpolarized,
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn identity() -> Mat3 {
    let (o, z) = (C::new(1.0, 0.0), C::new(0.0, 0.0));
    [[o, z, z], [z, o, z], [z, z, o]]
}

fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut m = [[C::new(0.0, 0.0); 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                m[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    m
}

fn apply(m: &Mat3, v: [C; 3]) -> [C; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Parses the coating of every surface once.
pub fn parse_coatings(system: &LensSystem) -> Result<Vec<Option<Coating>>, LensError> {
    system.surfaces.iter().enumerate().map(|(i, surface)| match surface.coating.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(text) => coating::parse(text, system.primary_wavelength()).map(Some).map_err(|e| LensError(format!("Coating on surface {}: {e}", i + 1))),
    }).collect()
}

/// Jones matrix of one bounce, including the power factor of the change of medium.
fn bounce_matrix(bounce: &Bounce, coating: Option<&Coating>, wavelength: f64) -> Mat3 {
    let normal = bounce.normal;
    let cos_i = dot(bounce.k_in, normal).abs().clamp(1e-9, 1.0);
    // ŝ perpendicular to the plane of incidence; at normal incidence any perpendicular direction will do.
    let mut s = cross(bounce.k_in, normal);
    let length = dot(s, s).sqrt();
    s = if length > 1e-9 {
        [s[0] / length, s[1] / length, s[2] / length]
    } else {
        let helper = if bounce.k_in[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        normalize(cross(bounce.k_in, helper))
    };
    let p_in = cross(bounce.k_in, s);
    let p_out = cross(bounce.k_out, s);
    let (a_s, a_p) = if bounce.mirror {
        match coating {
            // An uncoated mirror is a perfect reflector: the tangential field flips sign.
            None => (C::new(-1.0, 0.0), C::new(1.0, 0.0)),
            Some(c) => {
                let k = coefficients(Some(c), bounce.n_in, C::new(1.52, 0.0), wavelength, cos_i);
                (k.r_s, k.r_p)
            }
        }
    } else {
        let k = coefficients(coating, bounce.n_in, C::new(bounce.n_out, 0.0), wavelength, cos_i);
        let cos_out = dot(bounce.k_out, normal).abs().clamp(1e-9, 1.0);
        let power = (bounce.n_out * cos_out / (bounce.n_in * cos_i)).sqrt();
        (k.t_s * power, k.t_p * power)
    };
    let mut m = [[C::new(0.0, 0.0); 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = a_s * (s[i] * s[j]) + a_p * (p_out[i] * p_in[j]);
        }
    }
    m
}

/// The product of the Jones matrices along the ray, or None for a failed ray.
pub fn ray_matrix(ray: &RealRay, coatings: &[Option<Coating>], wavelength: f64) -> Option<Mat3> {
    if !ray.ok() {
        return None;
    }
    let mut total = identity();
    for bounce in &ray.bounces {
        total = mul(&bounce_matrix(bounce, coatings[bounce.surface].as_ref(), wavelength), &total);
    }
    Some(total)
}

/// Orthonormal transverse basis for a direction, from the global x axis.
fn basis(k: Vec3) -> (Vec3, Vec3) {
    let helper = if k[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x = normalize(cross(cross(k, helper), k));
    (x, cross(k, x))
}

/// Rotates the vector `v` the shortest way that takes `from` onto `to`.
fn transport(v: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let axis = cross(from, to);
    let (sin, cos) = (dot(axis, axis).sqrt(), dot(from, to));
    if sin < 1e-12 {
        if cos > 0.0 {
            return v;
        }
        // Reversed direction: rotate half a turn about any axis perpendicular to `from`.
        let (a, _) = basis(from);
        let along = dot(a, v);
        return [2.0 * along * a[0] - v[0], 2.0 * along * a[1] - v[1], 2.0 * along * a[2] - v[2]];
    }
    let a = [axis[0] / sin, axis[1] / sin, axis[2] / sin];
    let (c, s) = (cos, sin);
    let cr = cross(a, v);
    let d = dot(a, v);
    [v[0] * c + cr[0] * s + a[0] * d * (1.0 - c), v[1] * c + cr[1] * s + a[1] * d * (1.0 - c), v[2] * c + cr[2] * s + a[2] * d * (1.0 - c)]
}

/// Properties of one ray's polarization transformation.
#[derive(Clone, Copy, Debug)]
pub struct RayPolarization {
    /// Power transmission for the requested input.
    pub transmission: f64,
    pub diattenuation: f64,
    /// Degrees, 0 to 180.
    pub retardance: f64,
    /// Orientation and ellipticity of the output ellipse for the input (degrees); NaN for unpolarized light.
    pub orientation: f64,
    pub ellipticity: f64,
}

/// Singular values and unitary part of a 2×2 complex matrix; returns (σ₁, σ₂, retardance in radians).
fn diattenuation_retardance(j: [[C; 2]; 2]) -> (f64, f64, f64) {
    // H = J†J.
    let h = |a: usize, b: usize| (0..2).map(|k| j[k][a].conj() * j[k][b]).sum::<C>();
    let (h00, h11, h01) = (h(0, 0).re, h(1, 1).re, h(0, 1));
    let tr = h00 + h11;
    let det = h00 * h11 - h01.norm_sqr();
    let disc = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, (tr / 2.0 - disc).max(0.0));
    let (s1, s2) = (l1.sqrt(), l2.sqrt());
    if s1 < 1e-12 {
        return (0.0, 0.0, 0.0);
    }
    // Eigenvectors of H.
    let v1 = if h01.norm() > 1e-14 { [h01, C::new(l1 - h00, 0.0)] } else if h00 >= h11 { [C::new(1.0, 0.0), C::new(0.0, 0.0)] } else { [C::new(0.0, 0.0), C::new(1.0, 0.0)] };
    let norm1 = (v1[0].norm_sqr() + v1[1].norm_sqr()).sqrt();
    let v1 = [v1[0] / norm1, v1[1] / norm1];
    let v2 = [-v1[1].conj(), v1[0].conj()];
    let w = |v: [C; 2], s: f64| [(j[0][0] * v[0] + j[0][1] * v[1]) / s, (j[1][0] * v[0] + j[1][1] * v[1]) / s];
    let w1 = w(v1, s1);
    // With σ₂ ≈ 0 the second left vector is any vector orthogonal to the first.
    let w2 = if s2 > 1e-9 * s1 { w(v2, s2) } else { [-w1[1].conj(), w1[0].conj()] };
    // Q = W V†.
    let q = |a: usize, b: usize| w1[a] * v1[b].conj() + w2[a] * v2[b].conj();
    let (q00, q01, q10, q11) = (q(0, 0), q(0, 1), q(1, 0), q(1, 1));
    let trace = q00 + q11;
    let det = q00 * q11 - q01 * q10;
    let root = (trace * trace / 4.0 - det).sqrt();
    let (e1, e2) = (trace / 2.0 + root, trace / 2.0 - root);
    let mut retardance = (e1.arg() - e2.arg()).abs();
    if retardance > std::f64::consts::PI {
        retardance = 2.0 * std::f64::consts::PI - retardance;
    }
    (s1, s2, retardance)
}

/// Analyzes one ray for an input state; None when the ray fails.
pub fn analyze_ray(ray: &RealRay, coatings: &[Option<Coating>], wavelength: f64, input: PolInput) -> Option<RayPolarization> {
    let matrix = ray_matrix(ray, coatings, wavelength)?;
    let world = &ray.world;
    let (k0, kn) = (normalize(sub(world[1], world[0])), normalize(sub(world[world.len() - 1], world[world.len() - 2])));
    let (x0, y0) = basis(k0);
    let (xn, yn) = (transport(x0, k0, kn), transport(y0, k0, kn));
    let column = |e: Vec3| apply(&matrix, [C::new(e[0], 0.0), C::new(e[1], 0.0), C::new(e[2], 0.0)]);
    let out = |v: [C; 3]| [dot_c(xn, v), dot_c(yn, v)];
    let (a, b) = (out(column(x0)), out(column(y0)));
    // 2×2 Jones matrix from the input basis to the output basis.
    let jones = [[a[0], b[0]], [a[1], b[1]]];
    let (s1, s2, retardance) = diattenuation_retardance(jones);
    let total = s1 * s1 + s2 * s2;
    let diattenuation = if total > 0.0 { (s1 * s1 - s2 * s2) / total } else { 0.0 };
    let state = match input {
        PolInput::Linear { angle } => {
            let (s, c) = angle.to_radians().sin_cos();
            Some([C::new(c, 0.0), C::new(s, 0.0)])
        }
        PolInput::RightCircular => Some([C::new(std::f64::consts::FRAC_1_SQRT_2, 0.0), C::new(0.0, -std::f64::consts::FRAC_1_SQRT_2)]),
        PolInput::LeftCircular => Some([C::new(std::f64::consts::FRAC_1_SQRT_2, 0.0), C::new(0.0, std::f64::consts::FRAC_1_SQRT_2)]),
        PolInput::Unpolarized => None,
    };
    Some(match state {
        None => RayPolarization { transmission: 0.5 * total, diattenuation, retardance: retardance.to_degrees(), orientation: f64::NAN, ellipticity: f64::NAN },
        Some(e) => {
            let o = [jones[0][0] * e[0] + jones[0][1] * e[1], jones[1][0] * e[0] + jones[1][1] * e[1]];
            let (s0, s1) = (o[0].norm_sqr() + o[1].norm_sqr(), o[0].norm_sqr() - o[1].norm_sqr());
            let (s2, s3) = (2.0 * (o[0] * o[1].conj()).re, -2.0 * (o[0] * o[1].conj()).im);
            RayPolarization {
                transmission: s0,
                diattenuation,
                retardance: retardance.to_degrees(),
                orientation: if s0 > 1e-12 { 0.5 * s2.atan2(s1).to_degrees() } else { f64::NAN },
                ellipticity: if s0 > 1e-12 { 0.5 * (s3 / s0).clamp(-1.0, 1.0).asin().to_degrees() } else { f64::NAN },
            }
        }
    })
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot_c(v: Vec3, c: [C; 3]) -> C {
    c[0] * v[0] + c[1] * v[1] + c[2] * v[2]
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolarizationMap {
    pub field_angle: f64,
    pub size: usize,
    /// Row-major over the unit pupil, row 0 at +y; NaN (null) outside the pupil or where the ray fails.
    pub transmission: Vec<f64>,
    pub diattenuation: Vec<f64>,
    /// Degrees.
    pub retardance: Vec<f64>,
    pub orientation: Vec<f64>,
    pub ellipticity: Vec<f64>,
    pub mean_transmission: f64,
    pub min_transmission: f64,
    pub max_diattenuation: f64,
    pub rms_retardance: f64,
    /// Transmission of each surface along the chief ray (power), for the loss budget.
    pub surface_transmission: Vec<f64>,
}

fn pupil_grid(size: usize) -> Vec<(usize, f64, f64)> {
    let mut points = Vec::new();
    for i in 0..size {
        for j in 0..size {
            let (px, py) = ((j as f64 + 0.5) / size as f64 * 2.0 - 1.0, 1.0 - (i as f64 + 0.5) / size as f64 * 2.0);
            if px * px + py * py <= 1.0 {
                points.push((i * size + j, px, py));
            }
        }
    }
    points
}

pub fn polarization_map(context: &TraceContext, field: f64, wavelength: f64, size: usize, input: PolInput) -> Result<PolarizationMap, LensError> {
    let system = context.system;
    let coatings = parse_coatings(system)?;
    let n = system.medium_indices(wavelength)?;
    let nan = vec![f64::NAN; size * size];
    let (mut transmission, mut diattenuation, mut retardance, mut orientation, mut ellipticity) = (nan.clone(), nan.clone(), nan.clone(), nan.clone(), nan);
    let results: Vec<(usize, Option<RayPolarization>)> = pupil_grid(size).into_par_iter()
        .map(|(at, px, py)| (at, analyze_ray(&context.trace_ray(field, px, py, wavelength, &n), &coatings, wavelength, input)))
        .collect();
    let (mut sum, mut count, mut min_t, mut max_d, mut sum_r2) = (0.0, 0usize, f64::INFINITY, 0.0_f64, 0.0);
    for (at, result) in results {
        let Some(r) = result else { continue };
        transmission[at] = r.transmission;
        diattenuation[at] = r.diattenuation;
        retardance[at] = r.retardance;
        orientation[at] = r.orientation;
        ellipticity[at] = r.ellipticity;
        sum += r.transmission;
        count += 1;
        min_t = min_t.min(r.transmission);
        max_d = max_d.max(r.diattenuation);
        sum_r2 += r.retardance * r.retardance;
    }
    // Per-surface transmission along the chief ray, for unpolarized light.
    let chief = context.trace_ray(field, 0.0, 0.0, wavelength, &n);
    let mut surface_transmission = vec![1.0; system.surfaces.len()];
    for bounce in &chief.bounces {
        let single = RealRay { bounces: vec![*bounce], ..chief.clone() };
        let m = ray_matrix(&single, &coatings, wavelength).unwrap_or_else(identity);
        // Mean over two orthogonal input states of the squared output length.
        let (x, y) = basis(bounce.k_in);
        let power = |e: Vec3| apply(&m, [C::new(e[0], 0.0), C::new(e[1], 0.0), C::new(e[2], 0.0)]).iter().map(|c| c.norm_sqr()).sum::<f64>();
        surface_transmission[bounce.surface] = 0.5 * (power(x) + power(y));
    }
    Ok(PolarizationMap {
        field_angle: field,
        size,
        transmission,
        diattenuation,
        retardance,
        orientation,
        ellipticity,
        mean_transmission: if count > 0 { sum / count as f64 } else { f64::NAN },
        min_transmission: if count > 0 { min_t } else { f64::NAN },
        max_diattenuation: max_d,
        rms_retardance: if count > 0 { (sum_r2 / count as f64).sqrt() } else { f64::NAN },
        surface_transmission,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldTransmission {
    pub field: f64,
    pub mean: f64,
    pub min: f64,
    pub max_diattenuation: f64,
    pub rms_retardance: f64,
}

/// Transmission statistics for every field of the system.
pub fn transmission_by_field(context: &TraceContext, wavelength: f64, size: usize, input: PolInput) -> Result<Vec<FieldTransmission>, LensError> {
    context.system.fields.iter().map(|&field| {
        let map = polarization_map(context, field, wavelength, size, input)?;
        Ok(FieldTransmission { field, mean: map.mean_transmission, min: map.min_transmission, max_diattenuation: map.max_diattenuation, rms_retardance: map.rms_retardance })
    }).collect()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoatingCurves {
    pub surface: usize,
    pub description: String,
    /// Incidence angle sweep (degrees) at the chosen wavelength.
    pub angles: Vec<f64>,
    pub r_s: Vec<f64>,
    pub r_p: Vec<f64>,
    pub t_s: Vec<f64>,
    pub t_p: Vec<f64>,
    /// Wavelength sweep (µm) at normal incidence.
    pub wavelengths: Vec<f64>,
    pub r_wavelength: Vec<f64>,
    pub t_wavelength: Vec<f64>,
}

/// Reflectance and transmittance (power) of a surface's interface and coating.
pub fn coating_curves(system: &LensSystem, surface: usize, wavelength: f64, max_angle: f64, points: usize) -> Result<CoatingCurves, LensError> {
    let coatings = parse_coatings(system)?;
    let spec = system.surfaces.get(surface).ok_or_else(|| LensError(format!("No surface {}", surface + 1)))?;
    let n = system.medium_indices(wavelength)?;
    let n0 = n[surface].abs();
    let mirror = spec.is_mirror();
    let n_sub = if mirror { C::new(1.52, 0.0) } else { C::new(n[surface + 1].abs(), 0.0) };
    let coating = coatings[surface].as_ref();
    let evaluate = |wavelength: f64, angle: f64| {
        let n = system.medium_indices(wavelength).unwrap_or_else(|_| n.clone());
        let (n0, n_sub) = (n[surface].abs(), if mirror { C::new(1.52, 0.0) } else { C::new(n[surface + 1].abs(), 0.0) });
        let cos_i = angle.to_radians().cos();
        let k = coefficients(coating, n0, n_sub, wavelength, cos_i);
        let power = (n_sub_effective(coating, n_sub) * k.cos_t).re / (n0 * cos_i);
        let perfect = mirror && coating.is_none();
        let (rs, rp) = if perfect { (1.0, 1.0) } else { (k.r_s.norm_sqr(), k.r_p.norm_sqr()) };
        let (ts, tp) = if mirror { (0.0, 0.0) } else { (power * k.t_s.norm_sqr(), power * k.t_p.norm_sqr()) };
        (rs, rp, ts, tp)
    };
    let _ = (n0, n_sub);
    let points = points.max(2);
    let angles: Vec<f64> = (0..points).map(|i| max_angle * i as f64 / (points - 1) as f64).collect();
    let curves: Vec<_> = angles.iter().map(|&a| evaluate(wavelength, a)).collect();
    let (lo, hi) = (wavelength * 0.6, wavelength * 1.5);
    let wavelengths: Vec<f64> = (0..points).map(|i| lo + (hi - lo) * i as f64 / (points - 1) as f64).collect();
    let sweep: Vec<_> = wavelengths.iter().map(|&w| evaluate(w, 0.0)).collect();
    let description = match (&spec.coating, mirror) {
        (Some(text), _) => text.clone(),
        (None, true) => "perfect mirror".into(),
        (None, false) => "uncoated".into(),
    };
    Ok(CoatingCurves {
        surface,
        description,
        angles,
        r_s: curves.iter().map(|c| c.0).collect(),
        r_p: curves.iter().map(|c| c.1).collect(),
        t_s: curves.iter().map(|c| c.2).collect(),
        t_p: curves.iter().map(|c| c.3).collect(),
        wavelengths,
        r_wavelength: sweep.iter().map(|c| 0.5 * (c.0 + c.1)).collect(),
        t_wavelength: sweep.iter().map(|c| 0.5 * (c.2 + c.3)).collect(),
    })
}

fn n_sub_effective(coating: Option<&Coating>, n_sub: C) -> C {
    coating.and_then(|c| c.substrate).unwrap_or(n_sub)
}
