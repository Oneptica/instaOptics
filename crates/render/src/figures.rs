//! Analysis results as figures. Each builder mirrors what an OpticStudio analysis window shows.

use instaoptics_core::analysis::{
    ChromaticFocalShift, FieldCurves, Footprint, Illumination, MtfResult, MtfVsField, Psf, RayFanField, SeidelResult, SeidelTerms, SpotField,
    ThroughFocus, WavefrontMap,
};
use instaoptics_core::layout::Layout;
use instaoptics_core::polarization::{CoatingCurves, FieldTransmission, PolarizationMap};
use instaoptics_core::pop::PopResult;
use instaoptics_core::system::FieldType;
use instaoptics_core::{AnalysisResult, LensSystem};

use crate::{Axis, Chart, Color, Colormap, Dash, Figure, Item, LegendItem, Table, field_color, nice_ticks, wavelength_color};

/// Choices that change how a result is drawn but not what is computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub psf_log: bool,
    pub polarization_map: PolarizationView,
    pub beam: BeamView,
    /// Plane index for the beam cross section.
    pub beam_plane: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolarizationView {
    #[default]
    Transmission,
    Diattenuation,
    Retardance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BeamView {
    #[default]
    SideX,
    SideY,
    Radius,
    Section,
    Table,
}

pub fn field_label(value: f64, field_type: FieldType) -> String {
    match field_type {
        FieldType::Angle => format!("{}°", short(value)),
        _ => format!("{} mm", short(value)),
    }
}

/// Shortest readable form for values the user typed.
pub fn short(value: f64) -> String {
    if !value.is_finite() {
        return "Infinity".into();
    }
    let v: f64 = format!("{value:.9e}").parse().unwrap_or(value);
    format!("{v}")
}

fn um(mm: f64) -> String {
    if mm.is_finite() { format!("{:.3}", mm * 1000.0) } else { "—".into() }
}

fn opt(v: f64, digits: usize) -> String {
    if v.is_finite() { format!("{v:.digits$}") } else { "—".into() }
}

fn line(points: Vec<[f64; 2]>, color: Color, dash: Dash) -> Item {
    Item::Line { points, color, dash, width: 1.4 }
}

fn wavelength_legend(system: &LensSystem) -> Vec<LegendItem> {
    system.wavelengths.iter().map(|w| LegendItem { label: format!("{w:.4} µm"), color: wavelength_color(*w), dash: Dash::Solid }).collect()
}

fn field_legend(system: &LensSystem) -> Vec<LegendItem> {
    system.fields.iter().enumerate().map(|(i, f)| LegendItem { label: field_label(*f, system.field_type), color: field_color(i), dash: Dash::Solid }).collect()
}

fn ts_legend() -> [LegendItem; 2] {
    [LegendItem { label: "T".into(), color: Color::Muted, dash: Dash::Solid }, LegendItem { label: "S".into(), color: Color::Muted, dash: Dash::Dashed }]
}

fn finite_range(values: &[f64]) -> [f32; 2] {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values.iter().filter(|v| v.is_finite()) {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    if !lo.is_finite() {
        return [0.0, 1.0];
    }
    if hi <= lo { [lo as f32, lo as f32 + 1.0] } else { [lo as f32, hi as f32] }
}

fn image(size: usize, values: &[f64], colormap: Colormap, range: [f32; 2], half: f64) -> Item {
    Item::Image { width: size, height: size, values: values.iter().map(|v| *v as f32).collect(), colormap, range, extent: [-half, half, -half, half] }
}

/// The figure for one analysis result. `working_f_number` sizes the Airy disc on spot diagrams.
pub fn figure(result: &AnalysisResult, system: &LensSystem, working_f_number: Option<f64>, options: &Options) -> Figure {
    match result {
        AnalysisResult::Spot(spots) => spot(spots, system, working_f_number),
        AnalysisResult::RayFan(fans) => ray_fan(fans, system),
        AnalysisResult::Seidel(seidel) => seidel_figure(seidel),
        AnalysisResult::FieldCurves(curves) => field_curves(curves),
        AnalysisResult::Wavefront(map) => wavefront(map),
        AnalysisResult::Mtf(mtf) => mtf_figure(mtf, system),
        AnalysisResult::Illumination(ill) => illumination(ill),
        AnalysisResult::Psf(psf) => psf_figure(psf, options.psf_log),
        AnalysisResult::ThroughFocus(tf) => through_focus(tf, system),
        AnalysisResult::MtfVsField(m) => mtf_vs_field(m),
        AnalysisResult::ChromaticFocalShift(c) => chromatic(c),
        AnalysisResult::Footprint(f) => footprint(f, system),
        AnalysisResult::Coating(c) => coating(c),
        AnalysisResult::Polarization(p) => polarization(p, options.polarization_map),
        AnalysisResult::TransmissionByField(t) => transmission(t, system),
        AnalysisResult::Pop(p) => beam(p, system, options),
        AnalysisResult::MeritFunction(report) => {
            Figure { footer: format!("Merit {}", if report.merit.is_finite() { format!("{:.6e}", report.merit) } else { "—".into() }), ..Figure::default() }
        }
        AnalysisResult::GaussianBeam(_) | AnalysisResult::Layout3d(_) => Figure::default(),
    }
}

// ---------- spot diagram ----------

fn spot(spots: &[SpotField], system: &LensSystem, working_f_number: Option<f64>) -> Figure {
    let primary = system.wavelengths.get(system.primary_wavelength).copied().unwrap_or(0.55);
    let airy = working_f_number.filter(|f| f.is_finite()).map_or(0.0, |f| 1.22 * primary * 1e-3 * f);
    let largest = spots.iter().map(|s| s.geo_radius).filter(|r| r.is_finite()).fold(airy.max(1e-6), f64::max) * 1.05;
    let half = nice_ticks(0.0, largest, 2).into_iter().find(|t| *t >= largest).unwrap_or(largest * 1.2);
    let colors: Vec<Color> = system.wavelengths.iter().map(|w| wavelength_color(*w)).collect();
    let charts = spots
        .iter()
        .map(|s| {
            let title = format!("{}   RMS {}   GEO {} µm", field_label(s.field, system.field_type), um(s.rms_radius), um(s.geo_radius));
            let mut chart = Chart::new(&title, "x (mm)", "y (mm)", Axis::new(-half, half), Axis::new(-half, half));
            chart.equal_aspect = true;
            if airy > 0.0 {
                chart.items.push(Item::Circle { center: [0.0, 0.0], radius: airy, color: Color::Faint, dash: Dash::Solid });
            }
            for (w, color) in colors.iter().enumerate() {
                let points: Vec<[f64; 2]> = s.points.iter().filter(|p| p[2] as usize == w).map(|p| [p[0], p[1]]).collect();
                if !points.is_empty() {
                    chart.items.push(Item::Points { points, color: *color, radius: 1.3 });
                }
            }
            chart
        })
        .collect();
    let vignetted: Vec<String> = spots.iter().filter(|s| s.traced < s.total).map(|s| format!("{}: {} of {} rays vignetted", field_label(s.field, system.field_type), s.total - s.traced, s.total)).collect();
    let mut footer = format!("Box width {} µm · Airy radius {} µm · reference: primary chief ray", um(2.0 * half), um(airy));
    if !vignetted.is_empty() {
        footer = format!("{footer} · {}", vignetted.join(" · "));
    }
    Figure { columns: 0, charts, legend: wavelength_legend(system), footer, table: None }
}

// ---------- ray fan ----------

fn ray_fan(fans: &[RayFanField], system: &LensSystem) -> Figure {
    let limit = fans.iter().flat_map(|f| f.tangential.iter().chain(&f.sagittal)).flat_map(|c| c.error.iter()).filter(|e| e.is_finite()).fold(1e-9_f64, |m, e| m.max(e.abs())) * 1000.0 * 1.05;
    let mut charts = Vec::new();
    for (i, fan) in fans.iter().enumerate() {
        let label = field_label(system.fields.get(i).copied().unwrap_or(fan.field), system.field_type);
        for (name, curves, x, y) in [("Tangential", &fan.tangential, "Py", "ey (µm)"), ("Sagittal", &fan.sagittal, "Px", "ex (µm)")] {
            let mut chart = Chart::new(&format!("{label} · {name}"), x, y, Axis::new(-1.0, 1.0), Axis::new(-limit, limit));
            for c in curves {
                chart.items.push(line(c.pupil.iter().zip(&c.error).map(|(p, e)| [*p, e * 1000.0]).collect(), wavelength_color(c.wavelength), Dash::Solid));
            }
            charts.push(chart);
        }
    }
    Figure { columns: 2, charts, legend: wavelength_legend(system), ..Figure::default() }
}

// ---------- MTF ----------

fn mtf_figure(r: &MtfResult, system: &LensSystem) -> Figure {
    let top = r.frequencies.last().copied().unwrap_or(1.0);
    let mut chart = Chart::new("", "Spatial frequency (cycles/mm)", "Modulus of the OTF", Axis::new(0.0, top), Axis::new(0.0, 1.0));
    chart.items.push(Item::Line { points: r.frequencies.iter().zip(&r.diffraction).map(|(f, d)| [*f, *d]).collect(), color: Color::Text, dash: Dash::Dotted, width: 1.2 });
    for (i, f) in r.fields.iter().enumerate() {
        chart.items.push(line(r.frequencies.iter().zip(&f.tangential).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Solid));
        chart.items.push(line(r.frequencies.iter().zip(&f.sagittal).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Dashed));
    }
    let mut legend = field_legend(system);
    legend.extend(ts_legend());
    legend.push(LegendItem { label: "Diffraction".into(), color: Color::Text, dash: Dash::Dotted });
    Figure { columns: 1, charts: vec![chart], legend, footer: format!("Polychromatic diffraction MTF · cutoff {:.1} cycles/mm", r.cutoff), table: None }
}

fn through_focus(r: &ThroughFocus, system: &LensSystem) -> Figure {
    let (lo, hi) = (r.shifts.first().copied().unwrap_or(-1.0), r.shifts.last().copied().unwrap_or(1.0));
    let mut chart = Chart::new("", "Focus shift (mm)", &format!("MTF at {} lp/mm", short(r.frequency)), Axis::new(lo, hi), Axis::new(0.0, 1.0));
    for (i, f) in r.fields.iter().enumerate() {
        chart.items.push(line(r.shifts.iter().zip(&f.tangential).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Solid));
        chart.items.push(line(r.shifts.iter().zip(&f.sagittal).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Dashed));
    }
    let mut legend = field_legend(system);
    legend.extend(ts_legend());
    Figure { columns: 1, charts: vec![chart], legend, ..Figure::default() }
}

fn mtf_vs_field(r: &MtfVsField) -> Figure {
    let top = r.angles.last().copied().filter(|a| *a > 0.0).unwrap_or(1.0);
    let mut chart = Chart::new("", "Field angle (deg)", "Modulus of the OTF", Axis::new(0.0, top), Axis::new(0.0, 1.0));
    let mut legend = Vec::new();
    for (i, c) in r.curves.iter().enumerate() {
        chart.items.push(line(r.angles.iter().zip(&c.tangential).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Solid));
        chart.items.push(line(r.angles.iter().zip(&c.sagittal).map(|(x, y)| [*x, *y]).collect(), field_color(i), Dash::Dashed));
        if let Some(f) = r.frequencies.get(i) {
            legend.push(LegendItem { label: format!("{} lp/mm", short(*f)), color: field_color(i), dash: Dash::Solid });
        }
    }
    legend.extend(ts_legend());
    Figure { columns: 1, charts: vec![chart], legend, ..Figure::default() }
}

// ---------- PSF and wavefront ----------

fn psf_figure(p: &Psf, log: bool) -> Figure {
    let half = p.size as f64 * p.spacing * 500.0;
    let values: Vec<f64> = if log { p.data.iter().map(|v| v.max(1e-5).log10()).collect() } else { p.data.clone() };
    let range = if log { [-5.0, 0.0] } else { [0.0, 1.0] };
    let mut map = Chart::new(&format!("Strehl {:.3}", p.strehl), "x (µm)", "y (µm)", Axis::new(-half, half), Axis::new(-half, half));
    map.equal_aspect = true;
    map.items.push(image(p.size, &values, Colormap::Inferno, range, half));
    map.colorbar = Some((Colormap::Inferno, range, if log { "log₁₀ I".into() } else { "I / Iₘₐₓ".into() }));
    let quarter = p.radius.get((p.radius.len() / 4).min(p.radius.len().saturating_sub(1))).copied().unwrap_or(0.01) * 1000.0;
    let mut ee = Chart::new("Encircled energy", "Radius (µm)", "Fraction of energy", Axis::new(0.0, quarter.max(1e-6)), Axis::new(0.0, 1.0));
    ee.items.push(Item::Line { points: p.radius.iter().zip(&p.diffraction).map(|(r, d)| [r * 1000.0, *d]).collect(), color: Color::Text, dash: Dash::Dotted, width: 1.2 });
    ee.items.push(line(p.radius.iter().zip(&p.energy).map(|(r, e)| [r * 1000.0, *e]).collect(), field_color(0), Dash::Solid));
    Figure { columns: 2, charts: vec![map, ee], footer: format!("{} µm wide · field {}", um(p.size as f64 * p.spacing), short(p.field)), ..Figure::default() }
}

fn wavefront(m: &WavefrontMap) -> Figure {
    let range = finite_range(&m.values);
    let mut chart = Chart::new("Exit pupil", "Px", "Py", Axis::new(-1.0, 1.0), Axis::new(-1.0, 1.0));
    chart.equal_aspect = true;
    chart.items.push(image(m.size, &m.values, Colormap::Jet, range, 1.0));
    chart.colorbar = Some((Colormap::Jet, range, "waves".into()));
    Figure { columns: 1, charts: vec![chart], footer: format!("P-V {} waves · RMS {} waves · reference sphere through the exit pupil", opt(m.pv, 4), opt(m.rms, 4)), ..Figure::default() }
}

// ---------- aberrations ----------

fn field_curves(r: &FieldCurves) -> Figure {
    let top = r.angles.last().copied().filter(|a| *a > 0.0).unwrap_or(1.0);
    let focus = r.curves.iter().flat_map(|c| c.tangential.iter().chain(&c.sagittal)).copied();
    let mut curvature = Chart::new("Field curvature", "Focus shift (mm)", "Field angle (deg)", Axis::fit(focus, 0.08, true), Axis::new(0.0, top));
    for c in &r.curves {
        curvature.items.push(line(c.tangential.iter().zip(&r.angles).map(|(z, a)| [*z, *a]).collect(), wavelength_color(c.wavelength), Dash::Solid));
        curvature.items.push(line(c.sagittal.iter().zip(&r.angles).map(|(z, a)| [*z, *a]).collect(), wavelength_color(c.wavelength), Dash::Dashed));
    }
    let mut distortion = Chart::new("Distortion", "Distortion (%)", "Field angle (deg)", Axis::fit(r.distortion.iter().copied(), 0.08, true), Axis::new(0.0, top));
    distortion.items.push(line(r.distortion.iter().zip(&r.angles).map(|(d, a)| [*d, *a]).collect(), field_color(0), Dash::Solid));
    let mut legend: Vec<LegendItem> = r.curves.iter().map(|c| LegendItem { label: format!("{:.4} µm", c.wavelength), color: wavelength_color(c.wavelength), dash: Dash::Solid }).collect();
    legend.extend(ts_legend());
    Figure { columns: 2, charts: vec![curvature, distortion], legend, ..Figure::default() }
}

pub const SEIDEL_KEYS: [&str; 7] = ["S1", "S2", "S3", "S4", "S5", "CL", "CT"];
const SEIDEL_COLORS: [Color; 7] =
    [Color::Rgb(79, 143, 247), Color::Rgb(63, 185, 80), Color::Rgb(248, 81, 73), Color::Rgb(210, 153, 34), Color::Rgb(188, 140, 255), Color::Rgb(57, 197, 207), Color::Rgb(255, 123, 114)];

fn seidel_values(t: &SeidelTerms) -> [f64; 7] {
    [t.S1, t.S2, t.S3, t.S4, t.S5, t.CL, t.CT]
}

fn seidel_figure(r: &SeidelResult) -> Figure {
    let groups: Vec<[f64; 7]> = r.surfaces.iter().chain(std::iter::once(&r.total)).map(seidel_values).collect();
    let peak = groups.iter().flatten().filter(|v| v.is_finite()).fold(1e-9_f64, |m, v| m.max(v.abs())) * 1.05;
    let n = groups.len() as f64;
    let mut chart = Chart::new("", "Surface (last group: sum)", "Seidel coefficient", Axis::new(0.5, n + 0.5), Axis::new(-peak, peak));
    let bar = 0.8 / 7.0;
    for (k, color) in SEIDEL_COLORS.iter().enumerate() {
        let bars = groups.iter().enumerate().map(|(g, v)| [g as f64 + 1.0 - 0.4 + (k as f64 + 0.5) * bar, bar * 0.9, v[k]]).collect();
        chart.items.push(Item::Bars { bars, color: *color });
    }
    let legend = SEIDEL_KEYS.iter().zip(SEIDEL_COLORS).map(|(k, c)| LegendItem { label: (*k).into(), color: c, dash: Dash::Solid }).collect();
    let mut header = vec!["Surf".to_string()];
    header.extend(SEIDEL_KEYS.iter().map(|k| k.to_string()));
    let rows = groups
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let mut row = vec![if i + 1 == groups.len() { "SUM".to_string() } else { (i + 1).to_string() }];
            row.extend(v.iter().map(|x| format!("{x:.6}")));
            row
        })
        .collect();
    Figure { columns: 1, charts: vec![chart], legend, table: Some(Table { header, rows }), footer: String::new() }
}

fn chromatic(r: &ChromaticFocalShift) -> Figure {
    let (lo, hi) = (r.wavelengths.first().copied().unwrap_or(0.4), r.wavelengths.last().copied().unwrap_or(0.7));
    let mut chart = Chart::new("", "Focal shift (µm)", "Wavelength (µm)", Axis::fit(r.shift.iter().copied(), 0.08, true), Axis::new(lo, hi));
    chart.items.push(Item::Line { points: r.shift.iter().zip(&r.wavelengths).map(|(s, w)| [*s, *w]).collect(), color: field_color(0), dash: Dash::Solid, width: 1.6 });
    Figure { columns: 1, charts: vec![chart], footer: format!("Paraxial focus relative to the primary wavelength · maximum shift range {:.3} µm", r.range), ..Figure::default() }
}

fn illumination(r: &Illumination) -> Figure {
    let top = r.angles.last().copied().filter(|a| *a > 0.0).unwrap_or(1.0);
    let y = r.relative.iter().copied().filter(|v| v.is_finite()).fold(1.0, f64::max) * 1.02;
    let mut chart = Chart::new("", "Field angle (deg)", "Relative illumination", Axis::new(0.0, top), Axis::new(0.0, y));
    let pts = |v: &Vec<f64>| r.angles.iter().zip(v).map(|(a, b)| [*a, *b]).collect::<Vec<_>>();
    chart.items.push(line(pts(&r.cos4), Color::Muted, Dash::Dashed));
    chart.items.push(Item::Line { points: pts(&r.unvignetted), color: field_color(3), dash: Dash::Dotted, width: 1.6 });
    chart.items.push(Item::Line { points: pts(&r.relative), color: field_color(0), dash: Dash::Solid, width: 1.6 });
    let legend = vec![
        LegendItem { label: "Relative illumination".into(), color: field_color(0), dash: Dash::Solid },
        LegendItem { label: "cos⁴".into(), color: Color::Muted, dash: Dash::Dashed },
        LegendItem { label: "Unvignetted pupil".into(), color: field_color(3), dash: Dash::Dotted },
    ];
    let edge = r.relative.last().copied().unwrap_or(f64::NAN);
    Figure { columns: 1, charts: vec![chart], legend, footer: format!("Edge of field: {:.1}% · primary wavelength", 100.0 * edge), table: None }
}

fn footprint(r: &Footprint, system: &LensSystem) -> Figure {
    let half = r.fields.iter().flatten().fold(r.semi_diameter, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1e-6) * 1.08;
    let mut chart = Chart::new("", "x (mm)", "y (mm)", Axis::new(-half, half), Axis::new(-half, half));
    chart.equal_aspect = true;
    chart.items.push(Item::Circle { center: [0.0, 0.0], radius: r.semi_diameter, color: Color::Muted, dash: Dash::Solid });
    for (f, points) in r.fields.iter().enumerate() {
        chart.items.push(Item::Points { points: points.clone(), color: field_color(f), radius: 1.6 });
    }
    Figure { columns: 1, charts: vec![chart], legend: field_legend(system), footer: format!("Surface {} · clear semi-diameter {:.3} mm (circle) · primary wavelength", r.surface + 1, r.semi_diameter), table: None }
}

// ---------- polarization ----------

fn coating(r: &CoatingCurves) -> Figure {
    let top = r.angles.last().copied().unwrap_or(80.0);
    let mut chart = Chart::new("", "Angle of incidence (deg)", "Power", Axis::new(0.0, top), Axis::new(0.0, 1.0));
    let pts = |v: &Vec<f64>| r.angles.iter().zip(v).map(|(a, b)| [*a, *b]).collect::<Vec<_>>();
    chart.items.push(line(pts(&r.r_s), field_color(0), Dash::Solid));
    chart.items.push(line(pts(&r.r_p), field_color(0), Dash::Dashed));
    chart.items.push(line(pts(&r.t_s), field_color(1), Dash::Solid));
    chart.items.push(line(pts(&r.t_p), field_color(1), Dash::Dashed));
    let legend = vec![
        LegendItem { label: "Reflectance".into(), color: field_color(0), dash: Dash::Solid },
        LegendItem { label: "Transmittance".into(), color: field_color(1), dash: Dash::Solid },
        LegendItem { label: "s".into(), color: Color::Muted, dash: Dash::Solid },
        LegendItem { label: "p".into(), color: Color::Muted, dash: Dash::Dashed },
    ];
    let (r0, t0) = (r.r_s.first().copied().unwrap_or(f64::NAN), r.t_s.first().copied().unwrap_or(f64::NAN));
    Figure { columns: 1, charts: vec![chart], legend, footer: format!("{} · normal incidence R {}% T {}%", r.description, short(r0 * 100.0), short(t0 * 100.0)), table: None }
}

fn polarization(r: &PolarizationMap, view: PolarizationView) -> Figure {
    let (values, unit) = match view {
        PolarizationView::Transmission => (&r.transmission, "power"),
        PolarizationView::Diattenuation => (&r.diattenuation, ""),
        PolarizationView::Retardance => (&r.retardance, "deg"),
    };
    let range = finite_range(values);
    let mut chart = Chart::new("Exit pupil", "Px", "Py", Axis::new(-1.0, 1.0), Axis::new(-1.0, 1.0));
    chart.equal_aspect = true;
    chart.items.push(image(r.size, values, Colormap::Magma, range, 1.0));
    chart.colorbar = Some((Colormap::Magma, range, unit.into()));
    let footer = format!(
        "mean T {} · min T {} · max D {} · RMS retardance {}°",
        opt(r.mean_transmission, 4),
        opt(r.min_transmission, 4),
        opt(r.max_diattenuation, 4),
        opt(r.rms_retardance, 3)
    );
    Figure { columns: 1, charts: vec![chart], footer, ..Figure::default() }
}

fn transmission(r: &[FieldTransmission], system: &LensSystem) -> Figure {
    let unit = if system.field_type == FieldType::Angle { "deg" } else { "mm" };
    let hi = r.iter().map(|x| x.field).fold(1e-9, f64::max);
    let mut chart = Chart::new("", &format!("Field ({unit})"), "Transmission", Axis::new(0.0, hi), Axis::new(0.0, 1.0));
    chart.items.push(line(r.iter().map(|x| [x.field, x.mean]).collect(), field_color(0), Dash::Solid));
    chart.items.push(line(r.iter().map(|x| [x.field, x.min]).collect(), field_color(0), Dash::Dashed));
    let legend = vec![LegendItem { label: "Mean".into(), color: field_color(0), dash: Dash::Solid }, LegendItem { label: "Minimum".into(), color: field_color(0), dash: Dash::Dashed }];
    let footer = r.iter().map(|x| format!("{}: {:.1}%", field_label(x.field, system.field_type), x.mean * 100.0)).collect::<Vec<_>>().join(" · ");
    Figure { columns: 1, charts: vec![chart], legend, footer, table: None }
}

// ---------- beam propagation ----------

fn beam(r: &PopResult, system: &LensSystem, options: &Options) -> Figure {
    let n = system.surfaces.len();
    let plane_name = |s: usize| if s >= n { "Image".to_string() } else { format!("After surface {}", s + 1) };
    let surface_lines = || r.surface_z.iter().enumerate().map(|(i, z)| Item::VLine { x: *z, label: (i + 1).to_string() });
    let warnings = r.warnings.join(" · ");
    let mut figure = match options.beam {
        BeamView::SideX | BeamView::SideY => {
            let x_axis = options.beam == BeamView::SideX;
            let w = |s: &instaoptics_core::pop::Slice| if x_axis { s.w_x } else { s.w_y };
            let half = r.slices.iter().map(w).filter(|v| v.is_finite()).fold(1e-9, f64::max) * 1.15;
            // Resample the slices onto a regular z grid; each column is one slice's cut, normalized per plane.
            let (cols, rows) = (480usize, 160usize);
            let (z0, z1) = (r.z_start, r.image_z);
            let mut values = vec![f32::NAN; cols * rows];
            for c in 0..cols {
                let z = z0 + (c as f64 + 0.5) / cols as f64 * (z1 - z0);
                let k = r.slices.partition_point(|s| s.z <= z).saturating_sub(1).min(r.slices.len().saturating_sub(1));
                let Some(slice) = r.slices.get(k) else { continue };
                let cut = if x_axis { &slice.x_cut } else { &slice.y_cut };
                for row in 0..rows {
                    let x = (0.5 - (row as f64 + 0.5) / rows as f64) * 2.0 * half;
                    let u = x / slice.pitch + cut.len() as f64 / 2.0;
                    let i = u.floor();
                    let v = if i >= 0.0 && (i as usize + 1) < cut.len() {
                        let f = (u - i) as f32;
                        cut[i as usize] * (1.0 - f) + cut[i as usize + 1] * f
                    } else {
                        0.0
                    };
                    values[row * cols + c] = v.max(0.0).sqrt();
                }
            }
            let axis = if x_axis { "x" } else { "y" };
            let mut chart = Chart::new("", "Distance along the axis (mm)", &format!("{axis} (mm)"), Axis::new(z0, z1), Axis::new(-half, half));
            chart.items.push(Item::Image { width: cols, height: rows, values, colormap: Colormap::Magma, range: [0.0, 1.0], extent: [z0, z1, -half, half] });
            for sign in [1.0, -1.0] {
                chart.items.push(Item::Line { points: r.slices.iter().map(|s| [s.z, sign * w(s)]).collect(), color: Color::Rgb(235, 235, 235), dash: Dash::Solid, width: 1.0 });
            }
            chart.items.extend(surface_lines());
            Figure { columns: 1, charts: vec![chart], ..Figure::default() }
        }
        BeamView::Radius => {
            let last = r.surface_z.last().copied().unwrap_or(r.z_start);
            let mut chart = Chart::new("", "Distance along the axis (mm)", "Beam radius (mm)", Axis::new(r.z_start, r.image_z), Axis::fit(r.slices.iter().map(|s| 0.5 * (s.w_x + s.w_y)), 0.1, false));
            chart.y.min = 0.0;
            if let Some(analytic) = &r.analytic {
                chart.items.push(line(analytic.clone(), Color::Muted, Dash::Dashed));
            }
            chart.items.push(Item::Line { points: r.slices.iter().map(|s| [s.z, 0.5 * (s.w_x + s.w_y)]).collect(), color: field_color(0), dash: Dash::Solid, width: 1.6 });
            chart.items.extend(surface_lines());
            let mut charts = vec![chart];
            if r.slices.iter().any(|s| s.coupling.is_some()) {
                let mut c = Chart::new("Fibre coupling", "Distance along the axis (mm)", "Coupling", Axis::new(last, r.image_z), Axis::new(0.0, 1.0));
                c.items.push(line(r.slices.iter().filter(|s| s.z >= last).map(|s| [s.z, s.coupling.unwrap_or(0.0)]).collect(), field_color(1), Dash::Solid));
                charts.push(c);
            }
            let mut legend = vec![LegendItem { label: "Simulated".into(), color: field_color(0), dash: Dash::Solid }];
            if r.analytic.is_some() {
                legend.push(LegendItem { label: "Gaussian (q)".into(), color: Color::Muted, dash: Dash::Dashed });
            }
            Figure { columns: 1, charts, legend, ..Figure::default() }
        }
        BeamView::Section => {
            let Some(plane) = r.planes.iter().find(|p| p.surface == options.beam_plane).or(r.planes.last()) else { return Figure::default() };
            let half = plane.half_width;
            let to64 = |v: &Vec<f32>| v.iter().map(|x| *x as f64).collect::<Vec<_>>();
            let mut a = Chart::new(&plane_name(plane.surface), "x (mm)", "y (mm)", Axis::new(-half, half), Axis::new(-half, half));
            a.equal_aspect = true;
            a.items.push(image(plane.size, &to64(&plane.intensity), Colormap::Magma, [0.0, 1.0], half));
            a.colorbar = Some((Colormap::Magma, [0.0, 1.0], "I / Iₘₐₓ".into()));
            let mut b = Chart::new("Phase relative to the best-fit sphere", "x (mm)", "y (mm)", Axis::new(-half, half), Axis::new(-half, half));
            b.equal_aspect = true;
            let pi = std::f32::consts::PI;
            b.items.push(image(plane.size, &to64(&plane.phase), Colormap::Phase, [-pi, pi], half));
            b.colorbar = Some((Colormap::Phase, [-pi, pi], "rad".into()));
            Figure { columns: 2, charts: vec![a, b], footer: format!("w {} × {} µm", um(plane.w_x), um(plane.w_y)), ..Figure::default() }
        }
        BeamView::Table => {
            let header = ["Plane", "z (mm)", "w x (µm)", "w y (µm)", "Power", "Peak (1/mm²)"].map(String::from).to_vec();
            let rows = r
                .planes
                .iter()
                .map(|p| vec![if p.surface >= n { "Image".into() } else { (p.surface + 1).to_string() }, format!("{:.3}", p.z), um(p.w_x), um(p.w_y), format!("{:.4}", p.power), format!("{:.3e}", p.peak)])
                .collect();
            let footer = r.aberration.as_ref().map_or(String::new(), |a| format!("Wavefront aberration at the last surface: RMS {:.4} waves · P-V {:.4} waves", a.rms_waves, a.pv_waves));
            Figure { columns: 1, table: Some(Table { header, rows }), footer, ..Figure::default() }
        }
    };
    if let Some(c) = &r.coupling {
        let text = format!("Fibre coupling {:.2}% at the image, best {:.2}% at z {:.3} mm", 100.0 * c.at_image, 100.0 * c.best, c.best_z);
        figure.footer = if figure.footer.is_empty() { text } else { format!("{} · {text}", figure.footer) };
    }
    if !warnings.is_empty() {
        figure.footer = if figure.footer.is_empty() { warnings } else { format!("{} · {warnings}", figure.footer) };
    }
    figure
}

// ---------- layout, optimization, tolerancing, glass map ----------

/// The 2D lens layout (y–z section): element outlines, surface lines, rays coloured by field.
pub fn layout_figure(layout: &Layout, selected_surface: Option<usize>) -> Figure {
    let pts = layout.elements.iter().chain(&layout.surfaces).flatten().chain(layout.rays.iter().flat_map(|r| r.points.iter()));
    let (mut z0, mut z1, mut y0, mut y1) = (layout.start_z, layout.image_z, f64::INFINITY, f64::NEG_INFINITY);
    for p in pts.filter(|p| p[0].is_finite() && p[1].is_finite()) {
        z0 = z0.min(p[0]);
        z1 = z1.max(p[0]);
        y0 = y0.min(p[1]);
        y1 = y1.max(p[1]);
    }
    if !y0.is_finite() {
        (y0, y1) = (-1.0, 1.0);
    }
    let (pz, py) = ((z1 - z0).max(1e-6) * 0.04, (y1 - y0).max(1e-6) * 0.08);
    let mut chart = Chart::new("", "z (mm)", "y (mm)", Axis::new(z0 - pz, z1 + pz), Axis::new(y0 - py, y1 + py));
    chart.equal_aspect = true;
    chart.axes = false;
    for element in &layout.elements {
        chart.items.push(Item::Polygon { points: element.clone(), fill: Color::Faint, stroke: Color::Muted });
    }
    for (i, s) in layout.surfaces.iter().enumerate() {
        let color = if Some(i) == selected_surface { Color::Rgb(255, 196, 64) } else if layout.mirrors.contains(&i) { Color::Text } else { Color::Muted };
        chart.items.push(Item::Line { points: s.clone(), color, dash: Dash::Solid, width: if Some(i) == selected_surface { 2.2 } else { 1.2 } });
    }
    let stop = layout.stop_semi_diameter;
    for sign in [1.0, -1.0] {
        chart.items.push(Item::Line { points: vec![[layout.stop_z, sign * stop], [layout.stop_z, sign * (stop + py * 0.6)]], color: Color::Text, dash: Dash::Solid, width: 2.0 });
    }
    for ray in &layout.rays {
        chart.items.push(Item::Line { points: ray.points.clone(), color: field_color(ray.field), dash: Dash::Solid, width: 1.0 });
    }
    chart.items.push(Item::Line { points: vec![[layout.image_z, y0 - py * 0.5], [layout.image_z, y1 + py * 0.5]], color: Color::Muted, dash: Dash::Dashed, width: 1.0 });
    Figure { columns: 1, charts: vec![chart], ..Figure::default() }
}

pub fn merit_history_figure(history: &[f64]) -> Figure {
    let mut chart = Chart::new("Merit function", "Cycle", "Merit", Axis::new(0.0, (history.len().max(2) - 1) as f64), Axis::fit(history.iter().copied(), 0.05, false));
    chart.y.min = chart.y.min.max(0.0);
    chart.items.push(Item::Line { points: history.iter().enumerate().map(|(i, m)| [i as f64, *m]).collect(), color: field_color(0), dash: Dash::Solid, width: 1.6 });
    Figure { columns: 1, charts: vec![chart], ..Figure::default() }
}

/// Monte Carlo histogram and cumulative probability of the RMS spot radius (mm values, drawn in µm).
pub fn monte_carlo_figure(values: &[f64], nominal: f64) -> Figure {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).map(|v| v * 1000.0).collect();
    sorted.sort_by(f64::total_cmp);
    let (Some(&lo), Some(&hi)) = (sorted.first(), sorted.last()) else { return Figure::default() };
    let lo = lo.min(nominal * 1000.0);
    let bins = 30usize;
    let span = (hi - lo).max(1e-9);
    let mut counts = vec![0usize; bins];
    for v in &sorted {
        counts[(((v - lo) / span * bins as f64) as usize).min(bins - 1)] += 1;
    }
    let peak = counts.iter().copied().max().unwrap_or(1) as f64;
    let mut hist = Chart::new("Distribution of RMS spot radius", "RMS spot radius (µm)", "Trials", Axis::new(lo, lo + span), Axis::new(0.0, peak * 1.08));
    let w = span / bins as f64;
    hist.items.push(Item::Bars { bars: counts.iter().enumerate().map(|(i, c)| [lo + (i as f64 + 0.5) * w, w * 0.92, *c as f64]).collect(), color: field_color(0) });
    hist.items.push(Item::VLine { x: nominal * 1000.0, label: "nominal".into() });
    let mut cdf = Chart::new("Cumulative probability", "RMS spot radius (µm)", "Fraction of lenses", Axis::new(lo, lo + span), Axis::new(0.0, 1.0));
    let n = sorted.len() as f64;
    cdf.items.push(Item::Line { points: sorted.iter().enumerate().map(|(i, v)| [*v, (i + 1) as f64 / n]).collect(), color: field_color(0), dash: Dash::Solid, width: 1.6 });
    let mean = sorted.iter().sum::<f64>() / n;
    let std = (sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    let pct = |p: f64| sorted[((p / 100.0 * n) as usize).min(sorted.len() - 1)];
    let footer = format!(
        "{} trials · nominal {:.3} · mean {mean:.3} · σ {std:.3} · 50% < {:.3} · 90% < {:.3} · 98% < {:.3} µm",
        sorted.len(),
        nominal * 1000.0,
        pct(50.0),
        pct(90.0),
        pct(98.0)
    );
    Figure { columns: 2, charts: vec![hist, cdf], footer, ..Figure::default() }
}

/// Glass map: refractive index nd against Abbe number vd, vd decreasing to the right as glass makers draw it.
pub fn abbe_figure(glasses: &[(String, f64, f64, Color)], highlight: Option<&str>) -> Figure {
    let valid = || glasses.iter().filter(|g| g.1 > 1.0 && g.2 > 0.0);
    let vmax = valid().map(|g| g.2).fold(95.0, f64::max);
    let vmin = valid().map(|g| g.2).fold(20.0, f64::min);
    let nmin = valid().map(|g| g.1).fold(1.4, f64::min);
    let nmax = valid().map(|g| g.1).fold(2.0, f64::max);
    let mut x = Axis::new(vmin - 2.0, vmax + 2.0);
    x.reversed = true;
    let mut chart = Chart::new("", "Abbe number vd", "nd", x, Axis::new(nmin - 0.02, nmax + 0.02));
    let mut by_color: Vec<(Color, Vec<[f64; 2]>)> = Vec::new();
    for g in valid() {
        match by_color.iter_mut().find(|(c, _)| *c == g.3) {
            Some((_, pts)) => pts.push([g.2, g.1]),
            None => by_color.push((g.3, vec![[g.2, g.1]])),
        }
    }
    for (color, points) in by_color {
        chart.items.push(Item::Points { points, color, radius: 2.5 });
    }
    if let Some(g) = highlight.and_then(|h| valid().find(|g| g.0 == h)) {
        chart.items.push(Item::Circle { center: [g.2, g.1], radius: 0.4, color: Color::Text, dash: Dash::Solid });
        chart.items.push(Item::Label { at: [g.2, g.1], text: g.0.clone() });
    }
    Figure { columns: 1, charts: vec![chart], ..Figure::default() }
}
