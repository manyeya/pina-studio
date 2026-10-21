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
        "roll" => {
            ws.track = 0;
            ws.region = Some((0, 0));
            ws.editor_h = 460.0;
            ws.show_bottom(Bottom::PianoRoll, cx);
        }
        "roll-pro" => {
            ws.track = 0;
            ws.region = Some((0, 0));
            ws.editor_h = 460.0;
            ws.show_bottom(Bottom::PianoRoll, cx);
            ws.picked = vec![0, 1, 2, 3];
            ws.roll.by_velocity = true;
            ws.roll.in_key = true;
            ws.roll.stamp = crate::editors::pianoroll::Stamp::Triad;
            if let Some(notes) = ws.session.project.notes_mut((0, 0)) {
                for (i, n) in notes.iter_mut().enumerate() {
                    n.velocity = (40 + i * 6).min(127) as u8;
                }
            }
        }
        "sampler" => {
            use pina_engine::song::project::{
                FilterMode, Instrument, Sampler, SamplerVoice, Track,
            };
            let file =
                PathBuf::from(std::env::var("PINA_SHOT_SAMPLE").expect("PINA_SHOT_SAMPLE: a loop"));
            let (sides, rate) = pina_engine::audio::engine::read_audio(&file).unwrap();
            let found = pina_engine::dsp::sampler::onsets(&sides[0], rate as f32);
            let cut = pina_engine::dsp::sampler::cuts(&found, 8);
            let zones = pina_engine::dsp::sampler::slices(&file, &cut, 36);
            let voice = SamplerVoice {
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
            ws.session.project.tracks.push(Track {
                name: "Break".into(),
                instrument: Some(Instrument::Sampler(Sampler { zones, voice })),
                ..Default::default()
            });
            ws.session.project.assign_ids();
            ws.select_track(ws.session.project.tracks.len() - 1);
            ws.sampler_ui.zone = 2;
            ws.show_bottom(Bottom::Sampler, cx);
        }
        "steps-empty" => {
            ws.select_track(1);
            ws.region = Some((1, 0));
            ws.editor_h = 380.0;
            ws.show_bottom(Bottom::Steps, cx);
        }
        "steps" | "steps-mod" => {
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
            ws.editor_h = 380.0;
            ws.new_pattern(cx);
            // A groove to look at: kick, snare, hats with accents, a three-step perc.
            let at = ws.pattern().unwrap();
            if let Content::Pattern { rows, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                rows[0].cells = vec![120, 0, 0, 0, 0, 0, 90, 0, 110, 0, 0, 0, 0, 0, 70, 0];
                rows[1].cells = vec![0, 0, 0, 0, 127, 0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 40];
                rows[2].cells = (0..16).map(|k| if k % 2 == 0 { 100 } else { 55 }).collect();
                rows[3].cells = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 100, 0];
                rows[4].cells = vec![90, 0, 0];
                rows[4].mute = false;
            }
            ws.steps_row = 2;
            if page == "steps-mod" {
                use pina_engine::dsp::modulate::{
                    LfoShape, ModKind, ModRate, ModRoute, ModTarget, Modulator, SoundParam,
                };
                if let Content::Pattern { rows, .. } =
                    &mut ws.session.project.tracks[at.0].regions[at.1].content
                {
                    rows[0].repeat = vec![1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 3, 1];
                    rows[1].chance = vec![
                        100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
                        50,
                    ];
                    rows[2].transpose = vec![0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 7, 0];
                }
                let t = &mut ws.session.project.tracks[at.0];
                t.modulators = vec![
                    Modulator {
                        kind: ModKind::Lfo {
                            shape: LfoShape::Sine,
                            rate: ModRate::Beats(2.0),
                            phase: 0.0,
                            unipolar: false,
                        },
                        routes: vec![ModRoute {
                            target: ModTarget::Sound(2, SoundParam::Velocity),
                            depth: -40.0,
                        }],
                    },
                    Modulator {
                        kind: ModKind::Steps {
                            values: vec![
                                1.0, 0.2, 0.6, 0.2, 0.9, 0.2, 0.6, 0.35, 1.0, 0.2, 0.6, 0.2, 0.9,
                                0.4, 0.7, 0.3,
                            ],
                            rate: ModRate::Beats(0.25),
                            glide: 0.0,
                        },
                        routes: vec![ModRoute {
                            target: ModTarget::Volume,
                            depth: -12.0,
                        }],
                    },
                ];
                ws.editor_h = 470.0;
            }
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
