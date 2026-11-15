//! Damped least-squares (Levenberg–Marquardt) optimization. Each call to `iterate` performs one LM step so the
//! caller can report progress and stop between steps.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::{OpdEvaluator, hexapolar};
use crate::paraxial::paraxial_data;
use crate::surface::sag;
use crate::configs::{self, Parameter};
use crate::paraxial::{resolved_config, trace as paraxial_trace};
use crate::system::{LensSystem, Objective, Operand, OperandKind, Relation};
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
        if v.radius && !configs::is_locked(system, surface, Parameter::Radius) {
            list.push(Variable::Radius { surface });
        }
        if v.thickness && !configs::is_locked(system, surface, Parameter::Thickness) {
            list.push(Variable::Thickness { surface });
        }
        if v.conic && !configs::is_locked(system, surface, Parameter::Conic) {
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
    if system.validate().is_err() {
        return vec![f64::INFINITY];
    }
    let mut out = Vec::new();
    for configuration in 0..configs::configuration_count(system) {
        match config_residuals(system, configuration) {
            Some(part) => out.extend(part),
            None => return vec![f64::INFINITY],
        }
    }
    out
}

/// Real-ray quantities an operand may need, traced once per configuration.
struct OperandContext<'a> {
    system: &'a LensSystem,
    paraxial: &'a crate::paraxial::ParaxialData,
    context: &'a TraceContext<'a>,
    indices: &'a [f64],
    pupil: &'a [[f64; 2]],
}

/// The value of an operand's quantity for one field (or the system), or None when it cannot be evaluated.
fn operand_quantity(c: &OperandContext, operand: &Operand, field: Option<usize>) -> Option<f64> {
    let (system, paraxial) = (c.system, c.paraxial);
    let primary = system.primary_wavelength();
    let angle = field.and_then(|f| system.fields.get(f).copied());
    Some(match operand.kind {
        OperandKind::Efl => paraxial.efl,
        OperandKind::TotalTrack => paraxial.total_track,
        OperandKind::BackFocus => paraxial.bfl,
        OperandKind::FNumber => paraxial.f_number,
        OperandKind::ImageHeight => paraxial.image_height,
        OperandKind::Thickness => system.surfaces.get(operand.surface?)?.thickness,
        OperandKind::Radius => system.surfaces.get(operand.surface?)?.radius,
        OperandKind::ChiefRayAngle => {
            let chief = c.context.trace_ray(angle?, 0.0, 0.0, primary, c.indices);
            if !chief.ok() { return None; }
            chief.direction[2].abs().min(1.0).acos().to_degrees()
        }
        OperandKind::Distortion => {
            let angle = angle?;
            if angle == 0.0 { return Some(0.0); }
            let slope = angle.to_radians().tan();
            let last = system.last();
            let chief = paraxial_trace(system, c.indices, -paraxial.entrance_pupil_z * slope, slope);
            let reference = chief.heights[last] + chief.angles[last] * system.surfaces[last].thickness;
            let real = c.context.trace_ray(angle, 0.0, 0.0, primary, c.indices);
            if !real.ok() || reference == 0.0 { return None; }
            100.0 * (real.end()[1] - reference) / reference
        }
        OperandKind::SpotRadius => {
            let angle = angle?;
            let hits: Vec<[f64; 2]> = c.pupil.iter().filter_map(|&[px, py]| {
                let ray = c.context.trace_ray(angle, px, py, primary, c.indices);
                ray.ok().then(|| [ray.end()[0], ray.end()[1]])
            }).collect();
            if hits.is_empty() { return None; }
            let n = hits.len() as f64;
            let (cx, cy) = (hits.iter().map(|h| h[0]).sum::<f64>() / n, hits.iter().map(|h| h[1]).sum::<f64>() / n);
            (hits.iter().map(|h| (h[0] - cx).powi(2) + (h[1] - cy).powi(2)).sum::<f64>() / n).sqrt()
        }
    })
}

/// Residual scale for each kind so that typical errors are comparable to the image-quality terms.
fn operand_scale(kind: OperandKind, target: f64) -> f64 {
    let relative = |t: f64| 5.0 / t.abs().max(1e-6);
    match kind {
        OperandKind::Efl | OperandKind::FNumber => relative(target),
        OperandKind::ImageHeight => relative(target),
        OperandKind::TotalTrack | OperandKind::BackFocus | OperandKind::Thickness => PENALTY,
        OperandKind::Radius => 1.0,
        OperandKind::ChiefRayAngle => CRA_WEIGHT,
        OperandKind::Distortion => 0.05,
        OperandKind::SpotRadius => 1.0,
    }
}

fn operand_fields(system: &LensSystem, operand: &Operand) -> Vec<Option<usize>> {
    match operand.kind {
        OperandKind::ChiefRayAngle | OperandKind::Distortion | OperandKind::SpotRadius => match operand.field {
            Some(f) => vec![Some(f)],
            None => (0..system.fields.len()).map(Some).collect(),
        },
        _ => vec![None],
    }
}

/// Residuals of one configuration.
fn config_residuals(system: &LensSystem, configuration: usize) -> Option<Vec<f64>> {
    let Ok(system) = &resolved_config(system, configuration) else { return None };
    let Ok(paraxial) = paraxial_data(system) else { return None };
    if !paraxial.efl.is_finite() || !paraxial.entrance_pupil_z.is_finite() {
        return None;
    }
    let settings = &system.optimization;
    let context = TraceContext::new(system, &paraxial);
    let pupil = hexapolar(settings.rings.unwrap_or(3).clamp(1, 12));
    let Ok(indices) = system.wavelengths.iter().map(|&w| system.medium_indices(w)).collect::<Result<Vec<_>, _>>() else { return None };
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
        Objective::None => {}
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

    if !settings.operands.is_empty() {
        let primary_indices = &indices[system.primary_wavelength.min(indices.len() - 1)];
        let operand_context = OperandContext { system, paraxial: &paraxial, context: &context, indices: primary_indices, pupil: &pupil };
        for operand in settings.operands.iter().filter(|o| o.config.is_none_or(|c| c == configuration)) {
            for field in operand_fields(system, operand) {
                let scale = operand.weight * operand_scale(operand.kind, operand.target);
                out.push(match operand_quantity(&operand_context, operand, field) {
                    Some(value) if value.is_finite() => scale * match operand.relation {
                        Relation::Equal => value - operand.target,
                        Relation::AtMost => (value - operand.target).max(0.0),
                        Relation::AtLeast => (operand.target - value).max(0.0),
                    },
                    _ => PENALTY * operand.weight.abs().max(1.0),
                });
            }
        }
    }

    let Ok(semi) = automatic_semi_diameters(&context) else { return None };
    let (min_glass, min_edge, min_air) = (settings.min_glass_center.unwrap_or(0.5), settings.min_glass_edge.unwrap_or(0.3), settings.min_air.unwrap_or(0.0));
    for (i, surface) in system.surfaces.iter().enumerate() {
        let Some(next) = system.surfaces.get(i + 1) else {
            out.push(if surface.thickness < 0.0 { PENALTY * -surface.thickness } else { 0.0 });
            continue;
        };
        if surface.coordinate_break.is_some() || next.coordinate_break.is_some() {
            out.extend([0.0, 0.0]);
            continue;
        }
        let glass = surface.is_glass();
        // Thicknesses after an odd number of mirrors are negative; measure them along the light.
        let sign = indices[0][i + 1].signum();
        let thickness = sign * surface.thickness;
        let centre_min = if glass { min_glass } else { min_air };
        out.push(if thickness < centre_min { PENALTY * (centre_min - thickness) } else { 0.0 });
        let h = semi[i].max(semi[i + 1]);
        let edge = thickness + sign * (sag(next, h.min(semi[i + 1])) - sag(surface, h.min(semi[i])));
        let edge_min = if glass { min_edge } else { min_air };
        out.push(if edge.is_finite() && edge < edge_min { PENALTY * (edge_min - edge) } else { 0.0 });
    }
    Some(out)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeritReport {
    pub merit: f64,
    /// Current value of each operand in the active configuration; the field furthest from the target for
    /// all-field operands; None when it cannot be evaluated or does not apply to the configuration.
    pub values: Vec<Option<f64>>,
}

pub fn merit_report(system: &LensSystem) -> MeritReport {
    let configuration = configs::active_configuration(system);
    let mut values = vec![None; system.optimization.operands.len()];
    if let Ok(resolved) = resolved_config(system, configuration) {
        if let Ok(paraxial) = paraxial_data(&resolved) {
            if let Ok(indices) = resolved.medium_indices(resolved.primary_wavelength()) {
                let context = TraceContext::new(&resolved, &paraxial);
                let pupil = hexapolar(resolved.optimization.rings.unwrap_or(3).clamp(1, 12));
                let c = OperandContext { system: &resolved, paraxial: &paraxial, context: &context, indices: &indices, pupil: &pupil };
                for (slot, operand) in values.iter_mut().zip(&resolved.optimization.operands) {
                    if operand.config.is_some_and(|k| k != configuration) {
                        continue;
                    }
                    *slot = operand_fields(&resolved, operand).into_iter()
                        .filter_map(|f| operand_quantity(&c, operand, f))
                        .filter(|v| v.is_finite())
                        .max_by(|a, b| (a - operand.target).abs().total_cmp(&(b - operand.target).abs()));
                }
            }
        }
    }
    MeritReport { merit: merit(system), values }
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
