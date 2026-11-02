//! The Browser's play buttons, and MIDI in it: Play in Key and the song's fit doing what they say,
//! the map and the kit. All by the clicks a person would make.

use super::*;
use crate::sounds::browser::{KeyFilter, Results};
use pina_engine::{
    music::Key,
    samples::{
        library::Library,
        midi::{self, MidiSample},
    },
    song::project::{Content, Instrument, Note, PPQ, Project, Region, Track},
};

const BAR: u64 = PPQ * 4;

/// The roots of a round of chords on a minor key's tonic.
const ROUND: [u8; 4] = [0, 8, 3, 10];

/// Notes of a part: chords, one note a bar, a line, or a beat, on the tonic `root`.
fn notes(part: &str, root: u8, major: bool) -> Vec<(u64, u64, u8)> {
    let round = |i: usize| match major {
        true => [0, 5, 7, 9][i],
        false => ROUND[i] as i32,
    };
    let at = |i: usize, up: i32| (root as i32 + round(i) + up) as u8;
    match part {
        "Chords" => (0..4)
            .flat_map(|i| {
                let third = if major { 4 } else { 3 };
                [0, third, 7].map(|up| (i as u64 * BAR, BAR, at(i, 48 + up)))
            })
            .collect(),
        "Bass" => (0..4).map(|i| (i as u64 * BAR, BAR, at(i, 24))).collect(),
        "Lead" => {
            let scale = if major { [0, 2, 4, 7] } else { [0, 3, 5, 7] };
            let line = (0..8).map(|i| (i as u64 * PPQ, PPQ, root + 60 + scale[i % 4]));
            line.collect()
        }
        _ => (0..2)
            .flat_map(|b| [(b * BAR, PPQ, 36), (b * BAR + PPQ, PPQ, 38)])
            .collect(),
    }
}

/// A file in a pack, as the library would hold it: `part` on `root`, at `tempo`.
fn file(
    root_dir: &Path,
    pack: &str,
    name: &str,
    part: &str,
    (root, major): (u8, bool),
    tempo: f64,
) -> MidiSample {
    let mut p = Project::new(48_000);
    p.tempo = tempo;
    let notes = notes(part, root, major);
    let length = notes.iter().map(|n| n.0 + n.1).max().unwrap().div_ceil(BAR) * BAR;
    p.tracks = vec![Track {
        name: part.into(),
        instrument: Some(match part {
            "Beat" => Instrument::Kit,
            _ => Instrument::Synth,
        }),
        regions: vec![Region {
            length,
            content: Content::Midi {
                notes: notes
                    .into_iter()
                    .map(|(start, length, pitch)| Note {
                        start: start as i64,
                        length,
                        pitch,
                        velocity: 100,
                    })
                    .collect(),
                controls: Vec::new(),
            },
            ..Default::default()
        }],
        ..Default::default()
    }];
    let dir = root_dir.join(pack);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.mid"));
    std::fs::write(&path, pina_engine::audio::bounce::midi(&p, None)).unwrap();
    midi::study_file(&path, root_dir, 0, 0).unwrap()
}

/// In E minor at 120, a library of chords in keys that do and don't fit, and a beat.
fn open_midi<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, PathBuf) {
    let dir = demo_copy(name);
    let root = dir.join("Library/MIDI");
    let mut library: Library = little_library(&dir.join("Library"));
    library.midi = vec![
        file(
            &root,
            "Night",
            "Night Chords - Am",
            "Chords",
            (57, false),
            120.0,
        ),
        file(
            &root,
            "Night",
            "Night Bass - Am",
            "Bass",
            (57, false),
            120.0,
        ),
        file(
            &root,
            "Night",
            "Night Lead - Am",
            "Lead",
            (57, false),
            120.0,
        ),
        file(&root, "Night", "Night Beat", "Beat", (0, false), 120.0),
        file(
            &root,
            "Home",
            "Home Chords - Em",
            "Chords",
            (52, false),
            120.0,
        ),
        file(
            &root,
            "Bright",
            "Bright Chords - G",
            "Chords",
            (55, true),
            120.0,
        ),
        file(&root, "Day", "Day Chords - D", "Chords", (50, true), 120.0),
        file(
            &root,
            "Slow",
            "Slow Chords - Em",
            "Chords",
            (52, false),
            88.0,
        ),
    ];
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| {
        ws.session.project.key = Some("E minor".into());
        ws.session.project.tempo = 120.0;
        ws.set_library(library, cx);
    });
    cx.run_until_parked();
    (ws, cx, dir)
}

fn found(ws: &Entity<Workspace>, cx: &VisualTestContext) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        let b = &ws.browser;
        let mut names: Vec<String> = b
            .midi_hits
            .iter()
            .map(|h| b.library.midi[h.index].name.clone())
            .collect();
        names.sort();
        names
    })
}

fn index_of(ws: &Entity<Workspace>, cx: &VisualTestContext, name: &str) -> usize {
    ws.read_with(cx, |ws, _| {
        let of = |m: &MidiSample| m.name == name;
        ws.browser.library.midi.iter().position(of).unwrap()
    })
}

fn click(cx: &mut VisualTestContext, name: &'static str) {
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    cx.run_until_parked();
}

fn show_midi_panel(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("f");
    click(cx, "scope-Sounds");
    cx.simulate_keystrokes("escape");
    click(cx, "kind-midi");
}

#[gpui::test]
fn every_sound_has_a_play_button_that_plays_and_stops_it(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_browser(cx, "browser-play");
    ws.update(cx, |ws, cx| {
        ws.toggle_samples(cx);
        ws.browse(crate::sounds::catalog::Scope::Sounds, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    // Any, one-shots and loops: the same rows.
    for kind in ["kind-None", "kind-Some(OneShot)", "kind-Some(Loop)"] {
        click(cx, kind);
        assert!(cx.debug_bounds("panel-play-0").is_some(), "{kind}");
    }
    click(cx, "kind-None");
    // Pressing it hears the sound and doesn't pick it up to drag.
    let button = cx.debug_bounds("panel-play-0").unwrap().center();
    cx.simulate_mouse_down(button, MouseButton::Left, Modifiers::none());
    assert!(ws.read_with(cx, |ws, _| ws.drag.is_none()));
    cx.simulate_mouse_up(button, MouseButton::Left, Modifiers::none());
    settle(cx, &ws, |ws| ws.browser.audition.is_some());
    assert!(ws.read_with(cx, |ws, _| ws.browser.selected.is_some()));
    // Pressed again, it stops.
    click(cx, "panel-play-0");
    assert!(ws.read_with(cx, |ws, _| ws.browser.audition.is_none()));
    // The page's rows have the same.
    ws.update(cx, |ws, cx| ws.toggle_samples_page(cx));
    cx.run_until_parked();
    click(cx, "sample-play-0");
    settle(cx, &ws, |ws| ws.browser.audition.is_some());
    click(cx, "sample-play-0");
    assert!(ws.read_with(cx, |ws, _| ws.browser.audition.is_none()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn play_in_key_moves_midi_as_soon_as_its_switched_on(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-in-key");
    show_midi_panel(cx);
    assert_eq!(found(&ws, cx).len(), 8);
    let night = index_of(&ws, cx, "Night Chords - Am");
    let up = midi::transpose_to(
        &Key::parse("A minor").unwrap(),
        &Key::parse("E minor").unwrap(),
    );
    assert_ne!(up, 0);
    assert_eq!(ws.read_with(cx, |ws, _| ws.midi_transpose(night)), 0);

    // Hearing it as it is, then switched on while it plays: it's heard moved, with no other click.
    let pos = ws.read_with(cx, |ws, _| {
        let hits = &ws.browser.midi_hits;
        hits.iter().position(|h| h.index == night).unwrap()
    });
    ws.update(cx, |ws, cx| ws.select_midi(night, true, cx));
    settle(cx, &ws, |ws| ws.browser.midi_playing.is_some());
    assert!(pos < 8);
    click(cx, "play-in-key");
    assert_eq!(ws.read_with(cx, |ws, _| ws.midi_transpose(night)), up);
    let moved = |ws: &Workspace| {
        let path = &ws.browser.library.midi[night].path;
        let want = (up as f32).to_bits();
        ws.browser.heard.keys().any(|k| &k.0 == path && k.2 == want)
    };
    settle(cx, &ws, moved);

    // Nothing was lost by asking for a key: the beat has none, and it's still there.
    assert_eq!(found(&ws, cx).len(), 8);
    // What's already in the key leads.
    let first = ws.read_with(cx, |ws, _| {
        let b = &ws.browser;
        b.library.midi[b.midi_hits[0].index].name.clone()
    });
    assert!(first.ends_with("Em") || first.ends_with("- G"), "{first}");

    // Drums aren't moved, and off again nothing is.
    let beat = index_of(&ws, cx, "Night Beat");
    assert_eq!(ws.read_with(cx, |ws, _| ws.midi_transpose(beat)), 0);
    click(cx, "play-in-key");
    assert_eq!(ws.read_with(cx, |ws, _| ws.midi_transpose(night)), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn play_in_key_says_so_when_the_song_has_no_key(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-no-key");
    ws.update(cx, |ws, _| {
        ws.session.project.key = None;
        ws.session.project.tracks.clear();
    });
    show_midi_panel(cx);
    click(cx, "play-in-key");
    let said = ws.read_with(cx, |ws, _| ws.status.clone().unwrap_or_default());
    assert!(said.contains("no key"), "{said}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn fits_keeps_midi_to_the_songs_key_and_tempo(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-fits");
    show_midi_panel(cx);
    assert_eq!(found(&ws, cx).len(), 8);

    // As they are: what's in E minor's notes at about 120, and drums. Not the slow one, not
    // those in other keys.
    click(cx, "fits-song");
    assert_eq!(
        found(&ws, cx),
        ["Bright Chords - G", "Home Chords - Em", "Night Beat"]
    );
    // The part chips count what's left.
    let chords = ws.read_with(cx, |ws, _| ws.browser.midi_counts.get("chords").copied());
    assert_eq!(chords, Some(2));

    // Moved into the key, the other minor files fit too; the major one wouldn't keep its mood.
    click(cx, "play-in-key");
    let names = found(&ws, cx);
    assert!(
        names.contains(&"Night Chords - Am".to_string()),
        "{names:?}"
    );
    assert!(!names.contains(&"Day Chords - D".to_string()), "{names:?}");
    assert!(
        !names.contains(&"Slow Chords - Em".to_string()),
        "{names:?}"
    );

    // Off, they're all back.
    click(cx, "fits-song");
    assert_eq!(found(&ws, cx).len(), 8);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn the_midi_map_hears_picks_and_boxes_in(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-map");
    show_midi_panel(cx);
    ws.update(cx, |ws, cx| ws.toggle_samples_page(cx));
    cx.run_until_parked();
    click(cx, "results-Map");
    settle(cx, &ws, |ws| ws.browser.midi_map.len() == 8);
    assert!(cx.debug_bounds("sample-map").is_some());
    cx.run_until_parked();

    // A dot: it's picked, and heard.
    let (dot, at) = ws.read_with(cx, |ws, _| {
        let b = ws.browser.map_bounds.get();
        let p = ws.map_point(b, ws.browser.midi_map[3]);
        (ws.map_dot_at(p).unwrap(), point(px(p.x), px(p.y)))
    });
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.browser.midi_selected),
        Some(dot)
    );
    settle(cx, &ws, |ws| ws.browser.midi_playing.is_some());

    // A box round part of it: the results keep to those in it, and the chip lets them go.
    let map = cx.debug_bounds("sample-map").unwrap();
    let corner = |x: f32, y: f32| map.origin + point(map.size.width * x, map.size.height * y);
    drag(cx, corner(0.005, 0.005), corner(0.55, 0.995));
    let (boxed, all) = ws.read_with(cx, |ws, _| {
        (ws.browser.area.clone(), ws.browser.library.midi.len())
    });
    let boxed = boxed.expect("a box");
    assert_eq!(found(&ws, cx).len(), boxed.len());
    assert!(boxed.len() < all);
    click(cx, "clear-area");
    assert_eq!(found(&ws, cx).len(), 8);

    // Back to samples, the box and map are theirs again.
    click(cx, "kind-None");
    assert!(ws.read_with(cx, |ws, _| !ws.browser.midi));
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.view), Results::Map);
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_midi_kit_is_heard_changed_and_added_to_the_song(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-kit");
    show_midi_panel(cx);
    ws.update(cx, |ws, cx| ws.toggle_samples_page(cx));
    cx.run_until_parked();
    click(cx, "results-Kit");
    let roles = ws.read_with(cx, |ws, _| {
        let k = &ws.browser.midi_kit;
        k.iter().map(|k| k.slot.role.clone()).collect::<Vec<_>>()
    });
    assert_eq!(roles, ["drums", "bass", "chords", "lead"]);
    // In the song's key.
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.browser.midi_kit_key.clone())
            .as_deref(),
        Some("E minor")
    );
    let picked = |ws: &Workspace, role: &str| {
        let k = ws
            .browser
            .midi_kit
            .iter()
            .find(|k| k.slot.role == role)
            .unwrap();
        ws.browser.library.midi[k.hit().unwrap().index].name.clone()
    };
    // The bass, lead and beat all come from the pack the chords do.
    let chords = ws.read_with(cx, |ws, _| picked(ws, "chords"));
    let bass = ws.read_with(cx, |ws, _| picked(ws, "bass"));
    assert!(chords.starts_with("Night"), "{chords}");
    assert!(bass.starts_with("Night"), "{bass}");

    // Together, by the button.
    click(cx, "midi-kit-hear");
    settle(cx, &ws, |ws| ws.kit_heard());

    // Another part, while it plays: heard in the kit straight away, a new mix made.
    let made = ws.read_with(cx, |ws, _| ws.browser.heard.len());
    click(cx, "midi-kit-2-1");
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.midi_kit[2].choice), 1);
    settle(cx, &ws, |ws| ws.browser.heard.len() > made);
    assert!(ws.read_with(cx, |ws, _| ws.kit_heard()));
    click(cx, "midi-kit-2--1");

    // A part muted is left out of what's heard and what's added.
    click(cx, "midi-kit-on-3");
    assert!(ws.read_with(cx, |ws, _| !ws.browser.midi_kit[3].slot.on));

    // Locked, the chords stay through new picks, and the mute with them.
    click(cx, "midi-kit-lock-2");
    click(cx, "midi-kit-new");
    assert_eq!(ws.read_with(cx, |ws, _| picked(ws, "chords")), chords);
    assert!(ws.read_with(cx, |ws, _| !ws.browser.midi_kit[3].slot.on));

    // Into the song: three new tracks, moved into its key, going round together.
    let before = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    let moved = ws.read_with(cx, |ws, _| {
        let k = ws
            .browser
            .midi_kit
            .iter()
            .find(|k| k.slot.role == "bass")
            .unwrap();
        k.hit().unwrap().transpose
    });
    assert_ne!(moved, 0);
    click(cx, "midi-kit-add");
    let (tracks, loops, tempo, said) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        let added = &p.tracks[before..];
        let beat = added.first().map(|t| t.regions[0].clone());
        (
            added.len(),
            beat.is_some_and(|r| r.looped),
            p.tempo,
            ws.status.clone().unwrap_or_default(),
        )
    });
    assert_eq!(tracks, 3);
    assert!(
        loops,
        "the two-bar beat goes round under four bars of chords"
    );
    assert_eq!(tempo, 120.0, "the song keeps the tempo it was heard at");
    assert!(said.contains("in E minor"), "{said}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[gpui::test]
fn a_card_dragged_to_the_tracks_goes_as_it_was_heard(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_midi(cx, "midi-kit-drag");
    show_midi_panel(cx);
    ws.update(cx, |ws, cx| ws.toggle_samples_page(cx));
    cx.run_until_parked();
    click(cx, "results-Kit");
    let (bass, up) = ws.read_with(cx, |ws, _| {
        let h = ws.browser.midi_kit[1].hit().unwrap();
        (h.index, h.transpose)
    });
    assert_ne!(up, 0);
    let from = cx.debug_bounds("midi-kit-1").unwrap().center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    // Out of the page, the tracks come back to drop it on, Play in Key being off.
    let far = from + point(px(0.), px(200.));
    cx.simulate_mouse_move(far, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Arrange);
    let to = spot(&ws, cx, BAR, 0);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let file_low = ws.read_with(cx, |ws, _| {
        let m = &ws.browser.library.midi[bass];
        let notes = pina_engine::song::midi_file::open(&m.path).unwrap();
        notes.parts[0].notes.iter().map(|n| n.pitch).min().unwrap()
    });
    let lowest = ws.read_with(cx, |ws, _| {
        let regions = ws.session.project.tracks.iter().flat_map(|t| &t.regions);
        let names: Vec<&str> = regions.clone().map(|r| &r.name[..]).collect();
        let dropped = regions
            .filter(|r| r.name.contains("Bass"))
            .find_map(|r| match &r.content {
                Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).min(),
                _ => None,
            });
        dropped.unwrap_or_else(|| panic!("{names:?}"))
    });
    assert_eq!(lowest as i32, file_low as i32 + up);
    assert_eq!(ws.read_with(cx, |ws, _| ws.browser.key), KeyFilter::Any);
    std::fs::remove_dir_all(dir).unwrap();
}
