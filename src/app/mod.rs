use std::collections::VecDeque;
use std::time::Instant;

use eframe::egui;
use egui::{Color32, ColorImage, Pos2, TextureHandle, TextureOptions};

use crate::constants::{CANVAS_H, CANVAS_W, HISTORY_CAP};
use crate::font::FONT8X8;
use crate::geom::{
    copy_region, ellipse_perimeter, make_patch, patch_bbox, shape_geometry, Bbox, ShapeGeometry,
};
use crate::rng::next_rand;
use crate::types::{Patch, ShapeKind, Tool};
use crate::ui::{
    render_color_palette, render_drag_preview, render_eraser_cursor, render_file_status,
    render_text_cursor, render_tool_picker, render_tool_sliders, short_name,
};

pub struct RustPaint {
    /// Flat RGBA pixel buffer for the canvas (row-major).
    pixels: Vec<Color32>,
    /// GPU texture handle; rebuilt whenever `dirty` is true.
    texture: Option<TextureHandle>,
    tool: Tool,
    shape_kind: ShapeKind,
    brush_size: f32,
    /// Independent stroke thickness for Line and Shape tools.
    line_thickness: f32,
    color: Color32,
    /// Last cursor position while dragging — used by Brush/Eraser/Spray to
    /// interpolate strokes between frames.
    last_pos: Option<Pos2>,
    /// Anchor point of the current drag — used by Line, Shape, and Crop.
    drag_start: Option<Pos2>,
    /// Set whenever `pixels` changes; triggers a texture re-upload.
    dirty: bool,
    /// Changed canvas region to upload when `dirty` is true. None means full canvas.
    dirty_bbox: Option<Bbox>,
    /// Undo history: bounded ring of rectangular pre-image patches.
    history: VecDeque<Patch>,
    /// Redo stack: populated by undo, cleared by any new canvas action.
    redo: VecDeque<Patch>,
    /// Original pixels touched by an in-progress brush/eraser/spray stroke.
    stroke_originals: Vec<(usize, Color32)>,
    /// Reusable per-pixel mark array for de-duplicating `stroke_originals`.
    stroke_marks: Vec<u32>,
    stroke_epoch: u32,
    stroke_bbox: Option<Bbox>,
    stroke_active: bool,
    /// Path of the currently open file; empty when unsaved.
    file_path: String,
    /// Status message from the last file operation.
    file_status: Option<String>,
    /// True = success (green), false = error (red).
    file_status_ok: bool,
    /// When the status was set; cleared after 3 s.
    file_status_time: Option<Instant>,

    // ── Spray tool ───────────────────────────────────────────────────────────
    spray_radius: f32,
    spray_density: u32,
    /// xorshift32 PRNG state; seeded from system time at startup.
    rng: u32,

    // ── Text tool ────────────────────────────────────────────────────────────
    /// Canvas-pixel origin of the in-progress text entry (top-left of first char).
    text_origin: Option<(i32, i32)>,
    /// Characters typed so far (single line).
    text_buffer: String,
    /// Integer scale multiplier: glyph renders at (8*scale) × (8*scale) px.
    text_scale: u32,

    // ── Palette history ──────────────────────────────────────────────────────
    /// The 10 most-recently-used draw colors (newest first).
    recent_colors: VecDeque<Color32>,
    /// 8 user-saveable color slots. None = empty. Right-click a swatch to fill.
    saved_slots: [Option<Color32>; 8],
    /// True while a blocking rfd file dialog is running on the main thread.
    /// On macOS, `[NSSavePanel runModal]` / `[NSOpenPanel runModal]` spin a
    /// nested AppKit event loop. That loop can re-enter eframe and trigger a
    /// second `update()` call before the first one returns, which causes egui
    /// to see two `begin_frame` calls without an `end_frame` in between and
    /// immediately panics. Setting this flag before every `rfd::FileDialog`
    /// call and checking it at the top of `update()` makes re-entrant frames
    /// return early, keeping egui's frame counter consistent.
    dialog_open: bool,
}

impl Default for RustPaint {
    fn default() -> Self {
        let rng_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u32)
            .unwrap_or(0x1234_5678);
        Self {
            pixels: vec![Color32::WHITE; CANVAS_W * CANVAS_H],
            texture: None,
            tool: Tool::Brush,
            shape_kind: ShapeKind::Rect,
            brush_size: 5.0,
            line_thickness: 2.0,
            color: Color32::BLACK,
            last_pos: None,
            drag_start: None,
            dirty: true,
            dirty_bbox: None,
            history: VecDeque::new(),
            redo: VecDeque::new(),
            stroke_originals: Vec::new(),
            stroke_marks: vec![0; CANVAS_W * CANVAS_H],
            stroke_epoch: 1,
            stroke_bbox: None,
            stroke_active: false,
            file_path: String::new(),
            file_status: None,
            file_status_ok: true,
            file_status_time: None,
            spray_radius: 20.0,
            spray_density: 30,
            rng: if rng_seed == 0 { 0xdeadbeef } else { rng_seed },
            text_origin: None,
            text_buffer: String::new(),
            text_scale: 2,
            recent_colors: VecDeque::new(),
            saved_slots: [None; 8],
            dialog_open: false,
        }
    }
}

impl RustPaint {
    fn active_color(&self) -> Color32 {
        match self.tool {
            Tool::Eraser => Color32::WHITE,
            _ => self.color,
        }
    }

    /// Record the current foreground color as recently used (deduplicates, max 10).
    fn push_recent(&mut self) {
        let c = self.color;
        self.recent_colors.retain(|&x| x != c);
        self.recent_colors.push_front(c);
        while self.recent_colors.len() > 10 {
            self.recent_colors.pop_back();
        }
    }

    // ── History ──────────────────────────────────────────────────────────────

    fn mark_dirty_full(&mut self) {
        self.dirty = true;
        self.dirty_bbox = None;
    }

    fn mark_dirty_bbox(&mut self, bbox: Bbox) {
        let Some((lx, ty, rx, by)) = bbox.clamped() else {
            return;
        };
        let bbox = Bbox {
            lx: lx as i32,
            ty: ty as i32,
            rx: rx as i32,
            by: by as i32,
        };
        self.dirty_bbox = if self.dirty {
            self.dirty_bbox.map(|prev| prev.union(bbox))
        } else {
            Some(bbox)
        };
        self.dirty = true;
    }

    fn image_for_bbox(&self, bbox: Bbox) -> Option<([usize; 2], ColorImage)> {
        let (lx, ty, rx, by) = bbox.clamped()?;
        let w = rx - lx + 1;
        let h = by - ty + 1;
        Some((
            [lx, ty],
            ColorImage {
                size: [w, h],
                pixels: copy_region(&self.pixels, lx, ty, w, h),
            },
        ))
    }

    /// Push a patch to the undo history and clear the redo stack
    /// (any new user action breaks the redo chain).
    fn push_patch(&mut self, patch: Patch) {
        if self.history.len() >= HISTORY_CAP {
            self.history.pop_front();
        }
        self.history.push_back(patch);
        self.redo.clear();
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

    /// Begin a brush/eraser/spray stroke without cloning the full canvas. The
    /// undo data is captured per touched pixel as paint is applied.
    fn begin_stroke(&mut self) {
        self.commit_stroke();
        self.stroke_originals.clear();
        self.stroke_bbox = None;
        if self.stroke_epoch == u32::MAX {
            self.stroke_marks.fill(0);
            self.stroke_epoch = 1;
        } else {
            self.stroke_epoch += 1;
        }
        self.stroke_active = true;
    }

    fn extend_stroke_bbox(&mut self, b: Bbox) {
        if !self.stroke_active {
            return;
        }
        self.stroke_bbox = Some(self.stroke_bbox.map_or(b, |prev| prev.union(b)));
    }

    fn capture_stroke_original(&mut self, idx: usize) {
        if !self.stroke_active || self.stroke_marks[idx] == self.stroke_epoch {
            return;
        }
        self.stroke_marks[idx] = self.stroke_epoch;
        self.stroke_originals.push((idx, self.pixels[idx]));
    }

    /// Commit the pending stroke pre-image patch.
    fn commit_stroke(&mut self) {
        self.stroke_active = false;
        let Some(bbox) = self.stroke_bbox.take() else {
            self.stroke_originals.clear();
            return;
        };
        let Some((lx, ty, rx, by)) = bbox.clamped() else {
            self.stroke_originals.clear();
            return;
        };
        let w = rx - lx + 1;
        let h = by - ty + 1;
        let mut patch = Patch {
            x: lx as u32,
            y: ty as u32,
            w: w as u32,
            h: h as u32,
            pixels: Vec::with_capacity(w * h),
        };
        // Phase 1: snapshot the current (post-stroke) pixels in the bbox.
        // Phase 2: overwrite each touched pixel's slot with its pre-stroke value.
        // Invariant: capture_stroke_original only records pixels whose canvas
        // index falls inside stroke_bbox, so every entry in stroke_originals
        // is guaranteed to lie within [lx..=rx, ty..=by].
        debug_assert!(
            self.stroke_originals.iter().all(|(idx, _)| {
                let x = idx % CANVAS_W;
                let y = idx / CANVAS_W;
                x >= lx && x <= rx && y >= ty && y <= by
            }),
            "stroke_originals contains a pixel outside stroke_bbox — invariant violated"
        );
        for y in ty..=by {
            let row = y * CANVAS_W + lx;
            patch.pixels.extend_from_slice(&self.pixels[row..row + w]);
        }
        for (idx, original) in self.stroke_originals.drain(..) {
            let x = idx % CANVAS_W;
            let y = idx / CANVAS_W;
            if x >= lx && x <= rx && y >= ty && y <= by {
                patch.pixels[(y - ty) * w + (x - lx)] = original;
            }
        }
        self.push_patch(patch);
    }

    fn blit_patch(&mut self, patch: &Patch) {
        let (x, y, w, h) = (
            patch.x as usize,
            patch.y as usize,
            patch.w as usize,
            patch.h as usize,
        );
        for row in 0..h {
            let dst = (y + row) * CANVAS_W + x;
            self.pixels[dst..dst + w].copy_from_slice(&patch.pixels[row * w..row * w + w]);
        }
        self.mark_dirty_bbox(patch_bbox(patch));
    }

    fn undo(&mut self) {
        let Some(patch) = self.history.pop_back() else {
            return;
        };
        if let Some(redo_patch) = make_patch(&self.pixels, patch_bbox(&patch)) {
            if self.redo.len() >= HISTORY_CAP {
                self.redo.pop_front();
            }
            self.redo.push_back(redo_patch);
        }
        self.blit_patch(&patch);
    }

    fn apply_redo(&mut self) {
        let Some(patch) = self.redo.pop_back() else {
            return;
        };
        if let Some(hist_patch) = make_patch(&self.pixels, patch_bbox(&patch)) {
            if self.history.len() >= HISTORY_CAP {
                self.history.pop_front();
            }
            // Push directly — do NOT call push_patch() which would clear redo.
            self.history.push_back(hist_patch);
        }
        self.blit_patch(&patch);
    }

    fn clear_canvas(&mut self, record_undo: bool) {
        self.commit_stroke();
        self.commit_text();
        if record_undo {
            self.push_full_patch();
        }
        self.pixels.fill(Color32::WHITE);
        self.mark_dirty_full();
    }

    /// Start a fresh document: white canvas, cleared history, reset file path.
    fn new_file(&mut self) {
        self.clear_canvas(false);
        self.history.clear();
        self.redo.clear();
        self.file_path.clear();
        self.file_status = None;
    }

    // ── Drawing primitives ───────────────────────────────────────────────────

    fn stamp_circle(&mut self, cx: i32, cy: i32, r: i32, color: Color32) {
        if r < 0 {
            return;
        }
        let bbox = Bbox::point(cx, cy).expand(r);
        let mut changed = false;

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
            for x in lx as usize..=rx as usize {
                let idx = row + x;
                if self.pixels[idx] == color {
                    continue;
                }
                self.capture_stroke_original(idx);
                self.pixels[idx] = color;
                changed = true;
            }
        }
        if changed {
            self.extend_stroke_bbox(bbox);
            self.mark_dirty_bbox(bbox);
        }
    }

    fn to_canvas(&self, pos: Pos2, rect: egui::Rect) -> (i32, i32) {
        (
            ((pos.x - rect.min.x) / rect.width() * CANVAS_W as f32) as i32,
            ((pos.y - rect.min.y) / rect.height() * CANVAS_H as f32) as i32,
        )
    }

    /// Bresenham line stamping a circle of radius `r` at every step.
    fn stroke_canvas(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, r: i32, c: Color32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1i32 } else { -1 };
        let sy = if y0 < y1 { 1i32 } else { -1 };
        let mut err = dx - dy;
        loop {
            self.stamp_circle(x0, y0, r, c);
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
        let r = self.brush_size.round() as i32;
        let c = self.active_color();
        self.stamp_circle(cx, cy, r, c);
    }

    fn paint_stroke(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let r = self.brush_size.round() as i32;
        let c = self.active_color();
        self.stroke_canvas(x0, y0, x1, y1, r, c);
    }

    // ── Flood fill ───────────────────────────────────────────────────────────

    fn flood_fill(&mut self, pos: Pos2, rect: egui::Rect) -> Option<Patch> {
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
        let mut changed = vec![ty * CANVAS_W + tx];
        let mut bbox = Bbox::point(sx, sy);

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < CANVAS_W && ny < CANVAS_H && self.pixels[ny * CANVAS_W + nx] == target {
                    self.pixels[ny * CANVAS_W + nx] = fill;
                    queue.push_back((nx, ny));
                    changed.push(ny * CANVAS_W + nx);
                    bbox.include_point(nx as i32, ny as i32);
                }
            }
        }
        self.mark_dirty_bbox(bbox);
        let mut patch = make_patch(&self.pixels, bbox)?;
        let patch_x = patch.x as usize;
        let patch_y = patch.y as usize;
        let patch_w = patch.w as usize;
        for idx in changed {
            let x = idx % CANVAS_W;
            let y = idx / CANVAS_W;
            let patch_idx = (y - patch_y) * patch_w + (x - patch_x);
            patch.pixels[patch_idx] = target;
        }
        Some(patch)
    }

    // ── Eyedropper ───────────────────────────────────────────────────────────

    fn pick_color(&mut self, pos: Pos2, rect: egui::Rect) {
        let (x, y) = self.to_canvas(pos, rect);
        if x >= 0 && x < CANVAS_W as i32 && y >= 0 && y < CANVAS_H as i32 {
            self.color = self.pixels[y as usize * CANVAS_W + x as usize];
            self.push_recent();
        }
    }

    // ── Text tool ────────────────────────────────────────────────────────────

    fn stamp_char(&mut self, ch: char, px: i32, py: i32, scale: u32, color: Color32) {
        let idx = (ch as usize).wrapping_sub(32);
        if idx >= FONT8X8.len() {
            return;
        }
        let glyph = FONT8X8[idx];
        let s = scale as i32;
        let cw = CANVAS_W as i32;
        let ch_h = CANVAS_H as i32;
        for row in 0..8i32 {
            let byte = glyph[row as usize];
            for col in 0..8i32 {
                if byte & (0x80 >> col) != 0 {
                    for sy in 0..s {
                        for sx in 0..s {
                            let x = px + col * s + sx;
                            let y = py + row * s + sy;
                            if x >= 0 && x < cw && y >= 0 && y < ch_h {
                                self.pixels[y as usize * CANVAS_W + x as usize] = color;
                            }
                        }
                    }
                }
            }
        }
        self.mark_dirty_bbox(Bbox::from_two((px, py), (px + 8 * s - 1, py + 8 * s - 1)));
    }

    /// Rasterise `text_buffer` into the canvas at `text_origin` and reset both.
    fn commit_text(&mut self) {
        let Some((ox, oy)) = self.text_origin.take() else {
            return;
        };
        let buf = std::mem::take(&mut self.text_buffer);
        if buf.is_empty() {
            return;
        }
        let scale = self.text_scale;
        let glyph_w = 8 * scale as i32;
        let char_count = buf.chars().count() as i32;
        let bbox = Bbox {
            lx: ox,
            ty: oy,
            rx: (ox + char_count * glyph_w).min(CANVAS_W as i32 - 1),
            by: (oy + glyph_w).min(CANVAS_H as i32 - 1),
        };
        self.push_bbox_patch(bbox);
        let color = self.color;
        for (i, ch) in buf.chars().enumerate() {
            self.stamp_char(ch, ox + i as i32 * glyph_w, oy, scale, color);
        }
        self.push_recent();
    }

    // ── Save / Load PNG ──────────────────────────────────────────────────────

    fn save_png(&self, path: &str) -> Result<(), String> {
        let buf: Vec<u8> = self
            .pixels
            .iter()
            .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
            .collect();
        image::save_buffer(
            path,
            &buf,
            CANVAS_W as u32,
            CANVAS_H as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|e| e.to_string())
    }

    fn load_png(&mut self, path: &str) -> Result<(), String> {
        let img = image::open(path).map_err(|e| e.to_string())?;
        let img = img.to_rgba8();
        let (iw, ih) = img.dimensions();
        if iw == 0 || ih == 0 {
            return Err("image has zero dimensions".to_string());
        }
        let raw = img.into_raw();
        self.push_full_patch();
        let mut new_pixels = vec![Color32::WHITE; CANVAS_W * CANVAS_H];
        for dy in 0..CANVAS_H {
            for dx in 0..CANVAS_W {
                let sx = (dx * iw as usize / CANVAS_W).min(iw as usize - 1);
                let sy = (dy * ih as usize / CANVAS_H).min(ih as usize - 1);
                let i = (sy * iw as usize + sx) * 4;
                new_pixels[dy * CANVAS_W + dx] =
                    Color32::from_rgba_unmultiplied(raw[i], raw[i + 1], raw[i + 2], raw[i + 3]);
            }
        }
        self.pixels = new_pixels;
        self.mark_dirty_full();
        Ok(())
    }

    // ── File status helper ───────────────────────────────────────────────────

    fn set_status(&mut self, msg: String, ok: bool, ctx: &egui::Context) {
        self.file_status = Some(msg);
        self.file_status_ok = ok;
        self.file_status_time = Some(Instant::now());
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    // ── Commit handlers for Line / Shape / Crop drag tools ───────────────────

    fn line_or_shape_bbox(&self, a: Pos2, b: Pos2, rect: egui::Rect) -> Bbox {
        let p0 = self.to_canvas(a, rect);
        let p1 = self.to_canvas(b, rect);
        let r = self.line_thickness.round() as i32;
        Bbox::from_two(p0, p1).expand(r)
    }

    fn commit_line(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let r = self.line_thickness.round() as i32;
        self.stroke_canvas(x0, y0, x1, y1, r, self.color);
    }

    fn stroke_ellipse_pts(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, r: i32, c: Color32) {
        let n = ((std::f32::consts::TAU * rx.max(ry)) as usize).max(60);
        let mut prev = (cx + rx, cy);
        for (nx, ny) in ellipse_perimeter(cx, cy, rx, ry, n).skip(1) {
            self.stroke_canvas(
                prev.0.round() as i32,
                prev.1.round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                r,
                c,
            );
            prev = (nx, ny);
        }
        self.stroke_canvas(
            prev.0.round() as i32,
            prev.1.round() as i32,
            (cx + rx).round() as i32,
            cy.round() as i32,
            r,
            c,
        );
    }

    fn stroke_closed_polygon(&mut self, pts: &[(f32, f32)], r: i32, c: Color32) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        for i in 0..n {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % n];
            self.stroke_canvas(
                ax.round() as i32,
                ay.round() as i32,
                bx.round() as i32,
                by.round() as i32,
                r,
                c,
            );
        }
    }

    fn commit_geometry(&mut self, geom: &ShapeGeometry, r: i32, c: Color32) {
        match geom {
            ShapeGeometry::Rect { lx, ty, rx, by } => {
                let lx = *lx as i32;
                let ty = *ty as i32;
                let rx = *rx as i32;
                let by = *by as i32;
                self.stroke_canvas(lx, ty, rx, ty, r, c);
                self.stroke_canvas(rx, ty, rx, by, r, c);
                self.stroke_canvas(rx, by, lx, by, r, c);
                self.stroke_canvas(lx, by, lx, ty, r, c);
            }
            ShapeGeometry::Ellipse { cx, cy, rx, ry } => {
                self.stroke_ellipse_pts(*cx, *cy, *rx, *ry, r, c);
            }
            ShapeGeometry::Polygon(pts) => self.stroke_closed_polygon(pts, r, c),
        }
    }

    fn commit_shape(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let geom = shape_geometry(self.shape_kind, x0 as f32, y0 as f32, x1 as f32, y1 as f32);
        let r = self.line_thickness.round() as i32;
        self.commit_geometry(&geom, r, self.color);
    }

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
        self.mark_dirty_full();
    }

    fn commit_drag_tool(&mut self, start: Pos2, end: Pos2, rect: egui::Rect) {
        self.commit_stroke();
        match self.tool {
            Tool::Line | Tool::Shape => {
                let bbox = self.line_or_shape_bbox(start, end, rect);
                self.push_bbox_patch(bbox);
                match self.tool {
                    Tool::Line => self.commit_line(start, end, rect),
                    Tool::Shape => self.commit_shape(start, end, rect),
                    _ => unreachable!(),
                }
                self.push_recent();
            }
            Tool::Crop => {
                self.push_full_patch();
                self.commit_crop(start, end, rect);
            }
            _ => {}
        }
    }

    // ── UI helpers (called from update()) ────────────────────────────────────

    fn tick_file_status(&mut self, ctx: &egui::Context) {
        let expired = self
            .file_status_time
            .map(|t| t.elapsed().as_secs_f32() > 3.0)
            .unwrap_or(false);
        if expired {
            self.file_status = None;
            self.file_status_time = None;
            ctx.request_repaint();
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        render_tool_picker(ui, &mut self.tool, &mut self.shape_kind);
        render_tool_sliders(
            ui,
            self.tool,
            &mut self.brush_size,
            &mut self.spray_radius,
            &mut self.spray_density,
            &mut self.line_thickness,
            &mut self.text_scale,
        );

        if !matches!(self.tool, Tool::Eraser | Tool::Crop | Tool::Eyedropper) {
            let recents: Vec<Color32> = self.recent_colors.iter().copied().collect();
            render_color_palette(ui, &mut self.color, &recents, &mut self.saved_slots);
            ui.separator();
        }

        if ui.button("Clear Canvas").clicked() {
            self.clear_canvas(true);
        }

        let can_undo = !self.history.is_empty();
        if ui
            .add_enabled(can_undo, egui::Button::new("Undo"))
            .clicked()
        {
            self.undo();
        }

        let can_redo = !self.redo.is_empty();
        if ui
            .add_enabled(can_redo, egui::Button::new("Redo"))
            .clicked()
        {
            self.apply_redo();
        }
    }

    fn render_file_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui
            .button("New")
            .on_hover_text("New canvas (Ctrl+N)")
            .clicked()
        {
            self.new_file();
        }

        if ui
            .button("Open")
            .on_hover_text("Open an image file")
            .clicked()
        {
            self.dialog_open = true;
            let picked = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                .pick_file();
            self.dialog_open = false;
            if let Some(path) = picked {
                let s = path.to_string_lossy().to_string();
                let short = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| s.clone());
                match self.load_png(&s) {
                    Ok(()) => {
                        self.file_path = s;
                        self.set_status(format!("Opened {short}"), true, ctx);
                    }
                    Err(e) => self.set_status(e, false, ctx),
                }
            }
        }

        let has_path = !self.file_path.is_empty();
        if ui
            .add_enabled(has_path, egui::Button::new("Save"))
            .on_hover_text(if has_path {
                format!("Save to {}", short_name(&self.file_path))
            } else {
                "No file open — use Save As…".to_string()
            })
            .clicked()
        {
            let path = self.file_path.clone();
            let short = short_name(&path).to_string();
            match self.save_png(&path) {
                Ok(()) => self.set_status(format!("Saved {short}"), true, ctx),
                Err(e) => self.set_status(e, false, ctx),
            }
        }

        if ui
            .button("Save As…")
            .on_hover_text("Choose where to save")
            .clicked()
        {
            let default = if has_path {
                short_name(&self.file_path).to_string()
            } else {
                "untitled.png".to_string()
            };
            self.dialog_open = true;
            let saved = rfd::FileDialog::new()
                .add_filter("PNG Image", &["png"])
                .set_file_name(&default)
                .save_file();
            self.dialog_open = false;
            if let Some(path) = saved {
                let mut s = path.to_string_lossy().to_string();
                if !s.ends_with(".png") {
                    s.push_str(".png");
                }
                let short = short_name(&s).to_string();
                match self.save_png(&s) {
                    Ok(()) => {
                        self.file_path = s;
                        self.set_status(format!("Saved {short}"), true, ctx);
                    }
                    Err(e) => self.set_status(e, false, ctx),
                }
            }
        }

        ui.separator();
        let fname_text = if has_path {
            egui::RichText::new(short_name(&self.file_path))
                .color(Color32::from_gray(140))
                .size(12.0)
        } else {
            egui::RichText::new("untitled")
                .color(Color32::from_gray(70))
                .size(12.0)
        };
        ui.label(fname_text);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(msg) = self.file_status.clone() {
                render_file_status(ui, &msg, self.file_status_ok);
            }
        });
    }

    fn handle_tool_input(
        &mut self,
        rect: egui::Rect,
        ctx: &egui::Context,
        hover: Option<Pos2>,
        down: bool,
        pressed: bool,
    ) {
        match self.tool {
            Tool::FloodFill => {
                if let (true, Some(pos)) = (pressed, hover.filter(|&p| rect.contains(p))) {
                    self.commit_stroke();
                    if let Some(patch) = self.flood_fill(pos, rect) {
                        self.push_patch(patch);
                        self.push_recent();
                    }
                }
            }

            Tool::Eyedropper => {
                if let (true, Some(pos)) = (pressed, hover.filter(|&p| rect.contains(p))) {
                    self.pick_color(pos, rect);
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

            Tool::Spray => {
                if down {
                    if let Some(pos) = hover {
                        if rect.contains(pos) {
                            if self.last_pos.is_none() {
                                self.begin_stroke();
                            }
                            let (cx, cy) = self.to_canvas(pos, rect);
                            let r = self.spray_radius.round() as i32;
                            let c = self.color;
                            for _ in 0..self.spray_density {
                                let angle = next_rand(&mut self.rng) as f32
                                    * (std::f32::consts::TAU / u32::MAX as f32);
                                let dist = (next_rand(&mut self.rng) as f32 / u32::MAX as f32)
                                    .sqrt()
                                    * r as f32;
                                let dx = (dist * angle.cos()) as i32;
                                let dy = (dist * angle.sin()) as i32;
                                self.stamp_circle(cx + dx, cy + dy, 0, c);
                            }
                            self.last_pos = Some(pos);
                            ctx.request_repaint();
                        } else {
                            self.last_pos = None;
                        }
                    } else {
                        self.last_pos = None;
                    }
                } else {
                    self.last_pos = None;
                    self.commit_stroke();
                    self.push_recent();
                }
            }

            Tool::Text => {
                if pressed {
                    if let Some(pos) = hover {
                        self.commit_text();
                        if rect.contains(pos) {
                            let (cx, cy) = self.to_canvas(pos, rect);
                            self.text_origin = Some((cx, cy));
                            self.text_buffer.clear();
                        }
                    }
                }

                if self.text_origin.is_some() {
                    let (append, do_backspace, do_commit) = ctx.input(|i| {
                        let mut append = String::new();
                        let mut backspace = false;
                        let mut commit = false;
                        for event in &i.events {
                            match event {
                                egui::Event::Text(s) => append.push_str(s),
                                egui::Event::Key {
                                    key: egui::Key::Backspace,
                                    pressed: true,
                                    ..
                                } => backspace = true,
                                egui::Event::Key {
                                    key: egui::Key::Z,
                                    pressed: true,
                                    modifiers,
                                    ..
                                } if modifiers.command && !modifiers.shift => backspace = true,
                                egui::Event::Key {
                                    key: egui::Key::Enter,
                                    pressed: true,
                                    ..
                                }
                                | egui::Event::Key {
                                    key: egui::Key::Escape,
                                    pressed: true,
                                    ..
                                } => commit = true,
                                _ => {}
                            }
                        }
                        (append, backspace, commit)
                    });
                    if !append.is_empty() {
                        self.text_buffer.push_str(&append);
                        ctx.request_repaint();
                    }
                    if do_backspace {
                        self.text_buffer.pop();
                        ctx.request_repaint();
                    }
                    if do_commit {
                        self.commit_text();
                    }
                }
            }

            _ => {
                // Brush and Eraser
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
                    if self.tool == Tool::Brush {
                        self.push_recent();
                    }
                }
            }
        }
    }
}

impl eframe::App for RustPaint {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Guard against re-entrant calls.
        // On macOS, rfd's blocking dialogs run [NSSavePanel/NSOpenPanel runModal],
        // which spins a nested AppKit event loop. Drag-tracking and other events
        // dispatched by that loop can cause eframe to invoke update() again while
        // the outer call is still blocked inside rfd. A second begin_frame() call
        // before end_frame() panics inside egui; with panic="abort" in the release
        // profile that becomes an immediate SIGABRT. Returning early here keeps
        // egui's frame counter consistent without touching any app state.
        if self.dialog_open {
            return;
        }

        self.tick_file_status(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| self.render_toolbar(ui));
        });

        egui::TopBottomPanel::bottom("file_bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| self.render_file_bar(ui, ctx));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let size = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::drag());

            // Read input before uploading/painting the texture so a click can
            // affect the current frame instead of waiting for the next repaint.
            let hover = ctx.input(|i| i.pointer.hover_pos());

            // Commit pending text if the tool was switched away this frame.
            if self.tool != Tool::Text && self.text_origin.is_some() {
                self.commit_text();
            }

            let (down, pressed, undo_pressed, redo_pressed, new_pressed) = ctx.input(|i| {
                (
                    i.pointer.primary_down(),
                    i.pointer.primary_pressed(),
                    i.key_pressed(egui::Key::Z) && i.modifiers.command && !i.modifiers.shift,
                    (i.key_pressed(egui::Key::Y) && i.modifiers.command)
                        || (i.key_pressed(egui::Key::Z)
                            && i.modifiers.command
                            && i.modifiers.shift),
                    i.key_pressed(egui::Key::N) && i.modifiers.command,
                )
            });

            if undo_pressed && self.text_origin.is_none() {
                self.undo();
            }
            if redo_pressed && self.text_origin.is_none() {
                self.apply_redo();
            }
            if new_pressed {
                self.new_file();
            }

            if hover.map(|p| rect.contains(p)).unwrap_or(false) {
                let icon = match self.tool {
                    Tool::Eyedropper => egui::CursorIcon::PointingHand,
                    Tool::Text => egui::CursorIcon::Text,
                    _ => egui::CursorIcon::Crosshair,
                };
                ctx.set_cursor_icon(icon);
            }

            self.handle_tool_input(rect, ctx, hover, down, pressed);

            if self.texture.is_none() {
                self.texture = Some(ctx.load_texture(
                    "canvas",
                    ColorImage {
                        size: [CANVAS_W, CANVAS_H],
                        pixels: self.pixels.clone(),
                    },
                    TextureOptions::NEAREST,
                ));
                self.dirty = false;
                self.dirty_bbox = None;
            } else if self.dirty {
                let partial = self.dirty_bbox.and_then(|bbox| self.image_for_bbox(bbox));
                if let (Some(ref mut texture), Some((pos, img))) = (&mut self.texture, partial) {
                    texture.set_partial(pos, img, TextureOptions::NEAREST);
                } else if let Some(ref mut texture) = self.texture {
                    texture.set(
                        ColorImage {
                            size: [CANVAS_W, CANVAS_H],
                            pixels: self.pixels.clone(),
                        },
                        TextureOptions::NEAREST,
                    );
                }
                self.dirty = false;
                self.dirty_bbox = None;
            }

            if let Some(ref texture) = self.texture {
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }

            let painter = ui.painter().clone();
            render_drag_preview(
                self.tool,
                self.shape_kind,
                self.color,
                self.line_thickness,
                self.drag_start,
                rect,
                hover,
                &painter,
            );
            render_text_cursor(
                self.tool,
                self.text_origin,
                &self.text_buffer,
                self.text_scale,
                self.color,
                rect,
                &painter,
            );
            render_eraser_cursor(self.tool, self.brush_size, rect, hover, &painter);
        });
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
