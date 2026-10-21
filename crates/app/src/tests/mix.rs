//! The mixer: sends, plugins, the master, the strip's effects, and bouncing.

use super::*;

#[gpui::test]
fn a_send_to_a_new_aux_and_its_level(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    click_on(cx, "add-send-0", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_some()));
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left); // the only bus: New Aux
    let (auxes, sends) = ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        (p.auxes.len(), p.tracks[0].sends.clone())
    });
    assert_eq!(auxes, 1);
    assert_eq!(
        sends,
        [AuxSend {
            bus: 1,
            level_db: 0.0,
            ..Default::default()
        }]
    );

    // Dragging the send down 20 pixels takes 10 dB off.
    let at = cx.debug_bounds("send-0-0").unwrap().center();
    drag(cx, at, at + point(px(0.), px(20.)));
    let level = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sends[0].level_db);
    assert_eq!(level, -10.0);
    // The mixer has a strip for the aux.
    assert!(cx.debug_bounds("strip-Aux(0)").is_none());
    cx.simulate_keystrokes("x");
    assert!(cx.debug_bounds("strip-Aux(0)").is_some());
}

#[gpui::test]
fn a_plugin_that_wont_load_is_reported_and_removable(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    let missing = PluginInfo {
        format: Default::default(),
        path: "/nowhere/Missing.vst3".into(),
        class: "Missing".into(),
        uid: None,
        vendor: "Nobody".into(),
        instrument: false,
        categories: Vec::new(),
    };
    ws.update(cx, |ws, _| ws.plugins = Some(vec![missing]));
    // The Keys strip's empty insert slot offers No Plug-in and the effect.
    click_on(
        cx,
        "slot-Track(0)-Insert(0)",
        Modifiers::none(),
        MouseButton::Left,
    );
    // The effect, and the built-in EQ, compressor and saturator: nothing to take out of an empty
    // slot.
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.len());
    assert_eq!(items, 4);
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left);
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].inserts.len(),
            ws.status.clone(),
        )
    });
    assert_eq!(inserts, 1);
    assert!(status.unwrap().contains("Missing.vst3"));

    // Right-click the slot and take it out again: the error goes with it.
    click_on(
        cx,
        "slot-Track(0)-Insert(0)",
        Modifiers::none(),
        MouseButton::Right,
    );
    click_on(cx, "menu-0", Modifiers::none(), MouseButton::Left);
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].inserts.len(),
            ws.status.clone(),
        )
    });
    assert_eq!((inserts, status), (0, None));
}

#[gpui::test]
fn the_mixer_shapes_a_channel_with_its_eq_and_compressor(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    cx.simulate_keystrokes("x");
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Mixer);
    // The Keys strip's EQ: clicking the thumbnail adds one and opens its window.
    click_on(cx, "eq-Track(0)", Modifiers::none(), MouseButton::Left);
    assert!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.is_some()
            && ws.eq_view.is_some())
    );
    // A band where the display's double-clicked, dragged up: a boost.
    let display = cx.debug_bounds("eq-display").unwrap();
    double_click(cx, display.center());
    let dot = cx.debug_bounds("eq-band-0").unwrap().center();
    drag(cx, dot, dot - point(px(0.), px(80.)));
    let eq = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].eq.unwrap());
    assert_eq!(eq.bands.len(), 1);
    let b = eq.bands[0];
    assert!(b.shape == Shape::Bell && b.gain_db > 3.0, "{b:?}");
    click_on(cx, "eq-close", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.eq_view.is_none()));
    // A compressor from the add-effect menu, then its threshold turned down.
    click_on(cx, "add-effect", Modifiers::none(), MouseButton::Left);
    let items = ws.read_with(cx, |ws, _| ws.menu.as_ref().unwrap().items.clone());
    let comp = items
        .iter()
        .position(|(label, _)| label.starts_with("Compressor"))
        .unwrap();
    let name: &'static str = Box::leak(format!("menu-{comp}").into_boxed_str());
    click_on(cx, name, Modifiers::none(), MouseButton::Left);
    let knob = cx.debug_bounds("comp-Threshold").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(30.)));
    let threshold = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].comp.unwrap().threshold_db
    });
    assert!(threshold < -18.0, "{threshold}");
    // Its detector: peaks, or an RMS average.
    click_on(
        cx,
        "comp-detector-RMS",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| ws.session.project.tracks[0].comp.unwrap().rms));
    // All of it undoes as edits.
    cx.simulate_keystrokes("cmd-z cmd-z cmd-z cmd-z cmd-z cmd-z");
    let (eq, comp) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].eq,
            ws.session.project.tracks[0].comp,
        )
    });
    assert_eq!((eq, comp), (None, None));
}

#[gpui::test]
fn the_saturator_goes_on_from_the_browser_and_turns_in_the_mixer(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    let (ws, cx) = open(cx);
    let add = MenuAction::AddBuiltin {
        channel: Channel::Track(0),
        builtin: Builtin::Sat,
    };
    ws.update(cx, |ws, cx| ws.choose(add.clone(), cx));
    let sat = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sat);
    assert_eq!(
        sat(cx),
        Some(pina_engine::song::project::Saturator::default())
    );
    // Asking again shows it, in the mixer, without another edit.
    let edits = ws.read_with(cx, |ws, _| ws.session.project.clone());
    ws.update(cx, |ws, cx| ws.choose(add, cx));
    assert_eq!(ws.read_with(cx, |ws, _| ws.session.project.clone()), edits);
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Mixer);
    cx.run_until_parked();
    // Drive up.
    let knob = cx.debug_bounds("sat-Drive").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    assert!(sat(cx).unwrap().drive_db > 6.0);
    // Bypassed from its header.
    click_on(cx, "bypass-Sat", Modifiers::none(), MouseButton::Left);
    assert!(sat(cx).unwrap().bypass);
}

#[gpui::test]
fn sends_switch_between_pre_and_post_fader(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.choose(
            crate::ui::menu::MenuAction::Send {
                track: 0,
                bus: None,
            },
            cx,
        );
        ws.show_channel(crate::mix::strip::Channel::Track(0), cx);
    });
    cx.run_until_parked();
    click_on(cx, "send-pre-0", Modifiers::none(), MouseButton::Left);
    let pre = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].sends[0].pre)
    };
    assert!(pre(cx));
    click_on(cx, "send-pre-0", Modifiers::none(), MouseButton::Left);
    assert!(!pre(cx));
}

#[gpui::test]
fn the_master_takes_plugins_and_audio_tracks_pick_an_input(cx: &mut TestAppContext) {
    use crate::{mix::strip::Channel, ui::menu::MenuAction};
    let (ws, cx) = open(cx);
    let plugin = pina_engine::song::project::PluginRef {
        path: "/nonexistent/Missing.vst3".into(),
        class: "Missing".into(),
        ..Default::default()
    };
    let insert = MenuAction::Insert {
        channel: Channel::Master,
        index: 0,
        plugin: Some(plugin),
    };
    ws.update(cx, |ws, cx| ws.choose(insert, cx));
    let (inserts, status) = ws.read_with(cx, |ws, _| {
        (ws.session.project.master_inserts.len(), ws.status.clone())
    });
    assert_eq!(inserts, 1);
    assert!(status.unwrap().contains("Missing.vst3"));

    // Drums is an audio track: its strip's input opens a menu of inputs, and one can be picked.
    ws.update(cx, |ws, cx| ws.show_channel(Channel::Track(2), cx));
    cx.run_until_parked();
    click_on(cx, "input-2", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.menu.is_some()));
    let input = pina_engine::song::project::Input {
        channel: 1,
        stereo: false,
    };
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::Input {
                track: 2,
                input: Some(input),
            },
            cx,
        )
    });
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[2].input),
        Some(input)
    );
}

#[gpui::test]
fn the_bounce_sheet_writes_stems_in_several_formats(cx: &mut TestAppContext) {
    let song = demo_copy("bounce");
    let (ws, cx) = open_at(cx, &song);
    cx.simulate_keystrokes("cmd-b");
    assert!(ws.read_with(cx, |ws, _| ws.export.is_some()));
    click_on(
        cx,
        "bounce-what-Stems",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bounce-format-MP3",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(
        cx,
        "bounce-format-MIDI",
        Modifiers::none(),
        MouseButton::Left,
    );
    click_on(cx, "bounce-level-2", Modifiers::none(), MouseButton::Left); // to a loudness
    click_on(cx, "bounce-go", Modifiers::none(), MouseButton::Left);
    settle(cx, &ws, |ws| {
        ws.export
            .as_ref()
            .and_then(|e| e.run.as_ref())
            .is_some_and(|r| r.finished)
    });
    // A file of each format for each track with something on it.
    let name = song.file_name().unwrap().to_string_lossy().to_string();
    for track in ["Keys", "Lead", "Drums"] {
        for ext in ["wav", "mp3", "mid"] {
            let f = song.join("Bounces").join(format!("{name} - {track}.{ext}"));
            assert!(f.is_file(), "{}", f.display());
        }
    }
    // 24-bit: its header says so (bits per sample at byte 34).
    let wav = std::fs::read(song.join("Bounces").join(format!("{name} - Keys.wav"))).unwrap();
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 24);
    click_on(cx, "bounce-cancel", Modifiers::none(), MouseButton::Left); // Done
    assert!(ws.read_with(cx, |ws, _| ws.export.is_none()));
    let _ = std::fs::remove_dir_all(&song);
}

#[gpui::test]
fn bounce_in_place_puts_a_track_on_audio_and_mutes_it(cx: &mut TestAppContext) {
    use pina_engine::song::project::Content;
    let song = demo_copy("in-place");
    let (ws, cx) = open_at(cx, &song);
    let before = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    click_on(cx, "header-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Bounce in Place");
    settle(cx, &ws, |ws| ws.session.project.tracks.len() == before + 1);
    let (muted, new) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0].mute,
            ws.session.project.tracks[1].clone(),
        )
    });
    assert!(muted);
    assert!(new.instrument.is_none() && new.name.ends_with("(bounced)"));
    let Content::Audio { file, .. } = &new.regions[0].content else {
        panic!("not audio")
    };
    assert!(song.join(file).is_file());
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(
        ws.read_with(cx, |ws, _| (
            ws.session.project.tracks.len(),
            ws.session.project.tracks[0].mute
        )),
        (before, false)
    );
    let _ = std::fs::remove_dir_all(&song);
}
