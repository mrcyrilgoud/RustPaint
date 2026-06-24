use egui::Color32;

/// A rectangular pre-image of the canvas, used to undo/redo a destructive op.
pub struct Patch {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<Color32>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Tool {
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
pub enum ShapeKind {
    Rect,
    Ellipse,
    Circle,
    Triangle,
    Diamond,
    Pentagon,
}
