//! instaOptics figures. An analysis result becomes a [`Figure`]: a grid of charts with a legend, a table and a
//! footer line. The egui front end paints figures with its own painter (with zoom and readout); [`raster`] draws the
//! same figure into a PNG for export, the CLI and MCP, so every plot is defined once.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod figures;
pub mod frame;
pub mod raster;

/// Colours are either fixed (wavelengths, fields, colormaps) or a theme role the painter resolves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Rgb(u8, u8, u8),
    /// Strong foreground (reference curves).
    Text,
    /// Secondary foreground (cos⁴, legend keys for line styles).
    Muted,
    /// Grid, axes, apertures.
    Faint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colormap {
    Inferno,
    Magma,
    Jet,
    /// Cyclic, for phase.
    Phase,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A polyline; NaN points break it.
    Line { points: Vec<[f64; 2]>, color: Color, dash: Dash, width: f32 },
    Points { points: Vec<[f64; 2]>, color: Color, radius: f32 },
    /// A closed outline, filled with `fill` (lens elements in the layout).
    Polygon { points: Vec<[f64; 2]>, fill: Color, stroke: Color },
    Circle { center: [f64; 2], radius: f64, color: Color, dash: Dash },
    /// Vertical bars from 0: (x centre, width, height).
    Bars { bars: Vec<[f64; 3]>, color: Color },
    /// A raster over `extent` [x0, x1, y0, y1], row 0 at the top; NaN pixels stay transparent.
    Image { width: usize, height: usize, values: Vec<f32>, colormap: Colormap, range: [f32; 2], extent: [f64; 4] },
    VLine { x: f64, label: String },
    Label { at: [f64; 2], text: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub min: f64,
    pub max: f64,
    /// Values increase to the left (or downwards); glass maps draw vd this way.
    pub reversed: bool,
}

impl Axis {
    /// Fraction 0..1 along the axis for a value, honouring `reversed`.
    pub fn frac(&self, v: f64) -> f64 {
        let t = (v - self.min) / (self.max - self.min);
        if self.reversed { 1.0 - t } else { t }
    }

    pub fn value(&self, frac: f64) -> f64 {
        let t = if self.reversed { 1.0 - frac } else { frac };
        self.min + t * (self.max - self.min)
    }

    pub fn new(min: f64, max: f64) -> Self {
        let (min, max) = if !(min.is_finite() && max.is_finite()) {
            (0.0, 1.0)
        } else if max - min <= f64::EPSILON * max.abs().max(1.0) {
            (min - 0.5 * min.abs().max(1e-9), max + 0.5 * max.abs().max(1e-9))
        } else {
            (min, max)
        };
        Self { min, max, reversed: false }
    }

    /// The range of finite values, padded by `pad` of the span, optionally symmetric about zero.
    pub fn fit(values: impl Iterator<Item = f64>, pad: f64, symmetric: bool) -> Self {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in values.filter(|v| v.is_finite()) {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        if !lo.is_finite() {
            return Self::new(-1.0, 1.0);
        }
        if symmetric {
            let m = lo.abs().max(hi.abs()).max(1e-12) * (1.0 + pad);
            return Self::new(-m, m);
        }
        let span = (hi - lo).max(hi.abs().max(1e-12) * 1e-6);
        Self::new(lo - pad * span, hi + pad * span)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub x: Axis,
    pub y: Axis,
    /// One data unit is the same length on both axes (spot diagrams, layouts, footprints).
    pub equal_aspect: bool,
    /// Draw tick labels and grid lines; off for the lens layout.
    pub axes: bool,
    pub items: Vec<Item>,
    /// Colour bar for an image chart: colormap, range and unit.
    pub colorbar: Option<(Colormap, [f32; 2], String)>,
}

impl Chart {
    pub fn new(title: &str, x_label: &str, y_label: &str, x: Axis, y: Axis) -> Self {
        Self { title: title.into(), x_label: x_label.into(), y_label: y_label.into(), x, y, equal_aspect: false, axes: true, items: Vec::new(), colorbar: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendItem {
    pub label: String,
    pub color: Color,
    pub dash: Dash,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Figure {
    pub charts: Vec<Chart>,
    /// Charts per row.
    pub columns: usize,
    pub legend: Vec<LegendItem>,
    pub table: Option<Table>,
    pub footer: String,
}

// ---------- colours ----------

/// Field colours, in field order (cycled).
pub const FIELD_COLORS: [Color; 6] =
    [Color::Rgb(79, 143, 247), Color::Rgb(63, 185, 80), Color::Rgb(248, 81, 73), Color::Rgb(210, 153, 34), Color::Rgb(188, 140, 255), Color::Rgb(57, 197, 207)];

pub fn field_color(index: usize) -> Color {
    FIELD_COLORS[index % FIELD_COLORS.len()]
}

/// Approximate display colour for a wavelength in µm; infrared and ultraviolet get muted tones.
pub fn wavelength_color(wavelength: f64) -> Color {
    let nm = wavelength * 1000.0;
    if nm < 380.0 {
        return Color::Rgb(139, 127, 214);
    }
    if nm > 750.0 {
        return Color::Rgb(176, 96, 90);
    }
    const STOPS: [(f64, f64); 9] = [(380.0, 270.0), (440.0, 230.0), (490.0, 195.0), (520.0, 125.0), (565.0, 75.0), (590.0, 48.0), (625.0, 12.0), (645.0, 0.0), (750.0, 0.0)];
    let mut hue = 0.0;
    for w in STOPS.windows(2) {
        let ((a, ha), (b, hb)) = (w[0], w[1]);
        if nm >= a && nm <= b {
            hue = ha + (hb - ha) * (nm - a) / (b - a);
            break;
        }
    }
    hsl(hue, 0.85, 0.52)
}

fn hsl(h: f64, s: f64, l: f64) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(to(r), to(g), to(b))
}

/// Colormap lookup for t in [0, 1].
pub fn colormap(map: Colormap, t: f32) -> [u8; 3] {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let stops: &[[f32; 3]] = match map {
        Colormap::Inferno => &[[0.0, 0.0, 4.0], [40.0, 11.0, 84.0], [101.0, 21.0, 110.0], [159.0, 42.0, 99.0], [212.0, 72.0, 66.0], [245.0, 125.0, 21.0], [250.0, 193.0, 39.0], [252.0, 255.0, 164.0]],
        Colormap::Magma => &[[0.0, 0.0, 4.0], [28.0, 16.0, 68.0], [79.0, 18.0, 123.0], [129.0, 37.0, 129.0], [181.0, 54.0, 122.0], [229.0, 80.0, 100.0], [251.0, 135.0, 97.0], [254.0, 194.0, 135.0], [252.0, 253.0, 191.0]],
        Colormap::Jet => &[[0.0, 0.0, 131.0], [0.0, 60.0, 170.0], [5.0, 255.0, 255.0], [255.0, 255.0, 0.0], [250.0, 0.0, 0.0], [128.0, 0.0, 0.0]],
        Colormap::Phase => &[[46.0, 84.0, 196.0], [159.0, 72.0, 179.0], [221.0, 79.0, 98.0], [221.0, 140.0, 47.0], [146.0, 189.0, 56.0], [52.0, 176.0, 140.0], [46.0, 84.0, 196.0]],
    };
    let x = t * (stops.len() - 1) as f32;
    let i = (x.floor() as usize).min(stops.len() - 2);
    let f = x - i as f32;
    let (a, b) = (stops[i], stops[i + 1]);
    [0, 1, 2].map(|k| (a[k] + (b[k] - a[k]) * f).round() as u8)
}

/// Tick positions at 1, 2 or 5 × 10ⁿ covering [lo, hi] with about `count` ticks.
pub fn nice_ticks(lo: f64, hi: f64, count: usize) -> Vec<f64> {
    if !(lo.is_finite() && hi.is_finite()) || hi <= lo || count == 0 {
        return Vec::new();
    }
    let raw = (hi - lo) / count.max(3) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    if last - first > 1000 {
        return Vec::new();
    }
    (first..=last).map(|k| { let v = k as f64 * step; if v.abs() < step * 1e-9 { 0.0 } else { v } }).collect()
}

/// Tick text with as many decimals as the step needs.
pub fn tick_label(value: f64, step: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let a = value.abs();
    if a >= 1e5 || a < 1e-3 {
        return format!("{value:.1e}");
    }
    // Without a step (a single tick) the value keeps only the digits it needs.
    if step <= 0.0 {
        let v: f64 = format!("{value:.4e}").parse().unwrap_or(value);
        return format!("{v}");
    }
    let decimals = (-step.log10().floor()).clamp(0.0, 6.0) as usize;
    format!("{value:.decimals$}")
}
