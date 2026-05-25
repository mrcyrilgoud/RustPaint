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
}

struct MasterPaint {
    /// Flat RGBA pixel buffer for the canvas (row-major).
    pixels: Vec<Color32>,
    /// GPU texture handle; rebuilt whenever `dirty` is true.
    texture: Option<TextureHandle>,
    tool: Tool,
    brush_size: f32,
    color: Color32,
    /// Last cursor position while the mouse button was held, used to
    /// interpolate a stroke between frames.
    last_pos: Option<Pos2>,
    /// Set to true whenever `pixels` changes; triggers a texture upload.
    dirty: bool,
}

impl Default for MasterPaint {
    fn default() -> Self {
        Self {
            pixels: vec![Color32::WHITE; CANVAS_W * CANVAS_H],
            texture: None,
            tool: Tool::Brush,
            brush_size: 5.0,
            color: Color32::BLACK,
            last_pos: None,
            dirty: true,
        }
    }
}

impl MasterPaint {
    fn active_color(&self) -> Color32 {
        match self.tool {
            Tool::Brush => self.color,
            Tool::Eraser => Color32::WHITE,
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color32) {
        if x >= 0 && x < CANVAS_W as i32 && y >= 0 && y < CANVAS_H as i32 {
            self.pixels[y as usize * CANVAS_W + x as usize] = color;
        }
    }

    /// Paint a filled circle of `brush_size` radius centred at (cx, cy).
    fn stamp_circle(&mut self, cx: i32, cy: i32, color: Color32) {
        let r = self.brush_size as i32;
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

    fn paint_dot(&mut self, pos: Pos2, rect: egui::Rect) {
        let (cx, cy) = self.to_canvas(pos, rect);
        let c = self.active_color();
        self.stamp_circle(cx, cy, c);
        self.dirty = true;
    }

    /// Bresenham line from `a` to `b`, stamping a circle at every step so
    /// fast mouse movement doesn't produce gaps.
    fn paint_stroke(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (mut x0, mut y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let c = self.active_color();

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
                ui.separator();

                // Brush size slider
                ui.label("Size:");
                ui.add(egui::Slider::new(&mut self.brush_size, 1.0_f32..=50.0).step_by(1.0));
                ui.separator();

                // Color controls (only meaningful for Brush)
                if self.tool == Tool::Brush {
                    ui.label("Color:");
                    // Full RGB/HSV color picker popup
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
                        Color32::from_rgb(0, 200, 0),   // green
                        Color32::BLUE,
                        Color32::YELLOW,
                        Color32::from_rgb(255, 165, 0), // orange
                        Color32::from_rgb(160, 32, 240), // purple
                        Color32::from_rgb(0, 200, 200), // cyan
                        Color32::from_rgb(255, 20, 147), // pink
                        Color32::from_rgb(139, 69, 19), // brown
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

            // Allocate the full panel area as an interactive region.
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::drag());

            if let Some(ref texture) = self.texture {
                ui.painter().image(
                    texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }

            // Crosshair cursor over the canvas.
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);

            // Read mouse state and paint.
            let (hover, down) =
                ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_down()));

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
                }
            } else {
                self.last_pos = None;
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "MasterPaint",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1300.0, 900.0])
                .with_title("MasterPaint"),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(MasterPaint::default()))),
    )
}
