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
        p if p.starts_with("bloom") => bloom_shot(p, ws, cx),
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
        page if matches!(
            page,
            "mixer"
                | "mixer-drag"
                | "mixer-limit"
                | "mixer-stacked"
                | "mixer-midi-fx"
                | "mixer-mpe"
        ) || page.starts_with("midi-fx-") =>
        {
            use crate::mix::strip::Channel;
            use crate::workspace::{Drag, View};
            use pina_engine::song::project::{Comp, Eq, Format, Limiter, PluginRef, Saturator};
            for t in &mut ws.session.project.tracks {
                (t.eq, t.comp, t.sat) = (
                    Some(Eq::default()),
                    Some(Comp::default()),
                    Some(Saturator::default()),
                );
            }
            ws.session.project.tracks[0].limit = Some(Limiter {
                style: pina_engine::song::project::LimitStyle::Modern,
                gain_db: 6.5,
                target_lufs: Some(-14.0),
                ..Limiter::default()
            });
            let reverb = PluginRef {
                format: Format::Au,
                class: "Dragonfly Room Reverb".into(),
                uid: Some("aufx dfrr Mich".into()),
                ..Default::default()
            };
            ws.session.project.tracks[0]
                .set_insert(0, Some(reverb))
                .unwrap();
            // Three reverbs and two delays more on the first track: every one in sight.
            if page == "mixer-stacked" {
                use pina_engine::song::project::{DelayStyle, ReverbStyle, Unit};
                let t = &mut ws.session.project.tracks[0];
                for _ in 0..3 {
                    t.chain_mut()
                        .add_more(Unit::Reverb(ReverbStyle::Hall.reverb()));
                }
                for _ in 0..2 {
                    t.chain_mut()
                        .add_more(Unit::Delay(DelayStyle::Tape.delay()));
                }
            }
            // MIDI effects on the Keys, their panel open.
            // Every kind on the Keys, the window on one (midi-fx-3: the fourth), or adding.
            if let Some(k) = page.strip_prefix("midi-fx-") {
                use pina_engine::song::project::{MidiFx, MidiFxSlot};
                ws.session.project.tracks[0].midi_fx = MidiFx::KINDS
                    .map(|k| MidiFxSlot {
                        fx: MidiFx::fresh(k).unwrap(),
                        bypass: false,
                        plugin: None,
                    })
                    .to_vec();
                let mut v = crate::mix::midi_fx::MidiFxView::new(
                    0,
                    gpui::point(gpui::px(150.), gpui::px(150.)),
                );
                match k {
                    "add" => v.adding = true,
                    k => v.picked = k.parse().unwrap_or(0),
                }
                ws.midi_fx_view = Some(v);
            }
            if page == "mixer-midi-fx" {
                use pina_engine::song::project::{MidiFx, MidiFxSlot};
                ws.session.project.tracks[0].midi_fx =
                    ["Arpeggiator", "Chord Trigger", "Velocity Processor"]
                        .map(|k| MidiFxSlot {
                            fx: MidiFx::fresh(k).unwrap(),
                            bypass: k == "Velocity Processor",
                            plugin: None,
                        })
                        .to_vec();
                ws.midi_fx_view = Some(crate::mix::midi_fx::MidiFxView::new(
                    0,
                    gpui::point(gpui::px(150.), gpui::px(170.)),
                ));
            }
            // An Audio Unit instrument hearing MPE, its card open; an MPE Shaper before it.
            if page == "mixer-mpe" {
                use pina_engine::song::project::{Instrument, MidiFx, MidiFxSlot, MpeZone};
                let t = &mut ws.session.project.tracks[0];
                t.instrument = Some(Instrument::Plugin(PluginRef {
                    format: Format::Au,
                    class: "Expressive Keys".into(),
                    uid: Some("aumu exky pina".into()),
                    mpe: true,
                    mpe_zone: MpeZone {
                        bend: 24,
                        ..Default::default()
                    },
                    ..Default::default()
                }));
                t.midi_fx = vec![MidiFxSlot {
                    fx: MidiFx::fresh("MPE Shaper").unwrap(),
                    bypass: false,
                    plugin: None,
                }];
                ws.mpe_view = Some(crate::mix::mpe::MpeView {
                    track: 0,
                    at: gpui::point(gpui::px(150.), gpui::px(170.)),
                });
                ws.midi_fx_view = Some(crate::mix::midi_fx::MidiFxView::new(
                    0,
                    gpui::point(gpui::px(560.), gpui::px(170.)),
                ));
            }
            if page == "mixer-limit" {
                let t = &mut ws.session.project.tracks[0];
                (t.eq, t.comp, t.sat) = (None, None, None);
                ws.reductions.insert(Some(t.id), 4.2);
            }
            ws.session.project.assign_ids();
            ws.channel = Channel::Track(0);
            ws.view = View::Mixer;
            if page == "mixer-drag" {
                ws.drag = Some(Drag::Effect {
                    ch: Channel::Track(0),
                    row: 2,
                    to: 0,
                    card: false,
                    from: (180.0, 330.0),
                    at: (196.0, 262.0),
                    moved: true,
                });
            }
            cx.notify();
        }
        // A channel with built-ins stacked: an EQ before everything, a second compressor and a
        // second saturator after the plugin; the mixer panel lists them, and the second
        // compressor's window is open.
        "stacked" | "stacked-eq" => {
            use crate::mix::strip::{Builtin, Channel};
            use crate::workspace::View;
            use pina_engine::song::project::{
                Comp, CompStyle, Eq, Format, Fx, PluginRef, SatStyle, Saturator, Unit,
            };
            let t = &mut ws.session.project.tracks[0];
            (t.eq, t.comp, t.sat) = (
                Some(Eq::default()),
                Some(Comp::default()),
                Some(Saturator::default()),
            );
            let reverb = PluginRef {
                format: Format::Au,
                class: "Dragonfly Room Reverb".into(),
                uid: Some("aufx dfrr Mich".into()),
                ..Default::default()
            };
            t.set_insert(0, Some(reverb)).unwrap();
            let mut heavy = Saturator::default();
            heavy.bands[0].style = SatStyle::Heavy;
            t.more = vec![
                Unit::Eq(Eq::default()),
                Unit::Comp(Comp {
                    style: CompStyle::Punchy,
                    ratio: 8.0,
                    threshold_db: -24.0,
                    attack_ms: 2.0,
                    ..Comp::default()
                }),
                Unit::Sat(heavy),
            ];
            t.chain = vec![
                Fx::More,
                Fx::Eq,
                Fx::Comp,
                Fx::Sat,
                Fx::Plugin,
                Fx::More,
                Fx::More,
                Fx::Limit,
            ];
            ws.session.project.assign_ids();
            ws.channel = Channel::Track(0);
            ws.view = View::Mixer;
            match page {
                "stacked" => ws.open_fx_at(Channel::Track(0), Builtin::Comp, Some(1), cx),
                _ => ws.open_eq_at(Channel::Track(0), Some(0), cx),
            }
            cx.notify();
        }
        // Pina Studio's reverb and delay, on an aux the tracks send to, each in its window.
        "reverb" | "delay" => {
            use crate::mix::strip::{Builtin, Channel};
            use pina_engine::song::project::{DelayStyle, ReverbStyle, Unit};
            let i = ws.session.project.add_aux();
            let _ = i;
            let a = ws.session.project.auxes.len() - 1;
            ws.session.project.auxes[a].name = "Verb".into();
            let mut verb = ReverbStyle::Plate.reverb();
            verb.mix = 1.0;
            verb.predelay_beats = Some(0.0625);
            let mut echo = DelayStyle::Tape.delay();
            (echo.mix, echo.beats) = (1.0, [0.75, 0.5]);
            ws.session.project.auxes[a].more = vec![Unit::Reverb(verb), Unit::Delay(echo)];
            ws.session.project.assign_ids();
            match page {
                "reverb" => ws.open_fx_at(
                    Channel::Aux(a),
                    Builtin::Tool(pina_engine::song::project::Fx::Reverb),
                    Some(0),
                    cx,
                ),
                _ => ws.open_fx_at(
                    Channel::Aux(a),
                    Builtin::Tool(pina_engine::song::project::Fx::Delay),
                    Some(1),
                    cx,
                ),
            }
        }
        // The tools, each stacked on a track with something in its pictures.
        "filter" | "motion" | "shaper" | "gate" | "de-esser" | "stereo" | "multiband" | "pitch"
        | "tape-stop" | "tune" => {
            use crate::mix::strip::{Builtin, Channel};
            use pina_engine::song::project::{
                DeEsser, FilterFx, Gate, MotionStyle, Multiband, PitchShift, Shaper, Stereo,
                TapeStop, Tune, Unit,
            };
            let u = match page {
                "filter" => Unit::Filter(FilterFx {
                    cutoff_hz: 900.0,
                    resonance: 0.55,
                    lfo_depth: 1.5,
                    lfo_hz: 0.5,
                    ..FilterFx::default()
                }),
                "motion" => Unit::Motion(MotionStyle::Chorus.motion()),
                "shaper" => Unit::Shaper(Shaper {
                    attack: 0.6,
                    sustain: -0.4,
                    ..Shaper::default()
                }),
                "gate" => Unit::Gate(Gate {
                    threshold_db: -38.0,
                    ..Gate::default()
                }),
                "de-esser" => Unit::DeEsser(DeEsser {
                    threshold_db: -30.0,
                    ..DeEsser::default()
                }),
                "multiband" => Unit::Multiband(Multiband {
                    threshold_db: [-24.0, -30.0, -36.0],
                    lift: [0.0, 0.4, 0.7],
                    ..Multiband::default()
                }),
                "pitch" => Unit::Pitch(PitchShift {
                    semitones: 7.0,
                    feedback: 0.2,
                    ..PitchShift::default()
                }),
                "tape-stop" => Unit::TapeStop(TapeStop {
                    stop_beats: 1.5,
                    start_beats: 0.5,
                    ..TapeStop::default()
                }),
                "tune" => Unit::Tune(Tune {
                    notes: std::array::from_fn(|n| [0, 2, 4, 5, 7, 9, 11].contains(&n)),
                    ..Tune::default()
                }),
                _ => Unit::Stereo(Stereo {
                    width: [0.4, 1.0, 1.6],
                    mono_hz: 120.0,
                    ..Stereo::default()
                }),
            };
            let fx = u.kind();
            ws.session.project.tracks[0].more = vec![u];
            ws.open_fx_at(Channel::Track(0), Builtin::Tool(fx), Some(0), cx);
            let v = ws.fx_view.as_mut().unwrap();
            // A beat of kicks going past, what it heard and let through.
            for i in 0..240 {
                let t = (i % 40) as f32 / 40.0;
                let level = -6.0 - 60.0 * t;
                let out = if level < -38.0 { -80.0 } else { level };
                v.history.push_back(match page {
                    // A voice gliding between notes, a little flat, breaking for breath.
                    "tune" => {
                        let at = i as f32;
                        let sung = 62.0 + 3.0 * (at * 0.021).sin() + 0.3 * (at * 0.4).sin();
                        match i % 70 < 58 {
                            true => [sung, sung.round(), 0.0, 0.0],
                            false => [0.0; 4],
                        }
                    }
                    _ => [level, out, (level + 30.0).max(0.0) * 0.4, 0.0],
                });
            }
            v.reading = match page {
                "tape-stop" => [0.4, 0.0, 0.0],
                _ => [9.0, 3.5, 6.2],
            };
            for i in 0..1024 {
                let a = i as f32 * 0.09;
                v.scope[0].push(0.5 * a.sin() + 0.2 * (a * 3.1).sin());
                v.scope[1].push(0.5 * a.sin() - 0.2 * (a * 2.3).cos());
            }
            for (db, &f) in v.spectrum.iter_mut().zip(&v.columns) {
                let f = f as f32;
                let s = 10.0 * (-((f / 6500.0).ln() / 0.3).powi(2)).exp();
                *db = (-20.0 - 11.0 * (f / 120.0).max(1.0).log2() + s).max(-90.0);
            }
        }
        // The synth, in its window, a patch on the first track.
        "poly"
        | "poly-matrix"
        | "poly-effects"
        | "poly-effects-empty"
        | "poly-effects-eq"
        | "poly-effects-comp"
        | "poly-effects-drive"
        | "poly-effects-delay"
        | "poly-effects-motion"
        | "poly-lfo"
        | "poly-rates"
        | "poly-add"
        | "poly-save"
        | "poly-pack"
        | "poly-sounds"
        | "poly-tables"
        | "poly-sub" => {
            use crate::editors::poly::PolyPage;
            use pina_engine::song::project::{
                DelayStyle, Instrument, ReverbStyle, Unit, poly_patch,
            };
            let mut p = poly_patch("Evolving Pad").unwrap();
            p.tables[1] = "FM 7:2".into();
            // Its own rack: a drive, an ensemble, a delay and a hall, the hall's mix on its LFO.
            if page != "poly-effects-empty" {
                use pina_engine::song::project::{PolyDest, PolyRoute, PolySource, rack_effect};
                p.fx = ["Tube", "Ensemble", "Tape Delay", "Hall", "EQ", "Compressor"]
                    .map(|n| rack_effect(n).unwrap())
                    .to_vec();
                // PINA_SHOT_RACK: a rack of its own, by the names the rack offers.
                if let Ok(names) = std::env::var("PINA_SHOT_RACK") {
                    p.fx = names.split(',').filter_map(rack_effect).collect();
                }
                // The EQ with a low cut, a dip and an air shelf, as one would set it.
                if let Some(Unit::Eq(e)) = p.fx.get_mut(4) {
                    use pina_engine::song::project::{EqBand, Shape};
                    for (shape, freq, gain_db, q) in [
                        (Shape::LowCut, 90.0, 0.0, 0.7),
                        (Shape::Bell, 380.0, -4.5, 1.4),
                        (Shape::Bell, 2_400.0, 2.5, 0.9),
                        (Shape::HighShelf, 9_000.0, 3.5, 0.7),
                    ] {
                        e.bands.push(EqBand {
                            shape,
                            freq,
                            gain_db,
                            q,
                            ..Default::default()
                        });
                    }
                }
                p.routes.push(PolyRoute {
                    source: PolySource::Lfo(1),
                    dest: PolyDest::Rack(3, 0),
                    depth: 0.3,
                    by: None,
                });
            }
            if page == "poly-rates" {
                p.lfos[1].beats = 0.25 * 2.0 / 3.0;
            }
            let t = &mut ws.session.project.tracks[0];
            t.instrument = Some(Instrument::Poly(Box::new(p)));
            t.more = vec![
                Unit::Reverb(ReverbStyle::Plate.reverb()),
                Unit::Delay(DelayStyle::Tape.delay()),
            ];
            ws.session.project.assign_ids();
            // A library as someone's would be: packs of their own and one brought in.
            if matches!(page, "poly-sounds" | "poly-save" | "poly-pack") {
                use crate::editors::poly::library::{PackInfo, SavedSound};
                let lib = ws.library();
                let base = pina_engine::song::project::poly_patch("Evolving Pad").unwrap();
                let day = crate::editors::poly::library::now();
                let packs = [
                    (
                        "Night Drive",
                        false,
                        "Synthwave at night: glassy arps, round basses and wide pads.",
                        &[
                            (
                                "Neon Arp Glass",
                                "Pluck",
                                &["Glassy", "Bright"][..],
                                "short glassy plucks into a ping-pong delay",
                            ),
                            (
                                "Night Drive Bass",
                                "Bass",
                                &["Analog", "Warm"][..],
                                "round and low, a slow filter",
                            ),
                            (
                                "Sky Lead",
                                "Lead",
                                &["Bright", "Digital"][..],
                                "a lead that cuts, a long delay",
                            ),
                            (
                                "Chrome Pad",
                                "Pad",
                                &["Wide", "Evolving"][..],
                                "a pad that turns slowly",
                            ),
                        ][..],
                    ),
                    (
                        "Glass Keys",
                        false,
                        "Bells, mallets and glass.",
                        &[
                            (
                                "Ice Harp",
                                "Pluck",
                                &["Glassy", "Wide"][..],
                                "a bell table plucked, a long shimmer",
                            ),
                            (
                                "Tape Keys",
                                "Keys",
                                &["Warm", "Lo-fi"][..],
                                "keys through a worn tape",
                            ),
                        ][..],
                    ),
                    (
                        "Neon Vol 2",
                        true,
                        "Bright, wide and digital.",
                        &[
                            (
                                "Prism Pluck",
                                "Pluck",
                                &["Glassy", "Digital"][..],
                                "three detuned glass partials",
                            ),
                            (
                                "Shard",
                                "Pluck",
                                &["Glassy", "Aggressive"][..],
                                "folded sine, metal on the top",
                            ),
                        ][..],
                    ),
                ];
                for (k, (pack, imported, about, sounds)) in packs.iter().enumerate() {
                    let name = lib
                        .make_pack(PackInfo {
                            name: pack.to_string(),
                            by: if *imported {
                                "Neon Labs".into()
                            } else {
                                String::new()
                            },
                            about: about.to_string(),
                            imported: *imported,
                            ..Default::default()
                        })
                        .unwrap();
                    for (i, (n, part, words, line)) in sounds.iter().enumerate() {
                        lib.save(
                            SavedSound {
                                name: n.to_string(),
                                part: part.to_string(),
                                words: words.iter().map(|w| w.to_string()).collect(),
                                about: line.to_string(),
                                by: String::new(),
                                made: day - (k * 3 + i) as u64 * 86_400,
                                poly: base.clone(),
                            },
                            Some(&name),
                        )
                        .unwrap();
                    }
                }
                ws.settings.liked_sounds = vec!["Night Drive/Neon Arp Glass".into(), "Acid".into()];
            }
            ws.open_poly(0, cx);
            if let Some(v) = ws.poly_view.as_mut() {
                match page {
                    "poly-matrix" => v.page = PolyPage::Matrix,
                    "poly-effects" => {
                        v.page = PolyPage::Effects;
                        // PINA_SHOT_OPEN: which of the rack is on the stage.
                        v.fx_open = std::env::var("PINA_SHOT_OPEN")
                            .ok()
                            .and_then(|k| k.parse().ok())
                            .unwrap_or(3);
                    }
                    "poly-effects-empty" => v.page = PolyPage::Effects,
                    "poly-effects-eq"
                    | "poly-effects-comp"
                    | "poly-effects-drive"
                    | "poly-effects-delay"
                    | "poly-effects-motion" => {
                        v.page = PolyPage::Effects;
                        v.fx_open = match page {
                            "poly-effects-eq" => 4,
                            "poly-effects-comp" => 5,
                            "poly-effects-delay" => 2,
                            "poly-effects-motion" => 1,
                            _ => 0,
                        };
                    }
                    "poly-lfo" => v.source = pina_engine::song::project::PolySource::Lfo(1),
                    "poly-add" => {
                        v.page = PolyPage::Effects;
                        v.adding = true;
                        v.hearing = Some("Ensemble");
                    }
                    "poly-rates" => {
                        v.source = pina_engine::song::project::PolySource::Lfo(1);
                        v.rate_open = Some(1);
                    }
                    "poly-sounds" => v.page = PolyPage::Sounds,
                    "poly-pack" => {
                        v.page = PolyPage::Sounds;
                        v.lib.place =
                            crate::editors::poly::sounds::Place::Pack("Night Drive".into());
                    }
                    "poly-tables" => v.tables = Some(0),
                    "poly-sub" => v.picked = 3,
                    _ => {}
                }
                // As if playing: a chord held, the envelope and LFOs where a note has them, and
                // what came out, for the scope and the spectrum.
                use pina_engine::dsp::poly::{LIVE_SOURCES, POLY_LIVE};
                use pina_engine::song::project::PolySource;
                v.held = vec![57, 60, 64];
                for (k, s) in LIVE_SOURCES.iter().enumerate() {
                    v.live[k] = match s {
                        PolySource::Env(0) => 0.8,
                        PolySource::Env(1) => 0.55,
                        PolySource::Lfo(0) => 0.6,
                        PolySource::Lfo(1) => -0.4,
                        PolySource::Velocity => 0.8,
                        _ => 0.0,
                    };
                }
                v.live[POLY_LIVE - 1] = 3.0;
                v.scope = (0..4096)
                    .map(|i| {
                        let x = i as f32 * std::f32::consts::TAU / 182.0;
                        0.42 * x.sin() + 0.18 * (2.0 * x).sin() + 0.08 * (3.0 * x + 0.6).sin()
                    })
                    .collect();
                v.spectrum = (0..96)
                    .map(|k| {
                        let hz = 20.0 * 1000f32.powf(k as f32 / 95.0);
                        let near = [264.0f32, 528.0, 792.0, 1056.0, 1584.0]
                            .iter()
                            .map(|h| (-((hz / h).log2() * 9.0).powi(2)).exp())
                            .sum::<f32>();
                        -78.0 + 52.0 * near.min(1.0) - (hz / 2000.0).log2().max(0.0) * 6.0
                    })
                    .collect();
            }
            // Changed, then Save: its dialog over the synth.
            if page == "poly-save" {
                ws.edit(cx, |ws| ws.poly_mut(0).unwrap().params[0] += 0.1);
                ws.save_sound(0, false, None, cx);
                if let Some(crate::editors::poly::saving::Dialog::Save(f)) =
                    ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                {
                    f.words = vec!["Wide".into(), "Evolving".into(), "Glassy".into()];
                    f.pack = Some("Night Drive".into());
                }
            }
        }
        "comp" | "comp-bus" => {
            use crate::mix::strip::Builtin;
            use pina_engine::song::project::{Comp, CompStyle};
            let c = match page {
                "comp" => Comp {
                    style: CompStyle::Punchy,
                    threshold_db: -20.0,
                    ratio: 4.0,
                    attack_ms: 18.0,
                    release_ms: 120.0,
                    makeup_db: 5.0,
                    spectral: 0.3,
                    ..Comp::default()
                },
                _ => crate::mix::strip::bus_comp(),
            };
            ws.session.project.tracks[0].comp = Some(c);
            ws.open_fx(Channel::Track(0), Builtin::Comp, cx);
            // Kicks on the beat, the hat between: what it heard and did.
            let v = ws.fx_view.as_mut().unwrap();
            let mut gr = 0f32;
            for k in 0..720 {
                // 120 frames a second, 120 bpm: a kick each 60, a hat between.
                let kick = -5.0 - (k % 60) as f32 * 0.55;
                let hat = -19.0 - ((k + 30) % 60) as f32 * 1.4;
                let input = kick.max(hat).max(-30.0 + 2.5 * ((k as f32) * 0.21).sin());
                let want = input + c.makeup_db - pina_engine::dsp::comp::curve(&c, input);
                gr += (want - gr) * if want > gr { 0.55 } else { 0.12 };
                v.history
                    .push_back([input, input - gr + c.makeup_db, gr, 0.0]);
            }
            ws.reductions.insert(ws.channel_id(Channel::Track(0)), 6.2);
        }
        "limit" | "limit-learning" => {
            use crate::mix::strip::{Builtin, Channel};
            use pina_engine::{
                dsp::limit::Limiting,
                song::project::{LimitStyle, Limiter},
            };
            let mut l = Limiter {
                style: LimitStyle::Modern,
                gain_db: 8.5,
                ceiling_db: -1.0,
                release_ms: 125.0,
                clip: 0.2,
                oversample: 4,
                ..Limiter::default()
            };
            // A beat through the limiter itself, a frame (400 samples) at a time: what it heard
            // and did, as the window would have read it.
            let rate = 48_000.0;
            let mut limiting = Limiting::new(rate);
            let mut seed = 0x9e37_79b9_7f4a_7c15u64;
            let mut frames = Vec::new();
            for k in 0..1100 {
                let (mut a, mut b) = (vec![0f32; 400], vec![0f32; 400]);
                for (i, (x, y)) in a.iter_mut().zip(&mut b).enumerate() {
                    let t = (k * 400 + i) as f64 / rate;
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let noise = (seed >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
                    let beat = t % 0.5;
                    let off = (t + 0.25) % 0.5;
                    let kick =
                        (std::f64::consts::TAU * (48.0 + 90.0 * (-beat * 40.0).exp()) * beat).sin()
                            * (-beat * 9.0).exp();
                    let snare = if (t % 1.0) > 0.5 {
                        (-off * 22.0).exp()
                    } else {
                        0.0
                    };
                    let bass = 0.22 * (std::f64::consts::TAU * 55.0 * t).sin();
                    let swell = 0.75 + 0.25 * (t * 0.7).sin();
                    let v = ((kick * 0.55 + bass) * swell) as f32
                        + noise * (0.05 + 0.25 * snare as f32);
                    (*x, *y) = (v, v * 0.92);
                }
                limiting.process(&l, &mut a, &mut b, true);
                let r = limiting.last;
                let db = |x: f32| (20.0 * x.max(1e-6).log10()).max(-90.0);
                let short = limiting.meter.short.max(-90.0);
                frames.push([db(r.input), db(r.output), r.reduction, short]);
            }
            let m = limiting.meter.integrated;
            l.target_lufs = Some(m.round());
            ws.session.project.master_limit = Some(l);
            ws.open_fx(Channel::Master, Builtin::Limit, cx);
            let v = ws.fx_view.as_mut().unwrap();
            for f in &frames[frames.len() - 720..] {
                v.history.push_back(*f);
            }
            let last = frames[frames.len() - 1];
            v.bars = [last[0], 4.6, last[1], last[3]];
            v.meter = pina_engine::dsp::limit::LimitMeter {
                momentary: limiting.meter.momentary,
                short: limiting.meter.short,
                integrated: m,
                true_peak: limiting.meter.true_peak,
                ..Default::default()
            };
            if page == "limit-learning" {
                ws.learning = Some(Channel::Master);
            }
            cx.notify();
        }
        "sat" => {
            use crate::mix::strip::Builtin;
            use pina_engine::song::project::{SatStyle, Saturator};
            let mut s = Saturator::default();
            s.split(180.0);
            s.split(2_400.0);
            s.bands[0].style = SatStyle::WarmTube;
            s.bands[0].drive_db = 9.0;
            s.bands[1].drive_db = 4.5;
            s.bands[1].tone = 0.3;
            s.bands[2].style = SatStyle::Transformer;
            s.bands[2].drive_db = 12.0;
            s.bands[2].mix = 0.6;
            s.quality = 4;
            ws.session.project.tracks[0].sat = Some(s);
            ws.open_fx(Channel::Track(0), Builtin::Sat, cx);
            let v = ws.fx_view.as_mut().unwrap();
            v.band = 1;
            for (db, &f) in v.spectrum.iter_mut().zip(&v.columns) {
                let f = f as f32;
                let tilt = -3.5 * (f / 120.0).log2().max(0.0);
                let body = -24.0 - 14.0 * (60.0 / f).max(1.0).log2();
                let wobble = 3.0 * (f.ln() * 7.0).sin() + 2.0 * (f.ln() * 17.0).cos();
                *db = (body + tilt + wobble).max(-84.0);
            }
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

/// Bloom's window on track 0: a pad seed grown out, branch E picked, chords sounding.
fn bloom_shot(page: &str, ws: &mut Workspace, cx: &mut gpui::Context<Workspace>) {
    use crate::editors::bloom::{BloomPage, grow::Grown};
    use pina_engine::song::project::*;
    let mut b = Bloom::default();
    b.plant(SeedKind::Pad, 0x5eed_2026);
    b.plant(SeedKind::Pluck, 0xb100_0003);
    b.replant(7);
    let lengths = [
        0.62, 0.35, 0.8, 0.18, 0.95, 0.5, 0.28, 0.72, 0.0, 0.44, 0.58, 0.3,
    ];
    for (k, l) in lengths.iter().enumerate() {
        b.params[branch_length(k)] = *l;
        b.params[branch_bend(k)] = ((k as f32 * 1.7).sin() * 0.6).clamp(-1.0, 1.0);
    }
    b.chosen = 4;
    b.sound = "Glasshouse Pluck".into();
    b.locks = 1 << 32 | 1 << 33 | 1 << 46;
    b.edits.push(BranchEdit {
        branch: 4,
        gene: 0,
        tip: 0.05,
    });
    ws.session.project.tracks[0].set_instrument(Instrument::Bloom(Box::new(b.clone())));
    ws.open_bloom(0, cx);
    let Some(v) = ws.bloom_view.as_mut() else {
        return;
    };
    v.shown = std::array::from_fn(|k| b.reach(k, 0.0));
    v.hover = Some(7);
    match page {
        "bloom-strand" => {
            v.page = BloomPage::Strand;
            v.group = GeneGroup::Tone;
            v.gene_hover = Some(23);
        }
        "bloom-steer" => {
            v.page = BloomPage::Strand;
            v.steer = true;
            v.gene_hover = Some(0);
        }
        "bloom-sounds" => v.page = BloomPage::Sounds,
        "bloom-sounds-low" => {
            v.page = BloomPage::Sounds;
            v.sounds.family = Some(SeedFamily::Low);
        }
        "bloom-kinds" => {
            v.picking = true;
            v.kind = SeedKind::Reese;
        }
        "bloom-grow" => {
            v.page = BloomPage::Grow;
            // A recording made by a seed, grown toward a while.
            let mut made = Bloom::default();
            made.plant(SeedKind::Bell, 77);
            let r0 = pina_engine::dsp::bloom::grow::Recording {
                audio: vec![0.0; 36_000],
                note: 64,
                hz: Some(329.6),
                hold: 9000,
                velocity: 0.9,
            };
            let audio = pina_engine::dsp::bloom::grow::render(&made.seed(), &r0);
            let r = pina_engine::dsp::bloom::grow::recording(&audio, 0.0, 1.5);
            v.grow.takes.push(crate::editors::bloom::grow::Take::new(
                "Glass Bell E4.wav".into(),
                audio.clone(),
                1.5,
            ));
            let mut low = Bloom::default();
            low.plant(SeedKind::Bell, 78);
            let r1 = pina_engine::dsp::bloom::grow::Recording {
                audio: vec![0.0; 36_000],
                note: 52,
                hz: Some(164.8),
                hold: 9000,
                velocity: 0.6,
            };
            let audio1 = pina_engine::dsp::bloom::grow::render(&made.seed(), &r1);
            v.grow.takes.push(crate::editors::bloom::grow::Take::new(
                "Glass Bell E3 soft.wav".into(),
                audio1,
                1.5,
            ));
            v.grow.take = 0;
            let rs: Vec<_> = v.grow.takes.iter().map(|t| t.recording.clone()).collect();
            let mut g = pina_engine::dsp::bloom::grow::Grower::new(rs, 11);
            let mut ears: Vec<_> = (0..4)
                .map(|_| pina_engine::dsp::bloom::grow::Ear::new())
                .collect();
            for _ in 0..40 {
                g.step(&mut ears);
            }
            let mut ear = pina_engine::dsp::bloom::grow::Ear::new();
            let best = g.strands[1].finds.len() - 1;
            v.grow.picked_heard = Some(ear.hear(&pina_engine::dsp::bloom::grow::render(
                &g.strands[1].finds[best].genes,
                &r,
            )));
            v.grow.picked = Some((1, best));
            v.grow.picked_each = vec![(64, 0.79), (52, 0.81)];
            v.grow.grown = Some(std::sync::Arc::new(std::sync::Mutex::new(Grown {
                generation: g.generation,
                done: false,
                strands: g
                    .strands
                    .iter()
                    .map(|s| (s.kind, s.finds.clone()))
                    .collect(),
            })));
        }
        _ => {
            // Playing: a chord on three branches, the sap up them.
            v.held = vec![60, 64, 67];
            for k in [0usize, 4, 7] {
                v.glow[k] = 0.85;
                v.live[k] = 0.85;
                v.live[BRANCHES + k] = b.reach(k, 0.0);
            }
            v.wheel = 0.2;
        }
    }
    cx.notify();
}
