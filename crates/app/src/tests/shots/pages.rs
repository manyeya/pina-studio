//! The pages: song study, playbooks, settings and songs.

use super::*;

/// Sets up the page, if it's one of these; says whether it was.
pub(super) fn set_up(
    shot: &Shot,
    ws: &mut Workspace,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Workspace>,
) -> bool {
    let Shot { page, song, out } = *shot;
    let _ = (song, out, &window);
    match page {
        "study" => {
            ws.open_study(cx);
            if let Some(want) = &song {
                ws.studies.open = ws
                    .studies
                    .list
                    .iter()
                    .find(|s| s.file.to_string_lossy().contains(want))
                    .map(|s| s.file.clone());
            }
        }
        "study-empty" => {
            ws.open_study(cx);
            ws.studies.list.clear();
            ws.studies.open = None;
        }
        "playbooks" => ws.open_genres(None, cx),
        "settings-auto" => {
            ws.settings_section = crate::pages::settings::Section::Automation;
            ws.open_settings(cx);
        }
        "settings" => {
            ws.settings.hidden = vec!["score".into(), "takes".into(), "live_loops".into()];
            ws.open_settings(cx);
        }
        // Modulation: an LFO, steps, an envelope, a follower and random on the keys,
        // mapped to its pan, volume, low cut and presence; values as if playing.
        "home" | "songs" => {
            let demo = crate::pages::projects::summary(&demo()).unwrap();
            let names = [
                ("Night Drive", 92.0, 0),
                ("Rain on Glass", 74.0, 3_600),
                ("Untitled 9", 120.0, 7_200),
                ("Lo-fi Sketch", 84.0, 90_000),
                ("Warehouse", 128.0, 200_000),
                ("Morning Pages", 100.0, 400_000),
            ];
            ws.songs = names
                .iter()
                .map(|&(name, tempo, ago)| crate::pages::projects::Song {
                    dir: std::path::PathBuf::from(format!("/songs/{name}")),
                    name: name.to_string(),
                    modified: std::time::SystemTime::now() - std::time::Duration::from_secs(ago),
                    tempo,
                    ..demo.clone()
                })
                .collect();
            ws.session.on_disk = page == "songs";
            ws.view = View::Songs;
            cx.notify();
        }
        // The Tool menu open on the Fade tool, the drums faded in and out.
        _ => return false,
    }
    true
}
