use eframe::egui;
use egui::{Color32, Pos2, RichText, Stroke, StrokeKind};

use crate::color::{color_hex, palette_hex};
use crate::constants::{PALETTE_ROWS, SELECTION_COLOR};
use crate::geom::{ellipse_perimeter, shape_geometry, ShapeGeometry};
use crate::types::{ShapeKind, Tool};

pub fn short_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

pub fn swatch_ui(ui: &mut egui::Ui, color: Color32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, color);
        let (w, c) = if selected {
            (2.0, SELECTION_COLOR)
        } else {
            (1.0, Color32::from_gray(80))
        };
        painter.rect_stroke(rect, 2.0, Stroke::new(w, c), StrokeKind::Middle);
    }
    resp
}

pub fn empty_slot_ui(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, Color32::from_gray(55));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "+",
            egui::FontId::proportional(11.0),
            Color32::from_gray(160),
        );
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            StrokeKind::Middle,
        );
    }
    resp
}

pub fn render_palette_row(ui: &mut egui::Ui, row_idx: usize, color: &mut Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (&sw, hex) in PALETTE_ROWS[row_idx]
            .iter()
            .zip(palette_hex(row_idx).iter())
        {
            if swatch_ui(ui, sw, *color == sw)
                .on_hover_text(hex.as_str())
                .clicked()
            {
                *color = sw;
            }
        }
    });
}

pub fn render_tool_picker(ui: &mut egui::Ui, tool: &mut Tool, shape_kind: &mut ShapeKind) {
    ui.label("Tool:");
    ui.selectable_value(tool, Tool::Brush, "Brush");
    ui.selectable_value(tool, Tool::Eraser, "Eraser");
    ui.selectable_value(tool, Tool::Spray, "Spray");
    ui.selectable_value(tool, Tool::FloodFill, "Fill");
    ui.selectable_value(tool, Tool::Eyedropper, "Eyedropper");
    ui.selectable_value(tool, Tool::Line, "Line");
    ui.selectable_value(tool, Tool::Shape, "Shape");
    ui.selectable_value(tool, Tool::Crop, "Crop");
    ui.selectable_value(tool, Tool::Text, "Text");
    ui.separator();

    if *tool == Tool::Shape {
        ui.selectable_value(shape_kind, ShapeKind::Rect, "Rect");
        ui.selectable_value(shape_kind, ShapeKind::Ellipse, "Ellipse");
        ui.selectable_value(shape_kind, ShapeKind::Circle, "Circle");
        ui.selectable_value(shape_kind, ShapeKind::Triangle, "Triangle");
        ui.selectable_value(shape_kind, ShapeKind::Diamond, "Diamond");
        ui.selectable_value(shape_kind, ShapeKind::Pentagon, "Pentagon");
        ui.separator();
    }
}

pub fn render_tool_sliders(
    ui: &mut egui::Ui,
    tool: Tool,
    brush_size: &mut f32,
    spray_radius: &mut f32,
    spray_density: &mut u32,
    line_thickness: &mut f32,
    text_scale: &mut u32,
) {
    if matches!(tool, Tool::Brush | Tool::Eraser) {
        ui.label("Size:");
        ui.add(egui::Slider::new(brush_size, 1.0_f32..=50.0).step_by(1.0));
        ui.separator();
    }

    if tool == Tool::Spray {
        ui.label("Radius:");
        ui.add(egui::Slider::new(spray_radius, 5.0_f32..=60.0).step_by(1.0));
        ui.label("Density:");
        ui.add(egui::Slider::new(spray_density, 5_u32..=80));
        ui.separator();
    }

    if matches!(tool, Tool::Line | Tool::Shape) {
        ui.label("Thickness:");
        ui.add(egui::Slider::new(line_thickness, 1.0_f32..=30.0).step_by(1.0));
        ui.separator();
    }

    if tool == Tool::Text {
        ui.label("Size:");
        for (label, val) in [("1×", 1u32), ("2×", 2), ("3×", 3), ("4×", 4)] {
            let selected = *text_scale == val;
            let mut btn = egui::Button::new(label);
            if selected {
                btn = btn.stroke(Stroke::new(2.0, SELECTION_COLOR));
            }
            if ui.add(btn).clicked() {
                *text_scale = val;
            }
        }
        ui.separator();
    }
}

pub fn render_color_palette(
    ui: &mut egui::Ui,
    color: &mut Color32,
    recent_colors: &[Color32],
    saved_slots: &mut [Option<Color32>],
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;

        ui.horizontal(|ui| {
            ui.label("Color:");
            ui.color_edit_button_srgba(color);
            ui.label(
                RichText::new("Custom…")
                    .color(Color32::from_gray(140))
                    .size(11.0),
            );
        });

        ui.label(
            RichText::new("Palette")
                .color(Color32::from_gray(140))
                .size(10.0),
        );
        render_palette_row(ui, 0, color);
        render_palette_row(ui, 1, color);

        if !recent_colors.is_empty() {
            ui.label(
                RichText::new("Recent")
                    .color(Color32::from_gray(140))
                    .size(10.0),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for &c in recent_colors {
                    let hex = color_hex(c);
                    if swatch_ui(ui, c, *color == c).on_hover_text(hex).clicked() {
                        *color = c;
                    }
                }
            });
        }

        ui.label(
            RichText::new("Saved  (right-click to save current color)")
                .color(Color32::from_gray(140))
                .size(10.0),
        );
        let cur = *color;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for slot in saved_slots.iter_mut() {
                let resp = if let Some(c) = *slot {
                    swatch_ui(ui, c, cur == c).on_hover_text(color_hex(c))
                } else {
                    empty_slot_ui(ui).on_hover_text("Empty — right-click to save current color")
                };
                if resp.clicked() {
                    if let Some(c) = *slot {
                        *color = c;
                    }
                }
                if resp.secondary_clicked() {
                    *slot = Some(cur);
                }
            }
        });
    });
}

pub fn preview_shape_geometry(painter: &egui::Painter, geom: &ShapeGeometry, stroke: Stroke) {
    match geom {
        ShapeGeometry::Rect { lx, ty, rx, by } => {
            let pr = egui::Rect::from_min_max(egui::pos2(*lx, *ty), egui::pos2(*rx, *by));
            painter.rect_stroke(pr, 0.0, stroke, StrokeKind::Middle);
        }
        ShapeGeometry::Ellipse { cx, cy, rx, ry } => {
            let pts: Vec<Pos2> = ellipse_perimeter(*cx, *cy, *rx, *ry, 60)
                .map(|(x, y)| egui::pos2(x, y))
                .collect();
            painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
        }
        ShapeGeometry::Polygon(pts) => {
            let pts: Vec<Pos2> = pts.iter().map(|(x, y)| egui::pos2(*x, *y)).collect();
            painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_drag_preview(
    tool: Tool,
    shape_kind: ShapeKind,
    color: Color32,
    line_thickness: f32,
    drag_start: Option<Pos2>,
    canvas_rect: egui::Rect,
    hover: Option<Pos2>,
    painter: &egui::Painter,
) {
    let screen_line = line_thickness * canvas_rect.width() / crate::constants::CANVAS_W as f32;
    let Some(start) = drag_start else { return };
    let end = hover.unwrap_or(start);

    match tool {
        Tool::Line => {
            painter.line_segment([start, end], Stroke::new(screen_line.max(1.0), color));
        }
        Tool::Shape => {
            let stroke = Stroke::new(screen_line.max(1.0), color);
            let geom = shape_geometry(shape_kind, start.x, start.y, end.x, end.y);
            preview_shape_geometry(painter, &geom, stroke);
        }
        Tool::Crop => {
            let cr = egui::Rect::from_two_pos(start, end);
            painter.rect_stroke(
                cr,
                0.0,
                Stroke::new(2.0, SELECTION_COLOR),
                StrokeKind::Middle,
            );
            let overlay = Color32::from_rgba_unmultiplied(0, 0, 0, 60);
            painter.rect_filled(
                egui::Rect::from_min_max(canvas_rect.min, egui::pos2(canvas_rect.max.x, cr.min.y)),
                0.0,
                overlay,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(egui::pos2(canvas_rect.min.x, cr.max.y), canvas_rect.max),
                0.0,
                overlay,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(canvas_rect.min.x, cr.min.y),
                    egui::pos2(cr.min.x, cr.max.y),
                ),
                0.0,
                overlay,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(cr.max.x, cr.min.y),
                    egui::pos2(canvas_rect.max.x, cr.max.y),
                ),
                0.0,
                overlay,
            );
        }
        _ => {}
    }
}

pub fn render_text_cursor(
    tool: Tool,
    text_origin: Option<(i32, i32)>,
    text_buffer: &str,
    text_scale: u32,
    color: Color32,
    canvas_rect: egui::Rect,
    painter: &egui::Painter,
) {
    if tool != Tool::Text {
        return;
    }
    let Some((ox, oy)) = text_origin else {
        return;
    };
    let sx =
        canvas_rect.min.x + ox as f32 / crate::constants::CANVAS_W as f32 * canvas_rect.width();
    let sy =
        canvas_rect.min.y + oy as f32 / crate::constants::CANVAS_H as f32 * canvas_rect.height();
    let preview = format!("{text_buffer}_");
    let px_per_canvas = canvas_rect.width() / crate::constants::CANVAS_W as f32;
    let font_size = (8.0 * text_scale as f32 * px_per_canvas).max(8.0);
    painter.text(
        egui::pos2(sx, sy),
        egui::Align2::LEFT_TOP,
        &preview,
        egui::FontId::monospace(font_size),
        color,
    );
}

pub fn render_eraser_cursor(
    tool: Tool,
    brush_size: f32,
    canvas_rect: egui::Rect,
    hover: Option<Pos2>,
    painter: &egui::Painter,
) {
    if tool != Tool::Eraser {
        return;
    }
    let Some(pos) = hover else { return };
    if !canvas_rect.contains(pos) {
        return;
    }
    let half = brush_size * canvas_rect.width() / crate::constants::CANVAS_W as f32;
    let box_rect = egui::Rect::from_center_size(pos, egui::vec2(half * 2.0, half * 2.0));
    painter.rect_filled(box_rect, 0.0, Color32::WHITE);
    painter.rect_stroke(
        box_rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        StrokeKind::Middle,
    );
}

pub fn render_file_status(ui: &mut egui::Ui, msg: &str, ok: bool) {
    let color = if ok {
        Color32::from_rgb(74, 222, 128)
    } else {
        Color32::from_rgb(248, 113, 113)
    };
    ui.label(RichText::new(msg).color(color).size(12.0));
}
