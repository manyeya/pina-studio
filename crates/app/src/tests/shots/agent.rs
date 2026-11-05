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
        // The tempo lane, the song slowing into its third bar, and the drums following it.
        "tempo" => {
            use pina_engine::song::project::{Content, Follow};
            let p = &mut ws.session.project;
            p.set_tempo_at(BAR * 2, 92.0);
            p.set_tempo_at(BAR * 3, 104.0);
            let drums = p.tracks.iter().position(|t| t.name == "Drums").unwrap_or(2);
            p.set_follow((drums, 0), Follow::On);
            if let Content::Audio { follow, .. } = &mut p.tracks[drums].regions[0].content {
                *follow = Follow::Beats;
            }
            ws.show_inspector = true;
            ws.tempo_lane = Some(true);
            ws.region = Some((drums, 0));
            ws.track = drums;
            cx.notify();
        }
        // A kit as a stack: its kit track's pattern, a track a pad.
        "kit" => {
            use pina_engine::song::project::{NewPad, Sampler, Zone};
            let pad = |name: &str, note: u8, choke: Option<u8>, steps: &[usize]| NewPad {
                name: name.into(),
                sampler: Sampler {
                    zones: vec![Zone {
                        file: "Audio Files/drums.wav".into(),
                        name: name.into(),
                        note,
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                note,
                choke,
                steps: steps.to_vec(),
            };
            let pads = vec![
                pad("Kick", 36, None, &[0, 7, 8]),
                pad("Snare", 38, None, &[4, 12]),
                pad("Closed Hat", 42, Some(1), &[0, 2, 4, 8, 10, 12]),
                pad("Open Hat", 46, Some(1), &[6, 14]),
                pad("Perc", 63, None, &[3, 11]),
            ];
            let made = ws
                .session
                .project
                .add_kit("Kit in F minor", pads, 3, 0, true);
            ws.select_track(made.kit);
            ws.region = Some((made.kit, 0));
            ws.show_bottom(Bottom::Steps, cx);
            cx.notify();
        }
        // The Drum Machine on a kit of a real library folder's hits, a few pads struck; a
        // sample dragged over a pad, and over the pad's sound to layer it.
        "drums" | "drums-empty" | "drums-drop-pad" | "drums-drop-layer" => {
            ws.set_library(pina_engine::samples::library::Library::load(), cx);
            if page != "drums-empty" {
                let folder = "MusicRadar 90s Hip Hop/90s Hip Hop/Drum_Hits";
                ws.kit_from_folder(folder, None, cx);
                ws.drums.pad = 38;
                // A clap layered on the snare, the clap picked.
                let clap = ws.browser.library.samples.iter().position(|s| {
                    s.folder == folder
                        && s.analysis.category == pina_engine::samples::Category::Clap
                });
                if let Some(index) = clap {
                    use crate::editors::drums::DrumOp;
                    use pina_engine::song::project::Layering;
                    ws.drum_op(DrumOp::Layer { note: 38, index }, cx);
                    let how = Layering::Velocity;
                    ws.drum_op(DrumOp::Layering { note: 38, how }, cx);
                    ws.drum_op(DrumOp::Blend { note: 38, on: true }, cx);
                }
                for (note, v) in [(36u8, 1.0f32), (42, 0.6)] {
                    ws.drums.struck.insert(note, (std::time::Instant::now(), v));
                }
            } else {
                ws.show_bottom(crate::workspace::Bottom::Drums, cx);
            }
            cx.notify();
        }
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
        // An answer that went wrong: what it means, what was said, and asking again.
        // MIDI given to it and written by it: in the conversation, on the shelf.
        "agent-midi" | "agent-midi-shelf" => {
            use crate::agent::chat::{ChatPage, Item};
            use pina_engine::song::{
                midi_file::{MidiFile, Part},
                project::Note,
            };
            let dir = std::env::temp_dir().join(format!("pina-shot-midi-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let write = |name: &str, shift: u8, step: i64| {
                let notes = (0..16)
                    .map(|i| Note {
                        start: i * step,
                        length: step as u64 - 20,
                        pitch: 60 + shift + [0, 3, 7, 10][i as usize % 4],
                        velocity: 100,
                        ..Default::default()
                    })
                    .collect();
                let file = MidiFile {
                    parts: vec![Part {
                        name: name.into(),
                        channel: 0,
                        program: None,
                        notes,
                        controls: Vec::new(),
                    }],
                    tempo: Some(124.0),
                    time_signature: Some([4, 4]),
                    ..MidiFile::default()
                };
                let path = dir.join(format!("{name}.mid"));
                std::fs::write(&path, file.to_bytes()).unwrap();
                path
            };
            let (a, b) = (
                write("House Stabs Am", 9, 240),
                write("Night Chords", 2, 480),
            );
            let made = write("Stabs Like These", 4, 240);
            ws.show_agent = true;
            ws.show_browser = false;
            ws.show_inspector = false;
            ws.attach_midi(&[a.clone(), b.clone()], cx);
            let given = ws.midi_note().unwrap().1;
            ws.chat
                .items
                .push(Item::You("Write me one like these".into()));
            for path in given {
                ws.chat.items.push(Item::Midi { path, given: true });
            }
            ws.chat.items.push(Item::Words(
                "They share off-beat stabs on steps 3, 7, 11 and 15, leaning on the root and \
                 minor third. Here's one in the song's key: it scored 0.91 against them."
                    .into(),
            ));
            ws.attach_midi(std::slice::from_ref(&made), cx);
            ws.chat.attached.clear();
            ws.chat.shelf[2].made = true;
            ws.chat.items.push(Item::Midi {
                path: made,
                given: false,
            });
            ws.chat.hearing = Some((2, std::time::Instant::now(), 30.0));
            ws.attach_midi(&[a], cx);
            if page == "agent-midi-shelf" {
                ws.chat.page = ChatPage::Midi;
            }
            cx.notify();
        }
        // The Studio Agent at work: what it said, its steps folded, and the cards it chose.
        "agent-studio" | "agent-studio-plan" | "agent-history" | "agent-recent" => {
            use crate::agent::chat::{Change, ChatPage, Item, Step};
            use serde_json::json;
            let tool = |id: &str, name: &str, input: serde_json::Value, step: Step| Item::Tool {
                id: id.into(),
                name: format!("mcp__pina__{name}"),
                input,
                step,
            };
            let card = |id: &str, input: serde_json::Value| tool(id, "present", input, Step::Done);
            let edit = |id: &str, op: &str| {
                tool(id, "edit", json!({ "command": { "op": op } }), Step::Done)
            };
            ws.show_agent = true;
            ws.show_browser = false;
            ws.show_inspector = false;
            ws.session.project.genre = Some("progressive house".into());
            ws.panel_w = 400.0;
            let plan = |states: [&str; 4]| {
                json!({ "kind": "plan", "id": "drop", "title": "Building the drop", "steps": [
                    { "text": "Set A minor at 126", "state": states[0] },
                    { "text": "Write the drums and bass", "state": states[1] },
                    { "text": "Stack the lead an octave up", "state": states[2] },
                    { "text": "Check the mix", "state": states[3] },
                ]})
            };
            let done = ["done"; 4];
            let mut items = vec![
                Item::You("Build the drop: it should hit harder than the verse".into()),
                Item::Words(
                    "The verse sits around **-14 LUFS** and only uses the `Keys` and `Lead`, \
                     so there's plenty of room to lift.\n\nHere's how I'll build it:"
                        .into(),
                ),
                card("p", plan(done)),
                edit("1", "set_tempo"),
                edit("2", "add_track"),
                edit("3", "add_midi_region"),
                edit("4", "add_midi_region"),
                tool("5", "critique", json!({ "quick": true }), Step::Done),
                card(
                    "c",
                    json!({ "kind": "chords", "key": "A minor", "title": "The drop's chords", "chords": [
                        { "name": "Am", "numeral": "i" }, { "name": "F", "numeral": "VI" },
                        { "name": "C", "numeral": "III" }, { "name": "G", "numeral": "VII" },
                    ]}),
                ),
                card(
                    "s",
                    json!({ "kind": "sections", "sections": [
                        { "name": "Intro", "bars": 8, "energy": 0.25 },
                        { "name": "Verse", "bars": 16, "energy": 0.5 },
                        { "name": "Build", "bars": 8, "energy": 0.75 },
                        { "name": "Drop", "bars": 16, "energy": 1.0 },
                        { "name": "Outro", "bars": 8, "energy": 0.3 },
                    ]}),
                ),
                card(
                    "m",
                    json!({ "kind": "metrics", "title": "After the mix", "metrics": [
                        { "label": "Loudness", "value": "-9.8 LUFS", "target": "-10", "state": "good" },
                        { "label": "True peak", "value": "-0.9 dB", "target": "-1", "state": "warn" },
                        { "label": "Low end", "value": "Mono", "state": "good" },
                        { "label": "Width", "value": "68%", "state": "good" },
                    ]}),
                ),
                Item::Words(
                    "The drop now lands about **4 dB** above the verse, with the sub in mono \
                     and the kick cutting through.\n\n- The bass ducks under the kick\n- The \
                     lead doubles an octave up from bar 9"
                        .into(),
                ),
                Item::Changes {
                    lines: [
                        ("Tempo 124 to 126", None, None),
                        ("New track: Sub, 2 regions", Some("Keys"), Some(9)),
                        ("Lead: 16 notes added", Some("Lead"), Some(9)),
                        ("Drums: compressor", Some("Drums"), None),
                    ]
                    .map(
                        |(what, track, bar): (&str, Option<&str>, Option<u64>)| Change {
                            what: what.into(),
                            track: track.map(str::to_string),
                            bar,
                        },
                    )
                    .to_vec(),
                },
                Item::End {
                    seconds: 94.0,
                    steps: 5,
                    stopped: false,
                    error: None,
                },
                card(
                    "n",
                    json!({ "kind": "next", "options": [
                        { "label": "Write the breakdown" }, { "label": "Add a riser" },
                        { "label": "Compare with a reference" },
                    ]}),
                ),
            ];
            if page == "agent-studio-plan" {
                items = vec![
                    Item::You("Build the drop: it should hit harder than the verse".into()),
                    card("p", plan(["done", "doing", "todo", "todo"])),
                    edit("1", "set_tempo"),
                    tool(
                        "2",
                        "edit",
                        json!({ "command": { "op": "add_midi_region" } }),
                        Step::Running,
                    ),
                ];
                ws.chat.busy = true;
            }
            if page == "agent-history" || page == "agent-recent" {
                let dir = ws.session.dir.clone();
                let hour = 3_600;
                let now = crate::agent::chat::now();
                for (title, ago, asked) in [
                    ("Build the drop", hour, 6),
                    ("Tighten the low end", 5 * hour, 3),
                    ("Write a breakdown after the second drop", 30 * hour, 4),
                    ("Why does the chorus feel smaller?", 80 * hour, 2),
                    ("Master for streaming", 200 * hour, 1),
                ] {
                    ws.chat.past.push(crate::agent::chat::Past {
                        id: title.to_string(),
                        title: title.to_string(),
                        agent: crate::agent::Kind::ClaudeCode,
                        updated: now - ago,
                        asked,
                        said: String::new(),
                    });
                }
                ws.chat.past_of = dir;
                if page == "agent-history" {
                    ws.chat.page = ChatPage::History;
                }
                items.clear();
            }
            ws.chat.title = "Build the drop".into();
            ws.chat.items = items;
            ws.chat.scroll.scroll_to_bottom();
            cx.notify();
        }
        "agent-error" => {
            use crate::agent::{Kind, chat::Item};
            ws.show_agent = true;
            ws.chat.kind = Kind::Codex;
            ws.chat.items = vec![
                Item::You("Make a modern hip hop song in the style of Drake".into()),
                Item::End {
                    seconds: 12.0,
                    steps: 0,
                    stopped: false,
                    error: Some("workspace routing discovery failed".into()),
                },
            ];
            cx.notify();
        }
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

/// Once the page is drawn, what needs it drawn first: a sample dragged over a Drum Machine pad,
/// or the pad's sound.
pub(super) fn after_drawn(shot: &Shot, ws: &mut Workspace, cx: &mut gpui::Context<Workspace>) {
    // The Song Study sheets scrolled down to what they're about: the song against its
    // reference, a recording's blueprint.
    match shot.page {
        "study-compare" => {
            ws.studies.scroll.scroll_to_bottom();
            cx.notify();
            return;
        }
        "study-like" => {
            ws.studies
                .scroll
                .set_offset(gpui::point(gpui::px(0.), gpui::px(-1080.)));
            cx.notify();
            return;
        }
        _ => {}
    }
    let over = match shot.page {
        "drums-drop-pad" => ws
            .drums
            .cells
            .borrow()
            .1
            .iter()
            .find(|c| c.0 == 42)
            .map(|c| c.1),
        "drums-drop-layer" => ws.drums.strip.get(),
        _ => None,
    };
    let Some(b) = over else {
        return;
    };
    let index = ws
        .browser
        .library
        .samples
        .iter()
        .position(|s| s.analysis.category == pina_engine::samples::Category::HatClosed)
        .unwrap_or(0);
    let c = b.center();
    let at = gpui::point(f32::from(c.x), f32::from(c.y));
    ws.drag = Some(crate::workspace::Drag::Sample {
        index,
        from: at,
        at,
        moved: true,
    });
    cx.notify();
}
