//! Renders the app icon (`crates/app/icon/pina.svg`, from `scripts/icon.py`) at every size macOS
//! asks for, and packs them into `pina.icns` beside it with `iconutil`; and at Windows' sizes into
//! `pina.ico`.
//!
//! `cargo run -p pina --example icon`

use std::{path::Path, process::Command};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("icon");
    let svg = std::fs::read(dir.join("pina.svg")).expect("pina.svg: run scripts/icon.py");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).unwrap();
    let render = |px: u32| {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px).unwrap();
        let s = px as f32 / tree.size().width();
        let at = resvg::tiny_skia::Transform::from_scale(s, s);
        resvg::render(&tree, at, &mut pixmap.as_mut());
        pixmap
    };
    write_ico(&dir.join("pina.ico"), &render);
    let set = std::env::temp_dir().join("pina.iconset");
    let _ = std::fs::remove_dir_all(&set);
    std::fs::create_dir_all(&set).unwrap();
    for size in [16u32, 32, 128, 256, 512] {
        for (scale, suffix) in [(1, ""), (2, "@2x")] {
            render(size * scale)
                .save_png(set.join(format!("icon_{size}x{size}{suffix}.png")))
                .unwrap();
        }
    }
    let out = dir.join("pina.icns");
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

/// Windows' icon: a PNG at each size it shows, in an `.ico`.
fn write_ico(file: &Path, render: &dyn Fn(u32) -> resvg::tiny_skia::Pixmap) {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let pngs: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&px| render(px).encode_png().unwrap())
        .collect();
    let mut ico = vec![0, 0, 1, 0];
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&px, png) in sizes.iter().zip(&pngs) {
        // 256 is written as 0.
        let side = if px == 256 { 0 } else { px as u8 };
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    pngs.iter().for_each(|png| ico.extend(png));
    std::fs::write(file, ico).unwrap();
    println!("{}", file.display());
}
