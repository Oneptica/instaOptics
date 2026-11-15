//! Image-quality analyses built on the sequential tracer: spot diagrams, ray fans, Seidel sums, field curvature and
//! distortion, wavefront maps, FFT MTF and PSF, and relative illumination.

use rayon::prelude::*;
use serde::Serialize;

use crate::paraxial::{ParaxialData, trace as paraxial_trace};
use crate::system::{LensError, LensSystem};
use crate::trace::{RealRay, TraceContext};
use crate::vec3::{Vec3, dot};

fn mean(values: impl IntoIterator<Item = f64>) -> f64 {
    let (sum, count) = values.into_iter().fold((0.0, 0usize), |(s, c), v| (s + v, c + 1));
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn indices_by_wavelength(system: &LensSystem) -> Result<Vec<Vec<f64>>, LensError> {
    system.wavelengths.iter().map(|&w| system.medium_indices(w)).collect()
}

fn exit_point(ray: &RealRay) -> Vec3 {
    ray.points[ray.points.len() - 2]
}

/// Hexapolar pupil sampling: a centre point plus rings of 6·r points.
pub fn hexapolar(rings: usize) -> Vec<[f64; 2]> {
    let mut points = vec![[0.0, 0.0]];
    for r in 1..=rings {
        let radius = r as f64 / rings as f64;
        for k in 0..6 * r {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / (6 * r) as f64;
            points.push([radius * angle.sin(), radius * angle.cos()]);
        }
    }
    points
}

// ---------- spot diagram ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotField {
    pub field: f64,
    /// (x, y) relative to the reference in mm, and the wavelength index.
    pub points: Vec<[f64; 3]>,
    /// Primary chief ray image position (or the centroid when it fails), mm.
    pub reference: [f64; 2],
    pub rms_radius: f64,
    pub geo_radius: f64,
    pub traced: usize,
    pub total: usize,
}

pub fn spot_diagram(context: &TraceContext, rings: usize) -> Result<Vec<SpotField>, LensError> {
    let system = context.system;
    let pupil = hexapolar(rings);
    let indices = indices_by_wavelength(system)?;
    let primary = system.primary_wavelength.min(system.wavelengths.len() - 1);
    Ok(system.fields.par_iter().map(|&field| {
        let mut hits = Vec::new();
        for (w, &wavelength) in system.wavelengths.iter().enumerate() {
            for &[px, py] in &pupil {
                let ray = context.trace_ray(field, px, py, wavelength, &indices[w]);
                if ray.ok() {
                    let end = ray.end();
                    hits.push([end[0], end[1], w as f64]);
                }
            }
        }
        let chief = context.trace_ray(field, 0.0, 0.0, system.wavelengths[primary], &indices[primary]);
        let reference = if chief.ok() {
            [chief.end()[0], chief.end()[1]]
        } else {
            [mean(hits.iter().map(|h| h[0])), mean(hits.iter().map(|h| h[1]))]
        };
        let points: Vec<[f64; 3]> = hits.iter().map(|h| [h[0] - reference[0], h[1] - reference[1], h[2]]).collect();
        let radii: Vec<f64> = points.iter().map(|p| p[0].hypot(p[1])).collect();
        SpotField {
            field,
            reference,
            rms_radius: mean(radii.iter().map(|r| r * r)).sqrt(),
            geo_radius: radii.iter().cloned().fold(0.0, f64::max),
            traced: points.len(),
            total: pupil.len() * system.wavelengths.len(),
            points,
        }
    }).collect())
}

// ---------- ray fans ----------

#[derive(Clone, Debug, Serialize)]
pub struct FanCurve {
    pub wavelength: f64,
    pub pupil: Vec<f64>,
    /// Transverse error in mm relative to the primary chief ray.
    pub error: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RayFanField {
    pub field: f64,
    pub tangential: Vec<FanCurve>,
    pub sagittal: Vec<FanCurve>,
}

/// Transverse ray aberration: ey against py (tangential) and ex against px (sagittal).
pub fn ray_fan(context: &TraceContext, samples: usize) -> Result<Vec<RayFanField>, LensError> {
    let system = context.system;
    let indices = indices_by_wavelength(system)?;
    let primary = system.primary_wavelength.min(system.wavelengths.len() - 1);
    let pupil: Vec<f64> = (0..samples).map(|i| -1.0 + 2.0 * i as f64 / (samples - 1).max(1) as f64).collect();
    Ok(system.fields.par_iter().map(|&field| {
        let chief = context.trace_ray(field, 0.0, 0.0, system.wavelengths[primary], &indices[primary]);
        let reference = chief.ok().then(|| chief.end());
        let fan = |tangential: bool| -> Vec<FanCurve> {
            system.wavelengths.iter().enumerate().map(|(w, &wavelength)| {
                let mut curve = FanCurve { wavelength, pupil: Vec::new(), error: Vec::new() };
                if let Some(reference) = reference {
                    for &p in &pupil {
                        let (px, py) = if tangential { (0.0, p) } else { (p, 0.0) };
                        let ray = context.trace_ray(field, px, py, wavelength, &indices[w]);
                        if !ray.ok() {
                            continue;
                        }
                        let end = ray.end();
                        curve.pupil.push(p);
                        curve.error.push(if tangential { end[1] - reference[1] } else { end[0] - reference[0] });
                    }
                }
                curve
            }).collect()
        };
        RayFanField { field, tangential: fan(true), sagittal: fan(false) }
    }).collect())
}

// ---------- Seidel aberrations ----------

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[allow(non_snake_case)]
pub struct SeidelTerms {
    pub S1: f64,
    pub S2: f64,
    pub S3: f64,
    pub S4: f64,
    pub S5: f64,
    pub CL: f64,
    pub CT: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeidelResult {
    pub surfaces: Vec<SeidelTerms>,
    pub total: SeidelTerms,
}

/// Third-order Seidel sums per surface (Welford's convention, mm) for the largest field and full aperture.
/// Chromatic terms use the shortest and longest wavelengths.
pub fn seidel(system: &LensSystem, paraxial: &ParaxialData) -> Result<SeidelResult, LensError> {
    let n = system.medium_indices(system.primary_wavelength())?;
    let shortest = system.wavelengths.iter().cloned().fold(f64::INFINITY, f64::min);
    let longest = system.wavelengths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let n_short = system.medium_indices(shortest)?;
    let n_long = system.medium_indices(longest)?;
    let dispersion: Vec<f64> = (0..n.len()).map(|i| (n_short[i] - n_long[i]) / n[i]).collect();

    let semi_pupil = system.entrance_pupil_diameter / 2.0;
    let z_ep = paraxial.entrance_pupil_z;
    let marginal_u0 = system.object_distance.map_or(0.0, |d| semi_pupil / (d + z_ep));
    let marginal = match system.object_distance {
        None => paraxial_trace(system, &n, semi_pupil, 0.0),
        Some(d) => paraxial_trace(system, &n, marginal_u0 * d, marginal_u0),
    };
    let slope = system.max_field().to_radians().tan();
    let chief = paraxial_trace(system, &n, -z_ep * slope, slope);
    let lagrange = n[0] * (slope * marginal.heights[0] - marginal_u0 * chief.heights[0]);

    let surfaces: Vec<SeidelTerms> = system.surfaces.iter().enumerate().map(|(i, surface)| {
        let c = surface.curvature();
        let (y, y_bar) = (marginal.heights[i], chief.heights[i]);
        let u = if i == 0 { marginal_u0 } else { marginal.angles[i - 1] };
        let u_bar = if i == 0 { slope } else { chief.angles[i - 1] };
        let a = n[i] * (u + y * c);
        let a_bar = n[i] * (u_bar + y_bar * c);
        let delta = marginal.angles[i] / n[i + 1] - u / n[i];
        let mut t = SeidelTerms {
            S1: -a * a * y * delta,
            S2: -a * a_bar * y * delta,
            S3: -a_bar * a_bar * y * delta,
            S4: -lagrange * lagrange * c * (1.0 / n[i + 1] - 1.0 / n[i]),
            S5: 0.0,
            CL: a * y * (dispersion[i + 1] - dispersion[i]),
            CT: a_bar * y * (dispersion[i + 1] - dispersion[i]),
        };
        t.S5 = if a.abs() > 1e-15 { a_bar / a * (t.S3 + t.S4) } else { 0.0 };
        // Conic and aspheric terms add a fourth-order departure a·r⁴ beyond the base sphere.
        let departure = surface.conic * c * c * c / 8.0 + surface.aspheric.first().copied().unwrap_or(0.0);
        if departure != 0.0 && y != 0.0 {
            let asphere = 8.0 * departure * y.powi(4) * (n[i + 1] - n[i]);
            let ratio = y_bar / y;
            t.S1 += asphere;
            t.S2 += asphere * ratio;
            t.S3 += asphere * ratio * ratio;
            t.S5 += asphere * ratio.powi(3);
        }
        t
    }).collect();
    let mut total = SeidelTerms::default();
    for t in &surfaces {
        total.S1 += t.S1;
        total.S2 += t.S2;
        total.S3 += t.S3;
        total.S4 += t.S4;
        total.S5 += t.S5;
        total.CL += t.CL;
        total.CT += t.CT;
    }
    Ok(SeidelResult { surfaces, total })
}

// ---------- field curvature and distortion ----------

#[derive(Clone, Debug, Serialize)]
pub struct FieldCurve {
    pub wavelength: f64,
    /// Focus shift from the image plane in mm; NaN (null) where rays fail.
    pub tangential: Vec<f64>,
    pub sagittal: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FieldCurves {
    pub angles: Vec<f64>,
    pub curves: Vec<FieldCurve>,
    /// Percent, primary wavelength.
    pub distortion: Vec<f64>,
}

fn field_angles(system: &LensSystem, samples: usize) -> Vec<f64> {
    let max = system.max_field();
    if max == 0.0 { vec![0.0] } else { (0..samples).map(|i| max * i as f64 / (samples - 1) as f64).collect() }
}

/// z where two meridional rays cross, from a.p + s·a.d = b.p + t·b.d in the y–z plane.
fn meridional_crossing(ap: Vec3, ad: Vec3, bp: Vec3, bd: Vec3) -> f64 {
    let det = ad[1] * -bd[2] - ad[2] * -bd[1];
    if det.abs() < 1e-18 {
        return f64::NAN;
    }
    let (dy, dz) = (bp[1] - ap[1], bp[2] - ap[2]);
    let s = (dy * -bd[2] - dz * -bd[1]) / det;
    ap[2] + s * ad[2]
}

pub fn field_curves(context: &TraceContext, samples: usize) -> Result<FieldCurves, LensError> {
    let system = context.system;
    let angles = field_angles(system, samples);
    let indices = indices_by_wavelength(system)?;
    let epsilon = 0.02;
    let curves = system.wavelengths.par_iter().enumerate().map(|(w, &wavelength)| {
        let n = &indices[w];
        let mut tangential = Vec::new();
        let mut sagittal = Vec::new();
        for &angle in &angles {
            let up = context.trace_ray(angle, 0.0, epsilon, wavelength, n);
            let down = context.trace_ray(angle, 0.0, -epsilon, wavelength, n);
            tangential.push(if up.ok() && down.ok() {
                meridional_crossing(exit_point(&up), up.direction, exit_point(&down), down.direction) - context.image_z
            } else {
                f64::NAN
            });
            let side = context.trace_ray(angle, epsilon, 0.0, wavelength, n);
            sagittal.push(if side.ok() && side.direction[0].abs() > 1e-15 {
                let p = exit_point(&side);
                p[2] - p[0] / side.direction[0] * side.direction[2] - context.image_z
            } else {
                f64::NAN
            });
        }
        FieldCurve { wavelength, tangential, sagittal }
    }).collect();

    let primary = system.primary_wavelength();
    let n = system.medium_indices(primary)?;
    let last = system.last();
    let distortion = angles.iter().map(|&angle| {
        if angle == 0.0 {
            return 0.0;
        }
        let slope = angle.to_radians().tan();
        let chief = paraxial_trace(system, &n, -context.entrance_pupil_z * slope, slope);
        let reference = chief.heights[last] + chief.angles[last] * system.surfaces[last].thickness;
        let real = context.trace_ray(angle, 0.0, 0.0, primary, &n);
        if real.ok() && reference != 0.0 { 100.0 * (real.end()[1] - reference) / reference } else { f64::NAN }
    }).collect();
    Ok(FieldCurves { angles, curves, distortion })
}

// ---------- wavefront ----------

/// Optical path difference (waves) relative to a reference sphere centred on the primary chief ray image point and
/// passing through the paraxial exit pupil.
pub struct OpdEvaluator<'c, 'a> {
    context: &'c TraceContext<'a>,
    field: f64,
    wavelength: f64,
    n: Vec<f64>,
    centre: Vec3,
    chief_dir: Vec3,
    pupil_point: Option<Vec3>,
    radius: f64,
    upstream: bool,
    reference: f64,
}

impl<'c, 'a> OpdEvaluator<'c, 'a> {
    pub fn new(context: &'c TraceContext<'a>, paraxial: &ParaxialData, field: f64, wavelength: f64) -> Result<Option<Self>, LensError> {
        Self::with_focus_shift(context, paraxial, field, wavelength, 0.0)
    }

    /// Like `new`, with the reference sphere centred `focus_shift` mm further along the image-space chief ray, e.g. at
    /// the paraxial focus when the image plane is elsewhere.
    pub fn with_focus_shift(context: &'c TraceContext<'a>, paraxial: &ParaxialData, field: f64, wavelength: f64, focus_shift: f64) -> Result<Option<Self>, LensError> {
        let system = context.system;
        let n = system.medium_indices(wavelength)?;
        let primary = system.primary_wavelength();
        let primary_chief = context.trace_ray(field, 0.0, 0.0, primary, &system.medium_indices(primary)?);
        let chief = context.trace_ray(field, 0.0, 0.0, wavelength, &n);
        if !primary_chief.ok() || !chief.ok() {
            return Ok(None);
        }
        let chief_dir = primary_chief.direction;
        let end = primary_chief.end();
        let centre = [end[0] + focus_shift * chief_dir[0], end[1] + focus_shift * chief_dir[1], end[2] + focus_shift * chief_dir[2]];
        let chief_start = exit_point(&primary_chief);
        let pupil_point = paraxial.exit_pupil_z.is_finite().then(|| {
            let t = (paraxial.exit_pupil_z - chief_start[2]) / chief_dir[2];
            [chief_start[0] + t * chief_dir[0], chief_start[1] + t * chief_dir[1], chief_start[2] + t * chief_dir[2]]
        });
        let (radius, upstream) = match pupil_point {
            Some(p) => {
                let v = [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
                (dot(v, v).sqrt(), dot(v, chief_dir) < 0.0)
            }
            None => (0.0, true),
        };
        let mut evaluator = OpdEvaluator { context, field, wavelength, n, centre, chief_dir, pupil_point, radius, upstream, reference: 0.0 };
        evaluator.reference = evaluator.path_to_reference(&chief);
        Ok(Some(evaluator))
    }

    fn path_to_reference(&self, ray: &RealRay) -> f64 {
        let image_index = self.n.last().unwrap().abs();
        let q = exit_point(ray);
        let d = ray.direction;
        let w = [q[0] - self.centre[0], q[1] - self.centre[1], q[2] - self.centre[2]];
        if self.pupil_point.is_none() {
            return ray.opl + image_index * -dot(w, self.chief_dir) / dot(d, self.chief_dir);
        }
        let b = dot(d, w);
        let c = dot(w, w) - self.radius * self.radius;
        let disc = b * b - c;
        if disc < 0.0 {
            return f64::NAN;
        }
        ray.opl + image_index * if self.upstream { -b - disc.sqrt() } else { -b + disc.sqrt() }
    }

    /// OPD in waves for normalized pupil coordinates, NaN when the ray fails.
    pub fn opd(&self, px: f64, py: f64) -> f64 {
        let ray = self.context.trace_ray(self.field, px, py, self.wavelength, &self.n);
        if !ray.ok() {
            return f64::NAN;
        }
        (self.reference - self.path_to_reference(&ray)) / (self.wavelength * 1e-3)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WavefrontMap {
    pub field: f64,
    pub wavelength: f64,
    pub size: usize,
    /// OPD in waves, row-major, row 0 at py = +1; NaN (null) outside the pupil or where rays fail.
    pub values: Vec<f64>,
    pub pv: f64,
    pub rms: f64,
}

pub fn wavefront(context: &TraceContext, paraxial: &ParaxialData, field: f64, wavelength: f64, size: usize) -> Result<WavefrontMap, LensError> {
    let mut values = vec![f64::NAN; size * size];
    let Some(opd) = OpdEvaluator::new(context, paraxial, field, wavelength)? else {
        return Ok(WavefrontMap { field, wavelength, size, values, pv: f64::NAN, rms: f64::NAN });
    };
    values.par_chunks_mut(size).enumerate().for_each(|(row, line)| {
        let py = 1.0 - 2.0 * (row as f64 + 0.5) / size as f64;
        for (col, value) in line.iter_mut().enumerate() {
            let px = -1.0 + 2.0 * (col as f64 + 0.5) / size as f64;
            if px * px + py * py <= 1.0 {
                *value = opd.opd(px, py);
            }
        }
    });
    let collected: Vec<f64> = values.iter().cloned().filter(|v| v.is_finite()).collect();
    let average = mean(collected.iter().cloned());
    let rms = mean(collected.iter().map(|v| (v - average) * (v - average))).sqrt();
    let pv = if collected.is_empty() {
        f64::NAN
    } else {
        collected.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - collected.iter().cloned().fold(f64::INFINITY, f64::min)
    };
    Ok(WavefrontMap { field, wavelength, size, values, pv, rms })
}

// ---------- FFT ----------

/// In-place iterative radix-2 FFT; inverse (unnormalized) when sign = +1.
pub fn fft(re: &mut [f64], im: &mut [f64], sign: f64) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let angle = sign * 2.0 * std::f64::consts::PI / length as f64;
        let (wi, wr) = angle.sin_cos();
        for start in (0..n).step_by(length) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..length / 2 {
                let (a, b) = (start + k, start + k + length / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        length <<= 1;
    }
}

/// 2D FFT of a size × size row-major grid: rows in parallel, then columns via a transpose.
pub fn fft2(re: &mut [f64], im: &mut [f64], size: usize, sign: f64) {
    let rows = |re: &mut [f64], im: &mut [f64]| {
        re.par_chunks_mut(size).zip(im.par_chunks_mut(size)).for_each(|(r, i)| fft(r, i, sign));
    };
    rows(re, im);
    transpose(re, size);
    transpose(im, size);
    rows(re, im);
    transpose(re, size);
    transpose(im, size);
}

fn transpose(data: &mut [f64], size: usize) {
    for row in 0..size {
        for col in row + 1..size {
            data.swap(row * size + col, col * size + row);
        }
    }
}

// ---------- MTF ----------

#[derive(Clone, Debug, Serialize)]
pub struct MtfField {
    pub field: f64,
    pub tangential: Vec<f64>,
    pub sagittal: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MtfResult {
    /// Cycles per mm.
    pub frequencies: Vec<f64>,
    pub cutoff: f64,
    pub fields: Vec<MtfField>,
    pub diffraction: Vec<f64>,
}

/// OTF modulus slices from the pupil autocorrelation: index k is a pupil shift of k grid samples.
fn otf_slices(map: &WavefrontMap) -> (Vec<f64>, Vec<f64>) {
    let size = map.size;
    let padded = size * 2;
    let mut re = vec![0.0; padded * padded];
    let mut im = vec![0.0; padded * padded];
    for row in 0..size {
        for col in 0..size {
            let opd = map.values[row * size + col];
            if opd.is_nan() {
                continue;
            }
            let (s, c) = (2.0 * std::f64::consts::PI * opd).sin_cos();
            re[row * padded + col] = c;
            im[row * padded + col] = s;
        }
    }
    fft2(&mut re, &mut im, padded, -1.0);
    for i in 0..re.len() {
        re[i] = re[i] * re[i] + im[i] * im[i];
        im[i] = 0.0;
    }
    fft2(&mut re, &mut im, padded, 1.0);
    let norm = re[0].hypot(im[0]);
    let norm = if norm > 0.0 { norm } else { 1.0 };
    let tangential = (0..=size).map(|k| re[k * padded].hypot(im[k * padded]) / norm).collect();
    let sagittal = (0..=size).map(|k| re[k].hypot(im[k]) / norm).collect();
    (tangential, sagittal)
}

fn diffraction_mtf(s: f64) -> f64 {
    if s >= 1.0 { 0.0 } else { 2.0 / std::f64::consts::PI * (s.acos() - s * (1.0 - s * s).sqrt()) }
}

fn interpolate(values: &[f64], position: f64) -> f64 {
    let last = values.len() - 1;
    if position >= last as f64 {
        return if position > last as f64 { 0.0 } else { values[last] };
    }
    let i = position.floor() as usize;
    let f = position - i as f64;
    values[i] * (1.0 - f) + values[i + 1] * f
}

/// Polychromatic diffraction MTF (equal wavelength weights) from FFT autocorrelation of the pupil function.
pub fn mtf(context: &TraceContext, paraxial: &ParaxialData, size: usize, points: usize, max_frequency: Option<f64>) -> Result<MtfResult, LensError> {
    let system = context.system;
    let cutoff_for = |wavelength: f64| 1.0 / (wavelength * 1e-3 * paraxial.working_f_number);
    let cutoff = system.wavelengths.iter().map(|&w| cutoff_for(w)).fold(0.0, f64::max);
    let top = max_frequency.filter(|&f| f > 0.0).unwrap_or(cutoff);
    let frequencies: Vec<f64> = (0..points).map(|i| top * i as f64 / (points - 1) as f64).collect();
    let count = system.wavelengths.len() as f64;
    let mut fields = Vec::new();
    for &field in &system.fields {
        let slices = system.wavelengths.par_iter()
            .map(|&w| wavefront(context, paraxial, field, w, size).map(|map| (w, otf_slices(&map))))
            .collect::<Result<Vec<_>, _>>()?;
        let sample = |tangential: bool| -> Vec<f64> {
            frequencies.iter().map(|&f| {
                slices.iter().map(|(w, (t, s))| interpolate(if tangential { t } else { s }, f / cutoff_for(*w) * size as f64)).sum::<f64>() / count
            }).collect()
        };
        fields.push(MtfField { field, tangential: sample(true), sagittal: sample(false) });
    }
    let diffraction = frequencies.iter().map(|&f| system.wavelengths.iter().map(|&w| diffraction_mtf(f / cutoff_for(w))).sum::<f64>() / count).collect();
    Ok(MtfResult { frequencies, cutoff, fields, diffraction })
}

/// Polychromatic tangential and sagittal MTF of one field at the given frequencies (cycles/mm).
pub fn mtf_at(context: &TraceContext, paraxial: &ParaxialData, field: f64, frequencies: &[f64], size: usize) -> Result<(Vec<f64>, Vec<f64>), LensError> {
    let system = context.system;
    let cutoff_for = |wavelength: f64| 1.0 / (wavelength * 1e-3 * paraxial.working_f_number);
    let slices = system.wavelengths.par_iter()
        .map(|&w| wavefront(context, paraxial, field, w, size).map(|map| (w, otf_slices(&map))))
        .collect::<Result<Vec<_>, _>>()?;
    let count = system.wavelengths.len() as f64;
    let sample = |tangential: bool| -> Vec<f64> {
        frequencies.iter().map(|&f| {
            slices.iter().map(|(w, (t, s))| interpolate(if tangential { t } else { s }, f / cutoff_for(*w) * size as f64)).sum::<f64>() / count
        }).collect()
    };
    Ok((sample(true), sample(false)))
}

#[derive(Clone, Debug, Serialize)]
pub struct ThroughFocus {
    pub frequency: f64,
    /// Image plane shifts, mm.
    pub shifts: Vec<f64>,
    /// Per field: tangential and sagittal MTF at each shift.
    pub fields: Vec<MtfField>,
}

/// MTF at one frequency as the image plane moves through focus.
pub fn through_focus_mtf(system: &LensSystem, frequency: f64, range: f64, steps: usize, size: usize) -> Result<ThroughFocus, LensError> {
    let shifts: Vec<f64> = (0..steps).map(|i| -range + 2.0 * range * i as f64 / (steps - 1) as f64).collect();
    let last = system.last();
    let per_shift = shifts.par_iter().map(|&dz| {
        let mut shifted = system.clone();
        // Thickness after an odd number of mirrors is negative; move the image along the light.
        let sign = if shifted.surfaces[last].thickness < 0.0 { -1.0 } else { 1.0 };
        shifted.surfaces[last].thickness += sign * dz;
        let paraxial = crate::paraxial::paraxial_data(&shifted)?;
        let context = TraceContext::new(&shifted, &paraxial);
        shifted.fields.iter().map(|&f| mtf_at(&context, &paraxial, f, &[frequency], size).map(|(t, s)| (t[0], s[0]))).collect::<Result<Vec<_>, _>>()
    }).collect::<Result<Vec<_>, LensError>>()?;
    let fields = system.fields.iter().enumerate().map(|(k, &field)| MtfField {
        field,
        tangential: per_shift.iter().map(|v| v[k].0).collect(),
        sagittal: per_shift.iter().map(|v| v[k].1).collect(),
    }).collect();
    Ok(ThroughFocus { frequency, shifts, fields })
}

#[derive(Clone, Debug, Serialize)]
pub struct MtfVsField {
    /// Field angles, degrees.
    pub angles: Vec<f64>,
    pub frequencies: Vec<f64>,
    /// Per frequency: tangential and sagittal MTF at each angle.
    pub curves: Vec<MtfField>,
}

pub fn mtf_vs_field(context: &TraceContext, paraxial: &ParaxialData, frequencies: &[f64], samples: usize, size: usize) -> Result<MtfVsField, LensError> {
    let angles = field_angles(context.system, samples);
    let values = angles.par_iter().map(|&a| mtf_at(context, paraxial, a, frequencies, size)).collect::<Result<Vec<_>, _>>()?;
    let curves = frequencies.iter().enumerate().map(|(k, &f)| MtfField {
        field: f,
        tangential: values.iter().map(|v| v.0[k]).collect(),
        sagittal: values.iter().map(|v| v.1[k]).collect(),
    }).collect();
    Ok(MtfVsField { angles, frequencies: frequencies.to_vec(), curves })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromaticFocalShift {
    /// µm.
    pub wavelengths: Vec<f64>,
    /// Paraxial focus position relative to the primary wavelength's, µm.
    pub shift: Vec<f64>,
    /// Largest minus smallest shift, µm.
    pub range: f64,
}

/// Paraxial marginal-ray focus versus wavelength over the system's band.
pub fn chromatic_focal_shift(system: &LensSystem, samples: usize) -> Result<ChromaticFocalShift, LensError> {
    let lo = system.wavelengths.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = system.wavelengths.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = if hi - lo < 1e-6 { (lo * 0.85, hi * 1.15) } else { (lo, hi) };
    let focus = |w: f64| -> Result<f64, LensError> {
        let n = system.medium_indices(w)?;
        let last = system.last();
        let ray = match system.object_distance {
            None => paraxial_trace(system, &n, 1.0, 0.0),
            Some(d) => paraxial_trace(system, &n, 0.0, 1.0 / d),
        };
        Ok(-ray.heights[last] / ray.angles[last])
    };
    let reference = focus(system.primary_wavelength())?;
    let wavelengths: Vec<f64> = (0..samples).map(|i| lo + (hi - lo) * i as f64 / (samples - 1) as f64).collect();
    let shift = wavelengths.iter().map(|&w| focus(w).map(|f| (f - reference) * 1000.0)).collect::<Result<Vec<_>, _>>()?;
    let range = shift.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - shift.iter().cloned().fold(f64::INFINITY, f64::min);
    Ok(ChromaticFocalShift { wavelengths, shift, range })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Footprint {
    pub surface: usize,
    /// Per field: (x, y) of each ray on the surface, in its own coordinates (mm).
    pub fields: Vec<Vec<[f64; 2]>>,
    pub semi_diameter: f64,
}

/// Where the beam of each field lands on a surface, at the primary wavelength.
pub fn footprint(context: &TraceContext, surface: usize, rings: usize, semi_diameter: f64) -> Result<Footprint, LensError> {
    let system = context.system;
    let wavelength = system.primary_wavelength();
    let n = system.medium_indices(wavelength)?;
    let pupil = hexapolar(rings);
    let fields = system.fields.iter().map(|&field| {
        pupil.iter().filter_map(|&[px, py]| {
            let ray = context.trace_ray(field, px, py, wavelength, &n);
            ray.local.get(surface).copied().filter(|_| ray.failure.is_none_or(|f| f.surface > surface))
        }).collect()
    }).collect();
    Ok(Footprint { surface, fields, semi_diameter })
}

// ---------- relative illumination ----------

#[derive(Clone, Debug, Serialize)]
pub struct Illumination {
    pub angles: Vec<f64>,
    /// Image irradiance relative to the axis (projected solid angle of the exit pupil).
    pub relative: Vec<f64>,
    /// Fraction of the pupil that reaches the image.
    pub unvignetted: Vec<f64>,
    pub cos4: Vec<f64>,
}

/// Relative illumination from radiance conservation: the projected solid angle of the exit pupil seen from the real
/// chief ray image point, measured as the area the pupil covers in direction-cosine space.
pub fn relative_illumination(context: &TraceContext, paraxial: &ParaxialData, samples: usize, rings: usize, spokes: usize) -> Result<Illumination, LensError> {
    let system = context.system;
    let angles = field_angles(system, samples);
    let wavelength = system.primary_wavelength();
    let n = system.medium_indices(wavelength)?;
    let exit_pupil_z = paraxial.exit_pupil_z;
    let tau = 2.0 * std::f64::consts::PI;
    let measure = |angle: f64| -> (f64, f64) {
        let chief = context.trace_ray(angle, 0.0, 0.0, wavelength, &n);
        if !chief.ok() {
            return (0.0, 0.0);
        }
        let image = chief.end();
        let cosines = |ray: &RealRay| -> [f64; 2] {
            if !exit_pupil_z.is_finite() {
                return [ray.direction[0], ray.direction[1]];
            }
            let q = exit_point(ray);
            let d = ray.direction;
            let t = (exit_pupil_z - q[2]) / d[2];
            let v = [q[0] + t * d[0] - image[0], q[1] + t * d[1] - image[1], q[2] + t * d[2] - image[2]];
            let length = dot(v, v).sqrt();
            [v[0] / length, v[1] / length]
        };
        let grid: Vec<Vec<Option<[f64; 2]>>> = (0..=rings).map(|i| {
            let r = i as f64 / rings as f64;
            (0..if i == 0 { 1 } else { spokes }).map(|j| {
                let phi = tau * j as f64 / spokes as f64;
                let ray = context.trace_ray(angle, r * phi.sin(), r * phi.cos(), wavelength, &n);
                ray.ok().then(|| cosines(&ray))
            }).collect()
        }).collect();
        let (mut area, mut open) = (0.0, 0.0);
        for i in 0..rings {
            for j in 0..spokes {
                let at = |ring: usize, spoke: usize| grid[ring][if ring == 0 { 0 } else { spoke % spokes }];
                let corners = [at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)];
                if corners.iter().any(Option::is_none) {
                    continue;
                }
                let c: Vec<[f64; 2]> = corners.iter().map(|c| c.unwrap()).collect();
                let twice: f64 = (0..4).map(|k| c[k][0] * c[(k + 1) % 4][1] - c[(k + 1) % 4][0] * c[k][1]).sum();
                area += twice.abs() / 2.0;
                let (r0, r1) = (i as f64 / rings as f64, (i + 1) as f64 / rings as f64);
                open += std::f64::consts::PI * (r1 * r1 - r0 * r0) / spokes as f64;
            }
        }
        (area, open / std::f64::consts::PI)
    };
    let results: Vec<(f64, f64)> = angles.par_iter().map(|&a| measure(a)).collect();
    let axis = if results[0].0 > 0.0 { results[0].0 } else { 1.0 };
    Ok(Illumination {
        relative: results.iter().map(|r| r.0 / axis).collect(),
        unvignetted: results.iter().map(|r| r.1).collect(),
        cos4: angles.iter().map(|a| a.to_radians().cos().powi(4)).collect(),
        angles,
    })
}

// ---------- PSF ----------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Psf {
    pub field: f64,
    /// Side length of `data` in samples (a centred crop of the padded FFT grid).
    pub size: usize,
    /// mm per sample at the primary wavelength.
    pub spacing: f64,
    /// Polychromatic intensity normalized to a peak of 1, row 0 at +y.
    pub data: Vec<f64>,
    pub strehl: f64,
    /// Encircled energy against radius (mm), for the system and the diffraction limit.
    pub radius: Vec<f64>,
    pub energy: Vec<f64>,
    pub diffraction: Vec<f64>,
}

fn pupil_intensity(map: &WavefrontMap, padded: usize, perfect: bool) -> Vec<f64> {
    let size = map.size;
    let mut re = vec![0.0; padded * padded];
    let mut im = vec![0.0; padded * padded];
    for row in 0..size {
        for col in 0..size {
            let py = 1.0 - 2.0 * (row as f64 + 0.5) / size as f64;
            let px = -1.0 + 2.0 * (col as f64 + 0.5) / size as f64;
            let opd = map.values[row * size + col];
            if if perfect { px * px + py * py > 1.0 } else { opd.is_nan() } {
                continue;
            }
            let phase = if perfect { 0.0 } else { 2.0 * std::f64::consts::PI * opd };
            re[row * padded + col] = phase.cos();
            im[row * padded + col] = phase.sin();
        }
    }
    fft2(&mut re, &mut im, padded, -1.0);
    let half = padded / 2;
    let mut out = vec![0.0; padded * padded];
    for row in 0..padded {
        for col in 0..padded {
            let i = row * padded + col;
            out[((row + half) % padded) * padded + (col + half) % padded] = re[i] * re[i] + im[i] * im[i];
        }
    }
    out
}

fn sample_bilinear(data: &[f64], size: usize, x: f64, y: f64) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    if x0 < 0.0 || y0 < 0.0 || x0 >= (size - 1) as f64 || y0 >= (size - 1) as f64 {
        return 0.0;
    }
    let (fx, fy) = (x - x0, y - y0);
    let i = y0 as usize * size + x0 as usize;
    data[i] * (1.0 - fx) * (1.0 - fy) + data[i + 1] * fx * (1.0 - fy) + data[i + size] * (1.0 - fx) * fy + data[i + size + 1] * fx * fy
}

/// Polychromatic FFT PSF: each wavelength is resampled onto the primary wavelength's grid with equal energy.
pub fn psf(context: &TraceContext, paraxial: &ParaxialData, field: f64, pupil_samples: usize, padding: usize, crop: usize) -> Result<Psf, LensError> {
    let system = context.system;
    let padded = pupil_samples * padding;
    let half = padded / 2;
    let primary = system.primary_wavelength();
    let spacing_for = |w: f64| w * 1e-3 * paraxial.working_f_number * pupil_samples as f64 / padded as f64;
    let spacing = spacing_for(primary);
    let count = system.wavelengths.len() as f64;
    let per_wavelength = system.wavelengths.par_iter().map(|&wavelength| -> Result<(Vec<f64>, Vec<f64>), LensError> {
        let map = wavefront(context, paraxial, field, wavelength, pupil_samples)?;
        let scale = spacing / spacing_for(wavelength);
        let resample = |perfect: bool| {
            let intensity = pupil_intensity(&map, padded, perfect);
            let mut out = vec![0.0; padded * padded];
            for row in 0..padded {
                for col in 0..padded {
                    out[row * padded + col] = sample_bilinear(&intensity, padded, (col as f64 - half as f64) * scale + half as f64, (row as f64 - half as f64) * scale + half as f64);
                }
            }
            let total: f64 = out.iter().sum();
            if total > 0.0 {
                out.iter_mut().for_each(|v| *v /= total * count);
            }
            out
        };
        Ok((resample(false), resample(true)))
    }).collect::<Result<Vec<_>, _>>()?;
    let mut data = vec![0.0; padded * padded];
    let mut ideal = vec![0.0; padded * padded];
    for (real, perfect) in &per_wavelength {
        data.iter_mut().zip(real).for_each(|(a, b)| *a += b);
        ideal.iter_mut().zip(perfect).for_each(|(a, b)| *a += b);
    }
    let peak = |values: &[f64]| values.iter().cloned().fold(0.0, f64::max);
    let ideal_peak = peak(&ideal);
    let strehl = peak(&data) / if ideal_peak > 0.0 { ideal_peak } else { 1.0 };

    let bins = half;
    let encircle = |values: &[f64]| -> Vec<f64> {
        let mut ring = vec![0.0; bins + 1];
        for row in 0..padded {
            for col in 0..padded {
                let r = (col as f64 - half as f64).hypot(row as f64 - half as f64);
                if r <= bins as f64 {
                    ring[(r.ceil() as usize).min(bins)] += values[row * padded + col];
                }
            }
        }
        let mut sum = 0.0;
        ring.into_iter().map(|v| { sum += v; sum }).collect()
    };
    let energy = encircle(&data);
    let diffraction = encircle(&ideal);

    let crop = crop.min(padded);
    let start = half - crop / 2;
    let top = peak(&data);
    let top = if top > 0.0 { top } else { 1.0 };
    let mut cropped = Vec::with_capacity(crop * crop);
    for row in start..start + crop {
        cropped.extend(data[row * padded + start..row * padded + start + crop].iter().map(|v| v / top));
    }
    Ok(Psf {
        field,
        size: crop,
        spacing,
        data: cropped,
        strehl,
        radius: (0..=bins).map(|i| i as f64 * spacing).collect(),
        energy,
        diffraction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_round_trip() {
        let mut re: Vec<f64> = (0..16).map(|i| (i as f64 * 0.7).sin()).collect();
        let original = re.clone();
        let mut im = vec![0.0; 16];
        fft(&mut re, &mut im, -1.0);
        fft(&mut re, &mut im, 1.0);
        for (a, b) in re.iter().zip(&original) {
            assert!((a / 16.0 - b).abs() < 1e-12);
        }
    }

    #[test]
    fn hexapolar_counts() {
        assert_eq!(hexapolar(3).len(), 1 + 6 + 12 + 18);
    }
}
