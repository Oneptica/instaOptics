//! Where the parts of a figure go inside a window: legend row, chart cells, plot areas, colour bars, table and footer.
//! The egui painter and the PNG rasteriser share it, so an exported image looks like the window.

use crate::{Chart, Figure};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w: w.max(0.0), h: h.max(0.0) }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }
}

pub const LEGEND_HEIGHT: f32 = 22.0;
pub const FOOTER_HEIGHT: f32 = 20.0;
pub const TITLE_HEIGHT: f32 = 18.0;
pub const TABLE_ROW: f32 = 18.0;
const LEFT: f32 = 58.0;
const RIGHT: f32 = 14.0;
const BOTTOM: f32 = 36.0;
const COLORBAR: f32 = 54.0;

pub struct ChartFrame {
    /// The whole cell.
    pub cell: Rect,
    /// The data area; axes and ticks are drawn around it.
    pub plot: Rect,
    pub colorbar: Option<Rect>,
}

pub struct FigureFrame {
    pub legend: Option<Rect>,
    pub charts: Vec<ChartFrame>,
    pub table: Option<Rect>,
    pub footer: Option<Rect>,
}

pub fn layout(figure: &Figure, area: Rect) -> FigureFrame {
    let mut top = area.y;
    let mut bottom = area.bottom();
    let legend = (!figure.legend.is_empty()).then(|| {
        let r = Rect::new(area.x, top, area.w, LEGEND_HEIGHT);
        top += LEGEND_HEIGHT;
        r
    });
    let footer = (!figure.footer.is_empty()).then(|| {
        bottom -= FOOTER_HEIGHT;
        Rect::new(area.x, bottom, area.w, FOOTER_HEIGHT)
    });
    let mut table = None;
    if let Some(t) = &figure.table {
        let wanted = (t.rows.len() as f32 + 1.0) * TABLE_ROW + 6.0;
        let height = if figure.charts.is_empty() { bottom - top } else { wanted.min((bottom - top) * 0.45) };
        bottom -= height;
        table = Some(Rect::new(area.x, bottom, area.w, height));
    }
    let n = figure.charts.len();
    // Columns 0: square charts (spot diagrams) take the column count that makes them largest.
    let columns = if figure.columns == 0 {
        (1..=n.max(1)).max_by(|a, b| {
            let size = |c: usize| (area.w / c as f32).min((bottom - top) / n.div_ceil(c) as f32);
            size(*a).total_cmp(&size(*b))
        }).unwrap_or(1)
    } else {
        figure.columns.clamp(1, n.max(1))
    };
    let rows = n.div_ceil(columns).max(1);
    let (cw, ch) = (area.w / columns as f32, (bottom - top) / rows as f32);
    let charts = figure
        .charts
        .iter()
        .enumerate()
        .map(|(i, chart)| {
            let cell = Rect::new(area.x + (i % columns) as f32 * cw, top + (i / columns) as f32 * ch, cw, ch);
            chart_frame(chart, cell)
        })
        .collect();
    FigureFrame { legend, charts, table, footer }
}

fn chart_frame(chart: &Chart, cell: Rect) -> ChartFrame {
    let title = if chart.title.is_empty() { 6.0 } else { TITLE_HEIGHT + 4.0 };
    let (left, bottom, right) = if chart.axes { (LEFT, BOTTOM, RIGHT) } else { (8.0, 8.0, 8.0) };
    let bar = if chart.colorbar.is_some() { COLORBAR } else { 0.0 };
    let mut plot = Rect::new(cell.x + left, cell.y + title, cell.w - left - right - bar, cell.h - title - bottom);
    if chart.equal_aspect && plot.w > 1.0 && plot.h > 1.0 {
        let (sx, sy) = (plot.w as f64 / (chart.x.max - chart.x.min), plot.h as f64 / (chart.y.max - chart.y.min));
        let s = sx.min(sy);
        let (w, h) = (((chart.x.max - chart.x.min) * s) as f32, ((chart.y.max - chart.y.min) * s) as f32);
        // Square plots sit under their title; spare height goes below.
        plot = Rect::new(plot.x + (plot.w - w) / 2.0, plot.y, w, h);
    }
    let colorbar = chart.colorbar.as_ref().map(|_| Rect::new(plot.right() + 12.0, plot.y, 12.0, plot.h));
    ChartFrame { cell, plot, colorbar }
}

/// Screen position of a data point inside a plot rectangle (y up).
pub fn to_screen(chart: &Chart, plot: &Rect, p: [f64; 2]) -> (f32, f32) {
    let x = plot.x + (chart.x.frac(p[0]) as f32) * plot.w;
    let y = plot.bottom() - (chart.y.frac(p[1]) as f32) * plot.h;
    (x, y)
}

/// Data coordinates of a screen position inside a plot rectangle.
pub fn to_data(chart: &Chart, plot: &Rect, x: f32, y: f32) -> [f64; 2] {
    [chart.x.value(((x - plot.x) / plot.w) as f64), chart.y.value(((plot.bottom() - y) / plot.h) as f64)]
}
