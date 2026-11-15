//! Zemax OpticStudio .zmx import and export for the sequential subset instaOptics models: standard and even-asphere
//! surfaces, coordinate breaks, catalog or model glasses, the aperture and field types.

use instaoptics_core::system::{ApertureType, CoordinateBreak, FieldType};
use instaoptics_core::{LensSystem, Surface, glass};

use crate::{IoError, number};

const FIELD_CODES: [FieldType; 3] = [FieldType::Angle, FieldType::ObjectHeight, FieldType::ImageHeight];
const APERTURE_CODES: [ApertureType; 5] =
    [ApertureType::EntrancePupilDiameter, ApertureType::ImageFNumber, ApertureType::ObjectNa, ApertureType::FloatByStop, ApertureType::WorkingFNumber];

pub fn export_zmx(system: &LensSystem) -> String {
    let field_code = FIELD_CODES.iter().position(|f| *f == system.field_type).unwrap_or(0);
    let aperture_code = APERTURE_CODES.iter().position(|a| *a == system.aperture_type).unwrap_or(0);
    let value = number(system.aperture_value.unwrap_or(0.0));
    let mut lines = vec!["VERS 190513 80 123457 L123457".to_string(), "MODE SEQ".into(), format!("NAME {}", system.name), "UNIT MM X W X CM MR CPMM".into()];
    lines.push(match system.aperture_type {
        ApertureType::EntrancePupilDiameter => format!("ENPD {}", number(system.entrance_pupil_diameter)),
        ApertureType::ImageFNumber | ApertureType::WorkingFNumber => format!("FNUM {value} 0"),
        ApertureType::ObjectNa => format!("OBNA {value} 0"),
        ApertureType::FloatByStop => "FLOA".into(),
    });
    lines.push("GCAT SCHOTT".into());
    lines.push(format!("FTYP {field_code} {aperture_code} {} {} 0 0 0", system.fields.len(), system.wavelengths.len()));
    let join = |values: Vec<String>| values.join(" ");
    lines.push(format!("XFLN {}", join(system.fields.iter().map(|_| "0".to_string()).collect())));
    lines.push(format!("YFLN {}", join(system.fields.iter().map(|f| number(*f)).collect())));
    lines.push(format!("FWGN {}", join(system.fields.iter().map(|_| "1".to_string()).collect())));
    for (i, w) in system.wavelengths.iter().enumerate() {
        lines.push(format!("WAVM {} {} 1", i + 1, number(*w)));
    }
    lines.push(format!("PWAV {}", system.primary_wavelength + 1));
    let object = system.object_distance.map_or("INFINITY".to_string(), number);
    lines.extend(["SURF 0".into(), "  TYPE STANDARD".into(), "  CURV 0.0".into(), format!("  DISZ {object}")]);
    for (i, s) in system.surfaces.iter().enumerate() {
        lines.push(format!("SURF {}", i + 1));
        if i == system.stop_index {
            lines.push("  STOP".into());
        }
        if let Some(cb) = s.coordinate_break {
            lines.extend(["  TYPE COORDBRK".into(), "  CURV 0.0".into(), format!("  DISZ {}", number(s.thickness))]);
            for (j, v) in [cb.decenter[0], cb.decenter[1], cb.tilt[0], cb.tilt[1], cb.tilt[2], 0.0].iter().enumerate() {
                lines.push(format!("  PARM {} {}", j + 1, number(*v)));
            }
            continue;
        }
        let asphere = s.aspheric.iter().any(|a| *a != 0.0);
        lines.push(format!("  TYPE {}", if asphere { "EVENASPH" } else { "STANDARD" }));
        lines.push(format!("  CURV {}", number(if s.radius == 0.0 { 0.0 } else { 1.0 / s.radius })));
        lines.push(format!("  DISZ {}", number(s.thickness)));
        if glass::is_mirror(&s.material) {
            lines.push("  GLAS MIRROR 0 0".into());
        } else if !glass::is_air(&s.material) {
            lines.push(match glass::parse_model_glass(&s.material) {
                Some((nd, vd)) => format!("  GLAS ___BLANK 1 0 {} {} 0 0 0 0 0 0", number(nd), number(vd)),
                None => format!("  GLAS {} 0 0", s.material.trim()),
            });
        }
        if s.conic != 0.0 {
            lines.push(format!("  CONI {}", number(s.conic)));
        }
        if asphere {
            lines.push("  PARM 1 0".into());
            for (j, a) in s.aspheric.iter().enumerate() {
                lines.push(format!("  PARM {} {}", j + 2, number(*a)));
            }
        }
        if let Some(semi) = s.semi_diameter {
            lines.push(format!("  DIAM {} 1 0 0 1 \"\"", number(semi)));
            lines.push(format!("  CLAP 0 {} 0", number(semi)));
        }
    }
    lines.extend([format!("SURF {}", system.surfaces.len() + 1), "  TYPE STANDARD".into(), "  CURV 0.0".into(), "  DISZ 0".into()]);
    lines.join("\r\n") + "\r\n"
}

#[derive(Default)]
struct Raw {
    curv: f64,
    disz: f64,
    glass: String,
    conic: f64,
    parms: [f64; 9],
    stop: bool,
    kind: String,
    aperture: Option<f64>,
    diam: Option<f64>,
}

pub struct Imported {
    pub system: LensSystem,
    pub warnings: Vec<String>,
}

/// `known` maps a glass name to its spelling in a loaded catalog, or None when no catalog has it.
pub fn import_zmx(text: &str, known: &dyn Fn(&str) -> Option<String>) -> Result<Imported, IoError> {
    let mut warnings = Vec::new();
    let mut raw: Vec<Raw> = Vec::new();
    let (mut name, mut epd, mut fnum, mut obna) = ("Imported lens".to_string(), None, None, None);
    let (mut field_code, mut aperture_code) = (0usize, 0usize);
    let (mut fields, mut wavelengths, mut primary) = (Vec::<f64>::new(), Vec::<f64>::new(), 1usize);
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let Some(&key) = parts.first() else { continue };
        let val = |i: usize| parts.get(i).and_then(|p| p.parse::<f64>().ok());
        if key == "SURF" {
            raw.push(Raw { kind: "STANDARD".into(), ..Raw::default() });
            continue;
        }
        if let Some(s) = raw.last_mut() {
            match key {
                "TYPE" => s.kind = parts.get(1).unwrap_or(&"").to_string(),
                "CURV" => s.curv = val(1).unwrap_or(0.0),
                "DISZ" => s.disz = if parts.get(1) == Some(&"INFINITY") { f64::INFINITY } else { val(1).unwrap_or(0.0) },
                "STOP" => s.stop = true,
                "CONI" => s.conic = val(1).unwrap_or(0.0),
                "PARM" => {
                    if let (Some(i), Some(v)) = (val(1), val(2))
                        && let Some(slot) = s.parms.get_mut(i as usize)
                    {
                        *slot = v;
                    }
                }
                "CLAP" | "FLAP" => s.aperture = val(2),
                "DIAM" => s.diam = val(1),
                "GLAS" => {
                    let glass_name = parts.get(1).unwrap_or(&"");
                    s.glass = if glass_name.eq_ignore_ascii_case("MIRROR") {
                        "MIRROR".into()
                    } else if let Some(canonical) = known(glass_name) {
                        canonical
                    } else if let (Some(nd), Some(vd)) = (val(4), val(5))
                        && nd > 1.0
                    {
                        if *glass_name != "___BLANK" {
                            warnings.push(format!("{glass_name} is not in a loaded catalog; using model glass nd {}, vd {}", number(nd), number(vd)));
                        }
                        format!("{}/{}", number(nd), number(vd))
                    } else {
                        return Err(IoError::Format(format!("Unknown glass {glass_name}")));
                    };
                }
                _ => {}
            }
        }
        match key {
            "NAME" if parts.len() > 1 => name = line.trim().get(5..).unwrap_or("").to_string(),
            "ENPD" => epd = val(1),
            "FNUM" => fnum = val(1),
            "OBNA" => obna = val(1),
            "FTYP" => {
                field_code = val(1).unwrap_or(0.0) as usize;
                aperture_code = val(2).unwrap_or(0.0) as usize;
            }
            "YFLN" => fields = parts[1..].iter().filter_map(|p| p.parse().ok()).collect(),
            "WAVM" => {
                if let (Some(i), Some(w)) = (val(1), val(2))
                    && w > 0.0
                    && i >= 1.0
                {
                    let i = i as usize - 1;
                    if i < 64 {
                        wavelengths.resize(wavelengths.len().max(i + 1), 0.0);
                        wavelengths[i] = w;
                    }
                }
            }
            "WAVL" => wavelengths = parts[1..].iter().filter_map(|p| p.parse().ok()).collect(),
            "PWAV" => primary = val(1).unwrap_or(1.0).max(1.0) as usize,
            _ => {}
        }
    }
    if raw.len() < 3 {
        return Err(IoError::Format("No sequential surfaces found".into()));
    }
    if let Some(s) = raw.iter().find(|s| !["STANDARD", "EVENASPH", "COORDBRK"].contains(&s.kind.as_str())) {
        return Err(IoError::Format(format!("Surface type {} is not supported", s.kind)));
    }
    if field_code == 3 {
        warnings.push("Real image height fields were read as paraxial image heights".into());
    } else if field_code > 3 {
        warnings.push("This field type is not supported; field values were read as angles".into());
    }
    if aperture_code > 4 {
        warnings.push("This aperture type is not supported; using an entrance pupil diameter".into());
    }
    let field_type = match field_code {
        1 => FieldType::ObjectHeight,
        2 | 3 => FieldType::ImageHeight,
        _ => FieldType::Angle,
    };
    let aperture_type = APERTURE_CODES.get(aperture_code).copied().unwrap_or_default();
    let (object, image) = (&raw[0], &raw[raw.len() - 1]);
    if image.curv != 0.0 {
        warnings.push("A curved image surface was flattened".into());
    }
    let lens = &raw[1..raw.len() - 1];
    let mut surfaces = Vec::with_capacity(lens.len());
    for (i, s) in lens.iter().enumerate() {
        if s.kind == "COORDBRK" {
            let p = &s.parms;
            if p[6] != 0.0 {
                warnings.push(format!("Coordinate break on surface {} uses order 1 (tilt then decenter); it was read as order 0", i + 1));
            }
            let before = i.checked_sub(1).and_then(|k| lens.get(k)).map(|k| k.glass.clone()).unwrap_or_default();
            let material = if before.is_empty() || before == "MIRROR" { "AIR".to_string() } else { before };
            surfaces.push(Surface {
                thickness: s.disz,
                material,
                coordinate_break: Some(CoordinateBreak { decenter: [p[1], p[2]], tilt: [p[3], p[4], p[5]] }),
                ..Surface::default()
            });
            continue;
        }
        let mut surface = Surface::new(if s.curv == 0.0 { 0.0 } else { 1.0 / s.curv }, s.disz, if s.glass.is_empty() { "AIR" } else { &s.glass });
        surface.conic = s.conic;
        if s.kind == "EVENASPH" {
            if s.parms[1] != 0.0 {
                warnings.push("The r² term of an even asphere was ignored".into());
            }
            let aspheric = s.parms[2..6].to_vec();
            if aspheric.iter().any(|a| *a != 0.0) {
                surface.aspheric = aspheric;
            }
        }
        surface.semi_diameter = s.aperture.filter(|a| *a > 0.0);
        surfaces.push(surface);
    }
    let wavelengths: Vec<f64> = wavelengths.into_iter().filter(|w| *w > 0.0).collect();
    let stop_index = lens.iter().position(|s| s.stop).unwrap_or(0);
    let aperture_value = match aperture_type {
        ApertureType::EntrancePupilDiameter => None,
        ApertureType::ObjectNa => Some(obna.unwrap_or(0.1)),
        ApertureType::FloatByStop => Some(lens.iter().find(|s| s.stop).and_then(|s| s.diam).unwrap_or(5.0)),
        _ => Some(fnum.unwrap_or(5.0)),
    };
    let mut system = instaoptics_core::system::new_system();
    system.name = name;
    system.object_distance = Some(object.disz).filter(|d| d.is_finite() && *d > 0.0);
    system.surfaces = surfaces;
    system.stop_index = stop_index;
    system.entrance_pupil_diameter = epd.filter(|e| *e > 0.0).unwrap_or(10.0);
    system.aperture_type = aperture_type;
    system.aperture_value = aperture_value;
    system.fields = if fields.is_empty() { vec![0.0] } else { fields.iter().map(|f| f.abs()).collect() };
    system.field_type = field_type;
    system.primary_wavelength = primary.saturating_sub(1).min(wavelengths.len().saturating_sub(1));
    system.wavelengths = if wavelengths.is_empty() { vec![glass::LINE_D] } else { wavelengths };
    system.ray_aiming = true;
    Ok(Imported { system, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_cooke_triplet() {
        let cooke = instaoptics_core::system::cooke_triplet();
        let text = export_zmx(&cooke);
        let back = import_zmx(&text, &|name| glass::find(name).map(|g| g.name.to_string())).unwrap();
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_eq!(back.system.surfaces.len(), cooke.surfaces.len());
        for (a, b) in back.system.surfaces.iter().zip(&cooke.surfaces) {
            assert!((a.radius - b.radius).abs() < 1e-6 && (a.thickness - b.thickness).abs() < 1e-9 && a.material == b.material);
        }
        assert_eq!(back.system.stop_index, cooke.stop_index);
        assert_eq!(back.system.wavelengths, cooke.wavelengths);
    }
}
