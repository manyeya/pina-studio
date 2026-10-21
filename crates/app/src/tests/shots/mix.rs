//! Mixing: the Channel EQ in each of its states, and the Bounce sheet.

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
        "bounce" => {
            ws.open_export(cx);
            ws.choose(crate::ui::menu::MenuAction::Nothing, cx);
        }
        "bounce-stems" => {
            ws.export_stem(0, cx);
            if let Some(e) = ws.export.as_mut() {
                e.tracks = vec![true; e.tracks.len()];
                e.formats = vec![
                    pina_engine::audio::bounce::Format::Wav,
                    pina_engine::audio::bounce::Format::Mp3 { kbps: 320 },
                    pina_engine::audio::bounce::Format::Flac,
                ];
                e.level = pina_engine::audio::bounce::Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                };
            }
        }
        "bounce-done" => {
            ws.export_stem(0, cx);
            if let Some(e) = ws.export.as_mut() {
                e.tracks = vec![true; e.tracks.len()];
                e.formats = vec![
                    pina_engine::audio::bounce::Format::Wav,
                    pina_engine::audio::bounce::Format::Mp3 { kbps: 320 },
                ];
                e.level = pina_engine::audio::bounce::Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                };
                e.folder = out.join("bounces");
            }
            ws.start_export(cx);
        }
        "eq" | "eq-picked" | "eq-menu" | "eq-mixer" | "eq-hover" | "eq-multi" | "eq-piano"
        | "eq-analyzer" | "eq-empty" | "eq-dynamics" | "eq-output" | "eq-list" | "eq-match"
        | "eq-sketch" | "eq-grab" | "eq-compare" | "eq-spectral" | "eq-modes" | "eq-slopes"
        | "eq-instances" => {
            use pina_engine::song::project::{Bands, Dynamic, Eq, EqBand, Placement, Shape};
            let mut cut = EqBand::new(Shape::LowCut, 32.0, 0.0);
            cut.slope = 24.0;
            let mut side = EqBand::new(Shape::LowCut, 160.0, 0.0);
            side.placement = Placement::Side;
            let mut mud = EqBand::new(Shape::Bell, 320.0, -3.5);
            mud.q = 1.4;
            let mut harsh = EqBand::new(Shape::Bell, 3_400.0, 1.5);
            harsh.q = 2.2;
            harsh.dynamic = Some(Dynamic {
                range_db: -4.0,
                threshold_db: Some(-28.0),
                ..Dynamic::default()
            });
            let notch = EqBand::new(Shape::Notch, 1_150.0, 0.0);
            let air = EqBand::new(Shape::HighShelf, 11_000.0, 3.0);
            let mut eq = Eq {
                bands: Bands::from_slice(&[cut, side, mud, notch, harsh, air]),
                ..Eq::default()
            };
            if page == "eq-slopes" {
                eq.bands[0].slope = 9.5;
                eq.bands[2].slope = 30.5;
                eq.bands[4].slope = 48.0;
                eq.bands[4].gain_db = 6.0;
            }
            if page == "eq-spectral" {
                use pina_engine::song::project::Processing;
                (eq.processing, eq.resolution) = (Processing::Linear, 2);
                let d = eq.bands[4].dynamic.as_mut().unwrap();
                (d.spectral, d.density) = (true, 0.7);
            }
            ws.session.project.tracks[0].eq = Some(eq);
            if page == "eq-instances" {
                let others = [
                    (1, vec![EqBand::new(Shape::Bell, 2_500.0, 3.0), air]),
                    (2, vec![cut, EqBand::new(Shape::Bell, 60.0, 4.0)]),
                ];
                for (t, bands) in others {
                    ws.session.project.tracks[t].eq = Some(Eq {
                        bands: Bands::from_slice(&bands),
                        ..Eq::default()
                    });
                }
                ws.session.project.master_eq = Some(Eq {
                    bands: Bands::from_slice(&[EqBand::new(Shape::TiltShelf, 900.0, 1.5)]),
                    ..Eq::default()
                });
            }
            ws.channel = Channel::Track(0);
            if page == "eq-mixer" {
                ws.view = View::Mixer;
            } else {
                ws.open_eq(Channel::Track(0), cx);
                let rate = ws.session.project.sample_rate as f64;
                let v = ws.eq_view.as_mut().unwrap();
                v.pretend(&eq, rate);
                if page != "eq" {
                    v.picked = vec![eq.bands[4].id];
                }
                if page == "eq-menu" {
                    let id = eq.bands[4].id;
                    ws.band_menu(id, point(px(900.), px(300.)), cx);
                }
                let v = ws.eq_view.as_mut().unwrap();
                match page {
                    "eq" => v.picked.clear(),
                    "eq-hover" => {
                        v.picked.clear();
                        let g = v.geo(rate);
                        v.pointer = Some((g.w * 0.45, g.h * 0.3));
                    }
                    "eq-multi" => v.picked = vec![eq.bands[2].id, eq.bands[3].id, eq.bands[4].id],
                    "eq-piano" => {
                        v.piano = true;
                        v.picked = vec![eq.bands[2].id];
                        v.hover = Some(eq.bands[2].id);
                    }
                    "eq-analyzer" => {
                        v.picked.clear();
                        v.popup = Some(crate::mix::eq::Popup::Analyzer);
                    }
                    "eq-dynamics" | "eq-spectral" => v.dyn_open = true,
                    "eq-list" | "eq-match" | "eq-sketch" | "eq-grab" => {
                        v.picked.clear();
                        v.pretend_tool(&page[3..], rate);
                    }
                    "eq-instances" => {
                        v.picked.clear();
                        v.list = true;
                        v.listing.zoom = 1;
                        v.listing.expanded = Some(Channel::Track(1));
                    }
                    "eq-compare" => {
                        v.picked.clear();
                        v.ext = Some(ws.session.project.tracks[1].id);
                    }
                    "eq-output" => {
                        v.picked.clear();
                        v.popup = Some(crate::mix::eq::Popup::Output);
                    }
                    _ => {}
                }
                if page == "eq-instances" {
                    ws.open_list(cx);
                    ws.pretend_list();
                }
                if page == "eq-modes" {
                    ws.eq_view.as_mut().unwrap().picked.clear();
                    ws.eq_processing_menu(point(px(330.), px(560.)), cx);
                }
                if page == "eq-empty" {
                    ws.session.project.tracks[0].eq = Some(Eq::default());
                    let v = ws.eq_view.as_mut().unwrap();
                    v.picked.clear();
                    let g = v.geo(rate);
                    v.pointer = Some((g.w * 0.6, g.h * 0.35));
                }
            }
            cx.notify();
        }
        _ => return false,
    }
    true
}
