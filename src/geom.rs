use egui::Color32;

use crate::constants::{CANVAS_H, CANVAS_W};
use crate::types::{Patch, ShapeKind};

/// Inclusive axis-aligned bbox in canvas pixel coordinates.
#[derive(Clone, Copy)]
pub struct Bbox {
    pub lx: i32,
    pub ty: i32,
    pub rx: i32,
    pub by: i32,
}

impl Bbox {
    pub fn point(x: i32, y: i32) -> Self {
        Self {
            lx: x,
            ty: y,
            rx: x,
            by: y,
        }
    }

    pub fn from_two(a: (i32, i32), b: (i32, i32)) -> Self {
        Self {
            lx: a.0.min(b.0),
            ty: a.1.min(b.1),
            rx: a.0.max(b.0),
            by: a.1.max(b.1),
        }
    }

    pub fn expand(self, r: i32) -> Self {
        Self {
            lx: self.lx - r,
            ty: self.ty - r,
            rx: self.rx + r,
            by: self.by + r,
        }
    }

    pub fn union(self, o: Self) -> Self {
        Self {
            lx: self.lx.min(o.lx),
            ty: self.ty.min(o.ty),
            rx: self.rx.max(o.rx),
            by: self.by.max(o.by),
        }
    }

    pub fn include_point(&mut self, x: i32, y: i32) {
        self.lx = self.lx.min(x);
        self.rx = self.rx.max(x);
        self.ty = self.ty.min(y);
        self.by = self.by.max(y);
    }

    /// Clamp to the canvas; returns None if it lies entirely outside.
    pub fn clamped(self) -> Option<(usize, usize, usize, usize)> {
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

pub fn copy_region(src: &[Color32], lx: usize, ty: usize, w: usize, h: usize) -> Vec<Color32> {
    let mut buf = Vec::with_capacity(w * h);
    for row in ty..ty + h {
        let s = row * CANVAS_W + lx;
        buf.extend_from_slice(&src[s..s + w]);
    }
    buf
}

/// Copy a rectangular region of `src` into a new `Patch`. Returns `None` if
/// the bbox lies outside the canvas.
pub fn make_patch(src: &[Color32], bbox: Bbox) -> Option<Patch> {
    let (lx, ty, rx, by) = bbox.clamped()?;
    let w = rx - lx + 1;
    let h = by - ty + 1;
    Some(Patch {
        x: lx as u32,
        y: ty as u32,
        w: w as u32,
        h: h as u32,
        pixels: copy_region(src, lx, ty, w, h),
    })
}

pub fn patch_bbox(patch: &Patch) -> Bbox {
    Bbox {
        lx: patch.x as i32,
        ty: patch.y as i32,
        rx: (patch.x + patch.w - 1) as i32,
        by: (patch.y + patch.h - 1) as i32,
    }
}

pub fn ellipse_perimeter(
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

pub fn regular_polygon(cx: f32, cy: f32, r: f32, n: usize, start_angle: f32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = start_angle + std::f32::consts::TAU * i as f32 / n as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub enum ShapeGeometry {
    Rect {
        lx: f32,
        ty: f32,
        rx: f32,
        by: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Polygon(Vec<(f32, f32)>),
}

pub fn shape_geometry(kind: ShapeKind, x0: f32, y0: f32, x1: f32, y1: f32) -> ShapeGeometry {
    let lx = x0.min(x1);
    let rx = x0.max(x1);
    let ty = y0.min(y1);
    let by = y0.max(y1);
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let half_w = ((x1 - x0).abs() / 2.0).max(1.0);
    let half_h = ((y1 - y0).abs() / 2.0).max(1.0);
    let radius = half_w.min(half_h);

    match kind {
        ShapeKind::Rect => ShapeGeometry::Rect { lx, ty, rx, by },
        ShapeKind::Ellipse => ShapeGeometry::Ellipse {
            cx,
            cy,
            rx: half_w,
            ry: half_h,
        },
        ShapeKind::Circle => ShapeGeometry::Ellipse {
            cx,
            cy,
            rx: radius,
            ry: radius,
        },
        ShapeKind::Triangle => ShapeGeometry::Polygon(regular_polygon(
            cx,
            cy,
            radius,
            3,
            -std::f32::consts::FRAC_PI_2,
        )),
        ShapeKind::Pentagon => ShapeGeometry::Polygon(regular_polygon(
            cx,
            cy,
            radius,
            5,
            -std::f32::consts::FRAC_PI_2,
        )),
        ShapeKind::Diamond => ShapeGeometry::Polygon(vec![
            (cx, cy - half_h),
            (cx + half_w, cy),
            (cx, cy + half_h),
            (cx - half_w, cy),
        ]),
    }
}
