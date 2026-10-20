//! Install tempo: the app that comes on tempo's disk image. It puts tempo in Applications (or
//! updates the one there), adds the `tempo` command and the example songs if wanted, opens tempo
//! and puts the disk image away; or takes tempo out again. `scripts/release-mac.sh` builds it
//! with tempo inside it.

mod install;
mod ui;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};

/// Geist, tempo's type.
fn fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../../app/assets/fonts/Geist-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../app/assets/fonts/Geist-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../../app/assets/fonts/Geist-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../../app/assets/fonts/Geist-Bold.ttf")),
        Cow::Borrowed(include_bytes!(
            "../../app/assets/fonts/GeistMono-Regular.ttf"
        )),
    ]
}

/// The window's size.
const W: f32 = 780.0;
const H: f32 = 500.0;

fn main() {
    let plan = install::Plan::here();
    gpui_platform::application().run(move |cx: &mut App| {
        if let Err(e) = cx.text_system().add_fonts(fonts()) {
            eprintln!("fonts: {e}");
        }
        let bounds = Bounds::centered(None, size(px(W), px(H)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Install tempo".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(16.), px(16.))),
            }),
            is_resizable: false,
            ..Default::default()
        };
        cx.open_window(options, |_, cx| cx.new(|_| ui::Installer::new(plan)))
            .unwrap();
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ui::{Installer, Stage, State};

    /// The installer's screens drawn to PNGs, no window shown:
    /// `TEMPO_SHOT=<dir> cargo test -p tempo-installer shots -- --ignored` (all of them), with a
    /// tempo.app of the tests' own made beside them to install.
    #[test]
    #[ignore]
    fn shots() {
        let out = PathBuf::from(std::env::var("TEMPO_SHOT").expect("TEMPO_SHOT: a folder"));
        std::fs::create_dir_all(&out).unwrap();
        let root =
            std::env::temp_dir().join(format!("tempo-installer-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let payload = root.join("Resources/tempo.app");
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
            songs: root.join("Music/tempo"),
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
                        (ui::Job::Copy, State::Done("/Applications/tempo.app".into())),
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
                        (ui::Job::Copy, State::Done("/Applications/tempo.app".into())),
                        (
                            ui::Job::Register,
                            State::Done("Ready to open from anywhere".into()),
                        ),
                        (ui::Job::Command, State::Done("/usr/local/bin/tempo".into())),
                        (ui::Job::Examples, State::Done("1 in ~/Music/tempo".into())),
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
}
