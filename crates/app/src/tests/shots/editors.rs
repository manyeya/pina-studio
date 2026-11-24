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
        // The audio editor, on the drums' first region, faded in and out and turned up past clipping.
        "audio" => {
            let drums = ws
                .session
                .project
                .tracks
                .iter()
                .position(|t| t.name == "Drums")
                .unwrap_or(0);
            ws.select_track(drums);
            ws.region = Some((drums, 0));
            let r = &mut ws.session.project.tracks[drums].regions[0];
            (r.fade_in, r.fade_out, r.gain_db) = (r.length / 16, r.length / 8, 6.0);
            ws.editor_h = 360.0;
            ws.show_bottom(Bottom::Audio, cx);
        }
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
        "roll-pro" | "roll-chance" | "roll-transform" | "roll-mpe" | "roll-mpe-tab"
        | "roll-press" | "roll-harmony" | "roll-progressions" | "roll-generate" | "roll-shape"
        | "roll-narrow" => {
            ws.track = 0;
            ws.region = Some((0, 0));
            ws.editor_h = 460.0;
            ws.show_bottom(Bottom::PianoRoll, cx);
            ws.picked = vec![0, 1, 2, 3];
            ws.roll.by_velocity = !page.starts_with("roll-mpe") && page != "roll-press";
            ws.roll.in_key = true;
            ws.roll.stamp = crate::editors::pianoroll::Stamp::Triad;
            // Notes with their own expression, as a hand on an MPE controller plays them: a glide
            // into the next, a vibrato, a fall, a bend; each with a slide and a pressure swell.
            if page.starts_with("roll-mpe") || page == "roll-press" {
                use pina_engine::song::project::{Gesture, PPQ};
                ws.editor_h = 560.0;
                let r = &mut ws.session.project.tracks[0].regions[0];
                if let Content::Midi { notes, .. } = &mut r.content {
                    let next: Vec<i64> = notes.iter().map(|n| n.start).collect();
                    let shapes = [
                        Gesture::Glide,
                        Gesture::Vibrato,
                        Gesture::Fall,
                        Gesture::BendUp,
                    ];
                    let mut k = 0;
                    for (i, n) in notes.iter_mut().enumerate() {
                        if n.start < 1920 || k >= shapes.len() {
                            continue;
                        }
                        let to = next
                            .iter()
                            .any(|&s| s >= n.start + n.length as i64)
                            .then_some(-3.0);
                        n.expression.pitch = shapes[k].points(n.length, PPQ, 1.0, 3.0, to);
                        n.expression.slide = Gesture::Open.points(n.length, PPQ, 0.8, 3.0, None);
                        n.expression.press = Gesture::Swell.points(n.length, PPQ, 1.0, 3.0, None);
                        if k == 1 {
                            ws.picked = vec![i];
                        }
                        k += 1;
                    }
                }
                ws.roll.lane = match page {
                    "roll-press" => crate::editors::pianoroll::Lane::Press,
                    "roll-mpe-tab" => crate::editors::pianoroll::Lane::Slide,
                    _ => crate::editors::pianoroll::Lane::Pitch,
                };
                if page == "roll-mpe-tab" {
                    ws.roll.panel = crate::editors::pianoroll::Panel::Mpe;
                }
            }
            // The track headers' column as narrow as it's dragged.
            if page == "roll-narrow" {
                ws.header_w = 280.0;
            }
            if page == "roll-harmony" || page == "roll-progressions" || page == "roll-narrow" {
                use pina_engine::music::harmony;
                let key = ws.session.project.song_key().unwrap();
                ws.roll.panel = crate::editors::pianoroll::Panel::Harmony;
                ws.roll_writing.chords = [0, 5, 3, 4]
                    .map(|k| harmony::in_key(key, false)[k].1)
                    .to_vec();
                ws.roll_writing.performance = harmony::Performance::Arp;
                ws.roll_writing.bass = true;
                ws.roll_writing.progressions = page == "roll-progressions";
            }
            if page == "roll-generate" || page == "roll-shape" {
                ws.roll.panel = crate::editors::pianoroll::Panel::Generate;
                if page == "roll-shape" {
                    ws.roll_writing.writer = crate::editors::pianoroll::Writer::Shape;
                }
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
            ws.steps.folder = vec!["Playbooks".into()];
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
            ws.mod_values.insert(id, {
                let mut v = [0.0; pina_engine::dsp::modulate::MAX_MODS];
                v[..5].copy_from_slice(&[0.6, 0.85, 0.4, 0.7, -0.3]);
                v
            });
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
        // Compose writing the keys and lead over bars 5–8: first run (no model yet), ready,
        // writing (take 2 of 4 coming in), and the takes in, the second picked. Settings ›
        // Composers too. The composers' folder is the shot's own.
        "compose-first" | "compose-ready" | "compose-writing" | "compose-takes" | "composers" => {
            compose_shot(page, ws, cx)
        }
        // A stack of the keys and drums (gathered), its row and its lane; the keys'
        // volume lane moved by an LFO. Folded, its tracks go.
        _ => return false,
    }
    true
}

/// Notes a take might have written: chords on the keys, a line on the lead, over bars 5–8.
fn composed(seed: u64, keys: u64, lead: u64) -> crate::composer::Take {
    use pina_engine::song::project::Note;
    let bar = 4 * pina_engine::song::project::PPQ;
    let chords: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [55, 59, 62], [52, 55, 59]];
    let mut k = Vec::new();
    let mut l = Vec::new();
    let mut r = seed.wrapping_mul(2654435761) | 1;
    let mut next = || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        r
    };
    for b in 4..8u64 {
        let c = chords[(b as usize + seed as usize) % 4];
        let hits: &[u64] = match seed % 3 {
            0 => &[0],
            1 => &[0, 2],
            _ => &[0, 1, 2, 3],
        };
        for &q in hits {
            for &p in &c {
                k.push(Note {
                    start: (b * bar + q * bar / 4) as i64,
                    length: bar / hits.len() as u64 - 40,
                    pitch: p,
                    velocity: 90,
                    ..Default::default()
                });
            }
        }
        let mut at = 0;
        while at < bar {
            let len = [bar / 8, bar / 4, bar / 8, 3 * bar / 16][(next() % 4) as usize];
            if next() % 5 != 0 {
                l.push(Note {
                    start: (b * bar + at) as i64,
                    length: len - 20,
                    pitch: [69, 71, 72, 74, 76, 79][(next() % 6) as usize],
                    velocity: 100,
                    ..Default::default()
                });
            }
            at += len;
        }
    }
    crate::composer::Take {
        seed: seed as i64 * 7919 % 100_000,
        notes: vec![(keys, k), (lead, l)],
    }
}

fn compose_shot(page: &str, ws: &mut Workspace, cx: &mut gpui::Context<Workspace>) {
    let dir = std::env::temp_dir().join(format!("pina-shot-composers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let model = dir.join("midigpt-yellow-small");
    if page != "compose-first" {
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("model.safetensors"), b"").unwrap();
        let info = serde_json::json!({
            "resolution": 12,
            "attributes": { "note_density": 10, "min_polyphony": 10, "max_polyphony": 10,
                "min_note_duration": 6, "max_note_duration": 6 },
            "attribute_track_types": { "note_density": "drum", "min_polyphony": "melodic",
                "max_polyphony": "melodic", "min_note_duration": "melodic",
                "max_note_duration": "melodic" },
        });
        std::fs::write(model.join("info.json"), info.to_string()).unwrap();
    }
    std::fs::create_dir_all(dir.join("hf")).unwrap();
    if page != "compose-first" {
        std::fs::write(dir.join("hf").join("token"), "hf_shot").unwrap();
    }
    unsafe {
        std::env::set_var("PINA_COMPOSERS", &dir);
        std::env::set_var("HF_HOME", dir.join("hf"));
    }
    let bar = 4 * pina_engine::song::project::PPQ;
    let p = &mut ws.session.project;
    p.cycle = Some(pina_engine::song::project::Cycle {
        start: 4 * bar,
        end: 8 * bar,
        off: false,
    });
    let ids: Vec<u64> = p.tracks.iter().map(|t| t.id).collect();
    let (keys, lead) = (ids[0], ids[1]);
    ws.track = 0;
    ws.tracks_picked = vec![keys, lead];
    ws.region = None;
    ws.composer
        .steer
        .insert(lead, [("min_polyphony".to_string(), 3)].into());
    if page == "composers" {
        ws.composer.up = Some("midigpt-yellow-small".into());
        ws.show_settings(crate::pages::settings::Section::Composers, cx);
        return;
    }
    let target = crate::composer::Target {
        tracks: vec![keys, lead],
        bars: (4, 8),
    };
    match page {
        "compose-writing" => {
            let first = composed(1, keys, lead);
            let second = composed(2, keys, lead);
            ws.composer.target = Some(target);
            ws.composer.takes = vec![first];
            ws.composer.job = Some(crate::composer::Job {
                stage: crate::composer::Stage::Writing { take: 2, of: 4 },
                started: std::time::Instant::now() - std::time::Duration::from_secs(7),
                stop: Default::default(),
                run: 0,
                tokens: 412,
                live: second
                    .notes
                    .iter()
                    .flat_map(|(id, n)| n.iter().map(|n| (*id, n.clone())))
                    .filter(|(_, n)| n.start < (5 * bar + bar / 2) as i64)
                    .collect(),
            });
        }
        "compose-takes" => {
            ws.composer.target = Some(target);
            ws.composer.takes = (1..=4).map(|s| composed(s, keys, lead)).collect();
            ws.composer.picked = Some(1);
        }
        _ => {}
    }
    ws.editor_h = 360.0;
    ws.show_bottom(Bottom::Compose, cx);
}
