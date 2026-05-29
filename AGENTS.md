# AGENTS.md

## Cursor Cloud specific instructions

### Overview

RustPaint (MasterPaint) is a single-binary desktop painting app built with Rust, egui 0.31, and eframe 0.31 (glow/OpenGL backend). No database, no network services, no Docker.

### Build / Run / Lint / Test

| Task | Command |
|------|---------|
| Build | `cargo build --features eframe/x11` |
| Run | `DISPLAY=:99 cargo run --features eframe/x11` |
| Lint | `cargo clippy --features eframe/x11` |
| Test | No automated tests exist in this codebase |

The `--features eframe/x11` flag is **required** on Linux because the `Cargo.toml` sets `default-features = false` on eframe, omitting platform-specific windowing features.

### Display server

The app requires an X11 display. Start Xvfb before running:

```sh
Xvfb :99 -screen 0 1920x1080x24 &
export DISPLAY=:99
```

### System libraries required (already in VM snapshot)

`libgl1-mesa-dev`, `libxcb-*`, `libxkbcommon-dev`, `libxkbcommon-x11-0`, `xvfb`, and related X11 dev packages. These are pre-installed; if a fresh VM is created, `sudo apt-get install -y libgl1-mesa-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libxkbcommon-x11-0 libxcb-render0-dev libxcb-xkb-dev libxcb-icccm4-dev libxcb-image0-dev libxcb-keysyms1-dev libxcb-randr0-dev libxcb-cursor-dev libxcb-util-dev libx11-dev libxi-dev libxtst-dev xvfb` is needed.

### Gotchas

- Rust stable must be kept up-to-date (`rustup update stable && rustup default stable`) because transitive dependencies (e.g. `moxcms`) require edition 2024 features.
- The release profile uses aggressive optimizations (`opt-level = "z"`, LTO) so release builds take significantly longer than dev builds.
