//! The tracks: their headers and menus, stacks, tools, automation and the New Track palette.

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
        // The Plugins page, on the plugins as last scanned; their pictures from a folder
        // PINA_SHOT_PICTURES names (index.json in it: key → picture, size, load).
        p if p.starts_with("plugins") => {
            use crate::sounds::{
                catalog::Scope,
                plugins::{Fact, HealthSet, Shown},
            };
            let cached: serde_json::Value = std::fs::read(pina_engine::plugin::scan::cache_file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let plugins: Vec<PluginInfo> = cached
                .as_object()
                .map(|m| {
                    m.values()
                        .flat_map(|c| {
                            serde_json::from_value::<Vec<PluginInfo>>(c["plugins"].clone())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            ws.plugins = Some(plugins);
            ws.catalog_plugins();
            if let Some(dir) = std::env::var_os("PINA_SHOT_PICTURES") {
                let index: serde_json::Value =
                    std::fs::read(std::path::Path::new(&dir).join("index.json"))
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .unwrap_or_default();
                for (key, v) in index.as_object().into_iter().flatten() {
                    ws.plugin_page.facts.insert(
                        key.clone(),
                        Fact {
                            picture: v["picture"].as_str().map(String::from),
                            w: v["w"].as_u64().unwrap_or(0) as u32,
                            h: v["h"].as_u64().unwrap_or(0) as u32,
                            load: v["load"].as_f64().map(|l| l as f32),
                            ..Default::default()
                        },
                    );
                }
            }
            ws.catalog.collections = vec![
                crate::sounds::plugins::Collection {
                    name: "Mix bus".into(),
                    color: 0xff9f43,
                    devices: vec![
                        "own:eq".into(),
                        "own:comp".into(),
                        "plugin:apple/aupeaklimiter".into(),
                    ],
                },
                crate::sounds::plugins::Collection {
                    name: "Vocal chain".into(),
                    color: 0x5b8cff,
                    devices: vec![
                        "plugin:apple/aureverb2".into(),
                        "plugin:apple/audelay".into(),
                    ],
                },
            ];
            ws.catalog.keys = vec![
                ("g".into(), "plugin:apple/aumatrixreverb".into()),
                ("f13".into(), "plugin:apple/augraphiceq".into()),
            ];
            ws.view = View::Plugins;
            ws.catalog.scope = Scope::Effects;
            ws.refresh_plugin_facts(cx);
            let page = &mut ws.plugin_page;
            page.picked = Some("plugin:apple/aumatrixreverb".into());
            match p {
                "plugins-list" => page.shown = Shown::List,
                "plugins-keys" => page.shown = Shown::Keys,
                "plugins-health" => {
                    page.shown = Shown::List;
                    page.set = Some(HealthSet::NeverInASong);
                }
                "plugins-instruments" => {
                    ws.catalog.scope = Scope::Instruments;
                    ws.plugin_page.picked = None;
                }
                "plugins-reverb" => page.cats = vec![crate::sounds::catalog::Cat::Reverb],
                _ => {}
            }
            // The Browser beside the tracks, its effects with their pictures.
            if p == "plugins-browser" {
                ws.view = View::Arrange;
                ws.show_browser = true;
            }
            if p == "plugins-mini" {
                ws.view = View::Mixer;
                let ch = crate::mix::strip::Channel::Track(0);
                ws.open_mini(
                    ch,
                    pina_engine::song::project::Which::Insert(0),
                    gpui::point(px(240.), px(300.)),
                    window,
                    cx,
                );
            }
        }
        p if p.starts_with("adder") => {
            // The plugins as last scanned, their presets as last read, the library.
            let cached: serde_json::Value = std::fs::read(pina_engine::plugin::scan::cache_file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let plugins: Vec<PluginInfo> = cached
                .as_object()
                .map(|m| {
                    m.values()
                        .flat_map(|c| {
                            serde_json::from_value::<Vec<PluginInfo>>(c["plugins"].clone())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            ws.plugins = Some(plugins);
            ws.catalog_plugins();
            ws.presets_now();
            // Read, not rescanned: the person's library is only looked at.
            ws.set_library(pina_engine::samples::library::Library::load(), cx);
            ws.open_adder(gpui::point(px(250.), px(110.)), window, cx);
            if let Some(q) = p.strip_prefix("adder-") {
                ws.adder
                    .as_ref()
                    .unwrap()
                    .search
                    .clone()
                    .update(cx, |s, cx| s.set_text(q, cx));
            }
        }
        "stack-rows" | "stack-folded" => {
            use pina_engine::dsp::modulate::{ModKind, ModRoute, ModTarget, Modulator};
            ws.session.project.tracks[0].modulators.push(Modulator {
                kind: ModKind::lfo(),
                routes: vec![ModRoute {
                    target: ModTarget::Volume,
                    depth: -6.0,
                }],
            });
            ws.make_stack(&[0, 2], cx);
            ws.session.project.auxes[0].name = "Rhythm".into();
            ws.session.project.auxes[0].volume_db = -3.0;
            ws.show_automation = page == "stack-rows";
            if page == "stack-folded" {
                ws.session.project.auxes[0].folded = true;
            }
        }
        // A stack of the keys and lead, both picked, sharing an LFO and a macro.
        "stack-shared" | "stack-shared-menu" => {
            use pina_engine::dsp::modulate::{ModKind, ModRoute, ModTarget};
            let p = &mut ws.session.project;
            let (bus, _) = p.add_stack(&[0, 1]);
            p.add_shared(bus, ModKind::lfo());
            p.add_shared(bus, ModKind::Macro { value: 0.35 });
            p.tracks[0].modulators[0].routes = vec![ModRoute {
                target: ModTarget::Pan,
                depth: 0.6,
            }];
            p.tracks[0].modulators[1].routes = vec![ModRoute {
                target: ModTarget::Volume,
                depth: -9.0,
            }];
            p.tracks[0]
                .modulators
                .push(pina_engine::dsp::modulate::Modulator {
                    kind: ModKind::random(),
                    routes: vec![],
                });
            let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
            ws.tracks_picked = vec![ids[0], ids[1]];
            ws.mod_values
                .insert(ids[0], [0.6, 0.35, -0.3, 0.0, 0.0, 0.0, 0.0, 0.0]);
            ws.track = 0;
            ws.editor_h = 330.0;
            ws.show_bottom(Bottom::Modulation, cx);
            if page == "stack-shared-menu" {
                ws.menu = Some(crate::ui::menu::Menu {
                    at: gpui::point(px(1180.), px(560.)),
                    items: ws.shared_menu(0).unwrap(),
                    ..Default::default()
                });
            }
        }
        // The console headers: a stereo pan, a binaural one, a tall track, a muted region;
        // and each right-click menu.
        // Automation: the keys' volume rising, the lead's pan swinging, in Read and Latch.
        "automation" => {
            use pina_engine::song::project::{AutoMode, AutoPoint, Param};
            let pt = |tick, value| AutoPoint::new(tick, value);
            let bent = |tick, value, curve| AutoPoint {
                curve,
                ..pt(tick, value)
            };
            let step = |tick, value| AutoPoint {
                step: true,
                ..pt(tick, value)
            };
            let ids: Vec<u64> = ws.session.project.tracks.iter().map(|t| t.id).collect();
            let tracks = &mut ws.session.project.tracks;
            // Keys: a swell in, held, a dip; its pan swinging.
            tracks[0].automation.volume = vec![
                bent(0, -40.0, 0.6),
                pt(BAR * 2, -4.0),
                pt(BAR * 3, -4.0),
                bent(BAR * 3 + BAR / 2, -14.0, -0.5),
                pt(BAR * 4, -6.0),
            ];
            tracks[0].automation.pan = (0..9)
                .map(|i| {
                    bent(
                        i * BAR / 2,
                        if i % 2 == 0 { -0.7 } else { 0.7 },
                        if i % 2 == 0 { -0.3 } else { 0.3 },
                    )
                })
                .collect();
            tracks[0].automation.mode = AutoMode::Read;
            // Lead in Latch; the drums muted in steps, in Touch.
            tracks[1].automation.volume = vec![pt(0, -6.0), pt(BAR * 4, -6.0)];
            tracks[1].automation.mode = AutoMode::Latch;
            tracks[2].automation.mute = vec![
                step(0, 0.0),
                step(BAR + BAR * 3 / 4, 1.0),
                step(BAR * 2, 0.0),
                step(BAR * 3 + BAR * 3 / 4, 1.0),
                step(BAR * 4, 0.0),
            ];
            tracks[2].automation.mode = AutoMode::Touch;
            ws.auto_lanes
                .insert(ids[0], vec![Param::Volume, Param::Pan]);
            ws.auto_picked = Some(crate::arrange::automation::Picked {
                track: ids[0],
                param: Param::Volume,
                points: vec![2, 3],
            });
            ws.show_automation = true;
            let at = ws.session.project.samples(BAR + BAR / 4, ws.player.rate());
            ws.player.locate(at);
            cx.notify();
        }
        // The automation lanes' menus: a mode's, the shapes', a point's.
        "auto-modes" | "auto-shapes" | "auto-point" => {
            use pina_engine::song::project::{AutoPoint, Param};
            let id = ws.session.project.tracks[0].id;
            ws.session.project.tracks[0].automation.volume = vec![
                AutoPoint {
                    curve: 0.5,
                    ..AutoPoint::new(0, -30.0)
                },
                AutoPoint {
                    ease: true,
                    ..AutoPoint::new(BAR * 2, -3.0)
                },
                AutoPoint::new(BAR * 4, -20.0),
            ];
            // A wobbly pan, as recorded, its second half smoothed.
            let a = &mut ws.session.project.tracks[0].automation;
            a.pan = (0..=64)
                .map(|i| {
                    let wob = ((i * 7919) % 13) as f32 / 13.0 - 0.5;
                    let v = i as f32 / 40.0 - 0.6 + wob * 0.5;
                    AutoPoint::new(i * PPQ / 4, v.clamp(-1.0, 1.0))
                })
                .collect();
            a.smooth(Param::Pan, BAR * 2, BAR * 4, PPQ / 2);
            ws.auto_lanes.insert(id, vec![Param::Volume, Param::Pan]);
            ws.show_automation = true;
            let at = point(px(900.), px(240.));
            match page {
                "auto-modes" => ws.mode_menu(0, at, cx),
                "auto-shapes" => ws.auto_lane_menu(0, 0, at, cx),
                _ => ws.point_menu(0, 0, 1, at, cx),
            }
        }
        // The Songs page: as Pina Studio opens with no song ("home"), or with one open.
        "tools" => {
            let drums = &mut ws.session.project.tracks[2];
            drums.height = Some(110.0);
            (drums.regions[0].fade_in, drums.regions[0].fade_out) = (PPQ * 2, PPQ / 2);
            drums.regions[3].fade_out = PPQ * 3;
            ws.tool = Tool::Fade;
            ws.status = Some(Tool::Fade.hint().into());
            ws.tool_menu(point(px(ws.side_w() + 14.), px(96.)), cx);
        }
        // The Channel EQ's window: a mix's bands, the analyzer as if playing.
        "tracks" | "track-menu" | "pan-menu" | "region-menu" | "lane-menu" | "panner" => {
            use pina_engine::song::project::PanMode;
            let tracks = &mut ws.session.project.tracks;
            tracks[0].pan = -0.3;
            tracks[1].mute = page == "tracks";
            tracks[1].height = (page == "tracks").then_some(44.0);
            tracks[1].pan_mode = PanMode::Stereo;
            tracks[1].spread = Some(0.6);
            tracks[2].pan_mode = PanMode::Binaural;
            tracks[2].pan = 0.45;
            tracks[2].height = Some(130.0);
            tracks[2].regions[1].mute = true;
            // Levels, as if playing.
            for (t, now) in [(0, [0.62f32, 0.55]), (1, [0.3, 0.34]), (2, [0.9, 0.85])] {
                let id = ws.session.project.tracks[t].id;
                let mut level = crate::workspace::Level::default();
                (level.now, level.held) = (now, now.map(|l| (l * 1.25).min(1.0)));
                ws.levels.insert(Some(id), level);
            }
            let at = point(px(330.), px(160.));
            match page {
                "track-menu" => ws.track_menu(0, at, cx),
                "pan-menu" => ws.pan_menu(1, at, cx),
                "region-menu" => ws.region_menu((0, 0), point(px(700.), px(160.)), cx),
                "lane-menu" => ws.lane_menu(0, BAR * 6, point(px(900.), px(160.)), cx),
                "panner" => {
                    let t = &mut ws.session.project.tracks[2];
                    (t.pan, t.elevation, t.distance) = (0.3, Some(28.0), Some(1.7));
                    ws.levels.insert(Some(t.id), Default::default());
                    ws.open_panner(2, cx);
                }
                _ => cx.notify(),
            }
        }
        _ => return false,
    }
    true
}
