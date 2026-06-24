// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rustpaint::RustPaint;

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
