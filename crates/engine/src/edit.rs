//! Pure edits of a lens system. Each returns false when nothing changed.

use instaoptics_core::configs::{ConfigParameter, ConfigRow, ConfigValue, Configurations, Parameter, Solve};
use instaoptics_core::system::{CoordinateBreak, Variables};
use instaoptics_core::{LensSystem, Surface};

/// Inserts a flat dummy surface before `index` (`index == len` appends). It continues the medium it sits in, so the
/// system is optically unchanged.
pub fn insert_surface(system: &mut LensSystem, index: usize) {
    let index = index.min(system.surfaces.len());
    let material = index.checked_sub(1).and_then(|i| system.surfaces.get(i)).map_or("AIR".to_string(), |s| s.material.clone());
    let material = if instaoptics_core::glass::is_mirror(&material) { "AIR".to_string() } else { material };
    system.surfaces.insert(index, Surface::new(0.0, 0.0, &material));
    if system.stop_index >= index && system.surfaces.len() > 1 {
        system.stop_index += 1;
    }
    remap_surface_refs(system, |i| Some(if i >= index { i + 1 } else { i }));
}

pub fn delete_surface(system: &mut LensSystem, index: usize) -> bool {
    if system.surfaces.len() <= 1 || index >= system.surfaces.len() {
        return false;
    }
    system.surfaces.remove(index);
    if system.stop_index > index {
        system.stop_index -= 1;
    }
    system.stop_index = system.stop_index.min(system.surfaces.len() - 1);
    remap_surface_refs(system, |i| match i.cmp(&index) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(i - 1),
        std::cmp::Ordering::Less => Some(i),
    });
    true
}

/// Rewrites every stored surface number (pickups, configuration rows, operands) after surfaces move.
pub fn remap_surface_refs(system: &mut LensSystem, map: impl Fn(usize) -> Option<usize>) {
    for s in &mut system.surfaces {
        if let Some(Solve::Pickup { surface, .. }) = &mut s.solve {
            match map(*surface) {
                Some(t) => *surface = t,
                None => s.solve = None,
            }
        }
    }
    if let Some(configs) = &mut system.configs {
        configs.rows.retain_mut(|row| map(row.surface).map(|t| row.surface = t).is_some());
    }
    system.optimization.operands.retain_mut(|o| match o.surface {
        None => true,
        Some(s) => map(s).map(|t| o.surface = Some(t)).is_some(),
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableKey {
    Radius,
    Thickness,
    Conic,
    Aspheric(usize),
}

impl VariableKey {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "radius" => Self::Radius,
            "thickness" => Self::Thickness,
            "conic" => Self::Conic,
            _ => {
                let order: usize = text.strip_prefix('a')?.parse().ok()?;
                // a4, a6, a8, a10 → terms 0..3
                if order < 4 || order % 2 == 1 {
                    return None;
                }
                Self::Aspheric((order - 4) / 2)
            }
        })
    }
}

pub fn is_variable(surface: &Surface, key: VariableKey) -> bool {
    let Some(v) = &surface.variable else { return false };
    match key {
        VariableKey::Radius => v.radius,
        VariableKey::Thickness => v.thickness,
        VariableKey::Conic => v.conic,
        VariableKey::Aspheric(t) => v.aspheric.get(t).copied().unwrap_or(false),
    }
}

pub fn set_variable(system: &mut LensSystem, index: usize, key: VariableKey, on: bool) -> bool {
    let Some(surface) = system.surfaces.get_mut(index) else { return false };
    if surface.coordinate_break.is_some() || is_variable(surface, key) == on {
        return false;
    }
    let v = surface.variable.get_or_insert_with(Variables::default);
    match key {
        VariableKey::Radius => v.radius = on,
        VariableKey::Thickness => v.thickness = on,
        VariableKey::Conic => v.conic = on,
        VariableKey::Aspheric(t) => {
            if v.aspheric.len() <= t {
                v.aspheric.resize(t + 1, false);
            }
            v.aspheric[t] = on;
            while v.aspheric.last() == Some(&false) {
                v.aspheric.pop();
            }
        }
    }
    if !v.radius && !v.thickness && !v.conic && v.aspheric.is_empty() {
        surface.variable = None;
    }
    true
}

pub fn clear_variables(system: &mut LensSystem) -> bool {
    let any = system.surfaces.iter().any(|s| s.variable.is_some());
    for s in &mut system.surfaces {
        s.variable = None;
    }
    any
}

/// Sets an even asphere coefficient (term 0 is A4), dropping trailing zero terms.
pub fn set_aspheric(surface: &mut Surface, term: usize, value: f64) {
    if surface.aspheric.len() <= term {
        surface.aspheric.resize(term + 1, 0.0);
    }
    surface.aspheric[term] = value;
    while surface.aspheric.last() == Some(&0.0) {
        surface.aspheric.pop();
    }
}

pub fn toggle_coordinate_break(system: &mut LensSystem, index: usize) -> bool {
    let Some(s) = system.surfaces.get_mut(index) else { return false };
    if s.coordinate_break.is_some() {
        s.coordinate_break = None;
    } else {
        s.coordinate_break = Some(CoordinateBreak::default());
        s.radius = 0.0;
        s.conic = 0.0;
        s.aspheric.clear();
        s.variable = None;
    }
    true
}

/// Which parameter of its own surface a solve controls.
pub fn solved_parameter(solve: Option<&Solve>) -> Option<Parameter> {
    match solve? {
        Solve::MarginalRayHeight { .. } => Some(Parameter::Thickness),
        Solve::Pickup { parameter, .. } => Some(*parameter),
    }
}

fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    match t.as_str() {
        "inf" | "infinity" | "∞" => Some(f64::INFINITY),
        "-inf" | "-infinity" => Some(f64::NEG_INFINITY),
        "" => None,
        _ => t.parse::<f64>().ok().filter(|v| v.is_finite()),
    }
}

/// Solve text as in the Lens Data editor: "M" or "M 0.5" is a marginal ray height solve on the thickness; "Pr1 -1 0"
/// copies the radius (r), thickness (t) or conic (k) of surface 1 with a scale and offset. Blank is no solve.
pub fn parse_solve(text: &str, surface_count: usize) -> Result<Option<Solve>, String> {
    let value = text.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let mut words = value.split_whitespace();
    let head = words.next().unwrap_or_default();
    let rest: Vec<&str> = words.collect();
    let bad = || format!("Invalid solve {value}");
    if head.eq_ignore_ascii_case("m") {
        let height = match rest.first() {
            None => 0.0,
            Some(h) => parse_number(h).ok_or_else(bad)?,
        };
        return Ok(Some(Solve::MarginalRayHeight { height }));
    }
    let mut chars = head.chars();
    if chars.next().is_some_and(|c| c.eq_ignore_ascii_case(&'p')) {
        let parameter = match chars.next().map(|c| c.to_ascii_lowercase()) {
            Some('r') => Parameter::Radius,
            Some('t') => Parameter::Thickness,
            Some('k') => Parameter::Conic,
            _ => return Err(bad()),
        };
        let surface: usize = chars.as_str().parse().map_err(|_| bad())?;
        if surface == 0 || surface > surface_count {
            return Err(bad());
        }
        let scale = rest.first().map_or(Some(1.0), |t| parse_number(t)).ok_or_else(bad)?;
        let offset = rest.get(1).map_or(Some(0.0), |t| parse_number(t)).ok_or_else(bad)?;
        return Ok(Some(Solve::Pickup { surface: surface - 1, parameter, scale, offset }));
    }
    Err(bad())
}

pub fn format_solve(solve: Option<&Solve>) -> String {
    match solve {
        None => String::new(),
        Some(Solve::MarginalRayHeight { height }) => if *height == 0.0 { "M".into() } else { format!("M {height}") },
        Some(Solve::Pickup { surface, parameter, scale, offset }) => {
            let letter = match parameter {
                Parameter::Radius => 'r',
                Parameter::Thickness => 't',
                Parameter::Conic => 'k',
            };
            let tail = if *offset != 0.0 { format!(" {scale} {offset}") } else if *scale != 1.0 { format!(" {scale}") } else { String::new() };
            format!("P{letter}{}{tail}", surface + 1)
        }
    }
}

// ---------- configurations ----------

pub fn active_configuration(system: &LensSystem) -> usize {
    system.configs.as_ref().map_or(0, |c| c.active.min(c.names.len().saturating_sub(1)))
}

pub fn config_row(system: &LensSystem, surface: usize, parameter: ConfigParameter) -> Option<usize> {
    system.configs.as_ref()?.rows.iter().position(|r| r.surface == surface && r.parameter == parameter)
}

fn current_value(system: &LensSystem, surface: usize, parameter: ConfigParameter) -> ConfigValue {
    let Some(s) = system.surfaces.get(surface) else { return ConfigValue::Number(0.0) };
    let n = |v: f64| ConfigValue::Number(v);
    match parameter {
        ConfigParameter::Radius => n(s.radius),
        ConfigParameter::Thickness => n(s.thickness),
        ConfigParameter::Conic => n(s.conic),
        ConfigParameter::Material => ConfigValue::Text(s.material.clone()),
        ConfigParameter::DecenterX => n(s.decenter.map_or(0.0, |d| d[0])),
        ConfigParameter::DecenterY => n(s.decenter.map_or(0.0, |d| d[1])),
        ConfigParameter::TiltX => n(s.tilt.map_or(0.0, |t| t[0])),
        ConfigParameter::TiltY => n(s.tilt.map_or(0.0, |t| t[1])),
    }
}

pub fn add_configuration(system: &mut LensSystem) {
    let Some(configs) = &mut system.configs else {
        system.configs = Some(Configurations { names: vec!["Config 1".into(), "Config 2".into()], rows: Vec::new(), active: 0 });
        return;
    };
    let last = configs.names.len().saturating_sub(1);
    configs.names.push(format!("Config {}", configs.names.len() + 1));
    for row in &mut configs.rows {
        let v = row.values.get(last).cloned().unwrap_or(ConfigValue::Number(0.0));
        row.values.push(v);
    }
}

pub fn remove_configuration(system: &mut LensSystem, index: usize) -> bool {
    let Some(configs) = &mut system.configs else { return false };
    if configs.names.len() <= 1 || index >= configs.names.len() {
        return false;
    }
    configs.names.remove(index);
    for row in &mut configs.rows {
        if index < row.values.len() {
            row.values.remove(index);
        }
    }
    if configs.active > index {
        configs.active -= 1;
    }
    configs.active = configs.active.min(configs.names.len() - 1);
    system.optimization.operands.retain_mut(|o| match o.config {
        Some(c) if c == index => false,
        Some(c) if c > index => {
            o.config = Some(c - 1);
            true
        }
        _ => true,
    });
    true
}

pub fn add_config_row(system: &mut LensSystem, surface: usize, parameter: ConfigParameter) -> bool {
    if surface >= system.surfaces.len() || config_row(system, surface, parameter).is_some() {
        return false;
    }
    let value = current_value(system, surface, parameter);
    let Some(configs) = &mut system.configs else { return false };
    configs.rows.push(ConfigRow { surface, parameter, values: vec![value; configs.names.len()] });
    true
}

pub fn set_config_value(system: &mut LensSystem, row: usize, config: usize, value: ConfigValue) -> bool {
    let Some(configs) = &mut system.configs else { return false };
    let count = configs.names.len();
    let Some(r) = configs.rows.get_mut(row) else { return false };
    if config >= count {
        return false;
    }
    r.values.resize(count, value.clone());
    r.values[config] = value;
    true
}
