// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{Color32, ColorImage, Pos2, TextureHandle, TextureOptions};
use std::collections::VecDeque;

const HISTORY_CAP: usize = 50;

/// Fixed canvas resolution in pixels.
const CANVAS_W: usize = 1200;
const CANVAS_H: usize = 800;

/// A rectangular pre-image of the canvas, used to undo a destructive op.
struct Patch {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    pixels: Vec<Color32>,
}

/// Inclusive axis-aligned bbox in canvas pixel coordinates.
#[derive(Clone, Copy)]
struct Bbox {
    lx: i32,
    ty: i32,
    rx: i32,
    by: i32,
}

impl Bbox {
    fn point(x: i32, y: i32) -> Self {
        Self { lx: x, ty: y, rx: x, by: y }
    }
    fn from_two(a: (i32, i32), b: (i32, i32)) -> Self {
        Self {
            lx: a.0.min(b.0),
            ty: a.1.min(b.1),
            rx: a.0.max(b.0),
            by: a.1.max(b.1),
        }
    }
    fn expand(self, r: i32) -> Self {
        Self { lx: self.lx - r, ty: self.ty - r, rx: self.rx + r, by: self.by + r }
    }
    fn union(self, o: Self) -> Self {
        Self {
            lx: self.lx.min(o.lx),
            ty: self.ty.min(o.ty),
            rx: self.rx.max(o.rx),
            by: self.by.max(o.by),
        }
    }
    fn include_point(&mut self, x: i32, y: i32) {
        if x < self.lx { self.lx = x; }
        if x > self.rx { self.rx = x; }
        if y < self.ty { self.ty = y; }
        if y > self.by { self.by = y; }
    }
    /// Clamp to the canvas; returns None if it lies entirely outside.
    fn clamped(self) -> Option<(usize, usize, usize, usize)> {
        if self.rx < 0 || self.by < 0
            || self.lx >= CANVAS_W as i32 || self.ty >= CANVAS_H as i32
        {
            return None;
        }
        let lx = self.lx.max(0) as usize;
        let rx = (self.rx as usize).min(CANVAS_W - 1);
        let ty = self.ty.max(0) as usize;
        let by = (self.by as usize).min(CANVAS_H - 1);
        if rx < lx || by < ty { None } else { Some((lx, ty, rx, by)) }
    }
}

/// Copy a rectangular region of `src` into a new `Patch`. Returns `None` if
/// the bbox lies outside the canvas.
fn make_patch(src: &[Color32], bbox: Bbox) -> Option<Patch> {
    let (lx, ty, rx, by) = bbox.clamped()?;
    let w = rx - lx + 1;
    let h = by - ty + 1;
    let mut buf = Vec::with_capacity(w * h);
    for row in ty..=by {
        let s = row * CANVAS_W + lx;
        buf.extend_from_slice(&src[s..s + w]);
    }
    Some(Patch {
        x: lx as u32,
        y: ty as u32,
        w: w as u32,
        h: h as u32,
        pixels: buf,
    })
}

/// Generate `n` points around an ellipse centred at (cx, cy) with radii (rx, ry).
fn ellipse_perimeter(
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    n: usize,
) -> impl Iterator<Item = (f32, f32)> {
    (0..n).map(move |i| {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        (cx + rx * a.cos(), cy + ry * a.sin())
    })
}

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
    /// Undo history: bounded ring of rectangular pre-image patches.
    history: VecDeque<Patch>,
    /// Pre-image captured at the start of an in-progress brush/eraser stroke.
    /// Cropped to `stroke_bbox` and committed to `history` on pointer-up.
    stroke_pre: Option<Vec<Color32>>,
    /// Bbox accumulated as the in-progress stroke paints.
    stroke_bbox: Option<Bbox>,
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
            history: VecDeque::new(),
            stroke_pre: None,
            stroke_bbox: None,
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

    // ── History ──────────────────────────────────────────────────────────────

    fn push_patch(&mut self, patch: Patch) {
        if self.history.len() >= HISTORY_CAP {
            self.history.pop_front();
        }
        self.history.push_back(patch);
    }

    fn push_full_patch(&mut self) {
        let patch = Patch {
            x: 0,
            y: 0,
            w: CANVAS_W as u32,
            h: CANVAS_H as u32,
            pixels: self.pixels.clone(),
        };
        self.push_patch(patch);
    }

    fn push_bbox_patch(&mut self, bbox: Bbox) {
        if let Some(p) = make_patch(&self.pixels, bbox) {
            self.push_patch(p);
        }
    }

    /// Begin a brush/eraser stroke: snapshot the canvas, reset the bbox.
    /// If a prior stroke was still pending (pointer left and re-entered while
    /// held), commit it first to preserve the existing per-segment behaviour.
    fn begin_stroke(&mut self) {
        self.commit_stroke();
        self.stroke_pre = Some(self.pixels.clone());
        self.stroke_bbox = None;
    }

    fn extend_stroke_bbox(&mut self, b: Bbox) {
        if self.stroke_pre.is_none() {
            return;
        }
        self.stroke_bbox = Some(match self.stroke_bbox {
            None => b,
            Some(prev) => prev.union(b),
        });
    }

    /// Crop the pending stroke pre-image to its bbox and push it as a patch.
    fn commit_stroke(&mut self) {
        let Some(pre) = self.stroke_pre.take() else { return };
        let Some(bbox) = self.stroke_bbox.take() else { return };
        if let Some(p) = make_patch(&pre, bbox) {
            self.push_patch(p);
        }
    }

    fn undo(&mut self) {
        let Some(patch) = self.history.pop_back() else { return };
        let (x, y, w, h) = (
            patch.x as usize,
            patch.y as usize,
            patch.w as usize,
            patch.h as usize,
        );
        for row in 0..h {
            let dst = (y + row) * CANVAS_W + x;
            let src = row * w;
            self.pixels[dst..dst + w].copy_from_slice(&patch.pixels[src..src + w]);
        }
        self.dirty = true;
    }

    // ── Drawing primitives ───────────────────────────────────────────────────

    /// Paint a filled circle of `brush_size` radius centred at (cx, cy).
    /// Scanline form: one slice-fill per row, no per-pixel branching.
    fn stamp_circle(&mut self, cx: i32, cy: i32, color: Color32) {
        let r = self.brush_size.round() as i32;
        if r < 0 {
            return;
        }
        self.extend_stroke_bbox(Bbox::point(cx, cy).expand(r));
        self.dirty = true;

        let r2 = r * r;
        let cw = CANVAS_W as i32;
        let ch = CANVAS_H as i32;
        for dy in -r..=r {
            let y = cy + dy;
            if y < 0 || y >= ch {
                continue;
            }
            let xw = ((r2 - dy * dy) as f32).sqrt() as i32;
            let lx = (cx - xw).max(0);
            let rx = (cx + xw).min(cw - 1);
            if rx < lx {
                continue;
            }
            let row = y as usize * CANVAS_W;
            self.pixels[row + lx as usize..=row + rx as usize].fill(color);
        }
    }

    fn to_canvas(&self, pos: Pos2, rect: egui::Rect) -> (i32, i32) {
        (
            ((pos.x - rect.min.x) / rect.width() * CANVAS_W as f32) as i32,
            ((pos.y - rect.min.y) / rect.height() * CANVAS_H as f32) as i32,
        )
    }

    /// Bresenham line stamping a circle at every step.
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
    }

    fn paint_stroke(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let c = self.active_color();
        self.stroke_canvas(x0, y0, x1, y1, c);
    }

    // ── Flood fill ───────────────────────────────────────────────────────────

    /// BFS flood fill. Returns the bbox of changed pixels, or None if nothing
    /// was filled (seed out of bounds or seed already matches the fill colour).
    fn flood_fill(&mut self, pos: Pos2, rect: egui::Rect) -> Option<Bbox> {
        let (sx, sy) = self.to_canvas(pos, rect);
        if sx < 0 || sx >= CANVAS_W as i32 || sy < 0 || sy >= CANVAS_H as i32 {
            return None;
        }
        let (tx, ty) = (sx as usize, sy as usize);
        let target = self.pixels[ty * CANVAS_W + tx];
        let fill = self.color;
        if target == fill {
            return None;
        }

        let mut queue = VecDeque::new();
        self.pixels[ty * CANVAS_W + tx] = fill;
        queue.push_back((tx, ty));
        let mut bbox = Bbox::point(sx, sy);

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
                    bbox.include_point(nx as i32, ny as i32);
                }
            }
        }
        self.dirty = true;
        Some(bbox)
    }

    // ── Commit handlers for Line / Shape / Crop drag tools ───────────────────

    fn line_or_shape_bbox(&self, a: Pos2, b: Pos2, rect: egui::Rect) -> Bbox {
        let p0 = self.to_canvas(a, rect);
        let p1 = self.to_canvas(b, rect);
        let r = self.brush_size.round() as i32;
        Bbox::from_two(p0, p1).expand(r)
    }

    fn commit_line(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        self.stroke_canvas(x0, y0, x1, y1, self.color);
    }

    fn commit_shape(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let c = self.color;

        match self.shape_kind {
            ShapeKind::Rect => {
                let (lx, rx) = (x0.min(x1), x0.max(x1));
                let (ty, by) = (y0.min(y1), y0.max(y1));
                self.stroke_canvas(lx, ty, rx, ty, c);
                self.stroke_canvas(rx, ty, rx, by, c);
                self.stroke_canvas(rx, by, lx, by, c);
                self.stroke_canvas(lx, by, lx, ty, c);
            }
            ShapeKind::Ellipse => {
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let rx = ((x1 - x0).abs() / 2).max(1) as f32;
                let ry = ((y1 - y0).abs() / 2).max(1) as f32;
                let n =
                    ((std::f32::consts::TAU * rx.max(ry)) as usize).max(60);
                let mut prev = (cx + rx, cy);
                for (nx, ny) in ellipse_perimeter(cx, cy, rx, ry, n).skip(1) {
                    self.stroke_canvas(
                        prev.0.round() as i32,
                        prev.1.round() as i32,
                        nx.round() as i32,
                        ny.round() as i32,
                        c,
                    );
                    prev = (nx, ny);
                }
                // Close the loop back to the starting sample.
                self.stroke_canvas(
                    prev.0.round() as i32,
                    prev.1.round() as i32,
                    (cx + rx).round() as i32,
                    cy.round() as i32,
                    c,
                );
            }
        }
    }

    /// Crop: nearest-neighbour scale of the selected region to the full canvas.
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

        // Swap out the source so we can write the new image into a fresh
        // buffer without aliasing self.pixels.
        let src = std::mem::take(&mut self.pixels);
        let mut dst = vec![Color32::WHITE; CANVAS_W * CANVAS_H];
        for dy in 0..CANVAS_H {
            let sy = (ty + dy * sh / CANVAS_H).min(CANVAS_H - 1);
            let src_row = sy * CANVAS_W;
            let dst_row = dy * CANVAS_W;
            for dx in 0..CANVAS_W {
                let sx = (lx + dx * sw / CANVAS_W).min(CANVAS_W - 1);
                dst[dst_row + dx] = src[src_row + sx];
            }
        }
        self.pixels = dst;
        self.dirty = true;
    }

    /// Dispatch a drag-tool release: snapshot the affected region into history
    /// and commit the operation. Called only when `self.tool` is Line, Shape,
    /// or Crop (gated by the caller's outer match).
    fn commit_drag_tool(&mut self, start: Pos2, end: Pos2, rect: egui::Rect) {
        self.commit_stroke();
        match self.tool {
            Tool::Line => {
                let bbox = self.line_or_shape_bbox(start, end, rect);
                self.push_bbox_patch(bbox);
                self.commit_line(start, end, rect);
            }
            Tool::Shape => {
                let bbox = self.line_or_shape_bbox(start, end, rect);
                self.push_bbox_patch(bbox);
                self.commit_shape(start, end, rect);
            }
            Tool::Crop => {
                self.push_full_patch();
                self.commit_crop(start, end, rect);
            }
            _ => {}
        }
    }
}

impl eframe::App for MasterPaint {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Toolbar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Tool:");
                ui.selectable_value(&mut self.tool, Tool::Brush, "Brush");
                ui.selectable_value(&mut self.tool, Tool::Eraser, "Eraser");
                ui.selectable_value(&mut self.tool, Tool::FloodFill, "Fill");
                ui.selectable_value(&mut self.tool, Tool::Line, "Line");
                ui.selectable_value(&mut self.tool, Tool::Shape, "Shape");
                ui.selectable_value(&mut self.tool, Tool::Crop, "Crop");
                ui.separator();

                if self.tool == Tool::Shape {
                    ui.selectable_value(&mut self.shape_kind, ShapeKind::Rect, "Rect");
                    ui.selectable_value(&mut self.shape_kind, ShapeKind::Ellipse, "Ellipse");
                    ui.separator();
                }

                if self.tool != Tool::FloodFill && self.tool != Tool::Crop {
                    ui.label("Size:");
                    ui.add(egui::Slider::new(&mut self.brush_size, 1.0_f32..=50.0).step_by(1.0));
                    ui.separator();
                }

                if self.tool != Tool::Eraser && self.tool != Tool::Crop {
                    ui.label("Color:");
                    ui.color_edit_button_srgba(&mut self.color);
                    ui.separator();

                    for swatch in [
                        Color32::BLACK,
                        Color32::DARK_GRAY,
                        Color32::GRAY,
                        Color32::LIGHT_GRAY,
                        Color32::WHITE,
                        Color32::RED,
                        Color32::from_rgb(0, 200, 0),
                        Color32::BLUE,
                        Color32::YELLOW,
                        Color32::from_rgb(255, 165, 0),
                        Color32::from_rgb(160, 32, 240),
                        Color32::from_rgb(0, 200, 200),
                        Color32::from_rgb(255, 20, 147),
                        Color32::from_rgb(139, 69, 19),
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
                    self.commit_stroke();
                    self.push_full_patch();
                    self.pixels.fill(Color32::WHITE);
                    self.dirty = true;
                }

                let can_undo = !self.history.is_empty();
                if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                    self.undo();
                }
            });
        });

        // ── Canvas ───────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();

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

            // ── Live drag preview ─────────────────────────────────────────────
            let screen_brush = self.brush_size * rect.width() / CANVAS_W as f32;

            if let Some(start) = self.drag_start {
                let end = ui.ctx().input(|i| i.pointer.hover_pos()).unwrap_or(start);
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
                        let stroke = egui::Stroke::new(screen_brush.max(1.0), self.color);
                        match self.shape_kind {
                            ShapeKind::Rect => {
                                painter.rect_stroke(pr, 0.0, stroke, egui::StrokeKind::Middle);
                            }
                            ShapeKind::Ellipse => {
                                let c = pr.center();
                                let rx = pr.width() / 2.0;
                                let ry = pr.height() / 2.0;
                                let pts: Vec<egui::Pos2> =
                                    ellipse_perimeter(c.x, c.y, rx, ry, 60)
                                        .map(|(x, y)| egui::pos2(x, y))
                                        .collect();
                                painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                            }
                        }
                    }
                    Tool::Crop => {
                        let cr = egui::Rect::from_two_pos(start, end);
                        painter.rect_stroke(
                            cr,
                            0.0,
                            egui::Stroke::new(2.0, Color32::from_rgb(30, 120, 255)),
                            egui::StrokeKind::Middle,
                        );
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
                            egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, cr.min.y),
                                egui::pos2(cr.min.x, cr.max.y),
                            ),
                            0.0, overlay,
                        );
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(cr.max.x, cr.min.y),
                                egui::pos2(rect.max.x, cr.max.y),
                            ),
                            0.0, overlay,
                        );
                    }
                    _ => {}
                }
            }

            // ── Input ─────────────────────────────────────────────────────────
            let (hover, down, pressed, undo_pressed) = ctx.input(|i| {
                (
                    i.pointer.hover_pos(),
                    i.pointer.primary_down(),
                    i.pointer.primary_pressed(),
                    i.key_pressed(egui::Key::Z) && i.modifiers.command,
                )
            });

            if undo_pressed {
                self.undo();
            }

            if hover.map(|p| rect.contains(p)).unwrap_or(false) {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }

            match self.tool {
                Tool::FloodFill => {
                    if pressed {
                        if let Some(pos) = hover {
                            if rect.contains(pos) {
                                self.commit_stroke();
                                let pre = self.pixels.clone();
                                if let Some(bbox) = self.flood_fill(pos, rect) {
                                    if let Some(p) = make_patch(&pre, bbox) {
                                        self.push_patch(p);
                                    }
                                }
                            }
                        }
                    }
                }

                Tool::Line | Tool::Shape | Tool::Crop => {
                    if down {
                        if let Some(pos) = hover {
                            if self.drag_start.is_none() && rect.contains(pos) {
                                self.drag_start = Some(pos);
                            }
                        }
                    } else if let Some(start) = self.drag_start.take() {
                        let end = hover.unwrap_or(start);
                        self.commit_drag_tool(start, end, rect);
                    }
                }

                _ => {
                    if down {
                        if let Some(pos) = hover {
                            if rect.contains(pos) {
                                if let Some(last) = self.last_pos {
                                    self.paint_stroke(last, pos, rect);
                                } else {
                                    self.begin_stroke();
                                    self.paint_dot(pos, rect);
                                }
                                self.last_pos = Some(pos);
                            } else {
                                self.last_pos = None;
                            }
                        } else {
                            self.last_pos = None;
                        }
                    } else {
                        self.last_pos = None;
                        self.commit_stroke();
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
