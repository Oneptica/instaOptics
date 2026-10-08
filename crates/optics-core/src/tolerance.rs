//! Tolerance analysis: per-parameter sensitivity and Monte Carlo, with an optional back-focus compensator.
//! The criterion is the polychromatic RMS spot radius about each field's centroid, RMS-averaged over ±fields.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analysis::hexapolar;
use crate::glass::is_air;
use crate::paraxial::paraxial_data;
use crate::system::{GlassOffset, LensSystem};
use crate::trace::TraceContext;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Compensator {
    #[default]
    Focus,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToleranceSettings {
    /// ± percent of each radius.
    pub radius: f64,
    /// ± mm on every thickness except the image distance.
    pub thickness: f64,
    /// ± mm per surface.
    pub decenter: f64,
    /// ± arcmin per surface.
    pub tilt: f64,
    /// ± nd per glass.
    pub index: f64,
    /// ± percent of vd per glass.
    pub abbe: f64,
    #[serde(default)]
    pub compensator: Compensator,
    #[serde(default = "default_trials")]
    pub trials: usize,
    #[serde(default = "default_seed")]
    pub seed: u32,
}

fn default_trials() -> usize {
    200
}

fn default_seed() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParameterKind {
    Radius,
    Thickness,
    Decenter,
    Tilt,
    Index,
    Abbe,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Parameter {
    pub surface: usize,
    pub kind: ParameterKind,
    pub tolerance: f64,
}

/// Every toleranced parameter. Decenter and tilt act in y / about x.
pub fn parameters(system: &LensSystem, settings: &ToleranceSettings) -> Vec<Parameter> {
    let mut list = Vec::new();
    let last = system.last();
    for (i, surface) in system.surfaces.iter().enumerate() {
        let glass_before = i > 0 && !is_air(&system.surfaces[i - 1].material);
        let optical = surface.radius != 0.0 || !is_air(&surface.material) || glass_before;
        let mut push = |kind, tolerance: f64, when: bool| {
            if when && tolerance > 0.0 {
                list.push(Parameter { surface: i, kind, tolerance });
            }
        };
        push(ParameterKind::Radius, settings.radius, surface.radius != 0.0);
        push(ParameterKind::Thickness, settings.thickness, i < last);
        push(ParameterKind::Decenter, settings.decenter, optical);
        push(ParameterKind::Tilt, settings.tilt, optical);
        push(ParameterKind::Index, settings.index, !is_air(&surface.material));
        push(ParameterKind::Abbe, settings.abbe, !is_air(&surface.material));
    }
    list
}

pub fn perturb(system: &LensSystem, perturbations: &[(Parameter, f64)]) -> LensSystem {
    let mut out = system.clone();
    for &(parameter, value) in perturbations {
        let surface = &mut out.surfaces[parameter.surface];
        match parameter.kind {
            ParameterKind::Radius => surface.radius *= 1.0 + value / 100.0,
            ParameterKind::Thickness => surface.thickness += value,
            ParameterKind::Decenter => {
                let [x, y] = surface.decenter.unwrap_or([0.0, 0.0]);
                surface.decenter = Some([x, y + value]);
            }
            ParameterKind::Tilt => {
                let [x, y] = surface.tilt.unwrap_or([0.0, 0.0]);
                surface.tilt = Some([x + value / 60.0, y]);
            }
            ParameterKind::Index => {
                let offset = surface.glass_offset.unwrap_or_default();
                surface.glass_offset = Some(GlassOffset { index: offset.index + value, ..offset });
            }
            ParameterKind::Abbe => {
                let offset = surface.glass_offset.unwrap_or_default();
                surface.glass_offset = Some(GlassOffset { abbe: offset.abbe + value / 100.0, ..offset });
            }
        }
    }
    out
}

/// RMS spot radius (mm) over ±fields and all wavelengths, each field about its own centroid. With `refocus`, the
/// image-plane shift that minimizes it is solved in closed form: landing points move linearly with the shift, so the
/// mean square radius is quadratic in it.
pub fn criterion(system: &LensSystem, refocus: bool) -> f64 {
    let Ok(paraxial) = paraxial_data(system) else { return f64::INFINITY };
    let context = TraceContext::new(system, &paraxial);
    let pupil = hexapolar(4);
    let mut fields: Vec<f64> = system.fields.iter().flat_map(|&f| if f == 0.0 { vec![0.0] } else { vec![f, -f] }).collect();
    fields.sort_by(f64::total_cmp);
    fields.dedup();
    let Ok(indices) = system.wavelengths.iter().map(|&w| system.medium_indices(w)).collect::<Result<Vec<_>, _>>() else { return f64::INFINITY };
    let (mut rr, mut ra, mut aa) = (0.0, 0.0, 0.0);
    for &field in &fields {
        let mut hits: Vec<[f64; 4]> = Vec::new();
        let mut lost = 0;
        for (w, &wavelength) in system.wavelengths.iter().enumerate() {
            for &[px, py] in &pupil {
                let ray = context.trace_ray(field, px, py, wavelength, &indices[w]);
                if !ray.ok() {
                    lost += 1;
                    continue;
                }
                let (end, d) = (ray.end(), ray.direction);
                hits.push([end[0], end[1], d[0] / d[2], d[1] / d[2]]);
            }
        }
        if hits.is_empty() || lost > hits.len() {
            return f64::INFINITY;
        }
        let count = hits.len() as f64;
        let mean: Vec<f64> = (0..4).map(|k| hits.iter().map(|h| h[k]).sum::<f64>() / count).collect();
        let (mut fr, mut fra, mut faa) = (0.0, 0.0, 0.0);
        for h in &hits {
            let (x, y, ax, ay) = (h[0] - mean[0], h[1] - mean[1], h[2] - mean[2], h[3] - mean[3]);
            fr += x * x + y * y;
            fra += x * ax + y * ay;
            faa += ax * ax + ay * ay;
        }
        rr += fr / count;
        ra += fra / count;
        aa += faa / count;
    }
    let shift = if refocus && aa > 0.0 { -ra / aa } else { 0.0 };
    ((rr + 2.0 * ra * shift + aa * shift * shift) / fields.len() as f64).max(0.0).sqrt()
}

fn evaluate(system: &LensSystem, settings: &ToleranceSettings) -> f64 {
    criterion(system, settings.compensator == Compensator::Focus)
}

#[derive(Clone, Debug, Serialize)]
pub struct SensitivityRow {
    pub parameter: Parameter,
    /// Change of the criterion (mm) at +tolerance and −tolerance.
    pub plus: f64,
    pub minus: f64,
    pub worst: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Sensitivity {
    pub nominal: f64,
    /// Sorted by worst change, largest first.
    pub rows: Vec<SensitivityRow>,
    /// Nominal plus the root-sum-square of the worst increases.
    pub estimated: f64,
}

pub fn sensitivity(system: &LensSystem, settings: &ToleranceSettings) -> Sensitivity {
    let nominal = evaluate(system, settings);
    let mut rows: Vec<SensitivityRow> = parameters(system, settings).into_par_iter().map(|parameter| {
        let plus = evaluate(&perturb(system, &[(parameter, parameter.tolerance)]), settings);
        let minus = evaluate(&perturb(system, &[(parameter, -parameter.tolerance)]), settings);
        SensitivityRow { parameter, plus: plus - nominal, minus: minus - nominal, worst: plus.max(minus) - nominal }
    }).collect();
    rows.sort_by(|a, b| b.worst.total_cmp(&a.worst));
    let estimated = nominal + rows.iter().map(|r| r.worst.max(0.0).powi(2)).sum::<f64>().sqrt();
    Sensitivity { nominal, rows, estimated }
}

/// mulberry32: small, fast and reproducible.
struct Random(u32);

impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }

    fn gaussian(&mut self) -> f64 {
        let mut u = 0.0;
        while u == 0.0 {
            u = self.next();
        }
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * self.next()).cos()
    }
}

/// Criterion values for trials `start..start + count`. Each trial has its own random stream (seed, trial), so results
/// do not depend on how the run is split into chunks. Parameters are normal with 2σ = tolerance, truncated at ±tolerance.
pub fn monte_carlo(system: &LensSystem, settings: &ToleranceSettings, start: usize, count: usize) -> Vec<f64> {
    let list = parameters(system, settings);
    (start..start + count).into_par_iter().map(|trial| {
        let mut random = Random(settings.seed.wrapping_mul(0x9e3779b9).wrapping_add(trial as u32).wrapping_mul(2654435761).wrapping_add(1));
        let perturbations: Vec<(Parameter, f64)> = list.iter().map(|&p| (p, (random.gaussian() / 2.0).clamp(-1.0, 1.0) * p.tolerance)).collect();
        evaluate(&perturb(system, &perturbations), settings)
    }).collect()
}

pub fn nominal(system: &LensSystem, settings: &ToleranceSettings) -> f64 {
    evaluate(system, settings)
}
