//! Pages drawn to PNGs, to look at without a window on screen.

mod agent;
mod editors;
mod mix;
mod pages;
mod tracks;

use super::*;

/// The page to draw, the song study to show on it, and where its files go.
#[derive(Clone, Copy)]
struct Shot<'a> {
    page: &'a str,
    song: Option<&'a str>,
    out: &'a Path,
}

/// Pages drawn to PNGs, to look at without a window on screen:
/// `PINA_SHOT=<dir> PINA_SHOT_PAGE=study cargo test -p pina shots -- --ignored`. The Song
/// Study page shows the studies kept on this Mac, the first or the one whose file name has
/// `PINA_SHOT_SONG` in it.
#[test]
#[ignore]
fn shots() {
    let out = PathBuf::from(std::env::var("PINA_SHOT").expect("PINA_SHOT: a folder for the PNGs"));
    std::fs::create_dir_all(&out).unwrap();
    // Playbooks of its own, as every test's: opening the window makes the folder if it's missing.
    let books = std::env::temp_dir().join(format!("pina-shot-books-{}", std::process::id()));
    unsafe { std::env::set_var("PINA_PLAYBOOKS", &books) };
    let page = std::env::var("PINA_SHOT_PAGE").unwrap_or_else(|_| "study".into());
    let tall: f32 = std::env::var("PINA_SHOT_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900.0);
    let mut cx = gpui::HeadlessAppContext::with_platform(
        std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
        std::sync::Arc::new(()),
        || {
            Ok(Some(Box::new(
                gpui_apple::metal_renderer::MetalHeadlessRenderer::new(),
            )))
        },
    );
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(crate::ui::theme::fonts())
            .unwrap();
        bind_keys(cx);
    });
    // PINA_SHOT_PROJECT: a song of its own to open instead of the demo.
    let project = std::env::var_os("PINA_SHOT_PROJECT").map_or_else(demo, PathBuf::from);
    let session = Session::open(&project).unwrap();
    let player = Player::silent(&session.project, &session.dir);
    let window = cx
        .open_window(gpui::size(px(1440.), px(tall)), |window, cx| {
            gpui::AppContext::new(cx, |cx| Workspace::new(session, player, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    let song = std::env::var("PINA_SHOT_SONG").ok();
    cx.update(|cx| {
        window
            .update(cx, |ws, window, cx| {
                // PINA_SHOT_ZOOM: the tracks' pixels per beat, to show more of the song.
                if let Some(z) = std::env::var("PINA_SHOT_ZOOM")
                    .ok()
                    .and_then(|z| z.parse().ok())
                {
                    ws.px_per_beat = z;
                    cx.notify();
                }
                let shot = Shot {
                    page: &page,
                    song: song.as_deref(),
                    out: &out,
                };
                for set_up in [
                    agent::set_up,
                    editors::set_up,
                    mix::set_up,
                    pages::set_up,
                    tracks::set_up,
                ] {
                    if set_up(&shot, ws, window, cx) {
                        break;
                    }
                }
            })
            .unwrap()
    });
    cx.run_until_parked();
    // A bounce runs to its end before it's drawn.
    for _ in 0..600 {
        let running = cx.update(|cx| {
            window
                .read(cx)
                .map(|ws| {
                    ws.export
                        .as_ref()
                        .and_then(|e| e.run.as_ref())
                        .is_some_and(|r| !r.finished)
                })
                .unwrap_or(false)
        });
        if !running {
            break;
        }
        cx.advance_clock(Duration::from_millis(150));
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
    let image = cx.capture_screenshot(window.into()).unwrap();
    let file = out.join(format!("{page}.png"));
    image.save(&file).unwrap();
    println!("{}", file.display());
}
