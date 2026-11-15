//! 3D layout: lens elements as shaded bodies of revolution and real rays, drawn with the egui painter. Drag orbits,
//! right-drag pans, the wheel zooms, double-click resets.

use egui::{Color32, Mesh, Pos2, Sense, Shape, Stroke, Ui, Vec2};
use instaoptics_core::layout::{Body3d, Layout3d};
use instaoptics_core::{AnalysisRequest, AnalysisResult};

use crate::app::Workbench;
use crate::panels::Panel;
use crate::worker::{JobKey, Output, tag};

pub struct State {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan: Vec2,
}

impl Default for State {
    fn default() -> Self {
        Self { yaw: -0.6, pitch: 0.35, zoom: 1.0, pan: Vec2::ZERO }
    }
}

fn to_global(body: &Body3d, x: f64, y: f64, z: f64) -> [f64; 3] {
    let (o, r) = (body.origin, body.rotation);
    [o[0] + r[0] * x + r[1] * y + r[2] * z, o[1] + r[3] * x + r[4] * y + r[5] * z, o[2] + r[6] * x + r[7] * y + r[8] * z]
}

pub fn ui(wb: &mut Workbench, ui: &mut Ui) {
    let key = JobKey::Analysis(Panel::Layout3d);
    let request = AnalysisRequest::Layout3d { ring: 8 };
    let input = tag(&[&wb.system_json, "layout3d"]);
    let window = wb.analyses.entry(Panel::Layout3d).or_insert_with(|| crate::analysis::AnalysisWindow::new(Panel::Layout3d));
    if window.computed != input && wb.worker.running_tag(key) != Some(input) {
        let system = wb.session.system().clone();
        wb.worker.spawn(key, input, move |_, _| Output::Analysis(instaoptics_core::analyze(&system, &request).map_err(|e| e.0)));
    }
    let t = wb.tokens;
    let Some(AnalysisResult::Layout3d(layout)) = wb.analyses.get(&Panel::Layout3d).and_then(|w| w.result.as_ref()) else {
        ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Computing…").color(t.muted)));
        return;
    };
    let st = &mut wb.view3d;
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    if response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        st.yaw += d.x * 0.008;
        st.pitch = (st.pitch + d.y * 0.008).clamp(-1.5, 1.5);
    }
    if response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle) {
        st.pan += response.drag_delta();
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        st.zoom = (st.zoom * (scroll / 300.0).exp()).clamp(0.05, 50.0);
    }
    if response.double_clicked() {
        *st = State::default();
    }
    let painter = ui.painter_at(rect);
    draw(&painter, rect, layout, st, &t);
}

fn draw(painter: &egui::Painter, rect: egui::Rect, layout: &Layout3d, st: &State, t: &crate::theme::Tokens) {
    // Scene centre and size from the axial extent.
    let (z0, z1) = (layout.start_z, layout.image_z);
    let center = [0.0, 0.0, (z0 + z1) / 2.0];
    let extent = ((z1 - z0).abs().max(1e-3)) as f32;
    let scale = rect.width().min(rect.height() * 1.6) / extent * 0.85 * st.zoom;
    let (cy, sy, cp, sp) = (st.yaw.cos(), st.yaw.sin(), st.pitch.cos(), st.pitch.sin());
    // View: the optical axis (z) runs left to right at yaw 0, y up.
    let project = |p: [f64; 3]| -> (Pos2, f32) {
        let (x, y, z) = ((p[0] - center[0]) as f32, (p[1] - center[1]) as f32, (p[2] - center[2]) as f32);
        let (u, w) = (z * cy - x * sy, z * sy + x * cy);
        let (v, depth) = (y * cp - w * sp, y * sp + w * cp);
        (rect.center() + st.pan + Vec2::new(u * scale, -v * scale), depth)
    };
    let light = [0.4f32, 0.6, 0.7];
    let base = if t.dark { [150.0f32, 175.0, 205.0] } else { [120.0, 150.0, 190.0] };
    let mut tris: Vec<(f32, [Pos2; 3], Color32)> = Vec::new();
    let segments = 48;
    for body in &layout.elements {
        let ring: Vec<Vec<([f64; 3], Pos2, f32)>> = body
            .outline
            .iter()
            .map(|[r, z]| {
                (0..=segments)
                    .map(|k| {
                        let a = k as f64 / segments as f64 * std::f64::consts::TAU;
                        let g = to_global(body, r * a.cos(), r * a.sin(), *z);
                        let (p, d) = project(g);
                        (g, p, d)
                    })
                    .collect()
            })
            .collect();
        for w in ring.windows(2) {
            for k in 0..segments {
                let (a, b, c, d) = (&w[0][k], &w[0][k + 1], &w[1][k + 1], &w[1][k]);
                let e1 = [b.0[0] - a.0[0], b.0[1] - a.0[1], b.0[2] - a.0[2]];
                let e2 = [d.0[0] - a.0[0], d.0[1] - a.0[1], d.0[2] - a.0[2]];
                let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
                let shade = 0.35 + 0.65 * ((n[0] as f32 * light[0] + n[1] as f32 * light[1] + n[2] as f32 * light[2]) / len as f32).abs();
                let color = Color32::from_rgba_unmultiplied((base[0] * shade) as u8, (base[1] * shade) as u8, (base[2] * shade) as u8, 150);
                let depth = (a.2 + b.2 + c.2 + d.2) / 4.0;
                tris.push((depth, [a.1, b.1, c.1], color));
                tris.push((depth, [a.1, c.1, d.1], color));
            }
        }
    }
    // Back to front.
    tris.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut mesh = Mesh::default();
    for (_, [a, b, c], color) in &tris {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(*a, *color);
        mesh.colored_vertex(*b, *color);
        mesh.colored_vertex(*c, *color);
        mesh.add_triangle(i, i + 1, i + 2);
    }
    painter.add(Shape::mesh(mesh));
    for body in &layout.surfaces {
        for [r, z] in body.outline.iter().step_by((body.outline.len() / 6).max(1)) {
            let pts: Vec<Pos2> = (0..=segments).map(|k| { let a = k as f64 / segments as f64 * std::f64::consts::TAU; project(to_global(body, r * a.cos(), r * a.sin(), *z)).0 }).collect();
            painter.add(Shape::line(pts, Stroke::new(1.0, t.muted)));
        }
    }
    for ray in &layout.rays {
        let pts: Vec<Pos2> = ray.points.iter().map(|p| project(*p).0).collect();
        painter.add(Shape::line(pts, Stroke::new(1.0, t.color(instaoptics_render::field_color(ray.field)))));
    }
    // Image plane outline.
    let frame = &layout.image;
    let h = layout.image_semi_height.max(1e-3) * 1.1;
    let corners = [[-h, -h], [h, -h], [h, h], [-h, h], [-h, -h]];
    let body = Body3d { outline: Vec::new(), origin: frame.origin, rotation: frame.rotation };
    let pts: Vec<Pos2> = corners.iter().map(|c| project(to_global(&body, c[0], c[1], 0.0)).0).collect();
    painter.add(Shape::line(pts, Stroke::new(1.0, t.muted)));
}
