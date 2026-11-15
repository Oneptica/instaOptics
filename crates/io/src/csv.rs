//! CSV text: numbers keep full precision, missing and non-finite values become empty cells.

pub enum Cell {
    Num(f64),
    Text(String),
    Empty,
}

impl From<f64> for Cell {
    fn from(v: f64) -> Self {
        Cell::Num(v)
    }
}
impl From<usize> for Cell {
    fn from(v: usize) -> Self {
        Cell::Num(v as f64)
    }
}
impl From<&str> for Cell {
    fn from(v: &str) -> Self {
        Cell::Text(v.to_string())
    }
}
impl From<String> for Cell {
    fn from(v: String) -> Self {
        Cell::Text(v)
    }
}
impl From<Option<f64>> for Cell {
    fn from(v: Option<f64>) -> Self {
        v.map_or(Cell::Empty, Cell::Num)
    }
}

pub fn to_csv(header: &[&str], rows: &[Vec<Cell>]) -> String {
    let text = |cell: &Cell| match cell {
        Cell::Num(v) if v.is_finite() => format!("{v}"),
        Cell::Text(s) if s.contains([',', '"', '\n']) => format!("\"{}\"", s.replace('"', "\"\"")),
        Cell::Text(s) => s.clone(),
        _ => String::new(),
    };
    let mut out = header.join(",");
    out.push('\n');
    for row in rows {
        out.push_str(&row.iter().map(text).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// The data behind an analysis window as CSV, with the columns OpticStudio users expect (µm for spot and fan errors).
pub fn analysis_csv(result: &instaoptics_core::AnalysisResult, system: &instaoptics_core::LensSystem) -> Option<String> {
    use instaoptics_core::AnalysisResult as R;
    let c = |v: f64| Cell::Num(v);
    Some(match result {
        R::Spot(spots) => to_csv(
            &["field", "wavelength_um", "x_um", "y_um"],
            &spots.iter().flat_map(|f| f.points.iter().map(move |p| vec![c(f.field), c(system.wavelengths.get(p[2] as usize).copied().unwrap_or(f64::NAN)), c(p[0] * 1000.0), c(p[1] * 1000.0)])).collect::<Vec<_>>(),
        ),
        R::RayFan(fans) => to_csv(
            &["field", "fan", "wavelength_um", "pupil", "error_um"],
            &fans
                .iter()
                .flat_map(|f| [("tangential", &f.tangential), ("sagittal", &f.sagittal)].into_iter().flat_map(move |(k, curves)| curves.iter().flat_map(move |cv| cv.pupil.iter().zip(&cv.error).map(move |(p, e)| vec![c(f.field), k.into(), c(cv.wavelength), c(*p), c(e * 1000.0)]))))
                .collect::<Vec<_>>(),
        ),
        R::Mtf(m) => {
            let mut header = vec!["frequency_per_mm".to_string(), "diffraction".into()];
            for f in &m.fields {
                header.push(format!("T_{}", f.field));
                header.push(format!("S_{}", f.field));
            }
            let rows: Vec<Vec<Cell>> = m.frequencies.iter().enumerate().map(|(i, fq)| {
                let mut row = vec![c(*fq), c(m.diffraction.get(i).copied().unwrap_or(f64::NAN))];
                for f in &m.fields {
                    row.push(c(f.tangential.get(i).copied().unwrap_or(f64::NAN)));
                    row.push(c(f.sagittal.get(i).copied().unwrap_or(f64::NAN)));
                }
                row
            }).collect();
            to_csv(&header.iter().map(String::as_str).collect::<Vec<_>>(), &rows)
        }
        R::Psf(p) => to_csv(&["radius_um", "encircled", "diffraction"], &p.radius.iter().enumerate().map(|(i, r)| vec![c(r * 1000.0), c(p.energy[i]), c(p.diffraction[i])]).collect::<Vec<_>>()),
        R::Wavefront(m) => grid_csv(m.size, &m.values),
        R::Polarization(m) => grid_csv(m.size, &m.transmission),
        R::FieldCurves(r) => {
            let mut header = vec!["field_deg".to_string(), "distortion_pct".into()];
            for cv in &r.curves {
                header.push(format!("T_{}", cv.wavelength));
                header.push(format!("S_{}", cv.wavelength));
            }
            let rows: Vec<Vec<Cell>> = r.angles.iter().enumerate().map(|(i, a)| {
                let mut row = vec![c(*a), c(r.distortion[i])];
                for cv in &r.curves {
                    row.push(c(cv.tangential[i]));
                    row.push(c(cv.sagittal[i]));
                }
                row
            }).collect();
            to_csv(&header.iter().map(String::as_str).collect::<Vec<_>>(), &rows)
        }
        R::Seidel(s) => {
            let row = |name: String, t: &instaoptics_core::analysis::SeidelTerms| vec![name.into(), c(t.S1), c(t.S2), c(t.S3), c(t.S4), c(t.S5), c(t.CL), c(t.CT)];
            let mut rows: Vec<Vec<Cell>> = s.surfaces.iter().enumerate().map(|(i, t)| row((i + 1).to_string(), t)).collect();
            rows.push(row("SUM".into(), &s.total));
            to_csv(&["surface", "S1", "S2", "S3", "S4", "S5", "CL", "CT"], &rows)
        }
        R::Illumination(r) => to_csv(&["field_deg", "relative", "unvignetted", "cos4"], &r.angles.iter().enumerate().map(|(i, a)| vec![c(*a), c(r.relative[i]), c(r.unvignetted[i]), c(r.cos4[i])]).collect::<Vec<_>>()),
        R::ChromaticFocalShift(r) => to_csv(&["wavelength_um", "shift_um"], &r.wavelengths.iter().zip(&r.shift).map(|(w, s)| vec![c(*w), c(*s)]).collect::<Vec<_>>()),
        R::Footprint(r) => to_csv(&["field", "x_mm", "y_mm"], &r.fields.iter().enumerate().flat_map(|(f, pts)| pts.iter().map(move |p| vec![c(system.fields.get(f).copied().unwrap_or(f64::NAN)), c(p[0]), c(p[1])])).collect::<Vec<_>>()),
        R::Coating(r) => to_csv(&["angle_deg", "Rs", "Rp", "Ts", "Tp"], &r.angles.iter().enumerate().map(|(i, a)| vec![c(*a), c(r.r_s[i]), c(r.r_p[i]), c(r.t_s[i]), c(r.t_p[i])]).collect::<Vec<_>>()),
        R::TransmissionByField(r) => to_csv(&["field", "mean_T", "min_T", "max_diattenuation", "rms_retardance_deg"], &r.iter().map(|x| vec![c(x.field), c(x.mean), c(x.min), c(x.max_diattenuation), c(x.rms_retardance)]).collect::<Vec<_>>()),
        R::ThroughFocus(r) => series_csv("shift_mm", &r.shifts, &r.fields),
        R::MtfVsField(r) => series_csv("field_deg", &r.angles, &r.curves),
        R::Pop(p) => to_csv(&["z_mm", "w_x_mm", "w_y_mm", "power", "peak_irradiance_per_mm2"], &p.slices.iter().map(|s| vec![c(s.z), c(s.w_x), c(s.w_y), c(s.power), c(s.peak)]).collect::<Vec<_>>()),
        _ => return None,
    })
}

fn grid_csv(size: usize, values: &[f64]) -> String {
    let header: Vec<String> = std::iter::once("row".to_string()).chain((0..size).map(|i| format!("x{i}"))).collect();
    let rows: Vec<Vec<Cell>> = (0..size).map(|r| std::iter::once(Cell::Num(r as f64)).chain(values.iter().skip(r * size).take(size).map(|v| Cell::Num(*v))).collect()).collect();
    to_csv(&header.iter().map(String::as_str).collect::<Vec<_>>(), &rows)
}

fn series_csv(x_name: &str, x: &[f64], curves: &[instaoptics_core::analysis::MtfField]) -> String {
    let mut header = vec![x_name.to_string()];
    for f in curves {
        header.push(format!("T_{}", f.field));
        header.push(format!("S_{}", f.field));
    }
    let rows: Vec<Vec<Cell>> = x.iter().enumerate().map(|(i, v)| {
        let mut row = vec![Cell::Num(*v)];
        for f in curves {
            row.push(Cell::Num(f.tangential.get(i).copied().unwrap_or(f64::NAN)));
            row.push(Cell::Num(f.sagittal.get(i).copied().unwrap_or(f64::NAN)));
        }
        row
    }).collect();
    to_csv(&header.iter().map(String::as_str).collect::<Vec<_>>(), &rows)
}
