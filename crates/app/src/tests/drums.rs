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
    // The pane tall enough for the pads and the chain list under them.
    ws.update(cx, |ws, cx| {
        ws.set_library(library, cx);
        ws.editor_h = 720.0;
    });
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
    ws.update(cx, |ws, cx| {
        ws.tracks_keys = false;
        cx.notify();
    });
    cx.run_until_parked();
    let pad_on =
        |cx: &VisualTestContext, n: u8| ws.read_with(cx, |ws, _| ws.session.project.pad_on(kit, n));
    let kick = pad_on(cx, 36).expect("the first kick on C1");
    assert!(pad_on(cx, 38).is_some() && pad_on(cx, 42).is_some());
    // Sixteen pads at a time, from C1: the 808, on its note below, a bank down (an empty note
    // there shows too).
    assert!(pad_on(cx, 31).is_some());
    assert!(cx.debug_bounds("pad-36").is_some() && cx.debug_bounds("pad-51").is_some());
    assert!(cx.debug_bounds("pad-31").is_none());
    cx.simulate_keystrokes("down");
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.view), 20);
    assert!(cx.debug_bounds("pad-31").is_some());
    cx.simulate_keystrokes("up alt-up");
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.view), 40);
    cx.simulate_keystrokes("alt-down");
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.view), 36);
    let from = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| match &ws.session.project.tracks[t].instrument {
            Some(Instrument::Sampler(s)) => s.zones[0].from.clone(),
            _ => None,
        })
    };
    let first = from(cx, kick);
    assert!(first.as_ref().is_some_and(|f| f.ends_with("Kick_01.wav")));
    // Struck, it's picked; its next similar sound (the swap arrow on the pad) is the folder's
    // other kick, and back.
    click_on(cx, "pad-36", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.pad), 36);
    click_on(cx, "pad-36-swap-1", Modifiers::none(), MouseButton::Left);
    let kick = pad_on(cx, 36).unwrap();
    assert!(from(cx, kick).is_some_and(|f| f.ends_with("Kick_02.wav")));
    click_on(cx, "pad-36-swap--1", Modifiers::none(), MouseButton::Left);
    assert_eq!(from(cx, kick), first);
    // Shaped on its Drum Sampler: held, then decaying, rather than ringing out.
    let knob = cx.debug_bounds("sampler-Hold").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(60.)));
    let voice = ws.read_with(cx, |ws, _| {
        match &ws.session.project.tracks[kick].instrument {
            Some(Instrument::Sampler(s)) => s.voice,
            _ => panic!(),
        }
    });
    assert!(voice.sustain == 0.0 && voice.hold_ms > 0.0, "{voice:?}");
    // Choked with the hats, from its chain's row in the Chains tab.
    click_on(cx, "rack-tab-chains", Modifiers::none(), MouseButton::Left);
    click_on(cx, "chain-36-choke", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "2");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[kick]
            .pad
            .unwrap()
            .choke),
        Some(2)
    );
    click_on(cx, "rack-tab-sampler", Modifiers::none(), MouseButton::Left);
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
    // An empty pad given a library sound becomes a pad; deleted from its menu, it's gone.
    ws.update(cx, |ws, cx| ws.set_pad_sample(kit, 50, 0, 3, cx));
    assert!(pad_on(cx, 50).is_some());
    assert_eq!(pads(cx).len(), 6);
    cx.run_until_parked();
    click_on(cx, "pad-50", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Delete");
    assert_eq!(pad_on(cx, 50), None);
    assert_eq!(pads(cx).len(), 5);
}

#[gpui::test]
fn the_browsers_kit_from_a_folder_loads_into_a_kit_and_a_new_kit_starts_empty(
    cx: &mut TestAppContext,
) {
    let (ws, cx, _dir) = open_drums(cx, "browser");
    // A new kit starts empty, with no bar button over it.
    ws.update(cx, |ws, cx| {
        ws.new_drum_kit(cx);
    });
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    assert!(ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit).is_empty()));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pad-36").is_some() && cx.debug_bounds("drums-bar-kit").is_none());
    // Fitted to the song from the library: its drums on their notes.
    ws.update(cx, |ws, cx| {
        ws.drum_op(crate::editors::drums::DrumOp::Fit, cx)
    });
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
    // With no drums in the library, fitting a kit says why not, and a new kit is the
    // synthesized one.
    ws.update(cx, |ws, cx| {
        ws.set_library(Default::default(), cx);
        ws.fit_kit(kit, cx);
        assert!(ws.status.as_deref().is_some_and(|s| s.contains("no drums")));
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
    // The new layer is picked: its gain is its own.
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
    // In turn, then split by velocity, then blended.
    click_on(cx, "pad-layering", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Cycle");
    assert_eq!(sampler(cx).layering(), Some(Layering::Cycle));
    click_on(cx, "pad-layering", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Velocity");
    let ranges: Vec<_> = sampler(cx).zones.iter().map(|z| z.velocities).collect();
    assert_eq!(ranges, [Some((1, 63)), Some((64, 127))]);
    click_on(cx, "pad-layering", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "Velocity layers blend");
    assert!(sampler(cx).blends());
    // A sample from the browser dropped on the pad's waveform is layered on, then a file from
    // the Finder: the velocities shared out again.
    cx.run_until_parked();
    let strip = cx.debug_bounds("pad-layers").unwrap().center();
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
    // Swapping the whole kit keeps what's layered; the picked layer's taken out.
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

#[gpui::test]
fn a_pads_sounds_hot_swap_beside_it_to_click_step_and_go_back_to(cx: &mut TestAppContext) {
    use crate::editors::drums::{DrumOp, Source};
    let (ws, cx, _dir) = open_drums(cx, "sounds");
    ws.update(cx, |ws, cx| ws.kit_from_folder("Pack", None, cx));
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    cx.run_until_parked();
    let pad_on =
        |cx: &VisualTestContext, n: u8| ws.read_with(cx, |ws, _| ws.session.project.pad_on(kit, n));
    let kick = pad_on(cx, 36).unwrap();
    let sound = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            match &ws.session.project.tracks[kick].instrument {
                Some(Instrument::Sampler(s)) => s.zones[0]
                    .from
                    .as_ref()
                    .and_then(|f| f.file_name())
                    .map(|f| f.to_string_lossy().into_owned()),
                _ => None,
            }
        })
    };
    click_on(cx, "pad-36", Modifiers::none(), MouseButton::Left);
    assert_eq!(sound(cx).as_deref(), Some("Kick_01.wav"));
    // Hot-swap, from over its waveform: like it, its own sound first, the other kick next. A
    // click on that puts it on the pad.
    click_on(cx, "pad-hot-swap", Modifiers::none(), MouseButton::Left);
    let row = cx.debug_bounds("pad-sound-1").unwrap().center();
    cx.simulate_mouse_down(row, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(row, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(sound(cx).as_deref(), Some("Kick_02.wav"));
    // Back to: the sound it had (as Return to Reference does, from the waveform's menu).
    click_on(cx, "sounds-back", Modifiers::none(), MouseButton::Left);
    assert_eq!(sound(cx).as_deref(), Some("Kick_01.wav"));
    // With hot-swap open, down steps on; right goes to the next pad; D hot-swaps the kit.
    ws.update(cx, |ws, cx| {
        ws.tracks_keys = false;
        cx.notify();
    });
    cx.simulate_keystrokes("down");
    assert_eq!(sound(cx).as_deref(), Some("Kick_02.wav"));
    cx.simulate_keystrokes("right");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.drums.pad),
        37,
        "the second kick, next up"
    );
    cx.simulate_keystrokes("d");
    assert!(cx.debug_bounds("kit-swap").is_some());
    cx.simulate_keystrokes("d");
    assert!(cx.debug_bounds("kit-swap").is_none());
    // Every kick, from the library.
    click_on(cx, "sounds-kind", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.drums.source), Source::Kind);
    // The kick moved to a free note; a sample dropped on an empty pad makes a pad there.
    ws.update(cx, |ws, cx| {
        ws.drum_op(DrumOp::Note { note: 36, to: 60 }, cx);
        ws.drums.view = 36;
    });
    assert_eq!((pad_on(cx, 60), pad_on(cx, 36)), (Some(kick), None));
    cx.run_until_parked();
    let pads = ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit).len());
    let empty = cx.debug_bounds("pad-40").unwrap().center();
    let at = point(f32::from(empty.x), f32::from(empty.y));
    ws.update(cx, |ws, cx| {
        let drag = Drag::Sample {
            index: 2,
            from: at,
            at,
            moved: true,
        };
        ws.drop_sample(Some(drag), cx);
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit).len()),
        pads + 1
    );
    assert!(pad_on(cx, 40).is_some());
}

#[gpui::test]
fn the_drum_rack_has_its_chains_notes_locks_returns_macros_and_drum_sampler(
    cx: &mut TestAppContext,
) {
    use crate::editors::drums::DrumOp;
    use pina_engine::song::project::{DrumModDest, FilterMode, PlaybackFx};
    let (ws, cx, _dir) = open_drums(cx, "rack");
    ws.update(cx, |ws, cx| ws.kit_from_folder("Pack", None, cx));
    let kit = ws.read_with(cx, |ws, _| ws.drum_kit()).expect("a kit");
    cx.run_until_parked();
    let pad_on =
        |cx: &VisualTestContext, n: u8| ws.read_with(cx, |ws, _| ws.session.project.pad_on(kit, n));
    let (kick, snare) = (pad_on(cx, 36).unwrap(), pad_on(cx, 38).unwrap());
    let track = |cx: &VisualTestContext, t: usize| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[t].clone())
    };
    let voice = |cx: &VisualTestContext, t: usize| match track(cx, t).instrument {
        Some(Instrument::Sampler(s)) => s.voice,
        _ => panic!(),
    };
    click_on(cx, "pad-36", Modifiers::none(), MouseButton::Left);
    // Locked, swapping every pad leaves it; the snare moves on.
    click_on(cx, "pad-36-lock", Modifiers::none(), MouseButton::Left);
    assert!(track(cx, kick).pad.unwrap().lock);
    let (kick_was, snare_was) = (track(cx, kick).instrument, track(cx, snare).instrument);
    click_on(cx, "drums-kit-next", Modifiers::none(), MouseButton::Left);
    assert_eq!(track(cx, kick).instrument, kick_was);
    assert_ne!(track(cx, snare).instrument, snare_was);
    // The chain list: its Play note moves its sound; mute from its activator.
    click_on(cx, "rack-tab-chains", Modifiers::none(), MouseButton::Left);
    assert!(cx.debug_bounds("chain-36-receive").is_some());
    ws.update(cx, |ws, cx| {
        ws.drum_op(
            DrumOp::Play {
                note: 36,
                play: Some(43),
            },
            cx,
        )
    });
    assert_eq!(track(cx, kick).pad.unwrap().play, Some(43));
    assert_eq!(track(cx, kick).pad.unwrap().shift(), 7.0);
    click_on(cx, "chain-36-on", Modifiers::none(), MouseButton::Left);
    assert!(track(cx, kick).mute);
    // A return chain, its sends in the list.
    click_on(
        cx,
        "rack-option-returns",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "rack-add-return", Modifiers::none(), MouseButton::Left);
    let id = track(cx, kit).id;
    let bus = ws
        .read_with(cx, |ws, _| {
            ws.session
                .project
                .auxes
                .iter()
                .find(|a| a.kit == Some(id))
                .map(|a| a.bus)
        })
        .expect("a return");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds(Box::leak(format!("chain-36-send-{bus}").into_boxed_str()))
            .is_some()
    );
    // Macros: the first made on a click, shared with every pad; a variation kept.
    click_on(cx, "rack-tab-macros", Modifiers::none(), MouseButton::Left);
    click_on(cx, "rack-macro-0", Modifiers::none(), MouseButton::Left);
    let shared = |cx: &VisualTestContext, t: usize| {
        track(cx, t)
            .modulators
            .iter()
            .any(|m| matches!(m.kind, pina_engine::dsp::modulate::ModKind::Shared { .. }))
    };
    assert!(shared(cx, kit) && shared(cx, snare));
    click_on(
        cx,
        "rack-variation-new",
        Modifiers::none(),
        MouseButton::Left,
    );
    let variations = ws.read_with(cx, |ws, _| {
        let bus = ws.session.project.tracks[kit].output;
        ws.session
            .project
            .auxes
            .iter()
            .find(|a| Some(a.bus) == bus)
            .map_or(0, |a| a.variations.len())
    });
    assert_eq!(variations, 1);
    click_on(cx, "rack-tab-sampler", Modifiers::none(), MouseButton::Left);
    // The Drum Sampler: Gate, a playback effect switched on and off, a filter, the modulation.
    click_on(cx, "pad-gate", Modifiers::none(), MouseButton::Left);
    let t = track(cx, kick);
    let Some(Instrument::Sampler(s)) = &t.instrument else {
        panic!()
    };
    assert!(s.zones.iter().all(|z| z.gate) && s.voice.sustain == 1.0);
    ws.update(cx, |ws, cx| {
        ws.drum_op(
            DrumOp::Fx {
                note: 36,
                fx: PlaybackFx::SubOsc,
            },
            cx,
        )
    });
    let v = voice(cx, kick);
    assert_eq!(v.shown_fx(), Some(PlaybackFx::SubOsc));
    assert!(v.sub > 0.0 && v.sub_hz > 0.0);
    click_on(cx, "pad-fx-on", Modifiers::none(), MouseButton::Left);
    assert!(voice(cx, kick).fx_off && voice(cx, kick).heard().sub == 0.0);
    click_on(cx, "pad-fx-on", Modifiers::none(), MouseButton::Left);
    assert!(!voice(cx, kick).fx_off);
    // The X/Y pad: dragged to its top right, both settings at their most.
    cx.run_until_parked();
    let xy = cx.debug_bounds("pad-fx-xy").unwrap();
    drag(cx, xy.center(), xy.top_right() + point(px(10.), px(-10.)));
    let v = voice(cx, kick);
    assert!(v.sub >= 0.99 && v.sub_hz >= 119.0, "{} {}", v.sub, v.sub_hz);
    ws.update(cx, |ws, cx| {
        ws.drum_op(
            DrumOp::Filter {
                note: 36,
                mode: FilterMode::Peak,
            },
            cx,
        )
    });
    assert_eq!(voice(cx, kick).filter, FilterMode::Peak);
    cx.run_until_parked();
    assert!(cx.debug_bounds("sampler-PeakDb").is_some());
    click_on(cx, "pad-filter-on", Modifiers::none(), MouseButton::Left);
    assert!(voice(cx, kick).filter_off);
    ws.update(cx, |ws, cx| {
        ws.drum_op(
            DrumOp::ModDest {
                note: 36,
                dest: DrumModDest::Decay,
            },
            cx,
        )
    });
    assert_eq!(voice(cx, kick).drum_dest, DrumModDest::Decay);
    // Duplicated to the next free note, then extracted to a track of its own.
    let n = ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit).len());
    ws.update(cx, |ws, cx| ws.drum_op(DrumOp::Duplicate { note: 36 }, cx));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.pads_of(kit).len()),
        n + 1
    );
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    ws.update(cx, |ws, cx| ws.drum_op(DrumOp::Extract { note: 38 }, cx));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks + 1
    );
}
