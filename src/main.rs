// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{Color32, ColorImage, Pos2, RichText, TextureHandle, TextureOptions};
use std::collections::VecDeque;
use std::time::Instant;

const HISTORY_CAP: usize = 50;
const SELECTION_COLOR: Color32 = Color32::from_rgb(30, 120, 255);

/// Primary palette row 1: neutrals + classic hues (18 swatches).
#[rustfmt::skip]
const PALETTE_ROW1: [Color32; 18] = [
    Color32::from_rgb(  0,   0,   0), // Black
    Color32::from_rgb( 64,  64,  64), // Dark Gray
    Color32::from_rgb(128, 128, 128), // Gray
    Color32::from_rgb(192, 192, 192), // Silver
    Color32::from_rgb(255, 255, 255), // White
    Color32::from_rgb(255,   0,   0), // Red
    Color32::from_rgb(128,   0,   0), // Dark Red
    Color32::from_rgb(255, 128,   0), // Orange
    Color32::from_rgb(255,  69,   0), // Orange Red
    Color32::from_rgb(255, 215,   0), // Gold
    Color32::from_rgb(  0, 200,   0), // Lime Green
    Color32::from_rgb(  0, 128,   0), // Green
    Color32::from_rgb(  0,  80,   0), // Dark Green
    Color32::from_rgb(  0, 200, 200), // Cyan
    Color32::from_rgb(135, 206, 235), // Sky Blue
    Color32::from_rgb(  0,   0, 255), // Blue
    Color32::from_rgb(  0,   0, 128), // Navy
    Color32::from_rgb(128,   0, 128), // Purple
];

/// Primary palette row 2: pastels, earth tones, skin tones, neons (18 swatches).
#[rustfmt::skip]
const PALETTE_ROW2: [Color32; 18] = [
    Color32::from_rgb(255, 182, 193), // Light Pink
    Color32::from_rgb(250, 128, 114), // Salmon
    Color32::from_rgb(255, 218, 185), // Peach
    Color32::from_rgb(255, 250, 205), // Lemon Chiffon
    Color32::from_rgb(152, 251, 152), // Pale Green
    Color32::from_rgb(224, 255, 255), // Light Cyan
    Color32::from_rgb(173, 216, 230), // Light Blue
    Color32::from_rgb(221, 160, 221), // Plum
    Color32::from_rgb(255, 221, 193), // Skin Light
    Color32::from_rgb(232, 184, 154), // Skin Medium-Light
    Color32::from_rgb(198, 134,  66), // Skin Medium
    Color32::from_rgb(141,  85,  36), // Skin Medium-Dark
    Color32::from_rgb(139,  69,  19), // Saddle Brown
    Color32::from_rgb(210, 180, 140), // Tan
    Color32::from_rgb( 57, 255,  20), // Neon Green
    Color32::from_rgb(255, 255,   0), // Neon Yellow
    Color32::from_rgb(255,   0, 255), // Magenta
    Color32::from_rgb(255,  20, 147), // Deep Pink
];

/// Hex tooltip strings for the 36 static palette swatches, computed once.
static PALETTE_HEX: std::sync::OnceLock<([String; 18], [String; 18])> =
    std::sync::OnceLock::new();

fn palette_hex() -> &'static ([String; 18], [String; 18]) {
    PALETTE_HEX.get_or_init(|| {
        let row1 = std::array::from_fn(|i| {
            let sw = PALETTE_ROW1[i];
            format!("#{:02X}{:02X}{:02X}", sw.r(), sw.g(), sw.b())
        });
        let row2 = std::array::from_fn(|i| {
            let sw = PALETTE_ROW2[i];
            format!("#{:02X}{:02X}{:02X}", sw.r(), sw.g(), sw.b())
        });
        (row1, row2)
    })
}

/// Fixed canvas resolution in pixels.
const CANVAS_W: usize = 1200;
const CANVAS_H: usize = 800;

/// Public-domain 8×8 bitmap font covering ASCII 32–126 (95 glyphs).
/// Each entry is one glyph; each byte is one scanline; MSB = leftmost pixel.
/// Derived from the classic IBM CP437 VGA ROM font (public domain).
#[rustfmt::skip]
const FONT8X8: [[u8; 8]; 95] = [
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00], // 32  (space)
    [0x18,0x18,0x18,0x18,0x18,0x00,0x18,0x00], // 33  !
    [0x66,0x66,0x00,0x00,0x00,0x00,0x00,0x00], // 34  "
    [0x36,0x36,0x7f,0x36,0x7f,0x36,0x36,0x00], // 35  #
    [0x0c,0x3f,0x03,0x1e,0x30,0x3f,0x0c,0x00], // 36  $
    [0x00,0x63,0x33,0x18,0x0c,0x66,0x63,0x00], // 37  %
    [0x1c,0x36,0x1c,0x6e,0x3b,0x33,0x6e,0x00], // 38  &
    [0x06,0x06,0x03,0x00,0x00,0x00,0x00,0x00], // 39  '
    [0x18,0x0c,0x06,0x06,0x06,0x0c,0x18,0x00], // 40  (
    [0x06,0x0c,0x18,0x18,0x18,0x0c,0x06,0x00], // 41  )
    [0x00,0x66,0x3c,0xff,0x3c,0x66,0x00,0x00], // 42  *
    [0x00,0x0c,0x0c,0x3f,0x0c,0x0c,0x00,0x00], // 43  +
    [0x00,0x00,0x00,0x00,0x00,0x0c,0x0c,0x06], // 44  ,
    [0x00,0x00,0x00,0x3f,0x00,0x00,0x00,0x00], // 45  -
    [0x00,0x00,0x00,0x00,0x00,0x0c,0x0c,0x00], // 46  .
    [0x60,0x30,0x18,0x0c,0x06,0x03,0x01,0x00], // 47  /
    [0x3e,0x63,0x73,0x7b,0x6f,0x67,0x3e,0x00], // 48  0
    [0x0c,0x0e,0x0c,0x0c,0x0c,0x0c,0x3f,0x00], // 49  1
    [0x1e,0x33,0x30,0x1c,0x06,0x33,0x3f,0x00], // 50  2
    [0x1e,0x33,0x30,0x1c,0x30,0x33,0x1e,0x00], // 51  3
    [0x38,0x3c,0x36,0x33,0x7f,0x30,0x78,0x00], // 52  4
    [0x3f,0x03,0x1f,0x30,0x30,0x33,0x1e,0x00], // 53  5
    [0x1c,0x06,0x03,0x1f,0x33,0x33,0x1e,0x00], // 54  6
    [0x3f,0x33,0x30,0x18,0x0c,0x0c,0x0c,0x00], // 55  7
    [0x1e,0x33,0x33,0x1e,0x33,0x33,0x1e,0x00], // 56  8
    [0x1e,0x33,0x33,0x3e,0x30,0x18,0x0e,0x00], // 57  9
    [0x00,0x0c,0x0c,0x00,0x00,0x0c,0x0c,0x00], // 58  :
    [0x00,0x0c,0x0c,0x00,0x00,0x0c,0x0c,0x06], // 59  ;
    [0x18,0x0c,0x06,0x03,0x06,0x0c,0x18,0x00], // 60  <
    [0x00,0x00,0x3f,0x00,0x00,0x3f,0x00,0x00], // 61  =
    [0x06,0x0c,0x18,0x30,0x18,0x0c,0x06,0x00], // 62  >
    [0x1e,0x33,0x30,0x18,0x0c,0x00,0x0c,0x00], // 63  ?
    [0x3e,0x63,0x7b,0x7b,0x7b,0x03,0x1e,0x00], // 64  @
    [0x0c,0x1e,0x33,0x33,0x3f,0x33,0x33,0x00], // 65  A
    [0x3f,0x66,0x66,0x3e,0x66,0x66,0x3f,0x00], // 66  B
    [0x3c,0x66,0x03,0x03,0x03,0x66,0x3c,0x00], // 67  C
    [0x1f,0x36,0x66,0x66,0x66,0x36,0x1f,0x00], // 68  D
    [0x7f,0x46,0x16,0x1e,0x16,0x46,0x7f,0x00], // 69  E
    [0x7f,0x46,0x16,0x1e,0x16,0x06,0x0f,0x00], // 70  F
    [0x3c,0x66,0x03,0x03,0x73,0x66,0x7c,0x00], // 71  G
    [0x33,0x33,0x33,0x3f,0x33,0x33,0x33,0x00], // 72  H
    [0x1e,0x0c,0x0c,0x0c,0x0c,0x0c,0x1e,0x00], // 73  I
    [0x78,0x30,0x30,0x30,0x33,0x33,0x1e,0x00], // 74  J
    [0x67,0x66,0x36,0x1e,0x36,0x66,0x67,0x00], // 75  K
    [0x0f,0x06,0x06,0x06,0x46,0x66,0x7f,0x00], // 76  L
    [0x63,0x77,0x7f,0x7f,0x6b,0x63,0x63,0x00], // 77  M
    [0x63,0x67,0x6f,0x7b,0x73,0x63,0x63,0x00], // 78  N
    [0x1c,0x36,0x63,0x63,0x63,0x36,0x1c,0x00], // 79  O
    [0x3f,0x66,0x66,0x3e,0x06,0x06,0x0f,0x00], // 80  P
    [0x1e,0x33,0x33,0x33,0x3b,0x1e,0x38,0x00], // 81  Q
    [0x3f,0x66,0x66,0x3e,0x36,0x66,0x67,0x00], // 82  R
    [0x1e,0x33,0x07,0x0e,0x38,0x33,0x1e,0x00], // 83  S
    [0x3f,0x2d,0x0c,0x0c,0x0c,0x0c,0x1e,0x00], // 84  T
    [0x33,0x33,0x33,0x33,0x33,0x33,0x3f,0x00], // 85  U
    [0x33,0x33,0x33,0x33,0x33,0x1e,0x0c,0x00], // 86  V
    [0x63,0x63,0x63,0x6b,0x7f,0x77,0x63,0x00], // 87  W
    [0x63,0x63,0x36,0x1c,0x1c,0x36,0x63,0x00], // 88  X
    [0x33,0x33,0x33,0x1e,0x0c,0x0c,0x1e,0x00], // 89  Y
    [0x7f,0x63,0x31,0x18,0x4c,0x66,0x7f,0x00], // 90  Z
    [0x1e,0x06,0x06,0x06,0x06,0x06,0x1e,0x00], // 91  [
    [0x03,0x06,0x0c,0x18,0x30,0x60,0x40,0x00], // 92  \
    [0x1e,0x18,0x18,0x18,0x18,0x18,0x1e,0x00], // 93  ]
    [0x08,0x1c,0x36,0x63,0x00,0x00,0x00,0x00], // 94  ^
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xff], // 95  _
    [0x0c,0x0c,0x18,0x00,0x00,0x00,0x00,0x00], // 96  `
    [0x00,0x00,0x1e,0x30,0x3e,0x33,0x6e,0x00], // 97  a
    [0x07,0x06,0x06,0x3e,0x66,0x66,0x3b,0x00], // 98  b
    [0x00,0x00,0x1e,0x33,0x03,0x33,0x1e,0x00], // 99  c
    [0x38,0x30,0x30,0x3e,0x33,0x33,0x6e,0x00], // 100 d
    [0x00,0x00,0x1e,0x33,0x3f,0x03,0x1e,0x00], // 101 e
    [0x1c,0x36,0x06,0x0f,0x06,0x06,0x0f,0x00], // 102 f
    [0x00,0x00,0x6e,0x33,0x33,0x3e,0x30,0x1f], // 103 g
    [0x07,0x06,0x36,0x6e,0x66,0x66,0x67,0x00], // 104 h
    [0x0c,0x00,0x0e,0x0c,0x0c,0x0c,0x1e,0x00], // 105 i
    [0x30,0x00,0x30,0x30,0x30,0x33,0x33,0x1e], // 106 j
    [0x07,0x06,0x66,0x36,0x1e,0x36,0x67,0x00], // 107 k
    [0x0e,0x0c,0x0c,0x0c,0x0c,0x0c,0x1e,0x00], // 108 l
    [0x00,0x00,0x33,0x7f,0x7f,0x6b,0x63,0x00], // 109 m
    [0x00,0x00,0x1f,0x33,0x33,0x33,0x33,0x00], // 110 n
    [0x00,0x00,0x1e,0x33,0x33,0x33,0x1e,0x00], // 111 o
    [0x00,0x00,0x3b,0x66,0x66,0x3e,0x06,0x0f], // 112 p
    [0x00,0x00,0x6e,0x33,0x33,0x3e,0x30,0x78], // 113 q
    [0x00,0x00,0x3b,0x6e,0x66,0x06,0x0f,0x00], // 114 r
    [0x00,0x00,0x3e,0x03,0x1e,0x30,0x1f,0x00], // 115 s
    [0x08,0x0c,0x3e,0x0c,0x0c,0x2c,0x18,0x00], // 116 t
    [0x00,0x00,0x33,0x33,0x33,0x33,0x6e,0x00], // 117 u
    [0x00,0x00,0x33,0x33,0x33,0x1e,0x0c,0x00], // 118 v
    [0x00,0x00,0x63,0x6b,0x7f,0x7f,0x36,0x00], // 119 w
    [0x00,0x00,0x63,0x36,0x1c,0x36,0x63,0x00], // 120 x
    [0x00,0x00,0x33,0x33,0x33,0x3e,0x30,0x1f], // 121 y
    [0x00,0x00,0x3f,0x19,0x0c,0x26,0x3f,0x00], // 122 z
    [0x38,0x0c,0x0c,0x07,0x0c,0x0c,0x38,0x00], // 123 {
    [0x18,0x18,0x18,0x00,0x18,0x18,0x18,0x00], // 124 |
    [0x07,0x0c,0x0c,0x38,0x0c,0x0c,0x07,0x00], // 125 }
    [0x6e,0x3b,0x00,0x00,0x00,0x00,0x00,0x00], // 126 ~
];

/// Inline xorshift32 PRNG — no external crate needed.
fn next_rand(state: &mut u32) -> u32 {
    if *state == 0 {
        *state = 0xdeadbeef;
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

/// A rectangular pre-image of the canvas, used to undo/redo a destructive op.
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
        Self {
            lx: x,
            ty: y,
            rx: x,
            by: y,
        }
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
        Self {
            lx: self.lx - r,
            ty: self.ty - r,
            rx: self.rx + r,
            by: self.by + r,
        }
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
        self.lx = self.lx.min(x);
        self.rx = self.rx.max(x);
        self.ty = self.ty.min(y);
        self.by = self.by.max(y);
    }
    /// Clamp to the canvas; returns None if it lies entirely outside.
    fn clamped(self) -> Option<(usize, usize, usize, usize)> {
        if self.rx < 0 || self.by < 0 || self.lx >= CANVAS_W as i32 || self.ty >= CANVAS_H as i32 {
            return None;
        }
        let lx = self.lx.max(0) as usize;
        let rx = (self.rx as usize).min(CANVAS_W - 1);
        let ty = self.ty.max(0) as usize;
        let by = (self.by as usize).min(CANVAS_H - 1);
        if rx < lx || by < ty {
            None
        } else {
            Some((lx, ty, rx, by))
        }
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
    Spray,
    FloodFill,
    Eyedropper,
    Line,
    Shape,
    Crop,
    Text,
}

#[derive(PartialEq, Clone, Copy)]
enum ShapeKind {
    Rect,
    Ellipse,
    Circle,
    Triangle,
    Diamond,
    Pentagon,
}

/// Generate `n` equally-spaced points around a regular polygon, with the first
/// vertex at angle `start_angle` (radians).
fn regular_polygon(cx: f32, cy: f32, r: f32, n: usize, start_angle: f32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = start_angle + std::f32::consts::TAU * i as f32 / n as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

struct RustPaint {
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
        let mut pixels = Vec::with_capacity(w * h);
        for y in ty..=by {
            let row = y * CANVAS_W + lx;
            pixels.extend_from_slice(&self.pixels[row..row + w]);
        }
        Some((
            [lx, ty],
            ColorImage {
                size: [w, h],
                pixels,
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
        self.stroke_epoch = self.stroke_epoch.wrapping_add(1).max(1);
        if self.stroke_epoch == 1 {
            self.stroke_marks.fill(0);
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
        self.mark_dirty_bbox(Self::patch_bbox(patch));
    }

    fn patch_bbox(patch: &Patch) -> Bbox {
        Bbox {
            lx: patch.x as i32,
            ty: patch.y as i32,
            rx: (patch.x + patch.w - 1) as i32,
            by: (patch.y + patch.h - 1) as i32,
        }
    }

    fn undo(&mut self) {
        let Some(patch) = self.history.pop_back() else {
            return;
        };
        if let Some(redo_patch) = make_patch(&self.pixels, Self::patch_bbox(&patch)) {
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
        if let Some(hist_patch) = make_patch(&self.pixels, Self::patch_bbox(&patch)) {
            if self.history.len() >= HISTORY_CAP {
                self.history.pop_front();
            }
            // Push directly — do NOT call push_patch() which would clear redo.
            self.history.push_back(hist_patch);
        }
        self.blit_patch(&patch);
    }

    /// Start a fresh document: white canvas, cleared history, reset file path.
    fn new_file(&mut self) {
        self.commit_stroke();
        self.commit_text();
        self.pixels.fill(Color32::WHITE);
        self.history.clear();
        self.redo.clear();
        self.file_path.clear();
        self.file_status = None;
        self.mark_dirty_full();
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

    fn commit_shape(&mut self, a: Pos2, b: Pos2, rect: egui::Rect) {
        let (x0, y0) = self.to_canvas(a, rect);
        let (x1, y1) = self.to_canvas(b, rect);
        let r = self.line_thickness.round() as i32;
        let c = self.color;

        match self.shape_kind {
            ShapeKind::Rect => {
                let (lx, rx) = (x0.min(x1), x0.max(x1));
                let (ty, by) = (y0.min(y1), y0.max(y1));
                self.stroke_canvas(lx, ty, rx, ty, r, c);
                self.stroke_canvas(rx, ty, rx, by, r, c);
                self.stroke_canvas(rx, by, lx, by, r, c);
                self.stroke_canvas(lx, by, lx, ty, r, c);
            }
            ShapeKind::Ellipse => {
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let erx = ((x1 - x0).abs() / 2).max(1) as f32;
                let ery = ((y1 - y0).abs() / 2).max(1) as f32;
                self.stroke_ellipse_pts(cx, cy, erx, ery, r, c);
            }
            ShapeKind::Circle => {
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let cr = (((x1 - x0).abs()).min((y1 - y0).abs()) / 2).max(1) as f32;
                self.stroke_ellipse_pts(cx, cy, cr, cr, r, c);
            }
            ShapeKind::Triangle => {
                let sides = 3;
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let pr = (((x1 - x0).abs()).min((y1 - y0).abs()) / 2).max(1) as f32;
                let pts = regular_polygon(cx, cy, pr, sides, -std::f32::consts::FRAC_PI_2);
                for (&(ax, ay), &(bx, by)) in
                    pts.iter().zip(pts.iter().cycle().skip(1)).take(pts.len())
                {
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
            ShapeKind::Pentagon => {
                let sides = 5;
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let pr = (((x1 - x0).abs()).min((y1 - y0).abs()) / 2).max(1) as f32;
                let pts = regular_polygon(cx, cy, pr, sides, -std::f32::consts::FRAC_PI_2);
                for (&(ax, ay), &(bx, by)) in
                    pts.iter().zip(pts.iter().cycle().skip(1)).take(pts.len())
                {
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
            ShapeKind::Diamond => {
                let cx = (x0 + x1) as f32 / 2.0;
                let cy = (y0 + y1) as f32 / 2.0;
                let drx = ((x1 - x0).abs() / 2).max(1) as f32;
                let dry = ((y1 - y0).abs() / 2).max(1) as f32;
                let pts = [
                    (cx, cy - dry),
                    (cx + drx, cy),
                    (cx, cy + dry),
                    (cx - drx, cy),
                ];
                for (&(ax, ay), &(bx, by)) in pts.iter().zip(pts.iter().cycle().skip(1)).take(4) {
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
        }
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
            Tool::Line => {
                let bbox = self.line_or_shape_bbox(start, end, rect);
                self.push_bbox_patch(bbox);
                self.commit_line(start, end, rect);
                self.push_recent();
            }
            Tool::Shape => {
                let bbox = self.line_or_shape_bbox(start, end, rect);
                self.push_bbox_patch(bbox);
                self.commit_shape(start, end, rect);
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

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.label("Tool:");
        ui.selectable_value(&mut self.tool, Tool::Brush, "Brush");
        ui.selectable_value(&mut self.tool, Tool::Eraser, "Eraser");
        ui.selectable_value(&mut self.tool, Tool::Spray, "Spray");
        ui.selectable_value(&mut self.tool, Tool::FloodFill, "Fill");
        ui.selectable_value(&mut self.tool, Tool::Eyedropper, "Eyedropper");
        ui.selectable_value(&mut self.tool, Tool::Line, "Line");
        ui.selectable_value(&mut self.tool, Tool::Shape, "Shape");
        ui.selectable_value(&mut self.tool, Tool::Crop, "Crop");
        ui.selectable_value(&mut self.tool, Tool::Text, "Text");
        ui.separator();

        if self.tool == Tool::Shape {
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Rect, "Rect");
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Ellipse, "Ellipse");
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Circle, "Circle");
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Triangle, "Triangle");
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Diamond, "Diamond");
            ui.selectable_value(&mut self.shape_kind, ShapeKind::Pentagon, "Pentagon");
            ui.separator();
        }

        if matches!(self.tool, Tool::Brush | Tool::Eraser) {
            ui.label("Size:");
            ui.add(egui::Slider::new(&mut self.brush_size, 1.0_f32..=50.0).step_by(1.0));
            ui.separator();
        }

        if self.tool == Tool::Spray {
            ui.label("Radius:");
            ui.add(egui::Slider::new(&mut self.spray_radius, 5.0_f32..=60.0).step_by(1.0));
            ui.label("Density:");
            ui.add(egui::Slider::new(&mut self.spray_density, 5_u32..=80));
            ui.separator();
        }

        if matches!(self.tool, Tool::Line | Tool::Shape) {
            ui.label("Thickness:");
            ui.add(egui::Slider::new(&mut self.line_thickness, 1.0_f32..=30.0).step_by(1.0));
            ui.separator();
        }

        if self.tool == Tool::Text {
            ui.label("Size:");
            for (label, val) in [("1×", 1u32), ("2×", 2), ("3×", 3), ("4×", 4)] {
                let selected = self.text_scale == val;
                let mut btn = egui::Button::new(label);
                if selected {
                    btn = btn.stroke(egui::Stroke::new(2.0, SELECTION_COLOR));
                }
                if ui.add(btn).clicked() {
                    self.text_scale = val;
                }
            }
            ui.separator();
        }

        if !matches!(self.tool, Tool::Eraser | Tool::Crop | Tool::Eyedropper) {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;

                // Custom color picker row
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    ui.color_edit_button_srgba(&mut self.color);
                    ui.label(
                        RichText::new("Custom…")
                            .color(Color32::from_gray(140))
                            .size(11.0),
                    );
                });

                // Main palette — row 1
                ui.label(
                    RichText::new("Palette")
                        .color(Color32::from_gray(140))
                        .size(10.0),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (&sw, hex) in PALETTE_ROW1.iter().zip(palette_row1_hex().iter()) {
                        if swatch_ui(ui, sw, self.color == sw)
                            .on_hover_text(hex.as_str())
                            .clicked()
                        {
                            self.color = sw;
                        }
                    }
                });

                // Main palette — row 2
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (&sw, hex) in PALETTE_ROW2.iter().zip(palette_row2_hex().iter()) {
                        if swatch_ui(ui, sw, self.color == sw)
                            .on_hover_text(hex.as_str())
                            .clicked()
                        {
                            self.color = sw;
                        }
                    }
                });

                // Recent colors
                if !self.recent_colors.is_empty() {
                    ui.label(
                        RichText::new("Recent")
                            .color(Color32::from_gray(140))
                            .size(10.0),
                    );
                    // copy needed to split immutable borrow of recent_colors from mutable &mut self below
                    let recents: Vec<Color32> = self.recent_colors.iter().copied().collect();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for c in recents {
                            let hex = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
                            if swatch_ui(ui, c, self.color == c)
                                .on_hover_text(hex)
                                .clicked()
                            {
                                self.color = c;
                            }
                        }
                    });
                }

                // Saved slots
                ui.label(
                    RichText::new("Saved  (right-click to save current color)")
                        .color(Color32::from_gray(140))
                        .size(10.0),
                );
                // copy needed to split immutable borrow of saved_slots from mutable &mut self below
                let slots = self.saved_slots;
                let cur = self.color;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, slot) in slots.iter().copied().enumerate() {
                        let resp = if let Some(c) = slot {
                            let hex = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
                            swatch_ui(ui, c, cur == c).on_hover_text(hex)
                        } else {
                            empty_slot_ui(ui)
                                .on_hover_text("Empty — right-click to save current color")
                        };
                        if resp.clicked() {
                            if let Some(c) = slot {
                                self.color = c;
                            }
                        }
                        if resp.secondary_clicked() {
                            self.saved_slots[i] = Some(cur);
                        }
                    }
                });
            });
            ui.separator();
        }

        if ui.button("Clear Canvas").clicked() {
            self.commit_stroke();
            self.commit_text();
            self.push_full_patch();
            self.pixels.fill(Color32::WHITE);
            self.mark_dirty_full();
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
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                .pick_file()
            {
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
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG Image", &["png"])
                .set_file_name(&default)
                .save_file()
            {
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
            RichText::new(short_name(&self.file_path))
                .color(Color32::from_gray(140))
                .size(12.0)
        } else {
            RichText::new("untitled")
                .color(Color32::from_gray(70))
                .size(12.0)
        };
        ui.label(fname_text);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(ref msg) = self.file_status {
                let expired = self
                    .file_status_time
                    .map(|t| t.elapsed().as_secs_f32() > 3.0)
                    .unwrap_or(false);
                if expired {
                    self.file_status = None;
                    self.file_status_time = None;
                } else {
                    let color = if self.file_status_ok {
                        Color32::from_rgb(74, 222, 128)
                    } else {
                        Color32::from_rgb(248, 113, 113)
                    };
                    ui.label(RichText::new(msg.as_str()).color(color).size(12.0));
                }
            }
        });
    }

    fn render_drag_preview(&self, ui: &mut egui::Ui, rect: egui::Rect, hover: Option<Pos2>) {
        let screen_line = self.line_thickness * rect.width() / CANVAS_W as f32;
        let Some(start) = self.drag_start else { return };
        let end = hover.unwrap_or(start);
        let painter = ui.painter();

        match self.tool {
            Tool::Line => {
                painter.line_segment(
                    [start, end],
                    egui::Stroke::new(screen_line.max(1.0), self.color),
                );
            }
            Tool::Shape => {
                let pr = egui::Rect::from_two_pos(start, end);
                let stroke = egui::Stroke::new(screen_line.max(1.0), self.color);
                match self.shape_kind {
                    ShapeKind::Rect => {
                        painter.rect_stroke(pr, 0.0, stroke, egui::StrokeKind::Middle);
                    }
                    ShapeKind::Ellipse => {
                        let c = pr.center();
                        let pts: Vec<egui::Pos2> =
                            ellipse_perimeter(c.x, c.y, pr.width() / 2.0, pr.height() / 2.0, 60)
                                .map(|(x, y)| egui::pos2(x, y))
                                .collect();
                        painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                    }
                    ShapeKind::Circle => {
                        let c = pr.center();
                        let r = pr.width().min(pr.height()) / 2.0;
                        let pts: Vec<egui::Pos2> = ellipse_perimeter(c.x, c.y, r, r, 60)
                            .map(|(x, y)| egui::pos2(x, y))
                            .collect();
                        painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                    }
                    ShapeKind::Triangle => {
                        let sides = 3;
                        let c = pr.center();
                        let r = pr.width().min(pr.height()) / 2.0;
                        let pts: Vec<egui::Pos2> =
                            regular_polygon(c.x, c.y, r, sides, -std::f32::consts::FRAC_PI_2)
                                .into_iter()
                                .map(|(x, y)| egui::pos2(x, y))
                                .collect();
                        painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                    }
                    ShapeKind::Pentagon => {
                        let sides = 5;
                        let c = pr.center();
                        let r = pr.width().min(pr.height()) / 2.0;
                        let pts: Vec<egui::Pos2> =
                            regular_polygon(c.x, c.y, r, sides, -std::f32::consts::FRAC_PI_2)
                                .into_iter()
                                .map(|(x, y)| egui::pos2(x, y))
                                .collect();
                        painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                    }
                    ShapeKind::Diamond => {
                        let c = pr.center();
                        let (rx, ry) = (pr.width() / 2.0, pr.height() / 2.0);
                        let pts = vec![
                            egui::pos2(c.x, c.y - ry),
                            egui::pos2(c.x + rx, c.y),
                            egui::pos2(c.x, c.y + ry),
                            egui::pos2(c.x - rx, c.y),
                        ];
                        painter.add(egui::epaint::PathShape::closed_line(pts, stroke));
                    }
                }
            }
            Tool::Crop => {
                let cr = egui::Rect::from_two_pos(start, end);
                painter.rect_stroke(
                    cr,
                    0.0,
                    egui::Stroke::new(2.0, SELECTION_COLOR),
                    egui::StrokeKind::Middle,
                );
                let overlay = Color32::from_rgba_unmultiplied(0, 0, 0, 60);
                painter.rect_filled(
                    egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, cr.min.y)),
                    0.0,
                    overlay,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(rect.min.x, cr.max.y), rect.max),
                    0.0,
                    overlay,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, cr.min.y),
                        egui::pos2(cr.min.x, cr.max.y),
                    ),
                    0.0,
                    overlay,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(cr.max.x, cr.min.y),
                        egui::pos2(rect.max.x, cr.max.y),
                    ),
                    0.0,
                    overlay,
                );
            }
            _ => {}
        }
    }

    fn render_text_cursor(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        if self.tool != Tool::Text {
            return;
        }
        let Some((ox, oy)) = self.text_origin else {
            return;
        };
        let sx = rect.min.x + ox as f32 / CANVAS_W as f32 * rect.width();
        let sy = rect.min.y + oy as f32 / CANVAS_H as f32 * rect.height();
        let preview = format!("{}_", self.text_buffer);
        let px_per_canvas = rect.width() / CANVAS_W as f32;
        let font_size = (8.0 * self.text_scale as f32 * px_per_canvas).max(8.0);
        ui.painter().text(
            egui::pos2(sx, sy),
            egui::Align2::LEFT_TOP,
            &preview,
            egui::FontId::monospace(font_size),
            self.color,
        );
    }

    fn render_eraser_cursor(&self, ui: &mut egui::Ui, rect: egui::Rect, hover: Option<Pos2>) {
        if self.tool != Tool::Eraser {
            return;
        }
        let Some(pos) = hover else { return };
        if !rect.contains(pos) {
            return;
        }
        let half = self.brush_size * rect.width() / CANVAS_W as f32;
        let box_rect = egui::Rect::from_center_size(pos, egui::vec2(half * 2.0, half * 2.0));
        let painter = ui.painter();
        painter.rect_filled(box_rect, 0.0, Color32::WHITE);
        painter.rect_stroke(
            box_rect,
            0.0,
            egui::Stroke::new(1.0, Color32::from_gray(80)),
            egui::StrokeKind::Middle,
        );
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

            self.render_drag_preview(ui, rect, hover);
            self.render_text_cursor(ui, rect);
            self.render_eraser_cursor(ui, rect, hover);
        });
    }
}

fn palette_row1_hex() -> &'static [String; 18] {
    static HEX: std::sync::OnceLock<[String; 18]> = std::sync::OnceLock::new();
    HEX.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = PALETTE_ROW1[i];
            format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
        })
    })
}

fn palette_row2_hex() -> &'static [String; 18] {
    static HEX: std::sync::OnceLock<[String; 18]> = std::sync::OnceLock::new();
    HEX.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = PALETTE_ROW2[i];
            format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
        })
    })
}

/// Draw a 20×20 filled color swatch with a selection-aware border.
fn swatch_ui(ui: &mut egui::Ui, color: Color32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, color);
        let (w, c) = if selected {
            (2.0, SELECTION_COLOR)
        } else {
            (1.0, Color32::from_gray(80))
        };
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(w, c), egui::StrokeKind::Middle);
    }
    resp
}

/// Draw a 20×20 empty saved-slot placeholder with a "+" indicator.
fn empty_slot_ui(ui: &mut egui::Ui) -> egui::Response {
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
            egui::Stroke::new(1.0, Color32::from_gray(80)),
            egui::StrokeKind::Middle,
        );
    }
    resp
}

fn short_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
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
        Box::new(|_cc| Ok(Box::new(RustPaint::default()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    fn fresh() -> RustPaint {
        RustPaint::default()
    }

    // ── File I/O smoke checks ─────────────────────────────────────────────────

    #[test]
    fn save_writes_png_file() {
        let app = fresh();
        let p = tmp("rp_save_smoke.png");
        app.save_png(&p).expect("save");
        let size = std::fs::metadata(&p).expect("saved png metadata").len();
        std::fs::remove_file(&p).ok();
        assert!(size > 0, "saved png should not be empty");
    }

    #[test]
    fn load_reads_png_file() {
        let app = fresh();
        let p = tmp("rp_load_smoke.png");
        app.save_png(&p).expect("setup");
        let mut app2 = fresh();
        app2.load_png(&p).expect("load");
        std::fs::remove_file(&p).ok();
        assert!(app2.dirty, "load should mark canvas dirty");
    }

    // ── Format correctness: lossless pixel roundtrip ─────────────────────────

    #[test]
    fn white_roundtrip() {
        let mut a = fresh();
        a.pixels.fill(Color32::WHITE);
        let p = tmp("rp_white.png");
        a.save_png(&p).unwrap();
        let mut b = fresh();
        b.load_png(&p).unwrap();
        std::fs::remove_file(&p).ok();
        let bad = b
            .pixels
            .iter()
            .filter(|px| px.r() != 255 || px.g() != 255 || px.b() != 255)
            .count();
        assert_eq!(bad, 0, "{bad} pixels differ from white");
    }

    #[test]
    fn black_roundtrip() {
        let mut a = fresh();
        a.pixels.fill(Color32::BLACK);
        let p = tmp("rp_black.png");
        a.save_png(&p).unwrap();
        let mut b = fresh();
        b.load_png(&p).unwrap();
        std::fs::remove_file(&p).ok();
        let bad = b
            .pixels
            .iter()
            .filter(|px| px.r() != 0 || px.g() != 0 || px.b() != 0)
            .count();
        assert_eq!(bad, 0, "{bad} pixels differ from black");
    }

    #[test]
    fn load_sets_dirty() {
        let a = fresh();
        let p = tmp("rp_dirty.png");
        a.save_png(&p).unwrap();
        let mut b = fresh();
        b.dirty = false;
        b.load_png(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert!(b.dirty, "dirty should be true after load");
    }

    #[test]
    fn load_pushes_history() {
        let a = fresh();
        let p = tmp("rp_hist.png");
        a.save_png(&p).unwrap();
        let mut b = fresh();
        let before = b.history.len();
        b.load_png(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert!(b.history.len() > before, "load should push undo patch");
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn error_nonexistent_file() {
        let mut a = fresh();
        assert!(a.load_png("/tmp/rp_does_not_exist_xyzzy.png").is_err());
    }

    #[test]
    fn error_corrupt_file() {
        let p = tmp("rp_corrupt.png");
        std::fs::write(&p, b"not a png").unwrap();
        let mut a = fresh();
        let r = a.load_png(&p);
        std::fs::remove_file(&p).ok();
        assert!(r.is_err(), "corrupt file should error");
    }

    #[test]
    fn error_empty_file() {
        let p = tmp("rp_empty.png");
        std::fs::write(&p, b"").unwrap();
        let mut a = fresh();
        let r = a.load_png(&p);
        std::fs::remove_file(&p).ok();
        assert!(r.is_err(), "empty file should error");
    }

    #[test]
    fn error_bad_save_path() {
        let a = fresh();
        assert!(a.save_png("/no_such_dir_xyzzy/out.png").is_err());
    }

    // ── Undo / Redo correctness ───────────────────────────────────────────────

    #[test]
    fn redo_restores_after_undo() {
        let mut app = fresh();
        // Paint a single pixel to create a history entry.
        app.pixels[0] = Color32::RED;
        // Manually seed history to simulate a committed action.
        app.history.push_back(Patch {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            pixels: vec![Color32::WHITE],
        });
        // Confirm the pixel is red before undo.
        assert_eq!(app.pixels[0], Color32::RED);
        app.undo();
        assert_eq!(app.pixels[0], Color32::WHITE, "undo should restore white");
        assert!(!app.redo.is_empty(), "redo stack should be populated");
        app.apply_redo();
        assert_eq!(app.pixels[0], Color32::RED, "redo should restore red");
    }

    #[test]
    fn new_action_clears_redo() {
        let mut app = fresh();
        // Manually set up redo stack.
        app.redo.push_back(Patch {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            pixels: vec![Color32::WHITE],
        });
        assert!(!app.redo.is_empty());
        // Any new canvas action (push_patch) should clear redo.
        app.push_patch(Patch {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            pixels: vec![Color32::BLACK],
        });
        assert!(app.redo.is_empty(), "redo must be cleared after new action");
    }

    #[test]
    fn new_file_resets_state() {
        let mut app = fresh();
        app.pixels[0] = Color32::RED;
        app.file_path = "/tmp/test.png".to_string();
        app.history.push_back(Patch {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            pixels: vec![Color32::WHITE],
        });
        app.new_file();
        assert_eq!(
            app.pixels[0],
            Color32::WHITE,
            "canvas should be white after new_file"
        );
        assert!(app.file_path.is_empty(), "file_path should be cleared");
        assert!(app.history.is_empty(), "history should be cleared");
        assert!(app.redo.is_empty(), "redo should be cleared");
    }

    #[test]
    fn stroke_history_is_lazy_and_bounded_to_touched_region() {
        let mut app = fresh();
        let rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(CANVAS_W as f32, CANVAS_H as f32),
        );
        app.brush_size = 3.0;
        app.color = Color32::RED;
        app.dirty = false;
        app.dirty_bbox = None;

        app.begin_stroke();
        assert!(
            app.stroke_originals.is_empty() && app.stroke_bbox.is_none(),
            "begin_stroke must not clone, patch, or dirty the full canvas"
        );

        app.paint_dot(egui::pos2(10.0, 10.0), rect);
        let first_len = app.stroke_originals.len();
        assert!(
            first_len < CANVAS_W * CANVAS_H / 100,
            "single-click original capture should be local, got {first_len} pixels"
        );
        let dirty_len = app
            .dirty_bbox
            .and_then(|bbox| bbox.clamped())
            .map(|(lx, ty, rx, by)| (rx - lx + 1) * (by - ty + 1))
            .expect("first paint should dirty a local texture region");
        assert!(
            dirty_len < CANVAS_W * CANVAS_H / 100,
            "single-click texture upload should be local, got {dirty_len} pixels"
        );

        app.paint_stroke(egui::pos2(10.0, 10.0), egui::pos2(20.0, 20.0), rect);
        app.commit_stroke();
        let patch = app.history.back().expect("stroke should push undo history");
        assert!(
            patch.pixels.len() < CANVAS_W * CANVAS_H / 100,
            "short stroke patch should stay local, got {} pixels",
            patch.pixels.len()
        );

        assert_eq!(app.pixels[10 * CANVAS_W + 10], Color32::RED);
        app.undo();
        assert_eq!(app.pixels[10 * CANVAS_W + 10], Color32::WHITE);
    }

    #[test]
    fn non_stroke_drawing_does_not_create_pending_stroke_patch() {
        let mut app = fresh();
        app.stroke_canvas(0, 0, 10, 10, 2, Color32::BLACK);
        assert!(
            app.stroke_originals.is_empty() && app.stroke_bbox.is_none(),
            "shape/line helpers should not create stroke undo patches unless a brush stroke is active"
        );
    }

    #[test]
    fn no_op_stroke_does_not_dirty_or_push_history() {
        let mut app = fresh();
        let rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(CANVAS_W as f32, CANVAS_H as f32),
        );
        app.color = Color32::WHITE;
        app.dirty = false;
        app.dirty_bbox = None;

        app.begin_stroke();
        app.paint_dot(egui::pos2(10.0, 10.0), rect);
        app.commit_stroke();

        assert!(!app.dirty, "no-op paint should not dirty the texture");
        assert!(
            app.dirty_bbox.is_none(),
            "no-op paint should not upload a region"
        );
        assert!(
            app.history.is_empty(),
            "no-op paint should not push undo history"
        );
    }

    #[test]
    fn flood_fill_patch_restores_changed_region_without_full_clone() {
        let mut app = fresh();
        let rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(CANVAS_W as f32, CANVAS_H as f32),
        );

        for y in 10..20 {
            for x in 10..20 {
                app.pixels[y * CANVAS_W + x] = Color32::BLACK;
            }
        }
        app.pixels[15 * CANVAS_W + 15] = Color32::BLUE;
        app.color = Color32::RED;

        let patch = app
            .flood_fill(egui::pos2(10.0, 10.0), rect)
            .expect("fill should produce an undo patch");
        assert!(
            patch.pixels.len() < CANVAS_W * CANVAS_H / 100,
            "small fill patch should be local, got {} pixels",
            patch.pixels.len()
        );
        app.push_patch(patch);

        assert_eq!(app.pixels[10 * CANVAS_W + 10], Color32::RED);
        assert_eq!(app.pixels[15 * CANVAS_W + 15], Color32::BLUE);
        app.undo();
        assert_eq!(app.pixels[10 * CANVAS_W + 10], Color32::BLACK);
        assert_eq!(app.pixels[15 * CANVAS_W + 15], Color32::BLUE);
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn click_path_release_latency_smoke() {
        let mut app = fresh();
        let rect = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(CANVAS_W as f32, CANVAS_H as f32),
        );
        app.dirty = false;
        app.dirty_bbox = None;
        app.brush_size = 5.0;
        app.color = Color32::BLACK;

        let iterations = 500u32;
        let start = Instant::now();
        for i in 0..iterations {
            let x = 20.0 + (i % 100) as f32;
            let y = 20.0 + (i / 100) as f32;
            app.begin_stroke();
            app.paint_dot(egui::pos2(x, y), rect);
            app.commit_stroke();
        }
        let elapsed = start.elapsed();
        let per_click_us = elapsed.as_micros() as f64 / iterations as f64;
        eprintln!("click_path_release_latency_smoke: {per_click_us:.2}us/click");
        assert!(
            per_click_us < 1_000.0,
            "click path averaged {per_click_us:.2}us; expected under 1ms"
        );
    }

    #[test]
    fn text_stamp_chars() {
        let mut app = fresh();
        // Stamp 'A' at (0,0) scale 1 — should set at least one non-white pixel.
        app.stamp_char('A', 0, 0, 1, Color32::BLACK);
        let painted = app.pixels[..8 * CANVAS_W + 8].contains(&Color32::BLACK);
        assert!(painted, "stamp_char should paint pixels for 'A'");
    }

    #[test]
    fn spray_prng_nonzero() {
        // Verify that next_rand never returns the same sequence starting from a
        // known seed (i.e. the PRNG is not stuck at 0).
        let mut state = 0xdeadbeef_u32;
        let vals: Vec<u32> = (0..10).map(|_| next_rand(&mut state)).collect();
        let all_same = vals.windows(2).all(|w| w[0] == w[1]);
        assert!(!all_same, "PRNG should not produce constant output");
    }

    // ── 105-scenario failure-rate test (<1%) ─────────────────────────────────

    #[test]
    fn hundred_scenario_failure_rate() {
        let total: u32 = 105;
        let mut failures: u32 = 0;

        // ── 50 pixel-correctness roundtrips ──────────────────────────────────
        for i in 0..50u32 {
            let r = ((i * 13) % 256) as u8;
            let g = ((i * 17 + 40) % 256) as u8;
            let b = ((i * 23 + 80) % 256) as u8;
            let fill = Color32::from_rgb(r, g, b);
            let mut a = fresh();
            a.pixels.fill(fill);
            let p = tmp(&format!("rp_bulk_{i}.png"));

            if a.save_png(&p).is_err() {
                failures += 1;
                continue;
            }
            let mut b_app = fresh();
            if b_app.load_png(&p).is_err() {
                failures += 1;
                std::fs::remove_file(&p).ok();
                continue;
            }
            let mismatch = b_app
                .pixels
                .iter()
                .filter(|px| px.r() != fill.r() || px.g() != fill.g() || px.b() != fill.b())
                .count();
            if mismatch > 0 {
                failures += 1;
            }
            std::fs::remove_file(&p).ok();
        }

        // ── 30 save smoke checks ──────────────────────────────────────────────
        let ref_app = fresh();
        for i in 0..30u32 {
            let p = tmp(&format!("rp_time_s_{i}.png"));
            match ref_app.save_png(&p) {
                Ok(()) => {
                    match std::fs::metadata(&p) {
                        Ok(meta) if meta.len() > 0 => {}
                        _ => failures += 1,
                    }
                    std::fs::remove_file(&p).ok();
                }
                Err(_) => {
                    failures += 1;
                }
            }
        }

        // ── 20 load smoke checks ──────────────────────────────────────────────
        let ref_path = tmp("rp_time_ref.png");
        ref_app
            .save_png(&ref_path)
            .expect("reference save for timing");
        for _ in 0..20u32 {
            let mut la = fresh();
            match la.load_png(&ref_path) {
                Ok(()) => {
                    if !la.dirty {
                        failures += 1;
                    }
                }
                Err(_) => {
                    failures += 1;
                }
            }
        }
        std::fs::remove_file(&ref_path).ok();

        // ── 5 expected-error scenarios ────────────────────────────────────────
        let corrupt = tmp("rp_mass_corrupt.png");
        std::fs::write(&corrupt, b"garbage data").ok();

        let error_cases: &[&str] = &[
            "/no_such_dir_zzzz/out.png",          // bad save path
            "/tmp/rp_definitely_missing_xyz.png", // nonexistent load
            "/tmp/rp_definitely_missing_abc.png", // nonexistent load
            &corrupt,                             // corrupt file
            &corrupt,                             // corrupt again (idempotent)
        ];
        let mut error_app = fresh();
        for (idx, &case) in error_cases.iter().enumerate() {
            let ok = if idx == 0 {
                error_app.save_png(case).is_err()
            } else {
                error_app.load_png(case).is_err()
            };
            if !ok {
                failures += 1;
            }
        }
        std::fs::remove_file(&corrupt).ok();

        let pct = failures as f64 / total as f64 * 100.0;
        assert!(
            pct < 1.0,
            "{failures}/{total} scenarios failed = {pct:.2}% (limit: 1%)"
        );
    }
}
