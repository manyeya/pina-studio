//! The agent: the panel beside the tracks, its tuning and the palette.

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
        "beside" | "beside-agent" | "beside-sounds" => {
            ws.show_inspector = page != "beside";
            ws.show_browser = page != "beside-agent";
            ws.show_agent = page == "beside-agent";
            if page == "beside-sounds" {
                ws.catalog.scope = crate::sounds::catalog::Scope::Sounds;
            }
            ws.track = 1;
            ws.show_bottom(Bottom::PianoRoll, cx);
        }
        // The agent's model and effort, picked by Send.
        "agent-tuning" => {
            ws.chat.kind = crate::agent::Kind::ClaudeCode;
            ws.chat.tuning = crate::agent::Tuning {
                model: Some("opus".into()),
                effort: Some("high".into()),
            };
            ws.show_agent = true;
            let at = point(px(1440. - 200.), px(62.));
            ws.menu = Some(ws.tuning_menu(at));
            cx.notify();
        }
        // Sounds picked for the agent: in its message box, and the Browser picking them.
        "palette" | "palette-pick" | "palette-start" => {
            use pina_engine::{samples::Category, song::project::PaletteSound};
            if ws.browser.library.samples.is_empty() {
                ws.browser.library = pina_engine::samples::library::Library::load();
                ws.browser.loaded = true;
                ws.refilter(cx);
            }
            let lib = &ws.browser.library.samples;
            let mut sounds = Vec::new();
            for want in [
                Category::Kick,
                Category::Snare,
                Category::HatClosed,
                Category::Bass808,
                Category::Keys,
                Category::Pad,
            ] {
                if let Some(s) = lib.iter().find(|s| s.analysis.category == want) {
                    sounds.push(PaletteSound {
                        path: s.path.clone(),
                        role: crate::sounds::palette::role_for(want).map(str::to_string),
                    });
                }
            }
            sounds.truncate(3);
            if let Some(s) = sounds.get_mut(2) {
                s.role = None;
            }
            ws.session.project.palette.sounds = sounds;
            ws.session.project.palette.devices = ["own:synth", "own:comp"]
                .iter()
                .filter_map(|k| ws.catalog.device(k))
                .map(|d| crate::sounds::palette::palette_device(&ws.catalog.devices[d]))
                .collect();
            ws.session.project.palette.devices[0].role = Some("lead".into());
            ws.session.project.palette.only = true;
            ws.show_agent = true;
            if page == "palette-pick" {
                ws.pick_for_agent(crate::sounds::catalog::Scope::Instruments, cx);
            }
            if page == "palette-start" {
                ws.session.project.palette = Default::default();
            }
            cx.notify();
        }
        _ => return false,
    }
    true
}
