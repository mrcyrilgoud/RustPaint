use egui::Color32;

pub const HISTORY_CAP: usize = 50;
pub const SELECTION_COLOR: Color32 = Color32::from_rgb(30, 120, 255);
pub const CANVAS_W: usize = 1200;
pub const CANVAS_H: usize = 800;

/// Primary palette row 1: neutrals + classic hues (18 swatches).
#[rustfmt::skip]
pub const PALETTE_ROW1: [Color32; 18] = [
    Color32::from_rgb(  0,   0,   0),
    Color32::from_rgb( 64,  64,  64),
    Color32::from_rgb(128, 128, 128),
    Color32::from_rgb(192, 192, 192),
    Color32::from_rgb(255, 255, 255),
    Color32::from_rgb(255,   0,   0),
    Color32::from_rgb(128,   0,   0),
    Color32::from_rgb(255, 128,   0),
    Color32::from_rgb(255,  69,   0),
    Color32::from_rgb(255, 215,   0),
    Color32::from_rgb(  0, 200,   0),
    Color32::from_rgb(  0, 128,   0),
    Color32::from_rgb(  0,  80,   0),
    Color32::from_rgb(  0, 200, 200),
    Color32::from_rgb(135, 206, 235),
    Color32::from_rgb(  0,   0, 255),
    Color32::from_rgb(  0,   0, 128),
    Color32::from_rgb(128,   0, 128),
];

/// Primary palette row 2: pastels, earth tones, skin tones, neons (18 swatches).
#[rustfmt::skip]
pub const PALETTE_ROW2: [Color32; 18] = [
    Color32::from_rgb(255, 182, 193),
    Color32::from_rgb(250, 128, 114),
    Color32::from_rgb(255, 218, 185),
    Color32::from_rgb(255, 250, 205),
    Color32::from_rgb(152, 251, 152),
    Color32::from_rgb(224, 255, 255),
    Color32::from_rgb(173, 216, 230),
    Color32::from_rgb(221, 160, 221),
    Color32::from_rgb(255, 221, 193),
    Color32::from_rgb(232, 184, 154),
    Color32::from_rgb(198, 134,  66),
    Color32::from_rgb(141,  85,  36),
    Color32::from_rgb(139,  69,  19),
    Color32::from_rgb(210, 180, 140),
    Color32::from_rgb( 57, 255,  20),
    Color32::from_rgb(255, 255,   0),
    Color32::from_rgb(255,   0, 255),
    Color32::from_rgb(255,  20, 147),
];

pub const PALETTE_ROWS: [&[Color32; 18]; 2] = [&PALETTE_ROW1, &PALETTE_ROW2];
