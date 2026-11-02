//! The Drum Machine: kits made (fitted to the song, from a folder), pads played, stepped through
//! sounds like them, shaped, choked, moved and taken out; sounds layered on a pad; a folder's kit
//! loaded into a kit.

use super::*;
use crate::workspace::Drag;
use pina_engine::song::project::{Content, Instrument, Layering, Sampler};

/// The demo with the little library, its drum hits in one folder: a copy of its own for each
/// test (`name`), so tests side by side don't clear each other's.
fn open_drums<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workspace>, &'a mut VisualTestContext, PathBuf) {
    let dir = demo_copy(&format!("drums-{name}"));
    let library = little_library(&dir.join("Library"));
    let (ws, cx) = open_at(cx, &dir);
    ws.update(cx, |ws, cx| ws.set_library(library, cx));
    (ws, cx, dir)
}

#[gpui::test]
fn a_folder_becomes_a_kit_whose_pads_play_step_shape_and_move(cx: &mut TestAppContext) {
    let (ws, cx, _dir) = open_drums(cx, "folder");
    // A kit of the folder's hits, each its own pad, the Drum Machine open on it.
    ws.update(cx, |ws, cx| ws.kit_from_folder("Pack", None, cx));
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    let pads = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit));
    assert_eq!(pads(cx).len(), 5, "two kicks, a snare, a hat and an 808");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.bottom),
        Some(crate::workspace::Bottom::Drums)
    );
    cx.run_until_parked();
    let pad_on =
        |cx: &VisualTestContext, n: u8| ws.read_with(cx, |ws, _| ws.session.project.pad_on(kit, n));
    let kick = pad_on(cx, 36).expect("the first kick on C1");
    assert!(pad_on(cx, 38).is_some() && pad_on(cx, 42).is_some());
    // The 808, on its own note below the pads shown, is a click away.
    assert!(pad_on(cx, 31).is_some());
    click_on(cx, "drums-out-below", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.from), 28);
    ws.update(cx, |ws, _| ws.drums.from = 36);
    cx.run_until_parked();
    let from = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| match &ws.session.project.tracks[t].instrument {
            Some(Instrument::Sampler(s)) => s.zones[0].from.clone(),
            _ => None,
        })
    };
    let first = from(cx, kick);
    assert!(first.as_ref().is_some_and(|f| f.ends_with("Kick_01.wav")));
    // Struck, it's picked; its next sound is the folder's other kick.
    click_on(cx, "pad-36", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.pad), 36);
    click_on(cx, "pad-next", Modifiers::none(), MouseButton::Left);
    let kick = pad_on(cx, 36).unwrap();
    assert!(from(cx, kick).is_some_and(|f| f.ends_with("Kick_02.wav")));
    click_on(cx, "pad-back", Modifiers::none(), MouseButton::Left);
    assert_eq!(from(cx, kick), first);
    // Shaped: its length shortened, choked with the hats.
    let knob = cx.debug_bounds("sampler-Length").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(60.)));
    let voice = ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[kick].instrument {
            Some(Instrument::Sampler(s)) => s.voice,
            _ => panic!(),
        }
    });
    assert!(voice.sustain == 0.0 && voice.decay_ms > 0.0, "{voice:?}");
    click_on(cx, "pad-choke-2", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[kick]
            .pad
            .unwrap()
            .choke),
        Some(2)
    );
    // Dragged onto the snare's pad: they trade places, the kit's pattern following the kick.
    let (a, b) = (
        cx.debug_bounds("pad-36").unwrap().center(),
        cx.debug_bounds("pad-38").unwrap().center(),
    );
    drag(cx, a, b);
    assert_eq!(pad_on(cx, 38), Some(kick));
    let kick_row = ws.read_with(cx, |ws, _| {
        let Content::Pattern(p) = &ws.session.project.tracks[kit].regions[0].content else {
            panic!()
        };
        p.rows.iter().find(|r| r.name == "Kick").map(|r| r.pitch)
    });
    assert_eq!(kick_row, Some(38));
    // An empty pad given a library sound becomes a pad; taken out, it's gone.
    ws.update(cx, |ws, cx| ws.set_pad_sample(kit, 50, 0, 3, cx));
    assert!(pad_on(cx, 50).is_some());
    assert_eq!(pads(cx).len(), 6);
    cx.run_until_parked();
    click_on(cx, "pad-remove", Modifiers::none(), MouseButton::Left);
    assert_eq!(pad_on(cx, 50), None);
    assert_eq!(pads(cx).len(), 5);
}

#[gpui::test]
fn the_browsers_kit_from_a_folder_loads_into_a_kit_and_a_new_kit_fits_the_song(
    cx: &mut TestAppContext,
) {
    let (ws, cx, _dir) = open_drums(cx, "browser");
    // A new kit from the library, picked to fit: its drums on their notes.
    ws.update(cx, |ws, cx| {
        ws.new_drum_kit(cx);
    });
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    let notes = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let p = &ws.session.project;
            let mut n: Vec<u8> = p
                .pads_of(kit)
                .into_iter()
                .filter_map(|t| p.tracks[t].pad.map(|p| p.note))
                .collect();
            n.sort();
            n
        })
    };
    assert_eq!(notes(cx), [36, 38, 42]);
    // The folder's picks in the browser, loaded into it: its pads take the folder's sounds,
    // new ones come for the rest.
    ws.update(cx, |ws, cx| {
        ws.folder_picks("Pack", cx);
        assert_eq!(
            ws.browser.kit_folder.as_deref(),
            Some("Pack"),
            "the picks say they're the folder's"
        );
        ws.load_kit(kit, cx);
    });
    assert_eq!(notes(cx).len(), 5);
    // A kit made on the Samples page leaves the page be; the Drum Machine's there when the
    // tracks are next shown, and nothing dropped on the page lands on its pads.
    ws.update(cx, |ws, cx| {
        ws.view = crate::workspace::View::Samples;
        ws.kit_from_folder("Pack", None, cx);
        assert_eq!(ws.view, crate::workspace::View::Samples);
        assert_eq!(ws.bottom, Some(crate::workspace::Bottom::Drums));
        assert_eq!(ws.pad_under(point(400., 700.)), None);
    });
    // With no drums in the library, the synthesized kit.
    ws.update(cx, |ws, cx| {
        ws.set_library(Default::default(), cx);
        ws.new_drum_kit(cx);
    });
    let t = ws.read_with(cx, |ws, _| ws.track);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].instrument.clone()),
        Some(Instrument::Drums)
    );
}

#[gpui::test]
fn sounds_layer_on_a_pad_stacked_in_turn_or_by_velocity(cx: &mut TestAppContext) {
    let (ws, cx, dir) = open_drums(cx, "layers");
    ws.update(cx, |ws, cx| ws.kit_from_folder("Pack", None, cx));
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    cx.run_until_parked();
    let kick = ws
        .read_with(cx, |ws, _| ws.session.project.pad_on(kit, 36))
        .unwrap();
    let sampler = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[kick].instrument {
                Some(Instrument::Sampler(s)) => s.clone(),
                _ => panic!(),
            }
        })
    };
    let picked = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.pad_layer(kit, 36));
    click_on(cx, "pad-36", Modifiers::none(), MouseButton::Left);
    // A sub under the kick, from what goes with a kick: on the kick's note, stacked.
    click_on(cx, "pad-add-layer", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "808");
    let s = sampler(cx);
    assert_eq!(s.zones.len(), 2);
    assert!(
        s.zones[1]
            .from
            .as_ref()
            .is_some_and(|f| f.ends_with("808_G.wav"))
    );
    assert_eq!((s.zones[1].note, s.layering()), (36, Some(Layering::Stack)));
    // The new layer is picked: its level is its own.
    assert_eq!(picked(cx), 1);
    cx.run_until_parked();
    let knob = cx.debug_bounds("sampler-Gain(1)").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(40.)));
    let s = sampler(cx);
    assert!(
        s.zones[1].gain_db < 0.0 && s.zones[0].gain_db == 0.0,
        "{:?}",
        s.zones
    );
    // In turn, then split by velocity.
    click_on(
        cx,
        "pad-layering-cycle",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(sampler(cx).layering(), Some(Layering::Cycle));
    click_on(
        cx,
        "pad-layering-velocity",
        Modifiers::none(),
        MouseButton::Left,
    );
    let ranges: Vec<_> = sampler(cx).zones.iter().map(|z| z.velocities).collect();
    assert_eq!(ranges, [Some((1, 63)), Some((64, 127))]);
    // Blended, they fade into each other rather than switching.
    cx.run_until_parked();
    click_on(cx, "pad-blend", Modifiers::none(), MouseButton::Left);
    assert!(sampler(cx).blends());
    // A sample from the browser dropped on the pad's sound is layered on, then a file from the
    // Finder: the velocities shared out again.
    let strip = cx.debug_bounds("pad-sound").unwrap().center();
    let at = point(f32::from(strip.x), f32::from(strip.y));
    ws.update(cx, |ws, cx| {
        let drag = Drag::Sample {
            index: 2,
            from: at,
            at,
            moved: true,
        };
        ws.drop_sample(Some(drag), cx);
    });
    assert_eq!((sampler(cx).zones.len(), picked(cx)), (3, 2));
    let file = dir.join("Rim.wav");
    std::fs::copy(dir.join("Library/Hat_01.wav"), &file).unwrap();
    ws.update(cx, |ws, cx| {
        ws.drop_files(std::slice::from_ref(&file), at, cx)
    });
    let s = sampler(cx);
    assert_eq!(s.zones.len(), 4);
    assert_eq!(s.zones[3].from.as_deref(), Some(file.as_path()));
    assert_eq!(s.zones[3].velocities, Some((96, 127)));
    assert!(s.zones.iter().all(|z| z.velocity_fade > 0), "still blended");
    // Stepping the whole kit keeps what's layered; the picked layer's taken out.
    ws.update(cx, |ws, cx| ws.step_kit(1, cx));
    assert_eq!(sampler(cx).zones.len(), 4);
    cx.run_until_parked();
    click_on(cx, "pad-remove-layer", Modifiers::none(), MouseButton::Left);
    let s: Sampler = sampler(cx);
    assert_eq!((s.zones.len(), picked(cx)), (3, 2));
    assert_eq!(s.layering(), Some(Layering::Velocity));
    click_on(cx, "pad-layer-0", Modifiers::none(), MouseButton::Left);
    assert_eq!(picked(cx), 0);
}
