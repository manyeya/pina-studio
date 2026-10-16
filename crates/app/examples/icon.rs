//! Renders the app icon (`crates/app/icon/tempo.svg`, from `scripts/icon.py`) at every size macOS
//! asks for, and packs them into `tempo.icns` beside it with `iconutil`.
//!
//! `cargo run -p tempo --example icon`

use std::{path::Path, process::Command};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("icon");
    let svg = std::fs::read(dir.join("tempo.svg")).expect("tempo.svg: run scripts/icon.py");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).unwrap();
    let set = std::env::temp_dir().join("tempo.iconset");
    let _ = std::fs::remove_dir_all(&set);
    std::fs::create_dir_all(&set).unwrap();
    for size in [16u32, 32, 128, 256, 512] {
        for (scale, suffix) in [(1, ""), (2, "@2x")] {
            let px = size * scale;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px).unwrap();
            let s = px as f32 / tree.size().width();
            let at = resvg::tiny_skia::Transform::from_scale(s, s);
            resvg::render(&tree, at, &mut pixmap.as_mut());
            pixmap
                .save_png(set.join(format!("icon_{size}x{size}{suffix}.png")))
                .unwrap();
        }
    }
    let out = dir.join("tempo.icns");
    let ok = Command::new("iconutil")
        .args(["-c", "icns"])
        .arg(&set)
        .arg("-o")
        .arg(&out)
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok, "iconutil failed");
    std::fs::remove_dir_all(&set).unwrap();
    println!("{}", out.display());
}
