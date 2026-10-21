//! Install Pina Studio: the app that comes on Pina Studio's disk image. It puts Pina Studio in
//! Applications (or updates the one there), adds the `pina` command and the example songs if
//! wanted, opens Pina Studio and puts the disk image away; or takes it out again.
//! `scripts/release-mac.sh` builds it with Pina Studio inside it.

mod install;
mod ui;

use std::borrow::Cow;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};

/// Geist, Pina Studio's type.
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
                title: Some("Install Pina Studio".into()),
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
mod tests;
