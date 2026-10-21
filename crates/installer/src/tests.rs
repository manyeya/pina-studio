use std::path::PathBuf;

use super::*;
use crate::ui::{Installer, Stage, State};

/// The installer's screens drawn to PNGs, no window shown:
/// `PINA_SHOT=<dir> cargo test -p pina-installer shots -- --ignored` (all of them), with a
/// Pina Studio.app of the tests' own made beside them to install.
#[test]
#[ignore]
fn shots() {
    let out = PathBuf::from(std::env::var("PINA_SHOT").expect("PINA_SHOT: a folder"));
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("pina-installer-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let payload = root.join("Resources").join(install::APP);
    std::fs::create_dir_all(payload.join("Contents/MacOS")).unwrap();
    std::fs::write(
        payload.join("Contents/Info.plist"),
        "<key>CFBundleShortVersionString</key><string>0.2.0</string>",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("Resources/Examples/Midnight Drive")).unwrap();
    std::fs::write(
        root.join("Resources/Examples/Midnight Drive/project.json"),
        "{}",
    )
    .unwrap();
    let plan = install::Plan {
        payload,
        examples: Some(root.join("Resources/Examples")),
        apps: root.join("Applications"),
        bin: root.join("bin"),
        songs: root.join("Music/Pina Studio"),
    };
    type Set = Box<dyn Fn(&mut Installer)>;
    let pages: [(&str, Set); 6] = [
        ("install", Box::new(|_| {})),
        (
            "update",
            Box::new(|v| v.over = install::Over::Older("0.1.4".into())),
        ),
        (
            "working",
            Box::new(|v| {
                v.stage = Stage::Working;
                v.steps = vec![
                    (
                        ui::Job::Copy,
                        State::Done("/Applications/Pina Studio.app".into()),
                    ),
                    (
                        ui::Job::Register,
                        State::Done("Ready to open from anywhere".into()),
                    ),
                    (ui::Job::Command, State::Running),
                    (ui::Job::Examples, State::Waiting),
                ];
            }),
        ),
        (
            "done",
            Box::new(|v| {
                v.stage = Stage::Done;
                v.steps = vec![
                    (
                        ui::Job::Copy,
                        State::Done("/Applications/Pina Studio.app".into()),
                    ),
                    (
                        ui::Job::Register,
                        State::Done("Ready to open from anywhere".into()),
                    ),
                    (ui::Job::Command, State::Done("/usr/local/bin/pina".into())),
                    (
                        ui::Job::Examples,
                        State::Done("1 in ~/Music/Pina Studio".into()),
                    ),
                ];
            }),
        ),
        (
            "waiting",
            Box::new(|v| {
                v.stage = Stage::Waiting;
                v.steps = vec![
                    (ui::Job::Copy, State::Waiting),
                    (ui::Job::Register, State::Waiting),
                ];
            }),
        ),
        (
            "uninstall",
            Box::new(|v| {
                v.over = install::Over::Same;
                v.stage = Stage::Removing;
            }),
        ),
    ];
    for (name, set) in pages {
        let mut cx = gpui::HeadlessAppContext::with_platform(
            std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
            std::sync::Arc::new(()),
            || {
                Ok(Some(Box::new(
                    gpui_apple::metal_renderer::MetalHeadlessRenderer::new(),
                )))
            },
        );
        cx.update(|cx| cx.text_system().add_fonts(fonts()).unwrap());
        let plan = plan.clone();
        let window = cx
            .open_window(size(px(W), px(H)), |_, cx| {
                cx.new(|_| {
                    let mut v = Installer::new(plan);
                    set(&mut v);
                    // Mid-celebration when it's done; settled otherwise.
                    v.wind(if v.stage == Stage::Done { 0.9 } else { 3.0 });
                    v
                })
            })
            .unwrap();
        cx.run_until_parked();
        let image = cx.capture_screenshot(window.into()).unwrap();
        image
            .save(out.join(format!("installer-{name}.png")))
            .unwrap();
    }
    let _ = std::fs::remove_dir_all(root);
}
