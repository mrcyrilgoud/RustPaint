use std::sync::OnceLock;

use egui::Color32;

use crate::constants::PALETTE_ROWS;

pub fn color_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

static PALETTE_HEX: [OnceLock<[String; 18]>; 2] = [OnceLock::new(), OnceLock::new()];

pub fn palette_hex(row_idx: usize) -> &'static [String; 18] {
    PALETTE_HEX[row_idx]
        .get_or_init(|| std::array::from_fn(|i| color_hex(PALETTE_ROWS[row_idx][i])))
}
