//! Damped least-squares (Levenberg–Marquardt) optimization. Each call to `iterate` performs one LM step so the
//! caller can report progress and stop between steps.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{OpdEvaluator, hexapolar};
use crate::glass::is_air;
use crate::paraxial::paraxial_data;
use crate::surface::sag;
use crate::system::{LensSystem, Objective};
use crate::trace::{TraceContext, automatic_semi_diameters};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Variable {
    Radius { surface: usize },
    Thickness { surface: usize },
    Conic { surface: usize },
    /// Even asphere coefficient A(2·term + 4).
    Aspheric { surface: usize, term: usize },
}

pub fn list_variables(system: &LensSystem) -> Vec<Variable> {
    let mut list = Vec::new();
    for (surface, s) in system.surfaces.iter().enumerate() {
        let Some(v) = &s.variable else { continue };
        if v.radius {
            list.push(Variable::Radius { surface });
        }
        if v.thickness {
            list.push(Variable::Thickness { surface });
        }
        if v.conic {
            list.push(Variable::Conic { surface });
        }
        for (term, &on) in v.aspheric.iter().enumerate() {
            if on {
                list.push(Variable::Aspheric { surface, term });
            }
        }
    }
    list
}

/// Aspheric coefficients are optimized as their sag contribution at this height, so all terms have similar scale.
fn asphere_norm(system: &LensSystem) -> f64 {
    (system.entrance_pupil_diameter / 2.0).max(1.0)
}

/// Optimizer coordinates: curvature for radii (smooth through flat), sag-normalized aspheric terms.
fn read(system: &LensSystem, variable: Variable) -> f64 {
    match variable {
        Variable::Radius { surface } => system.surfaces[surface].curvature(),
        Variable::Thickness { surface } => system.surfaces[surface].thickness,
        Variable::Conic { surface } => system.surfaces[surface].conic,
        Variable::Aspheric { surface, term } => {
            system.surfaces[surface].aspheric.get(term).copied().unwrap_or(0.0) * asphere_norm(system).powi(2 * term as i32 + 4)
        }
    }
}

fn write(system: &LensSystem, variables: &[Variable], values: &[f64]) -> LensSystem {
    let mut out = system.clone();
    let norm = asphere_norm(system);
    for (variable, &value) in variables.iter().zip(values) {
        match *variable {
            Variable::Radius { surface } => out.surfaces[surface].radius = if value.abs() < 1e-12 { 0.0 } else { 1.0 / value },
            Variable::Thickness { surface } => out.surfaces[surface].thickness = value,
            Variable::Conic { surface } => out.surfaces[surface].conic = value,
            Variable::Aspheric { surface, term } => {
                let coefficients = &mut out.surfaces[surface].aspheric;
                if coefficients.len() <= term {
                    coefficients.resize(term + 1, 0.0);
                }
                coefficients[term] = value / norm.powi(2 * term as i32 + 4);
            }
        }
    }
    out
}

fn step(variable: Variable, value: f64) -> f64 {
    match variable {
        Variable::Radius { .. } => 1e-6,
        Variable::Thickness { .. } => 1e-5 * value.abs().max(1.0),
        Variable::Conic { .. } => 1e-5 * value.abs().max(1.0),
        Variable::Aspheric { .. } => 1e-6,
    }
}

const LOST_RAY: f64 = 1.0; // mm residual per coordinate for a ray that fails
const LOST_WAVES: f64 = 10.0; // waves residual for a ray that fails in the wavefront objective
const PENALTY: f64 = 10.0; // per mm of violated thickness / track constraint
const EFL_WEIGHT: f64 = 5.0; // 1 % EFL error costs as much as 50 µm of RMS spot
const CRA_WEIGHT: f64 = 0.1; // per degree above the chief ray angle limit

/// Residuals whose root-sum-square is the merit: the image-quality objective plus focal length and constraint terms.
pub fn residuals(system: &LensSystem) -> Vec<f64> {
    let failed = vec![f64::INFINITY];
    if system.validate().is_err() {
        return failed;
    }
    let Ok(paraxial) = paraxial_data(system) else { return failed };
    if !paraxial.efl.is_finite() || !paraxial.entrance_pupil_z.is_finite() {
        return failed;
    }
    let settings = &system.optimization;
    let context = TraceContext::new(system, &paraxial);
    let pupil = hexapolar(settings.rings.unwrap_or(3).clamp(1, 12));
    let Ok(indices) = system.wavelengths.iter().map(|&w| system.medium_indices(w)).collect::<Result<Vec<_>, _>>() else { return failed };
    let weight = 1.0 / ((system.fields.len() * system.wavelengths.len() * pupil.len()) as f64).sqrt();
    let mut out = Vec::new();

    match settings.objective {
        Objective::Wavefront => {
            for &field in &system.fields {
                for &wavelength in &system.wavelengths {
                    let evaluator = OpdEvaluator::new(&context, &paraxial, field, wavelength).ok().flatten();
                    let values: Vec<f64> = pupil.iter().map(|&[px, py]| evaluator.as_ref().map_or(f64::NAN, |e| e.opd(px, py))).collect();
                    let valid: Vec<f64> = values.iter().cloned().filter(|v| v.is_finite()).collect();
                    let mean = if valid.is_empty() { 0.0 } else { valid.iter().sum::<f64>() / valid.len() as f64 };
                    out.extend(values.iter().map(|v| if v.is_finite() { (v - mean) * weight } else { LOST_WAVES * weight }));
                }
            }
        }
        Objective::Spot => {
            for &field in &system.fields {
                let mut hits = Vec::with_capacity(pupil.len() * system.wavelengths.len());
                for (w, &wavelength) in system.wavelengths.iter().enumerate() {
                    for &[px, py] in &pupil {
                        let ray = context.trace_ray(field, px, py, wavelength, &indices[w]);
                        hits.push(ray.ok().then(|| [ray.end()[0], ray.end()[1]]));
                    }
                }
                let valid: Vec<[f64; 2]> = hits.iter().flatten().copied().collect();
                let count = valid.len().max(1) as f64;
                let cx = valid.iter().map(|h| h[0]).sum::<f64>() / count;
                let cy = valid.iter().map(|h| h[1]).sum::<f64>() / count;
                for hit in hits {
                    match hit {
                        Some([x, y]) => out.extend([(x - cx) * weight, (y - cy) * weight]),
                        None => out.extend([LOST_RAY * weight, LOST_RAY * weight]),
                    }
                }
            }
        }
    }

    if let Some(target) = system.target_efl.filter(|t| *t != 0.0) {
        out.push(EFL_WEIGHT * (paraxial.efl - target) / target);
    }
    let excess = |value: f64, limit: Option<f64>| limit.map_or(0.0, |l| (value - l).max(0.0));
    let last = system.last();
    out.push(PENALTY * excess(paraxial.total_track, settings.max_total_track));
    out.push(PENALTY * settings.min_back_focus.map_or(0.0, |min| (min - system.surfaces[last].thickness).max(0.0)));
    if settings.max_chief_ray_angle.is_some() {
        let primary = system.primary_wavelength();
        let n = &indices[system.primary_wavelength.min(indices.len() - 1)];
        for &field in &system.fields {
            let chief = context.trace_ray(field, 0.0, 0.0, primary, n);
            let angle = if chief.ok() { chief.direction[2].abs().min(1.0).acos().to_degrees() } else { 90.0 };
            out.push(CRA_WEIGHT * excess(angle, settings.max_chief_ray_angle));
        }
    }

    let Ok(semi) = automatic_semi_diameters(&context) else { return failed };
    let (min_glass, min_edge, min_air) = (settings.min_glass_center.unwrap_or(0.5), settings.min_glass_edge.unwrap_or(0.3), settings.min_air.unwrap_or(0.0));
    for (i, surface) in system.surfaces.iter().enumerate() {
        let Some(next) = system.surfaces.get(i + 1) else {
            out.push(if surface.thickness < 0.0 { PENALTY * -surface.thickness } else { 0.0 });
            continue;
        };
        let glass = !is_air(&surface.material);
        let centre_min = if glass { min_glass } else { min_air };
        out.push(if surface.thickness < centre_min { PENALTY * (centre_min - surface.thickness) } else { 0.0 });
        let h = semi[i].max(semi[i + 1]);
        let edge = surface.thickness + sag(next, h.min(semi[i + 1])) - sag(surface, h.min(semi[i]));
        let edge_min = if glass { min_edge } else { min_air };
        out.push(if edge.is_finite() && edge < edge_min { PENALTY * (edge_min - edge) } else { 0.0 });
    }
    out
}

pub fn merit_of(residuals: &[f64]) -> f64 {
    residuals.iter().map(|r| r * r).sum::<f64>().sqrt()
}

pub fn merit(system: &LensSystem) -> f64 {
    merit_of(&residuals(system))
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let sum: f64 = b[row] - (row + 1..n).map(|k| a[row][k] * x[k]).sum::<f64>();
        x[row] = sum / a[row][row];
    }
    Some(x)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub system: LensSystem,
    pub merit: f64,
    /// Merit before this step.
    pub previous: f64,
    /// Levenberg–Marquardt damping to pass to the next step.
    pub damping: f64,
    pub improved: bool,
    pub variables: usize,
}

/// One damped least-squares step from `system`. When no step lowers the merit, `improved` is false and the
/// system is returned unchanged.
pub fn iterate(system: &LensSystem, damping: f64) -> Iteration {
    let variables = list_variables(system);
    let x: Vec<f64> = variables.iter().map(|&v| read(system, v)).collect();
    let r = residuals(system);
    let merit = merit_of(&r);
    let unchanged = |damping: f64| Iteration { system: system.clone(), merit, previous: merit, damping, improved: false, variables: variables.len() };
    if variables.is_empty() || !merit.is_finite() {
        return unchanged(damping);
    }
    let jacobian: Vec<Vec<f64>> = variables.par_iter().enumerate().map(|(j, &variable)| {
        let h = step(variable, x[j]);
        let mut shifted = x.clone();
        shifted[j] += h;
        let r_shift = residuals(&write(system, &variables, &shifted));
        r.iter().enumerate().map(|(i, &value)| {
            if r_shift.len() == r.len() && r_shift[i].is_finite() && value.is_finite() { (r_shift[i] - value) / h } else { 0.0 }
        }).collect()
    }).collect();
    let size = variables.len();
    let normal: Vec<Vec<f64>> = (0..size).map(|a| (0..size).map(|b| jacobian[a].iter().zip(&jacobian[b]).map(|(p, q)| p * q).sum()).collect()).collect();
    let gradient: Vec<f64> = jacobian.iter().map(|column| column.iter().zip(&r).map(|(j, ri)| j * ri).sum()).collect();

    let mut damping = damping.max(1e-9);
    for _ in 0..10 {
        let damped: Vec<Vec<f64>> = normal.iter().enumerate()
            .map(|(a, row)| row.iter().enumerate().map(|(b, &v)| if a == b { v + damping * v.max(1e-12) } else { v }).collect())
            .collect();
        let Some(delta) = solve(damped, gradient.iter().map(|g| -g).collect()) else {
            damping *= 10.0;
            continue;
        };
        let candidate: Vec<f64> = x.iter().zip(&delta).map(|(a, b)| a + b).collect();
        let next = write(system, &variables, &candidate);
        let next_merit = merit_of(&residuals(&next));
        if next_merit.is_finite() && next_merit < merit {
            return Iteration { system: next, merit: next_merit, previous: merit, damping: (damping * 0.3).max(1e-9), improved: true, variables: size };
        }
        damping *= 6.0;
    }
    unchanged(damping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::{Variables, achromat};

    #[test]
    fn recovers_a_perturbed_doublet() {
        let mut lens = achromat();
        let original = merit(&lens);
        lens.surfaces[0].radius *= 1.04;
        lens.surfaces[2].radius *= 0.9;
        for i in [0, 1, 2] {
            lens.surfaces[i].variable = Some(Variables { radius: true, ..Default::default() });
        }
        lens.target_efl = Some(100.0);
        let start = merit(&lens);
        let mut damping = 1e-3;
        for _ in 0..30 {
            let step = iterate(&lens, damping);
            if !step.improved {
                break;
            }
            lens = step.system;
            damping = step.damping;
        }
        let end = merit(&lens);
        assert!(end < start * 0.5 && end < original * 2.0, "start {start} end {end} original {original}");
    }
}
