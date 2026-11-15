//! Paints a figure with egui. The wheel zooms a chart about the cursor, a drag pans it, a double-click resets it,
//! and the cursor position reads out in data units.

use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2};
use instaoptics_render::frame::{self, ChartFrame};
use instaoptics_render::{Axis, Chart, Dash, Figure, Item, colormap, nice_ticks, tick_label};

use crate::theme::Tokens;

/// Heatmap textures of the figures on screen. An entry not drawn for a few frames is dropped, which frees the GPU
/// texture; it is rebuilt when its window shows again.
#[derive(Default)]
pub struct Textures {
    map: std::collections::HashMap<egui::Id, (egui::TextureHandle, u64)>,
    frame: u64,
}

impl Textures {
    pub fn end_frame(&mut self) {
        let frame = self.frame;
        self.map.retain(|_, (_, used)| frame.saturating_sub(*used) < 3);
        self.frame += 1;
    }
    fn get(&mut self, key: egui::Id) -> Option<egui::TextureHandle> {
        let frame = self.frame;
        self.map.get_mut(&key).map(|(t, used)| {
            *used = frame;
            t.clone()
        })
    }
    fn insert(&mut self, key: egui::Id, texture: egui::TextureHandle) {
        self.map.insert(key, (texture, self.frame));
    }
}

fn r(rect: Rect) -> frame::Rect {
    frame::Rect::new(rect.min.x, rect.min.y, rect.width(), rect.height())
}

fn er(f: &frame::Rect) -> Rect {
    Rect::from_min_size(pos2(f.x, f.y), Vec2::new(f.w, f.h))
}

/// Draws `figure` into the available space. `id` keeps each chart's zoom.
pub fn show(ui: &mut Ui, id: egui::Id, figure: &Figure, t: &Tokens, textures: &mut Textures) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let layout = frame::layout(figure, r(rect));
    if let Some(l) = layout.legend {
        let mut x = l.right();
        for item in figure.legend.iter().rev() {
            let galley = painter.layout_no_wrap(item.label.clone(), FontId::proportional(11.0), t.text);
            x -= galley.size().x + 26.0;
            let y = l.y + 11.0;
            dashed(&painter, &[pos2(x, y), pos2(x + 16.0, y)], Stroke::new(1.6, t.color(item.color)), item.dash);
            painter.galley(pos2(x + 20.0, y - galley.size().y / 2.0), galley, t.text);
        }
    }
    for (i, (chart, f)) in figure.charts.iter().zip(&layout.charts).enumerate() {
        let key = id.with(i);
        let mut view = chart.clone();
        if let Some((x, y)) = ui.data(|d| d.get_temp::<(Axis, Axis)>(key)) {
            view.x = x;
            view.y = y;
        }
        let plot = er(&f.plot);
        let hover = response.hover_pos().filter(|p| plot.contains(*p));
        if let Some(p) = hover {
            let (scroll, double) = ui.input(|i| (i.smooth_scroll_delta.y, i.pointer.button_double_clicked(egui::PointerButton::Primary)));
            if double {
                ui.data_mut(|d| d.remove::<(Axis, Axis)>(key));
                view.x = chart.x;
                view.y = chart.y;
            } else if scroll != 0.0 {
                let k = (-scroll as f64 / 200.0).exp();
                let c = frame::to_data(&view, &f.plot, p.x, p.y);
                for (axis, c) in [(&mut view.x, c[0]), (&mut view.y, c[1])] {
                    axis.min = c + (axis.min - c) * k;
                    axis.max = c + (axis.max - c) * k;
                }
                ui.data_mut(|d| d.insert_temp(key, (view.x, view.y)));
            }
        }
        if response.dragged() && response.interact_pointer_pos().is_some_and(|p| plot.contains(p - response.drag_delta())) {
            let d = response.drag_delta();
            let (dx, dy) = (d.x as f64 / plot.width() as f64, d.y as f64 / plot.height() as f64);
            let (sx, sy) = (view.x.max - view.x.min, view.y.max - view.y.min);
            let (fx, fy) = (if view.x.reversed { -1.0 } else { 1.0 }, if view.y.reversed { -1.0 } else { 1.0 });
            view.x.min -= dx * sx * fx;
            view.x.max -= dx * sx * fx;
            view.y.min += dy * sy * fy;
            view.y.max += dy * sy * fy;
            ui.data_mut(|d| d.insert_temp(key, (view.x, view.y)));
        }
        chart_ui(ui, &painter, &view, f, t, hover, textures);
    }
    if let (Some(table), Some(tr)) = (&figure.table, layout.table) {
        let rect = er(&tr);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        egui::ScrollArea::both().id_salt(id.with("table")).show(&mut child, |ui| table_ui(ui, table, t));
    }
    if let Some(fr) = layout.footer {
        painter.text(pos2(fr.x + 4.0, fr.y + fr.h / 2.0), Align2::LEFT_CENTER, &figure.footer, FontId::monospace(11.0), t.muted);
    }
}

pub fn table_ui(ui: &mut Ui, table: &instaoptics_render::Table, t: &Tokens) {
    egui::Grid::new(ui.next_auto_id()).striped(true).spacing([16.0, 2.0]).show(ui, |ui| {
        for h in &table.header {
            ui.label(egui::RichText::new(h).color(t.muted).small());
        }
        ui.end_row();
        for row in &table.rows {
            for cell in row {
                ui.label(egui::RichText::new(cell).monospace());
            }
            ui.end_row();
        }
    });
}

fn dashed(painter: &egui::Painter, points: &[Pos2], stroke: Stroke, dash: Dash) {
    match dash {
        Dash::Solid => {
            painter.add(Shape::line(points.to_vec(), stroke));
        }
        Dash::Dashed => {
            painter.extend(Shape::dashed_line(points, stroke, 5.0, 3.0));
        }
        Dash::Dotted => {
            painter.extend(Shape::dashed_line(points, stroke, 1.2, 3.0));
        }
    }
}

fn chart_ui(ui: &Ui, painter: &egui::Painter, chart: &Chart, f: &ChartFrame, t: &Tokens, hover: Option<Pos2>, textures: &mut Textures) {
    let plot = er(&f.plot);
    if plot.width() < 4.0 || plot.height() < 4.0 {
        return;
    }
    let faint = t.color(instaoptics_render::Color::Faint);
    if !chart.title.is_empty() {
        painter.text(pos2(plot.center().x, plot.min.y - 6.0), Align2::CENTER_BOTTOM, &chart.title, FontId::proportional(12.0), t.text);
    }
    let xt = nice_ticks(chart.x.min.min(chart.x.max), chart.x.max.max(chart.x.min), ((plot.width() / 80.0) as usize).max(2));
    let yt = nice_ticks(chart.y.min.min(chart.y.max), chart.y.max.max(chart.y.min), ((plot.height() / 50.0) as usize).max(2));
    let to = |p: [f64; 2]| {
        let (x, y) = frame::to_screen(chart, &f.plot, p);
        pos2(x, y)
    };
    if chart.axes {
        for v in &xt {
            let x = to([*v, chart.y.min]).x;
            painter.line_segment([pos2(x, plot.min.y), pos2(x, plot.max.y)], Stroke::new(1.0, faint));
        }
        for v in &yt {
            let y = to([chart.x.min, *v]).y;
            painter.line_segment([pos2(plot.min.x, y), pos2(plot.max.x, y)], Stroke::new(1.0, faint));
        }
    }
    let clipped = painter.with_clip_rect(plot);
    for item in &chart.items {
        match item {
            Item::Line { points, color, dash, width } => {
                let stroke = Stroke::new(*width, t.color(*color));
                let mut run: Vec<Pos2> = Vec::new();
                for p in points {
                    if p[0].is_finite() && p[1].is_finite() {
                        run.push(to(*p));
                    } else {
                        if run.len() > 1 {
                            dashed(&clipped, &run, stroke, *dash);
                        }
                        run.clear();
                    }
                }
                if run.len() > 1 {
                    dashed(&clipped, &run, stroke, *dash);
                }
            }
            Item::Points { points, color, radius } => {
                let c = t.color(*color);
                for p in points.iter().filter(|p| p[0].is_finite() && p[1].is_finite()) {
                    clipped.circle_filled(to(*p), *radius, c);
                }
            }
            Item::Polygon { points, fill, stroke } => {
                let pts: Vec<Pos2> = points.iter().map(|p| to(*p)).collect();
                let fill = t.color(*fill).gamma_multiply(0.55);
                // Lens outlines are not always convex; fill them as a strip between the front and back halves.
                let n = pts.len();
                if n >= 4 {
                    let mut mesh = Mesh::default();
                    for p in &pts {
                        mesh.colored_vertex(*p, fill);
                    }
                    for i in 0..n / 2 - 1 {
                        let (a, b, c, d) = (i as u32, (i + 1) as u32, (n - 2 - i) as u32, (n - 1 - i) as u32);
                        mesh.add_triangle(a, b, d);
                        mesh.add_triangle(b, c, d);
                    }
                    clipped.add(Shape::mesh(mesh));
                }
                let mut closed = pts;
                if let Some(first) = closed.first().copied() {
                    closed.push(first);
                }
                clipped.add(Shape::line(closed, Stroke::new(1.0, t.color(*stroke))));
            }
            Item::Circle { center, radius, color, dash } => {
                let c = to(*center);
                let rad = (*radius / (chart.x.max - chart.x.min)).abs() as f32 * plot.width();
                let steps = 96;
                let pts: Vec<Pos2> = (0..=steps).map(|k| { let a = k as f32 / steps as f32 * std::f32::consts::TAU; c + rad * Vec2::new(a.cos(), a.sin()) }).collect();
                dashed(&clipped, &pts, Stroke::new(1.0, t.color(*color)), *dash);
            }
            Item::Bars { bars, color } => {
                let c = t.color(*color);
                for b in bars {
                    let a = to([b[0] - b[1] / 2.0, 0.0]);
                    let z = to([b[0] + b[1] / 2.0, b[2]]);
                    clipped.rect_filled(Rect::from_two_pos(a, z), 0.0, c);
                }
            }
            Item::Image { width, height, values, colormap: map, range, extent } => {
                // Figures are cached by their windows, so the data pointer identifies the image; the texture is built
                // once per result instead of once per frame.
                let key = egui::Id::unique(("figure-image", values.as_ptr() as usize, values.len(), *map as u8, range[0].to_bits(), range[1].to_bits()));
                let texture = match textures.get(key) {
                    Some(t) => t,
                    None => {
                        let span = (range[1] - range[0]).max(f32::EPSILON);
                        let pixels: Vec<Color32> = values
                            .iter()
                            .map(|v| if v.is_finite() { let c = colormap(*map, (v - range[0]) / span); Color32::from_rgb(c[0], c[1], c[2]) } else { Color32::TRANSPARENT })
                            .collect();
                        let image = egui::ColorImage::new([*width, *height], pixels);
                        let t = ui.ctx().load_texture(format!("{key:?}"), image, egui::TextureOptions::LINEAR);
                        textures.insert(key, t.clone());
                        t
                    }
                };
                let rect = Rect::from_two_pos(to([extent[0], extent[3]]), to([extent[1], extent[2]]));
                clipped.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            Item::VLine { x, label } => {
                let sx = to([*x, chart.y.min]).x;
                if sx >= plot.min.x && sx <= plot.max.x {
                    dashed(&clipped, &[pos2(sx, plot.min.y), pos2(sx, plot.max.y)], Stroke::new(1.0, t.muted), Dash::Dashed);
                    clipped.text(pos2(sx + 3.0, plot.min.y + 3.0), Align2::LEFT_TOP, label, FontId::proportional(10.0), t.muted);
                }
            }
            Item::Label { at, text } => {
                clipped.text(to(*at) + Vec2::new(6.0, -4.0), Align2::LEFT_BOTTOM, text, FontId::proportional(11.0), t.text);
            }
        }
    }
    if chart.axes {
        painter.rect_stroke(plot, 0.0, Stroke::new(1.0, t.muted), StrokeKind::Inside);
        let step = |v: &[f64]| if v.len() > 1 { v[1] - v[0] } else { 0.0 };
        let (sx, sy) = (step(&xt), step(&yt));
        for v in &xt {
            painter.text(pos2(to([*v, chart.y.min]).x, plot.max.y + 4.0), Align2::CENTER_TOP, tick_label(*v, sx), FontId::proportional(10.5), t.muted);
        }
        for v in &yt {
            painter.text(pos2(plot.min.x - 6.0, to([chart.x.min, *v]).y), Align2::RIGHT_CENTER, tick_label(*v, sy), FontId::proportional(10.5), t.muted);
        }
        painter.text(pos2(plot.center().x, plot.max.y + 20.0), Align2::CENTER_TOP, &chart.x_label, FontId::proportional(11.0), t.text);
        let galley = painter.layout_no_wrap(chart.y_label.clone(), FontId::proportional(11.0), t.text);
        let pos = pos2(f.cell.x + 4.0, plot.center().y + galley.size().x / 2.0);
        painter.add(egui::epaint::TextShape::new(pos, galley, t.text).with_angle(-std::f32::consts::FRAC_PI_2));
    }
    if let (Some((map, range, unit)), Some(bar)) = (&chart.colorbar, f.colorbar) {
        let bar = er(&bar);
        let steps = 64;
        for i in 0..steps {
            let c = colormap(*map, 1.0 - i as f32 / steps as f32);
            let y0 = bar.min.y + bar.height() * i as f32 / steps as f32;
            painter.rect_filled(Rect::from_min_size(pos2(bar.min.x, y0), Vec2::new(bar.width(), bar.height() / steps as f32 + 0.5)), 0.0, Color32::from_rgb(c[0], c[1], c[2]));
        }
        painter.rect_stroke(bar, 0.0, Stroke::new(1.0, t.muted), StrokeKind::Inside);
        let step = ((range[1] - range[0]) / 10.0) as f64;
        painter.text(pos2(bar.max.x + 4.0, bar.min.y), Align2::LEFT_TOP, tick_label(range[1] as f64, step), FontId::proportional(10.0), t.muted);
        painter.text(pos2(bar.max.x + 4.0, bar.max.y), Align2::LEFT_BOTTOM, tick_label(range[0] as f64, step), FontId::proportional(10.0), t.muted);
        painter.text(pos2(bar.min.x, bar.min.y - 4.0), Align2::LEFT_BOTTOM, unit, FontId::proportional(10.0), t.muted);
    }
    if let Some(p) = hover {
        let d = frame::to_data(chart, &f.plot, p.x, p.y);
        let text = format!("{}, {}", short(d[0]), short(d[1]));
        painter.text(pos2(plot.max.x - 4.0, plot.min.y + 4.0), Align2::RIGHT_TOP, text, FontId::monospace(10.5), t.muted);
    }
}

fn short(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && (a < 1e-3 || a >= 1e5) { format!("{v:.3e}") } else { format!("{v:.4}") }
}
