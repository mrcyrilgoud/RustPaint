use egui::Color32;

use super::*;
use crate::rng::next_rand;

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
