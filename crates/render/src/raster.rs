//! CPU rasteriser: a figure to an RGBA pixmap or PNG bytes (tiny-skia for shapes, ab_glyph for text).

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash, Transform};

use crate::frame::{self, Rect};
use crate::{Chart, Color, Dash, Figure, Item, colormap, nice_ticks, tick_label};

/// Colours for theme roles; the egui front end uses the same values.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub faint: [u8; 3],
}

impl Theme {
    pub const DARK: Theme = Theme { background: [30, 30, 30], text: [220, 220, 220], muted: [140, 140, 140], faint: [70, 70, 72] };
    pub const LIGHT: Theme = Theme { background: [255, 255, 255], text: [30, 30, 30], muted: [110, 110, 110], faint: [205, 205, 208] };

    pub fn rgb(&self, color: Color) -> [u8; 3] {
        match color {
            Color::Rgb(r, g, b) => [r, g, b],
            Color::Text => self.text,
            Color::Muted => self.muted,
            Color::Faint => self.faint,
        }
    }
}

struct Canvas<'f> {
    pixmap: Pixmap,
    font: FontRef<'f>,
    theme: Theme,
}

/// PNG bytes of a figure at `width` × `height` pixels.
pub fn render_png(figure: &Figure, width: u32, height: u32, theme: Theme) -> Result<Vec<u8>, String> {
    let pixmap = render(figure, width, height, theme)?;
    pixmap.encode_png().map_err(|e| e.to_string())
}

pub fn render(figure: &Figure, width: u32, height: u32, theme: Theme) -> Result<Pixmap, String> {
    let (width, height) = (width.clamp(16, 8192), height.clamp(16, 8192));
    let mut pixmap = Pixmap::new(width, height).ok_or("image size")?;
    let [r, g, b] = theme.background;
    pixmap.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
    let font = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).map_err(|e| e.to_string())?;
    let mut canvas = Canvas { pixmap, font, theme };
    canvas.figure(figure, Rect::new(8.0, 4.0, width as f32 - 16.0, height as f32 - 8.0));
    Ok(canvas.pixmap)
}

fn paint(rgb: [u8; 3], alpha: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(rgb[0], rgb[1], rgb[2], alpha);
    p.anti_alias = true;
    p
}

fn stroke(width: f32, dash: Dash) -> Stroke {
    let mut s = Stroke { width, line_cap: LineCap::Butt, ..Stroke::default() };
    s.dash = match dash {
        Dash::Solid => None,
        Dash::Dashed => StrokeDash::new(vec![5.0, 3.0], 0.0),
        Dash::Dotted => StrokeDash::new(vec![1.2, 3.0], 0.0),
    };
    s
}

impl Canvas<'_> {
    fn figure(&mut self, figure: &Figure, area: Rect) {
        let layout = frame::layout(figure, area);
        if let Some(r) = layout.legend {
            let mut x = r.right();
            for item in figure.legend.iter().rev() {
                let w = self.text_width(&item.label, 11.0) + 26.0;
                x -= w;
                let color = self.theme.rgb(item.color);
                self.polyline(&[(x, r.y + 11.0), (x + 16.0, r.y + 11.0)], color, 1.6, item.dash);
                self.text(&item.label, x + 20.0, r.y + 15.0, 11.0, self.theme.text, Align::Left);
            }
        }
        for (chart, f) in figure.charts.iter().zip(&layout.charts) {
            self.chart(chart, f);
        }
        if let (Some(table), Some(r)) = (&figure.table, layout.table) {
            self.table(table, r);
        }
        if let Some(r) = layout.footer {
            self.text(&figure.footer, r.x + 4.0, r.y + 14.0, 11.0, self.theme.muted, Align::Left);
        }
    }

    fn chart(&mut self, chart: &Chart, f: &frame::ChartFrame) {
        let plot = f.plot;
        if plot.w < 4.0 || plot.h < 4.0 {
            return;
        }
        let (text, muted, faint) = (self.theme.text, self.theme.muted, self.theme.faint);
        if !chart.title.is_empty() {
            self.text(&chart.title, plot.x + plot.w / 2.0, plot.y - 6.0, 12.0, text, Align::Center);
        }
        let xt = nice_ticks(chart.x.min, chart.x.max, ((plot.w / 80.0) as usize).max(2));
        let yt = nice_ticks(chart.y.min, chart.y.max, ((plot.h / 50.0) as usize).max(2));
        if chart.axes {
            for t in &xt {
                let (x, _) = frame::to_screen(chart, &plot, [*t, chart.y.min]);
                self.polyline(&[(x, plot.y), (x, plot.bottom())], faint, 1.0, Dash::Solid);
            }
            for t in &yt {
                let (_, y) = frame::to_screen(chart, &plot, [chart.x.min, *t]);
                self.polyline(&[(plot.x, y), (plot.right(), y)], faint, 1.0, Dash::Solid);
            }
        }
        let clip = plot;
        for item in &chart.items {
            self.item(chart, &plot, &clip, item);
        }
        if chart.axes {
            self.rect_stroke(plot, muted);
            let step = |t: &[f64]| if t.len() > 1 { t[1] - t[0] } else { 0.0 };
            let (sx, sy) = (step(&xt), step(&yt));
            for t in &xt {
                let (x, _) = frame::to_screen(chart, &plot, [*t, chart.y.min]);
                self.text(&tick_label(*t, sx), x, plot.bottom() + 14.0, 10.5, muted, Align::Center);
            }
            for t in &yt {
                let (_, y) = frame::to_screen(chart, &plot, [chart.x.min, *t]);
                self.text(&tick_label(*t, sy), plot.x - 6.0, y + 4.0, 10.5, muted, Align::Right);
            }
            self.text(&chart.x_label, plot.x + plot.w / 2.0, plot.bottom() + 30.0, 11.0, text, Align::Center);
            self.text_vertical(&chart.y_label, f.cell.x + 12.0, plot.y + plot.h / 2.0, 11.0, text);
        }
        if let (Some((map, range, unit)), Some(bar)) = (&chart.colorbar, f.colorbar) {
            let steps = bar.h.max(1.0) as usize;
            for i in 0..steps {
                let c = colormap(*map, 1.0 - i as f32 / steps as f32);
                self.fill_rect(Rect::new(bar.x, bar.y + i as f32, bar.w, 1.0), c);
            }
            self.rect_stroke(bar, muted);
            for (v, y) in [(range[1], bar.y + 8.0), (range[0], bar.bottom())] {
                self.text(&tick_label(v as f64, ((range[1] - range[0]) / 10.0) as f64), bar.right() + 4.0, y, 10.0, muted, Align::Left);
            }
            self.text(unit, bar.x, bar.y - 4.0, 10.0, muted, Align::Left);
        }
    }

    fn item(&mut self, chart: &Chart, plot: &Rect, clip: &Rect, item: &Item) {
        let to = |p: [f64; 2]| frame::to_screen(chart, plot, p);
        match item {
            Item::Line { points, color, dash, width } => {
                let rgb = self.theme.rgb(*color);
                let mut run: Vec<(f32, f32)> = Vec::new();
                for p in points {
                    if p[0].is_finite() && p[1].is_finite() {
                        run.push(to(*p));
                    } else if run.len() > 1 {
                        self.clipped_polyline(&run, rgb, *width, *dash, clip);
                        run.clear();
                    } else {
                        run.clear();
                    }
                }
                if run.len() > 1 {
                    self.clipped_polyline(&run, rgb, *width, *dash, clip);
                }
            }
            Item::Points { points, color, radius } => {
                let p = paint(self.theme.rgb(*color), 255);
                let mut pb = PathBuilder::new();
                for q in points.iter().filter(|q| q[0].is_finite() && q[1].is_finite()) {
                    let (x, y) = to(*q);
                    if clip.contains(x, y) {
                        pb.push_circle(x, y, *radius);
                    }
                }
                if let Some(path) = pb.finish() {
                    self.pixmap.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
                }
            }
            Item::Polygon { points, fill, stroke: line } => {
                let mut pb = PathBuilder::new();
                for (i, q) in points.iter().enumerate() {
                    let (x, y) = to(*q);
                    if i == 0 { pb.move_to(x, y) } else { pb.line_to(x, y) }
                }
                pb.close();
                if let Some(path) = pb.finish() {
                    self.pixmap.fill_path(&path, &paint(self.theme.rgb(*fill), 140), FillRule::EvenOdd, Transform::identity(), None);
                    self.pixmap.stroke_path(&path, &paint(self.theme.rgb(*line), 255), &stroke(1.0, Dash::Solid), Transform::identity(), None);
                }
            }
            Item::Circle { center, radius, color, dash } => {
                let (cx, cy) = to(*center);
                let r = (*radius / (chart.x.max - chart.x.min) * plot.w as f64) as f32;
                if let Some(path) = PathBuilder::from_circle(cx, cy, r.max(0.5)) {
                    self.pixmap.stroke_path(&path, &paint(self.theme.rgb(*color), 255), &stroke(1.0, *dash), Transform::identity(), None);
                }
            }
            Item::Bars { bars, color } => {
                let rgb = self.theme.rgb(*color);
                for b in bars {
                    let (x0, y0) = to([b[0] - b[1] / 2.0, 0.0]);
                    let (x1, y1) = to([b[0] + b[1] / 2.0, b[2]]);
                    self.fill_rect(Rect::new(x0.min(x1), y0.min(y1).max(plot.y), (x1 - x0).abs().max(0.5), (y1 - y0).abs().min(plot.h)), rgb);
                }
            }
            Item::Image { width, height, values, colormap: map, range, extent } => {
                let Some(mut img) = Pixmap::new((*width).max(1) as u32, (*height).max(1) as u32) else { return };
                let span = (range[1] - range[0]).max(f32::EPSILON);
                for (i, px) in img.pixels_mut().iter_mut().enumerate() {
                    let v = values.get(i).copied().unwrap_or(f32::NAN);
                    if v.is_finite() {
                        let c = colormap(*map, (v - range[0]) / span);
                        if let Some(color) = tiny_skia::PremultipliedColorU8::from_rgba(c[0], c[1], c[2], 255) {
                            *px = color;
                        }
                    }
                }
                let (x0, y0) = to([extent[0], extent[3]]);
                let (x1, y1) = to([extent[1], extent[2]]);
                let transform = Transform::from_row((x1 - x0) / *width as f32, 0.0, 0.0, (y1 - y0) / *height as f32, x0, y0);
                let quality = if *width >= plot.w as usize { tiny_skia::FilterQuality::Bilinear } else { tiny_skia::FilterQuality::Nearest };
                self.pixmap.draw_pixmap(0, 0, img.as_ref(), &PixmapPaint { quality, ..PixmapPaint::default() }, transform, None);
            }
            Item::VLine { x, label } => {
                let (sx, _) = to([*x, chart.y.min]);
                if sx >= plot.x && sx <= plot.right() {
                    let muted = self.theme.muted;
                    self.polyline(&[(sx, plot.y), (sx, plot.bottom())], muted, 1.0, Dash::Dashed);
                    self.text(label, sx + 3.0, plot.y + 11.0, 10.0, muted, Align::Left);
                }
            }
            Item::Label { at, text } => {
                let (x, y) = to(*at);
                let color = self.theme.text;
                self.text(text, x + 6.0, y - 4.0, 11.0, color, Align::Left);
            }
        }
    }

    fn table(&mut self, table: &crate::Table, r: Rect) {
        let cols = table.header.len().max(1);
        let w = r.w / cols as f32;
        let (text, muted) = (self.theme.text, self.theme.muted);
        for (c, h) in table.header.iter().enumerate() {
            self.text(h, r.x + (c as f32 + 1.0) * w - 6.0, r.y + 14.0, 11.0, muted, Align::Right);
        }
        for (i, row) in table.rows.iter().enumerate() {
            let y = r.y + (i as f32 + 2.0) * frame::TABLE_ROW - 4.0;
            if y > r.bottom() {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                self.text(cell, r.x + (c as f32 + 1.0) * w - 6.0, y, 11.0, text, Align::Right);
            }
        }
    }

    fn clipped_polyline(&mut self, points: &[(f32, f32)], rgb: [u8; 3], width: f32, dash: Dash, clip: &Rect) {
        let Some(mask_rect) = tiny_skia::Rect::from_xywh(clip.x, clip.y, clip.w, clip.h) else { return };
        let mut mask = tiny_skia::Mask::new(self.pixmap.width(), self.pixmap.height());
        if let Some(m) = mask.as_mut() {
            m.fill_path(&PathBuilder::from_rect(mask_rect), FillRule::Winding, false, Transform::identity());
        }
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 { pb.move_to(*x, *y) } else { pb.line_to(*x, *y) }
        }
        if let Some(path) = pb.finish() {
            self.pixmap.stroke_path(&path, &paint(rgb, 255), &stroke(width, dash), Transform::identity(), mask.as_ref());
        }
    }

    fn polyline(&mut self, points: &[(f32, f32)], rgb: [u8; 3], width: f32, dash: Dash) {
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 { pb.move_to(*x, *y) } else { pb.line_to(*x, *y) }
        }
        if let Some(path) = pb.finish() {
            self.pixmap.stroke_path(&path, &paint(rgb, 255), &stroke(width, dash), Transform::identity(), None);
        }
    }

    fn rect_stroke(&mut self, r: Rect, rgb: [u8; 3]) {
        if let Some(rect) = tiny_skia::Rect::from_xywh(r.x + 0.5, r.y + 0.5, r.w, r.h) {
            self.pixmap.stroke_path(&PathBuilder::from_rect(rect), &paint(rgb, 255), &stroke(1.0, Dash::Solid), Transform::identity(), None);
        }
    }

    fn fill_rect(&mut self, r: Rect, rgb: [u8; 3]) {
        if let Some(rect) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h) {
            self.pixmap.fill_rect(rect, &paint(rgb, 255), Transform::identity(), None);
        }
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        let font = self.font.as_scaled(PxScale::from(size));
        text.chars().map(|c| font.h_advance(self.font.glyph_id(c))).sum()
    }

    fn text(&mut self, text: &str, x: f32, baseline: f32, size: f32, rgb: [u8; 3], align: Align) {
        let width = self.text_width(text, size);
        let x = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        self.glyphs(text, x, baseline, size, rgb, false);
    }

    fn text_vertical(&mut self, text: &str, x: f32, center: f32, size: f32, rgb: [u8; 3]) {
        let width = self.text_width(text, size);
        self.glyphs(text, x, center + width / 2.0, size, rgb, true);
    }

    /// Blends glyph coverage into the pixmap; `vertical` runs the text bottom to top.
    fn glyphs(&mut self, text: &str, x: f32, y: f32, size: f32, rgb: [u8; 3], vertical: bool) {
        let scale = PxScale::from(size);
        let scaled = self.font.as_scaled(scale);
        let (w, h) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let mut pen = 0.0f32;
        let mut previous = None;
        for c in text.chars() {
            let id = self.font.glyph_id(c);
            if let Some(p) = previous {
                pen += scaled.kern(p, id);
            }
            previous = Some(id);
            let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, 0.0));
            pen += scaled.h_advance(id);
            let Some(outlined) = self.font.outline_glyph(glyph) else { continue };
            let bounds = outlined.px_bounds();
            let data = self.pixmap.data_mut();
            outlined.draw(|gx, gy, coverage| {
                let (lx, ly) = (bounds.min.x + gx as f32, bounds.min.y + gy as f32);
                let (px, py) = if vertical { (x + ly, y - lx) } else { (x + lx, y + ly) };
                let (px, py) = (px.round() as i32, py.round() as i32);
                if px < 0 || py < 0 || px >= w || py >= h {
                    return;
                }
                let i = ((py * w + px) * 4) as usize;
                let a = coverage.clamp(0.0, 1.0);
                for k in 0..3 {
                    data[i + k] = (data[i + k] as f32 * (1.0 - a) + rgb[k] as f32 * a).round() as u8;
                }
            });
        }
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}
