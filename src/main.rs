// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{Color32, ColorImage, Pos2, TextureHandle, TextureOptions};

/// Fixed canvas resolution in pixels.
const CANVAS_W: usize = 1200;
const CANVAS_H: usize = 800;

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Brush,
    Eraser,
    FloodFill,
    Line,
    Shape,
    Crop,
}

#[derive(PartialEq, Clone, Copy)]
enum ShapeKind {
    Rect,
    Ellipse,
}

struct MasterPaint {
    /// Flat RGBA pixel buffer for the canvas (row-major).
    pixels: Vec<Color32>,
    /// GPU texture handle; rebuilt whenever `dirty` is true.
    texture: Option<TextureHandle>,
    tool: Tool,
    shape_kind: ShapeKind,
    brush_size: f32,
    color: Color32,
    /// Last cursor position while dragging — used by Brush/Eraser to
    /// interpolate strokes between frames.
    last_pos: Option<Pos2>,
    /// Anchor point of the current drag — used by Line, Shape, and Crop.
    drag_start: Option<Pos2>,
    /// Set whenever `pixels` changes; triggers a texture re-upload.
    dirty: bool,
}

impl Default for MasterPaint {
    fn default() -> Self {
        Self {
            pixels: vec![Color32::WHITE; CANVAS_W * CANVAS_H],
            texture: None,
            tool: Tool::Brush,
            shape_kind: ShapeKind::Rect,
            brush_size: 5.0,
            color: Color32::BLACK,
            last_pos: None,
            drag_start: None,
            dirty: true,
        }
    }
}

impl MasterPaint {
    fn active_color(&self) -> Color32 {
        match self.tool {
            Tool::Eraser => Color32::WHITE,
            _ => self.color,
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color32) {
        if x >= 0 && x < CANVAS_W as i32 && y >= 0 && y < CANVAS_H as i32 {
            self.pixels[y as usize * CANVAS_W + x as usize] = color;
        }
    }

    /// Paint a filled circle of `brush_size` radius centred at (cx, cy).
    fn stamp_circle(&mut self, cx: i32, cy: i32, color: Color32) {
        let r = self.brush_size.round() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.set_pixel(cx + dx, cy + dy, color);
                }
            }
        }
    }

    /// Convert a screen position inside `rect` to canvas pixel coordinates.
    fn to_canvas(&self, pos: Pos2, rect: egui::Rect) -> (i32, i32) {
        (
            ((pos.x - rect.min.x) / rect.width() * CANVAS_W as f32) as i32,
            ((pos.y - rect.min.y) / rect.height() * CANVAS_H as f32) as i32,
        )
    }

    /// Core primitive: Bresenham line + stamp_circle in canvas pixel coords.
    /// Used by every tool that draws strokes.
    fn stroke_canvas(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: Color32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1i32 } else { -1 };
        let sy = if y0 < y1 { 1i32 } else { -1 };
        let mut err = dx - dy;
        loop {
            self.stamp_circle(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x0 += sx;
            }
            if e2 < dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn paint_dot(&mut self, pos: Pos2, rect: egui::Rect) {
        let (cx, cy) = self.to_canvas(pos, rect);
        let c = self.active_color();
        self.stamp_circle(cx, cy, c);
        self.dirty = true;
    }

    /// Bresenham line from `a` to `b`, stamping a circle at every step so
    /// fast mouse movement doesn't produce gaps.
    fn paint_stroke(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let c = self.active_color();
        self.stroke_canvas(x0, y0, x1, y1, c);
        self.dirty = true;
    }

    /// BFS flood fill: replace all pixels connected to the clicked point that
    /// share its colour with the current brush colour.
    fn flood_fill(&mut self, pos: Pos2, rect: egui::Rect) {
        let (sx, sy) = self.to_canvas(pos, rect);
        if sx < 0 || sx >= CANVAS_W as i32 || sy < 0 || sy >= CANVAS_H as i32 {
            return;
        }
        let (tx, ty) = (sx as usize, sy as usize);
        let target = self.pixels[ty * CANVAS_W + tx];
        let fill = self.color;
        if target == fill {
            return;
        }

        let mut queue = std::collections::VecDeque::new();
        // Mark the seed immediately to prevent duplicate enqueuing.
        self.pixels[ty * CANVAS_W + tx] = fill;
        queue.push_back((tx, ty));

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < CANVAS_W
                    && ny < CANVAS_H
                    && self.pixels[ny * CANVAS_W + nx] == target
                {
                    self.pixels[ny * CANVAS_W + nx] = fill;
                    queue.push_back((nx, ny));
                }
            }
        }
        self.dirty = true;
    }

    /// Commit a straight line stroke from `a` to `b`.
    fn commit_line(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        self.stroke_canvas(x0, y0, x1, y1, self.color);
        self.dirty = true;
    }

    /// Commit a shape (rect or ellipse outline) bounded by `a` and `b`.
    fn commit_shape(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let c = self.color;

        match self.shape_kind {
            ShapeKind::Rect => {
                let (lx, rx) = (x0.min(x1), x0.max(x1));
                let (ty, by) = (y0.min(y1), y0.max(y1));
                self.stroke_canvas(lx, ty, rx, ty, c); // top
                self.stroke_canvas(rx, ty, rx, by, c); // right
                self.stroke_canvas(rx, by, lx, by, c); // bottom
                self.stroke_canvas(lx, by, lx, ty, c); // left
            }
            ShapeKind::Ellipse => {
                let cx = (x0 + x1) / 2;
                let cy = (y0 + y1) / 2;
                let rx = ((x1 - x0).abs() / 2).max(1);
                let ry = ((y1 - y0).abs() / 2).max(1);
                // Sample ≥60 points around the perimeter.
                let n = ((std::f64::consts::TAU * rx.max(ry) as f64) as usize).max(60);
                let mut prev = (cx + rx, cy);
                for i in 1..=n {
                    let angle = std::f64::consts::TAU * i as f64 / n as f64;
                    let nx = cx + (rx as f64 * angle.cos()).round() as i32;
                    let ny = cy + (ry as f64 * angle.sin()).round() as i32;
                    self.stroke_canvas(prev.0, prev.1, nx, ny, c);
                    prev = (nx, ny);
                }
            }
        }
        self.dirty = true;
    }

    /// Crop: scale the selected region up to fill the full canvas
    /// (nearest-neighbour). The canvas stays 1200×800.
    fn commit_crop(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let lx = x0.min(x1).max(0) as usize;
        let rx = (x0.max(x1) as usize).min(CANVAS_W - 1);
        let ty = y0.min(y1).max(0) as usize;
        let by = (y0.max(y1) as usize).min(CANVAS_H - 1);
        let sw = rx.saturating_sub(lx);
        let sh = by.saturating_sub(ty);
        if sw < 2 || sh < 2 {
            return;
        }

        // Build a new buffer sampling [lx..rx, ty..by] scaled to the full canvas.
        let mut new_pixels = vec![Color32::WHITE; CANVAS_W * CANVAS_H];
        for dy in 0..CANVAS_H {
            for dx in 0..CANVAS_W {
                let sx = (lx + dx * sw / CANVAS_W).min(CANVAS_W - 1);
                let sy = (ty + dy * sh / CANVAS_H).min(CANVAS_H - 1);
                new_pixels[dy * CANVAS_W + dx] = self.pixels[sy * CANVAS_W + sx];
            }
        }
        self.pixels = new_pixels;
        self.dirty = true;
    }
}

impl eframe::App for MasterPaint {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Toolbar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Tool selector
                ui.label("Tool:");
                ui.selectable_value(&mut self.tool, Tool::Brush, "Brush");
                ui.selectable_value(&mut self.tool, Tool::Eraser, "Eraser");
                ui.selectable_value(&mut self.tool, Tool::FloodFill, "Fill");
                ui.selectable_value(&mut self.tool, Tool::Line, "Line");
                ui.selectable_value(&mut self.tool, Tool::Shape, "Shape");
                ui.selectable_value(&mut self.tool, Tool::Crop, "Crop");
                ui.separator();

                // Shape-kind sub-selector (only when Shape is active)
                if self.tool == Tool::Shape {
                    ui.selectable_value(&mut self.shape_kind, ShapeKind::Rect, "Rect");
                    ui.selectable_value(&mut self.shape_kind, ShapeKind::Ellipse, "Ellipse");
                    ui.separator();
                }

                // Brush size (not relevant for FloodFill or Crop)
                if self.tool != Tool::FloodFill && self.tool != Tool::Crop {
                    ui.label("Size:");
                    ui.add(
                        egui::Slider::new(&mut self.brush_size, 1.0_f32..=50.0)
                            .step_by(1.0),
                    );
                    ui.separator();
                }

                // Color controls (not shown for Eraser or Crop)
                if self.tool != Tool::Eraser && self.tool != Tool::Crop {
                    ui.label("Color:");
                    ui.color_edit_button_srgba(&mut self.color);
                    ui.separator();

                    // Quick-access color palette
                    for swatch in [
                        Color32::BLACK,
                        Color32::DARK_GRAY,
                        Color32::GRAY,
                        Color32::LIGHT_GRAY,
                        Color32::WHITE,
                        Color32::RED,
                        Color32::from_rgb(0, 200, 0),    // green
                        Color32::BLUE,
                        Color32::YELLOW,
                        Color32::from_rgb(255, 165, 0),  // orange
                        Color32::from_rgb(160, 32, 240), // purple
                        Color32::from_rgb(0, 200, 200),  // cyan
                        Color32::from_rgb(255, 20, 147), // pink
                        Color32::from_rgb(139, 69, 19),  // brown
                    ] {
                        let selected = self.color == swatch;
                        let stroke = if selected {
                            egui::Stroke::new(2.0, Color32::from_rgb(30, 120, 255))
                        } else {
                            egui::Stroke::new(1.0, Color32::DARK_GRAY)
                        };
                        if ui
                            .add(egui::Button::new("   ").fill(swatch).stroke(stroke))
                            .clicked()
                        {
                            self.color = swatch;
                        }
                    }
                    ui.separator();
                }

                if ui.button("Clear Canvas").clicked() {
                    self.pixels.fill(Color32::WHITE);
                    self.dirty = true;
                }
            });
        });

        // ── Canvas ───────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();

            // Upload pixel buffer to GPU whenever it has changed.
            if self.dirty || self.texture.is_none() {
                let img = ColorImage {
                    size: [CANVAS_W, CANVAS_H],
                    pixels: self.pixels.clone(),
                };
                match self.texture {
                    Some(ref mut t) => t.set(img, TextureOptions::NEAREST),
                    None => {
                        self.texture =
                            Some(ctx.load_texture("canvas", img, TextureOptions::NEAREST));
                    }
                }
                self.dirty = false;
            }

            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::drag());

            if let Some(ref texture) = self.texture {
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }

            // ── Live drag preview (Line / Shape / Crop) ───────────────────────
            // Scale brush size from canvas pixels → screen pixels for preview.
            let screen_brush = self.brush_size * rect.width() / CANVAS_W as f32;

            if let Some(start) = self.drag_start {
                let end = ui
                    .ctx()
                    .input(|i| i.pointer.hover_pos())
                    .unwrap_or(start);
                let painter = ui.painter();

                match self.tool {
                    Tool::Line => {
                        painter.line_segment(
                            [start, end],
                            egui::Stroke::new(screen_brush.max(1.0), self.color),
                        );
                    }
                    Tool::Shape => {
                        let pr = egui::Rect::from_two_pos(start, end);
                        let stroke =
                            egui::Stroke::new(screen_brush.max(1.0), self.color);
                        match self.shape_kind {
                            ShapeKind::Rect => {
                                painter.rect_stroke(pr, 0.0, stroke, egui::StrokeKind::Middle);
                            }
                            ShapeKind::Ellipse => {
                                let c = pr.center();
                                let rx = pr.width() / 2.0;
                                let ry = pr.height() / 2.0;
                                let n = 60usize;
                                let pts: Vec<egui::Pos2> = (0..n)
                                    .map(|i| {
                                        let a = std::f32::consts::TAU
                                            * i as f32
                                            / n as f32;
                                        egui::pos2(
                                            c.x + rx * a.cos(),
                                            c.y + ry * a.sin(),
                                        )
                                    })
                                    .collect();
                                painter.add(egui::epaint::PathShape::closed_line(
                                    pts, stroke,
                                ));
                            }
                        }
                    }
                    Tool::Crop => {
                        let cr = egui::Rect::from_two_pos(start, end);
                        // Blue selection rectangle
                        painter.rect_stroke(
                            cr,
                            0.0,
                            egui::Stroke::new(2.0, Color32::from_rgb(30, 120, 255)),
                            egui::StrokeKind::Middle,
                        );
                        // Semi-transparent overlay outside the selection
                        let overlay = Color32::from_rgba_unmultiplied(0, 0, 0, 60);
                        painter.rect_filled(
                            egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, cr.min.y)),
                            0.0, overlay,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(egui::pos2(rect.min.x, cr.max.y), rect.max),
                            0.0, overlay,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(egui::pos2(rect.min.x, cr.min.y), egui::pos2(cr.min.x, cr.max.y)),
                            0.0, overlay,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(egui::pos2(cr.max.x, cr.min.y), egui::pos2(rect.max.x, cr.max.y)),
                            0.0, overlay,
                        );
                    }
                    _ => {}
                }
            }

            // ── Input ─────────────────────────────────────────────────────────
            let (hover, down, pressed) = ctx.input(|i| (
                i.pointer.hover_pos(),
                i.pointer.primary_down(),
                i.pointer.primary_pressed(),
            ));

            // Crosshair only while over the canvas, not over toolbar widgets.
            if hover.map(|p| rect.contains(p)).unwrap_or(false) {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }

            match self.tool {
                // ── FloodFill: single click ────────────────────────────────────
                Tool::FloodFill => {
                    if pressed {
                        if let Some(pos) = hover {
                            if rect.contains(pos) {
                                self.flood_fill(pos, rect);
                            }
                        }
                    }
                }

                // ── Line / Shape / Crop: drag to define, commit on release ──────
                Tool::Line | Tool::Shape | Tool::Crop => {
                    if down {
                        if let Some(pos) = hover {
                            // Only start a drag from inside the canvas.
                            if self.drag_start.is_none() && rect.contains(pos) {
                                self.drag_start = Some(pos);
                            }
                        }
                    } else if let Some(start) = self.drag_start.take() {
                        let end = hover.unwrap_or(start);
                        match self.tool {
                            Tool::Line  => self.commit_line(start, end, rect),
                            Tool::Shape => self.commit_shape(start, end, rect),
                            Tool::Crop  => self.commit_crop(start, end, rect),
                            _ => unreachable!(),
                        }
                    }
                }

                // ── Brush / Eraser: continuous stroke ──────────────────────────
                _ => {
                    if down {
                        if let Some(pos) = hover {
                            if rect.contains(pos) {
                                if let Some(last) = self.last_pos {
                                    self.paint_stroke(last, pos, rect);
                                } else {
                                    self.paint_dot(pos, rect);
                                }
                                self.last_pos = Some(pos);
                            } else {
                                self.last_pos = None;
                            }
                        } else {
                            // Cursor left the window while button was held; clear so
                            // re-entry doesn't streak from the old position.
                            self.last_pos = None;
                        }
                    } else {
                        self.last_pos = None;
                    }
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "RustPaint",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1300.0, 900.0])
                .with_title("RustPaint"),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(MasterPaint::default()))),
    )
}
