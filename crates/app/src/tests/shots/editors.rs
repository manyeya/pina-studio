//! The editors: piano roll, sampler, step sequencer and modulation.

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
        "roll" | "roll-more" => {
            ws.track = 0;
            ws.region = Some((0, 0));
            ws.editor_h = 460.0;
            ws.show_bottom(Bottom::PianoRoll, cx);
            if page == "roll-more" {
                ws.roll.by_velocity = true;
                let at = gpui::point(gpui::px(1310.), gpui::px(452.));
                ws.menu = Some(crate::ui::menu::Menu {
                    at,
                    items: ws.roll_more(at),
                    ..Default::default()
                });
            }
        }
        "roll-pro" | "roll-chance" | "roll-transform" | "roll-mpe" | "roll-harmony"
        | "roll-generate" => {
            ws.track = 0;
            ws.region = Some((0, 0));
            ws.editor_h = 460.0;
            ws.show_bottom(Bottom::PianoRoll, cx);
            ws.picked = vec![0, 1, 2, 3];
            ws.roll.by_velocity = true;
            ws.roll.in_key = true;
            ws.roll.stamp = crate::editors::pianoroll::Stamp::Triad;
            // A note bending up two semitones and back, as a hand on an MPE controller does.
            if page == "roll-mpe" {
                use pina_engine::song::project::{ControlPoint, ControlValue, ExpressionKind};
                ws.picked.clear();
                ws.roll.lane = crate::editors::pianoroll::Lane::Pitch;
                let r = &mut ws.session.project.tracks[0].regions[0];
                if let Content::Midi { notes, controls } = &mut r.content {
                    for n in notes.iter().filter(|n| n.start >= 1920).take(3) {
                        for k in 0..16 {
                            let t = n.start + (n.length as i64 * k / 16);
                            let st = (k as f32 / 15.0 * std::f32::consts::PI).sin() * 2.0;
                            controls.push(ControlPoint {
                                start: t,
                                kind: ControlValue::Expression {
                                    pitch: n.pitch,
                                    kind: ExpressionKind::Tuning,
                                    value: st,
                                },
                            });
                        }
                    }
                    controls.sort_by_key(|c| c.start);
                }
            }
            if page == "roll-harmony" {
                use pina_engine::music::harmony;
                let key = ws.session.project.song_key().unwrap();
                ws.roll.panel = crate::editors::pianoroll::Panel::Harmony;
                ws.roll_writing.chords = [0, 5, 3, 4]
                    .map(|k| harmony::in_key(key, false)[k].1)
                    .to_vec();
                ws.roll_writing.performance = harmony::Performance::Arp;
                ws.roll_writing.bass = true;
            }
            if page == "roll-generate" {
                ws.roll.panel = crate::editors::pianoroll::Panel::Generate;
            }
            match page {
                "roll-chance" => ws.roll.lane = crate::editors::pianoroll::Lane::Chance,
                "roll-transform" => ws.roll.panel = crate::editors::pianoroll::Panel::Transform,
                _ => {}
            }
            if let Some(notes) = ws.session.project.notes_mut((0, 0)) {
                for (i, n) in notes.iter_mut().enumerate() {
                    n.velocity = (40 + i * 6).min(127) as u8;
                    // A few that only sometimes play, one muted, some with a velocity range.
                    n.chance = [100, 100, 100, 100, 60, 100, 35, 100][i % 8];
                    n.muted = i == 9;
                    n.spread = if i % 3 == 0 { 14 } else { 0 };
                }
            }
        }
        "sampler" | "sampler-grains" | "sampler-matrix" | "sampler-vintage" => {
            use pina_engine::dsp::sampler::Playing;
            use pina_engine::song::project::{
                FilterMode, Instrument, LoopMode, ModDest, ModSource, Route, Routes,
                SampleCharacter, SampleMode, Sampler, SamplerVoice, Track,
            };
            let file =
                PathBuf::from(std::env::var("PINA_SHOT_SAMPLE").expect("PINA_SHOT_SAMPLE: a loop"));
            let (sides, rate) = pina_engine::audio::engine::read_audio(&file).unwrap();
            let found = pina_engine::dsp::sampler::onsets(&sides[0], rate as f32);
            let cut = pina_engine::dsp::sampler::cuts(&found, 8);
            let mut zones = pina_engine::dsp::sampler::slices(&file, &cut, 36);
            let mut voice = SamplerVoice {
                filter: FilterMode::LowPass,
                cutoff_hz: 6000.0,
                resonance: 0.3,
                filter_env: 2.0,
                attack_ms: 3.0,
                decay_ms: 250.0,
                sustain: 0.7,
                release_ms: 180.0,
                ..Default::default()
            };
            // Playing: a voice in the picked slice, another in the next.
            let mut playing = vec![
                Playing {
                    zone: 2,
                    at: zones[2].start + 0.02,
                    level: 0.9,
                    grain: false,
                },
                Playing {
                    zone: 3,
                    at: zones[3].start + 0.05,
                    level: 0.6,
                    grain: false,
                },
            ];
            match page {
                "sampler-grains" => {
                    // One sound, frozen in a cloud: the whole loop, grains about its middle.
                    zones = vec![pina_engine::song::project::Zone {
                        file: file.clone(),
                        name: "Break".into(),
                        note: 24,
                        high: Some(96),
                        root: Some(60),
                        gate: true,
                        loop_mode: LoopMode::Forward,
                        loop_start: Some(0.3),
                        loop_end: Some(0.8),
                        loop_fade: 0.25,
                        ..Default::default()
                    }];
                    voice.mode = SampleMode::Grains;
                    voice.position = 0.46;
                    voice.spray = 0.25;
                    voice.density = 48.0;
                    voice.grain_ms = 140.0;
                    voice.freeze = true;
                    voice.filter = FilterMode::Ladder;
                    voice.cutoff_hz = 2400.0;
                    voice.resonance = 0.55;
                    playing = (0..26)
                        .map(|k| Playing {
                            zone: 0,
                            at: 0.46 + ((k * 37 % 50) as f32 / 50.0 - 0.5) * 0.12,
                            level: 0.3 + (k % 5) as f32 * 0.15,
                            grain: true,
                        })
                        .chain([Playing {
                            zone: 0,
                            at: 0.46,
                            level: 1.0,
                            grain: false,
                        }])
                        .collect();
                }
                "sampler-matrix" => {
                    voice.matrix = Routes::from_slice(&[
                        Route {
                            source: ModSource::Velocity,
                            dest: ModDest::Cutoff,
                            amount: 0.45,
                        },
                        Route {
                            source: ModSource::Lfo2,
                            dest: ModDest::Pan,
                            amount: -0.6,
                        },
                        Route {
                            source: ModSource::Random,
                            dest: ModDest::Pitch,
                            amount: 0.04,
                        },
                    ]);
                }
                "sampler-vintage" => {
                    voice.character = SampleCharacter::Vintage;
                    voice.mode = SampleMode::Stretch;
                    voice.filter = FilterMode::Formant;
                    voice.cutoff_hz = 400.0;
                    voice.sub = 0.4;
                    voice.pitch_env = 12.0;
                }
                _ => {}
            }
            ws.session.project.tracks.push(Track {
                name: "Break".into(),
                instrument: Some(Instrument::Sampler(Sampler { zones, voice })),
                ..Default::default()
            });
            ws.session.project.assign_ids();
            ws.select_track(ws.session.project.tracks.len() - 1);
            ws.sampler_ui.zone = if page == "sampler-grains" { 0 } else { 2 };
            ws.sampler_ui.pretend = playing;
            ws.sampler_ui.tab = match page {
                "sampler-matrix" => crate::editors::sampler::ModTab::Matrix,
                "sampler-vintage" => crate::editors::sampler::ModTab::Layers,
                "sampler-grains" => crate::editors::sampler::ModTab::Env,
                _ => crate::editors::sampler::ModTab::Lfo1,
            };
            ws.show_bottom(Bottom::Sampler, cx);
        }
        "steps-empty" => {
            ws.select_track(1);
            ws.region = Some((1, 0));
            ws.editor_h = 380.0;
            ws.show_bottom(Bottom::Steps, cx);
        }
        "steps" | "steps-velocity" | "steps-sub" | "steps-inspector" | "steps-browser"
        | "steps-loop" => {
            use crate::editors::steps::{EditMode, InspectorTab, Pane};
            use pina_engine::song::project::{Lane, PatternRow, RowTarget};
            ws.session
                .project
                .tracks
                .push(pina_engine::song::project::Track {
                    name: "Kit".into(),
                    instrument: Some(pina_engine::song::project::Instrument::Drums),
                    ..Default::default()
                });
            ws.session.project.assign_ids();
            ws.track = ws.session.project.tracks.len() - 1;
            ws.editor_h = 520.0;
            ws.new_pattern(cx);
            // A groove to look at: kick, snare, hats with accents, an open hat, a clap on its
            // own three-step loop; and a cutoff row.
            let at = ws.pattern().unwrap();
            if let Content::Pattern(p) = &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                p.set_length(32);
                let r = &mut p.rows;
                r[0].cells = [120, 0, 0, 0, 0, 0, 90, 0, 110, 0, 0, 0, 0, 0, 70, 0].repeat(2);
                r[1].cells = [0, 0, 0, 0, 127, 0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 40].repeat(2);
                r[2].cells = (0..32).map(|k| if k % 2 == 0 { 100 } else { 55 }).collect();
                r[3].cells = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0].repeat(2);
                r[4].cells = [90, 0, 0].repeat(11)[..32].to_vec();
                r[4].loop_end = Some(3);
                r[0].repeat = [1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 3, 1].repeat(2);
                r[0].tie = vec![false; 32];
                r[1].chance = [
                    100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 50,
                ]
                .repeat(2);
                r[2].gate = (0..32).map(|k| if k % 4 == 0 { 100 } else { 45 }).collect();
                r[2].open = page == "steps-sub";
                r[2].subrows = vec![Lane::Velocity, Lane::Repeat, Lane::Tie, Lane::Gate];
                r[2].repeat = (0..32).map(|k| if k % 8 == 6 { 3 } else { 1 }).collect();
                let mut cutoff = PatternRow::automation(RowTarget::Cc(74), 32, 0.5);
                cutoff.cells = vec![100; 32];
                cutoff.value = (0..32)
                    .map(|k| 0.2 + 0.6 * ((k % 8) as f32 / 7.0))
                    .collect();
                p.rows.push(cutoff);
                p.tidy();
            }
            ws.steps.row = 2;
            ws.steps.mode = match page {
                "steps-velocity" | "steps-sub" | "steps-inspector" => {
                    EditMode::Lane(Lane::Velocity)
                }
                "steps-loop" => EditMode::Loop,
                _ => EditMode::OnOff,
            };
            ws.steps.pane = match page {
                "steps-inspector" => Some(Pane::Inspector),
                "steps-browser" => Some(Pane::Browser),
                _ => None,
            };
            ws.steps.tab = InspectorTab::Row;
            ws.steps.folder = vec!["Drums".into()];
        }
        // The tracks with the sample browser beside them and the piano roll under them,
        // no inspector: the columns' edges, to line up.
        "modulation" => {
            use pina_engine::dsp::modulate::{
                LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator,
            };
            let drums = ws.session.project.tracks[2].id;
            let route = |target, depth| ModRoute { target, depth };
            let t = &mut ws.session.project.tracks[0];
            t.eq = Some(Default::default());
            t.modulators = vec![
                Modulator {
                    kind: ModKind::Lfo {
                        shape: LfoShape::Sine,
                        rate: ModRate::Beats(0.5),
                        phase: 0.0,
                        unipolar: false,
                    },
                    routes: vec![route(ModTarget::Pan, 0.7), route(ModTarget::EqGain(2), 6.0)],
                },
                Modulator {
                    kind: ModKind::steps(),
                    routes: vec![route(ModTarget::LowCut, 2.5)],
                },
                Modulator {
                    kind: ModKind::envelope(),
                    routes: vec![],
                },
                Modulator {
                    kind: ModKind::follower(drums),
                    routes: vec![route(ModTarget::Volume, -18.0)],
                },
                Modulator {
                    kind: ModKind::Random {
                        rate: ModRate::Beats(0.25),
                        smooth: true,
                    },
                    routes: vec![],
                },
            ];
            let id = t.id;
            ws.mod_values
                .insert(id, [0.6, 0.85, 0.4, 0.7, -0.3, 0.0, 0.0, 0.0]);
            for k in 0..5 {
                let trace = (0..96)
                    .map(|i| match k {
                        3 => (1.0 - (i % 24) as f32 / 24.0).powf(2.0) * 0.9,
                        _ => ((i as f32) * 0.21).sin() * 0.7,
                    })
                    .collect();
                ws.mod_trace.insert((id, k), trace);
            }
            ws.track = 0;
            ws.mod_edit = Some((0, 0));
            ws.editor_h = 330.0;
            ws.show_bottom(Bottom::Modulation, cx);
        }
        // A stack of the keys and drums (gathered), its row and its lane; the keys'
        // volume lane moved by an LFO. Folded, its tracks go.
        _ => return false,
    }
    true
}
