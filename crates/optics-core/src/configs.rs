//! Solves, pickups and multi-configuration data. All of them are applied by `resolved()` before anything is traced,
//! so every analysis sees the finished lens.

use serde::{Deserialize, Serialize};

use crate::paraxial::{paraxial_data, trace};
use crate::system::{LensError, LensSystem};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Parameter {
    Radius,
    Thickness,
    Conic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Solve {
    /// Sets this surface's thickness so the paraxial marginal ray has `height` (mm) at the next surface; after the
    /// last surface a height of 0 puts the image plane at the paraxial focus.
    MarginalRayHeight { height: f64 },
    /// Sets a parameter of this surface to `scale` × the same parameter of another surface + `offset`.
    Pickup { surface: usize, parameter: Parameter, scale: f64, offset: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigParameter {
    Radius,
    Thickness,
    Conic,
    Material,
    DecenterX,
    DecenterY,
    TiltX,
    TiltY,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    Number(f64),
    Text(String),
}

/// One row of the multi-configuration editor: a surface parameter and its value in each configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRow {
    pub surface: usize,
    pub parameter: ConfigParameter,
    pub values: Vec<ConfigValue>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Configurations {
    pub names: Vec<String>,
    #[serde(default)]
    pub rows: Vec<ConfigRow>,
    #[serde(default)]
    pub active: usize,
}

pub fn configuration_count(system: &LensSystem) -> usize {
    system.configs.as_ref().map_or(1, |c| c.names.len().max(1))
}

pub fn active_configuration(system: &LensSystem) -> usize {
    system.configs.as_ref().map_or(0, |c| c.active.min(c.names.len().saturating_sub(1)))
}

/// Whether the multi-configuration table or a solve sets this parameter, so the optimizer must leave it alone.
pub fn is_locked(system: &LensSystem, surface: usize, parameter: Parameter) -> bool {
    let config = match parameter {
        Parameter::Radius => ConfigParameter::Radius,
        Parameter::Thickness => ConfigParameter::Thickness,
        Parameter::Conic => ConfigParameter::Conic,
    };
    if system.configs.as_ref().is_some_and(|c| c.rows.iter().any(|r| r.surface == surface && r.parameter == config)) {
        return true;
    }
    match &system.surfaces[surface].solve {
        Some(Solve::MarginalRayHeight { .. }) => parameter == Parameter::Thickness,
        Some(Solve::Pickup { parameter: p, .. }) => *p == parameter,
        None => false,
    }
}

/// The system with the values of configuration `index` written into the lens data.
pub fn apply_configuration(system: &LensSystem, index: usize) -> LensSystem {
    let mut out = system.clone();
    let Some(configs) = &system.configs else { return out };
    for row in &configs.rows {
        let (Some(value), Some(surface)) = (row.values.get(index), out.surfaces.get_mut(row.surface)) else { continue };
        match (row.parameter, value) {
            (ConfigParameter::Material, ConfigValue::Text(text)) => surface.material = text.trim().to_uppercase(),
            (ConfigParameter::Radius, ConfigValue::Number(v)) => surface.radius = *v,
            (ConfigParameter::Thickness, ConfigValue::Number(v)) => surface.thickness = *v,
            (ConfigParameter::Conic, ConfigValue::Number(v)) => surface.conic = *v,
            (ConfigParameter::DecenterX, ConfigValue::Number(v)) => surface.decenter = Some([*v, surface.decenter.map_or(0.0, |d| d[1])]),
            (ConfigParameter::DecenterY, ConfigValue::Number(v)) => surface.decenter = Some([surface.decenter.map_or(0.0, |d| d[0]), *v]),
            (ConfigParameter::TiltX, ConfigValue::Number(v)) => surface.tilt = Some([*v, surface.tilt.map_or(0.0, |t| t[1])]),
            (ConfigParameter::TiltY, ConfigValue::Number(v)) => surface.tilt = Some([surface.tilt.map_or(0.0, |t| t[0]), *v]),
            _ => {}
        }
    }
    out
}

pub fn has_solves(system: &LensSystem) -> bool {
    system.surfaces.iter().any(|s| s.solve.is_some())
}

/// Applies pickups and marginal ray height solves in surface order.
pub fn apply_solves(system: &LensSystem) -> Result<LensSystem, LensError> {
    let mut out = system.clone();
    for i in 0..out.surfaces.len() {
        match out.surfaces[i].solve.clone() {
            None => {}
            Some(Solve::Pickup { surface, parameter, scale, offset }) => {
                let source = &out.surfaces[surface];
                let value = match parameter {
                    Parameter::Radius => source.radius,
                    Parameter::Thickness => source.thickness,
                    Parameter::Conic => source.conic,
                };
                let target = &mut out.surfaces[i];
                match parameter {
                    // A flat surface (radius 0) stays flat only when scaled from a flat one.
                    Parameter::Radius => target.radius = if value == 0.0 { 0.0 } else { scale * value + offset },
                    Parameter::Thickness => target.thickness = scale * value + offset,
                    Parameter::Conic => target.conic = scale * value + offset,
                }
            }
            Some(Solve::MarginalRayHeight { height }) => {
                let n = out.medium_indices(out.primary_wavelength())?;
                let semi = out.entrance_pupil_diameter / 2.0;
                let ray = match out.object_distance {
                    None => trace(&out, &n, semi, 0.0),
                    Some(distance) => {
                        let z = paraxial_data(&out)?.entrance_pupil_z;
                        let u = semi / (distance + z);
                        trace(&out, &n, u * distance, u)
                    }
                };
                let (y, u) = (ray.heights[i], ray.angles[i]);
                if u.abs() < 1e-12 {
                    return Err(LensError(format!("The marginal ray solve on surface {} fails: the ray is parallel to the axis", i + 1)));
                }
                out.surfaces[i].thickness = (height - y) / u;
            }
        }
    }
    Ok(out)
}

pub fn validate(system: &LensSystem) -> Result<(), LensError> {
    let count = system.surfaces.len();
    for (i, surface) in system.surfaces.iter().enumerate() {
        match &surface.solve {
            Some(Solve::Pickup { surface: source, .. }) if *source >= count || *source == i => {
                return Err(LensError(format!("Pickup on surface {} must refer to another surface", i + 1)));
            }
            Some(_) if surface.coordinate_break.is_some() => {
                return Err(LensError(format!("Surface {} is a coordinate break and cannot have a solve", i + 1)));
            }
            _ => {}
        }
    }
    if let Some(configs) = &system.configs {
        if configs.names.is_empty() {
            return Err(LensError("The configuration table has no configurations".into()));
        }
        for row in &configs.rows {
            if row.surface >= count {
                return Err(LensError(format!("Configuration row refers to missing surface {}", row.surface + 1)));
            }
            let text = row.parameter == ConfigParameter::Material;
            for value in &row.values {
                if matches!(value, ConfigValue::Text(_)) != text {
                    return Err(LensError(format!("Configuration value on surface {} has the wrong type", row.surface + 1)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::{cooke_triplet, Surface};

    #[test]
    fn marginal_solve_puts_the_image_at_focus() {
        let mut lens = cooke_triplet();
        let last = lens.last();
        let expected = lens.surfaces[last].thickness;
        lens.surfaces[last].thickness = expected + 3.0;
        lens.surfaces[last].solve = Some(Solve::MarginalRayHeight { height: 0.0 });
        let solved = apply_solves(&lens).unwrap();
        assert!((solved.surfaces[last].thickness - expected).abs() < 0.5, "{} vs {expected}", solved.surfaces[last].thickness);
        let data = paraxial_data(&solved).unwrap();
        assert!((data.image_distance - solved.surfaces[last].thickness).abs() < 1e-9);
    }

    #[test]
    fn pickup_copies_with_scale() {
        let mut lens = cooke_triplet();
        lens.surfaces[1].solve = Some(Solve::Pickup { surface: 0, parameter: Parameter::Radius, scale: -1.0, offset: 0.0 });
        let solved = apply_solves(&lens).unwrap();
        assert_eq!(solved.surfaces[1].radius, -lens.surfaces[0].radius);
    }

    #[test]
    fn configurations_override_values() {
        let mut lens = cooke_triplet();
        lens.surfaces.push(Surface::new(0.0, 0.0, "AIR"));
        lens.configs = Some(Configurations {
            names: vec!["A".into(), "B".into()],
            rows: vec![ConfigRow { surface: 1, parameter: ConfigParameter::Thickness, values: vec![ConfigValue::Number(5.0), ConfigValue::Number(9.0)] }],
            active: 0,
        });
        assert_eq!(apply_configuration(&lens, 0).surfaces[1].thickness, 5.0);
        assert_eq!(apply_configuration(&lens, 1).surfaces[1].thickness, 9.0);
        assert!(is_locked(&lens, 1, Parameter::Thickness));
        assert!(!is_locked(&lens, 1, Parameter::Radius));
        assert!(validate(&lens).is_ok());
    }
}
