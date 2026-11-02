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
    ws.update(cx, |ws, _| {
        ws.plugins = Some(vec![missing]);
        ws.catalog_plugins();
    });
    // The Keys strip's empty insert slot opens the picker: the effect found, Enter puts it in.
    click_on(
        cx,
        "slot-Track(0)-Insert(0)",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_some()));
    cx.simulate_input("missing");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
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
    // A compressor from the add-effect picker, then its threshold turned down.
    click_on(cx, "add-effect", Modifiers::none(), MouseButton::Left);
    cx.simulate_input("compressor");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
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
fn the_saturator_goes_on_from_the_browser_and_works_in_its_window(cx: &mut TestAppContext) {
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
    // Asking again stacks another after it, its window open; then back to the first.
    ws.update(cx, |ws, cx| {
        ws.fx_view = None;
        ws.choose(add, cx);
    });
    let more = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].more.clone());
    assert_eq!(
        more,
        [pina_engine::song::project::Unit::Sat(
            pina_engine::song::project::Saturator::default()
        )]
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.fx_view.as_ref().map(|v| v.stacked)),
        Some(Some(0))
    );
    ws.update(cx, |ws, cx| ws.open_fx(Channel::Track(0), Builtin::Sat, cx));
    cx.run_until_parked();
    // A band split off where it's double-clicked, picked, driven harder than the first.
    let d = cx.debug_bounds("sat-display").unwrap();
    double_click(
        cx,
        point(d.origin.x + d.size.width * 0.6, d.center().y + px(60.)),
    );
    assert_eq!(sat(cx).unwrap().bands.len(), 2);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.fx_view.as_ref().unwrap().band),
        1
    );
    let knob = cx.debug_bounds("fx-knob-drive").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    let s = sat(cx).unwrap();
    assert!(
        s.bands[1].drive_db > 6.0 && s.bands[0].drive_db == 6.0,
        "{s:?}"
    );
    // The split moved down by its handle.
    let split = s.crossovers[0];
    let handle = cx.debug_bounds("sat-split-0").unwrap().center();
    drag(cx, handle, handle - point(px(100.), px(0.)));
    assert!(sat(cx).unwrap().crossovers[0] < split * 0.7);
    // A style for the band, and off from the title bar.
    click_on(cx, "sat-style-heavy", Modifiers::none(), MouseButton::Left);
    assert_eq!(
        sat(cx).unwrap().bands[1].style,
        pina_engine::song::project::SatStyle::Heavy
    );
    click_on(cx, "fx-on", Modifiers::none(), MouseButton::Left);
    assert!(sat(cx).unwrap().bypass);
}

#[gpui::test]
fn the_limiter_goes_on_last_and_works_in_its_window(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::{Fx, LimitStyle, Limiter};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::AddBuiltin {
                channel: Channel::Master,
                builtin: Builtin::Limit,
            },
            cx,
        )
    });
    let limit = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.session.project.master_limit);
    assert_eq!(limit(cx), Some(Limiter::default()));
    // Last in the master's chain, and its window open.
    let shown = ws.read_with(cx, |ws, _| ws.shown_effects(Channel::Master));
    assert_eq!(shown.last().map(|e| e.1), Some(Fx::Limit));
    assert!(ws.read_with(cx, |ws, _| {
        ws.fx_view
            .as_ref()
            .is_some_and(|v| v.which == Builtin::Limit)
    }));
    cx.run_until_parked();
    // Driven harder, a style picked, true peak off, oversampled.
    let knob = cx.debug_bounds("fx-knob-gain").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    assert!(limit(cx).unwrap().gain_db > 3.0, "{:?}", limit(cx));
    click_on(
        cx,
        "limit-style-punchy",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(limit(cx).unwrap().style, LimitStyle::Punchy);
    click_on(cx, "limit-tp-off", Modifiers::none(), MouseButton::Left);
    click_on(cx, "limit-os-4×", Modifiers::none(), MouseButton::Left);
    let l = limit(cx).unwrap();
    assert_eq!((l.true_peak, l.oversample), (false, 4));
    // Listening aids, and Safe, which never clips.
    click_on(cx, "limit-delta", Modifiers::none(), MouseButton::Left);
    assert!(limit(cx).unwrap().delta);
    ws.update(cx, |ws, _| {
        ws.limiter_mut(Channel::Master).unwrap().clip = 0.5
    });
    click_on(cx, "limit-style-safe", Modifiers::none(), MouseButton::Left);
    let l = limit(cx).unwrap();
    assert_eq!(
        (l.style, l.clip, l.true_peak),
        (LimitStyle::Safe, 0.0, true)
    );
    click_on(cx, "fx-on", Modifiers::none(), MouseButton::Left);
    assert!(limit(cx).unwrap().bypass);
    // Taken out.
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::RemoveBuiltin {
                channel: Channel::Master,
                builtin: Builtin::Limit,
                stacked: None,
            },
            cx,
        )
    });
    assert_eq!(limit(cx), None);
}

#[gpui::test]
fn the_limiters_gain_is_automated_from_its_knob(cx: &mut TestAppContext) {
    use crate::mix::strip::{Builtin, Channel};
    use pina_engine::song::project::{AutoMode, LimitParam, Limiter, PPQ, Param};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].limit = Some(Limiter::default());
        ws.changed(cx);
        ws.open_fx(Channel::Track(0), Builtin::Limit, cx);
    });
    // Its settings are in the automation lanes' list.
    let items = ws.read_with(cx, |ws, _| ws.auto_params(0, 0));
    let labels: Vec<&String> = items.iter().map(|(l, _)| l).collect();
    assert!(
        labels.iter().any(|l| l.contains("Limiter: Gain")),
        "{labels:?}"
    );
    // Latch, playing: the gain turned is recorded, in dB.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].automation.mode = AutoMode::Latch;
        ws.changed(cx);
        ws.player.play();
    });
    until(cx, &ws, |ws| ws.playhead() > PPQ / 2);
    cx.run_until_parked();
    let knob = cx.debug_bounds("fx-knob-gain").unwrap().center();
    cx.simulate_mouse_down(knob, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        knob - point(px(0.), px(60.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    let held = ws.read_with(cx, |ws, _| ws.playhead());
    until(cx, &ws, |ws| ws.playhead() > held + PPQ / 4);
    cx.simulate_mouse_up(
        knob - point(px(0.), px(60.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    ws.update(cx, |ws, cx| ws.stop(cx));
    until(cx, &ws, |ws| ws.writing.is_empty() && !ws.was_playing);
    let points = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0]
            .automation
            .points(Param::Limit(LimitParam::Gain))
            .to_vec()
    });
    assert!(points.iter().any(|p| p.value > 6.0), "{points:?}");
}

#[gpui::test]
fn the_compressor_window_sets_its_style_and_turns_its_knobs(cx: &mut TestAppContext) {
    use crate::mix::strip::{Builtin, Channel};
    use pina_engine::song::project::CompStyle;
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.open_fx(Channel::Track(0), Builtin::Comp, cx)
    });
    cx.run_until_parked();
    let comp = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].comp.unwrap())
    };
    // Bus: its ratio, attack and release go to their steps.
    click_on(cx, "comp-style-bus", Modifiers::none(), MouseButton::Left);
    let c = comp(cx);
    assert_eq!((c.style, c.ratio), (CompStyle::Bus, 4.0));
    assert!(CompStyle::BUS_ATTACKS.contains(&c.attack_ms), "{c:?}");
    let knob = cx.debug_bounds("fx-knob-threshold").unwrap().center();
    drag(cx, knob, knob + point(px(0.), px(30.)));
    assert!(comp(cx).threshold_db < -18.0);
    click_on(
        cx,
        "comp-release-auto",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(comp(cx).auto_release);
    // ⌘-click puts a knob back.
    click_on(
        cx,
        "fx-knob-threshold",
        Modifiers::command(),
        MouseButton::Left,
    );
    assert_eq!(comp(cx).threshold_db, -18.0);
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

#[gpui::test]
fn more_strips_than_fit_scroll_sideways_with_the_wheel(cx: &mut TestAppContext) {
    use pina_engine::song::project::Track;
    let (ws, cx) = open(cx);
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    ws.update(cx, |ws, cx| {
        for n in 0..24 {
            let at = ws.session.project.tracks.len();
            ws.session.project.add_track(
                at,
                Track {
                    name: format!("Extra {n}"),
                    ..Default::default()
                },
            );
        }
        cx.notify();
    });
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    let first = cx.debug_bounds("strip-Track(0)").unwrap();
    let master = cx.debug_bounds("strip-Master").unwrap();
    let view = cx.debug_bounds("mixer-strips").unwrap();
    // The master, past the last track, is out of sight to the right.
    assert!(master.left() > view.right(), "{master:?} {view:?}");
    // A turn of the wheel (up and down, as a mouse's is) moves along the strips.
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: first.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-4000.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let now = cx.debug_bounds("strip-Master").unwrap();
    assert!(now.right() <= view.right() + px(1.), "{now:?} {view:?}");
}

#[gpui::test]
fn effects_are_dragged_into_another_order_and_a_click_still_opens_one(cx: &mut TestAppContext) {
    use crate::mix::strip::Channel;
    use pina_engine::song::project::{Comp, Eq, Fx};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.tracks[0].eq = Some(Eq::default());
            ws.session.project.tracks[0].comp = Some(Comp::default());
        });
        ws.select_track(0);
    });
    cx.run_until_parked();
    let chain = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].chain.clone())
    };
    // The compressor dragged up past the EQ.
    let comp = cx.debug_bounds("builtin-Track(0)-Comp").unwrap();
    drag(
        cx,
        comp.center(),
        comp.center() - point(px(0.), comp.size.height * 1.2),
    );
    assert_eq!(chain(cx)[..2], [Fx::Comp, Fx::Eq]);
    assert!(ws.read_with(cx, |ws, _| ws.fx_view.is_none()));
    // Clicked, it opens; undone, the order's as it was.
    click_on(
        cx,
        "builtin-Track(0)-Comp",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(ws.read_with(cx, |ws, _| {
        ws.fx_view
            .as_ref()
            .is_some_and(|v| v.ch == Channel::Track(0))
    }));
    ws.update(cx, |ws, cx| ws.undo(cx));
    assert!(chain(cx).is_empty());
}

#[gpui::test]
fn a_full_mixer_strip_still_takes_another_plugin(cx: &mut TestAppContext) {
    use crate::mix::strip::Channel;
    use pina_engine::song::project::{Comp, Eq, Saturator};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            let t = &mut ws.session.project.tracks[0];
            (t.eq, t.comp, t.sat) = (
                Some(Eq::default()),
                Some(Comp::default()),
                Some(Saturator::default()),
            );
            let delay = pina_engine::song::project::PluginRef {
                format: pina_engine::song::project::Format::Au,
                class: "AUDelay".into(),
                uid: Some("aufx dely appl".into()),
                ..Default::default()
            };
            for i in 0..4 {
                t.set_insert(i, Some(delay.clone())).unwrap();
            }
        });
        ws.show_channel(Channel::Track(0), cx);
    });
    cx.run_until_parked();
    // Seven effects, more than a mixer strip shows: the slot for another is still there.
    assert!(cx.debug_bounds("strip-more-Track(0)").is_some());
    assert!(cx.debug_bounds("slot-Track(0)-Insert(4)").is_some());
}

#[gpui::test]
fn effects_are_dragged_into_another_order_in_the_mixer(cx: &mut TestAppContext) {
    use crate::mix::strip::Channel;
    use pina_engine::song::project::{Comp, Eq, Fx, Saturator};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            let t = &mut ws.session.project.tracks[0];
            (t.eq, t.comp, t.sat) = (
                Some(Eq::default()),
                Some(Comp::default()),
                Some(Saturator::default()),
            );
        });
        ws.show_channel(Channel::Track(0), cx);
    });
    cx.run_until_parked();
    let chain = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].chain.clone())
    };
    // The saturator, the third, dragged to the top of the strip.
    let sat = cx.debug_bounds("builtin-Track(0)-Sat").unwrap();
    let eq = cx.debug_bounds("builtin-Track(0)-Eq").unwrap();
    drag(
        cx,
        sat.center(),
        point(sat.center().x, eq.center().y - px(4.)),
    );
    assert_eq!(chain(cx)[..3], [Fx::Sat, Fx::Eq, Fx::Comp]);
}

#[gpui::test]
fn effect_cards_are_dragged_by_their_grips_in_the_mixer_panel(cx: &mut TestAppContext) {
    use crate::mix::strip::Channel;
    use pina_engine::song::project::{Comp, Eq, Fx, Saturator};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            let t = &mut ws.session.project.tracks[0];
            (t.eq, t.comp, t.sat) = (
                Some(Eq::default()),
                Some(Comp::default()),
                Some(Saturator::default()),
            );
        });
        ws.show_channel(Channel::Track(0), cx);
    });
    cx.run_until_parked();
    let grip = cx.debug_bounds("card-grip-Track(0)-2").unwrap();
    let eq = cx.debug_bounds("card-grip-Track(0)-0").unwrap();
    drag(cx, grip.center(), eq.center());
    let chain = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].chain.clone());
    assert_eq!(chain[..3], [Fx::Sat, Fx::Eq, Fx::Comp]);
}

#[gpui::test]
fn a_stacks_channel_is_in_the_inspector_to_turn_down_and_put_effects_on(cx: &mut TestAppContext) {
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.add_stack(&[0, 2]);
        });
        ws.show_inspector = true;
        // A track in the stack: its own strip, and its stack's beside it (not the master's).
        ws.select_track(0);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("fader-Track(0)").is_some());
    assert!(
        cx.debug_bounds("fader-Aux(0)").is_some(),
        "its stack beside it"
    );
    assert!(cx.debug_bounds("fader-Master").is_none());
    // The stack picked: its own strip, its fader turning it down, and the master's.
    ws.update(cx, |ws, cx| {
        ws.pick_stack(0);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("fader-Master").is_some());
    let fader = cx
        .debug_bounds("fader-Aux(0)")
        .expect("the stack's fader")
        .center();
    drag(cx, fader, fader + point(px(0.), px(60.)));
    let down = ws.read_with(cx, |ws, _| ws.session.project.auxes[0].volume_db);
    assert!(down < -3.0, "{down}");
    // And from its own row's level, dragged up.
    let level = cx.debug_bounds("stack-level-0").unwrap().center();
    drag(cx, level, level - point(px(0.), px(40.)));
    assert!(ws.read_with(cx, |ws, _| ws.session.project.auxes[0].volume_db) > down);
    // Its effects go on it, not on a track: its strip has the slots.
    let slots = cx
        .debug_bounds("eq-Aux(0)")
        .or_else(|| cx.debug_bounds("builtin-Aux(0)-Eq"));
    assert!(slots.is_some(), "the stack's EQ slot");
}

#[gpui::test]
fn built_ins_stack_on_a_channel_each_in_its_own_window(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::{Comp, Fx, Unit};
    let (ws, cx) = open(cx);
    let ch = Channel::Track(0);
    let add = |b| MenuAction::AddBuiltin {
        channel: ch,
        builtin: b,
    };
    // A compressor, then another: the second stacked after the plugins, its window open.
    ws.update(cx, |ws, cx| ws.choose(add(Builtin::Comp), cx));
    ws.update(cx, |ws, cx| ws.choose(add(Builtin::Comp), cx));
    let state = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let t = &ws.session.project.tracks[0];
            (t.comp, t.more.clone())
        })
    };
    let (own, more) = state(cx);
    assert_eq!(own, Some(Comp::default()));
    assert_eq!(more, [Unit::Comp(Comp::default())]);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .fx_view
            .as_ref()
            .map(|v| (v.which, v.stacked))),
        Some((Builtin::Comp, Some(0)))
    );
    cx.run_until_parked();
    // Its knobs turn it, not the first: harder.
    let knob = cx.debug_bounds("fx-knob-ratio").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    let (own, more) = state(cx);
    assert_eq!(own, Some(Comp::default()));
    let Unit::Comp(second) = more[0] else {
        panic!("{more:?}")
    };
    assert!(second.ratio > Comp::default().ratio, "{second:?}");
    // In the strip, named after the first, opened from there.
    let shown = ws.read_with(cx, |ws, _| ws.shown_effects(ch));
    let row = shown.iter().position(|e| e.1 == Fx::More).unwrap();
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.stacked(ch, 0).map(|s| s.1)),
        Some("Compressor 2".to_string())
    );
    // An EQ stacked after it, moved before everything; its own window shows it.
    ws.update(cx, |ws, cx| {
        ws.choose(add(Builtin::Eq), cx);
        ws.choose(add(Builtin::Eq), cx);
    });
    let more = state(cx).1;
    assert!(matches!(more[1], Unit::Eq(_)), "{more:?}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.eq_view.as_ref().map(|v| v.stacked)),
        Some(Some(1))
    );
    let shown = ws.read_with(cx, |ws, _| ws.shown_effects(ch));
    let eq_row = shown.iter().rposition(|e| e.1 == Fx::More).unwrap();
    ws.update(cx, |ws, cx| ws.move_effect(ch, shown[eq_row].0, 0, cx));
    let more = state(cx).1;
    assert!(matches!(more[0], Unit::Eq(_)), "{more:?}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.shown_effects(ch))[0].1,
        Fx::More
    );
    let _ = row;
    // Taken out: the stacked compressor goes, the EQ stays.
    ws.update(cx, |ws, cx| {
        ws.choose(
            MenuAction::RemoveBuiltin {
                channel: ch,
                builtin: Builtin::Comp,
                stacked: Some(1),
            },
            cx,
        )
    });
    let (own, more) = state(cx);
    assert_eq!(own, Some(Comp::default()));
    assert_eq!(more.len(), 1);
    assert!(matches!(more[0], Unit::Eq(_)));
}

#[gpui::test]
fn a_reverb_and_a_delay_go_on_an_aux_and_work_in_their_windows(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::{DelayStyle, ReverbStyle, Unit};
    let (ws, cx) = open(cx);
    let aux = ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.add_aux();
        });
        Channel::Aux(ws.session.project.auxes.len() - 1)
    });
    let add = |b| MenuAction::AddBuiltin {
        channel: aux,
        builtin: b,
    };
    // On an aux: all reverb, a hall as halls are, its window open.
    ws.update(cx, |ws, cx| {
        ws.choose(
            add(Builtin::Tool(pina_engine::song::project::Fx::Reverb)),
            cx,
        )
    });
    let more = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| ws.more(aux).to_vec());
    let Unit::Reverb(r) = more(cx)[0] else {
        panic!("{:?}", more(cx))
    };
    assert_eq!((r.style, r.mix), (ReverbStyle::Hall, 1.0));
    assert_eq!(
        ws.read_with(cx, |ws, _| ws
            .fx_view
            .as_ref()
            .map(|v| (v.which, v.stacked))),
        Some((
            Builtin::Tool(pina_engine::song::project::Fx::Reverb),
            Some(0)
        ))
    );
    cx.run_until_parked();
    assert!(cx.debug_bounds("reverb-display").is_some());
    // A plate, as plates are, still all reverb; its decay turned up.
    click_on(
        cx,
        "reverb-style-plate",
        Modifiers::none(),
        MouseButton::Left,
    );
    let Unit::Reverb(r) = more(cx)[0] else {
        panic!()
    };
    assert_eq!((r.style, r.early, r.mix), (ReverbStyle::Plate, 0.0, 1.0));
    let before = r.decay_s;
    let knob = cx.debug_bounds("fx-knob-decay").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(40.)));
    let Unit::Reverb(r) = more(cx)[0] else {
        panic!()
    };
    assert!(r.decay_s > before, "{before} {}", r.decay_s);
    // Pre-delay in time with the song.
    click_on(cx, "reverb-sync-sync", Modifiers::none(), MouseButton::Left);
    let Unit::Reverb(r) = more(cx)[0] else {
        panic!()
    };
    assert!(r.predelay_beats.is_some(), "{r:?}");
    // A delay after it: ping-pong, its left time a step longer, frozen.
    ws.update(cx, |ws, cx| {
        ws.choose(
            add(Builtin::Tool(pina_engine::song::project::Fx::Delay)),
            cx,
        )
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("delay-display").is_some());
    click_on(
        cx,
        "delay-style-ping-pong",
        Modifiers::none(),
        MouseButton::Left,
    );
    let delay = |cx: &VisualTestContext| match more(cx)[1] {
        Unit::Delay(d) => d,
        u => panic!("{u:?}"),
    };
    assert_eq!(
        (delay(cx).style, delay(cx).cross, delay(cx).mix),
        (DelayStyle::PingPong, 1.0, 1.0)
    );
    let was = delay(cx).beats[0];
    let knob = cx.debug_bounds("fx-knob-left").unwrap().center();
    drag(cx, knob, knob - point(px(0.), px(12.)));
    assert!(delay(cx).beats[0] > was, "{:?}", delay(cx));
    click_on(
        cx,
        "delay-freeze-freeze",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(delay(cx).freeze);
    // Off from the title bar; named in the strip.
    click_on(cx, "fx-on", Modifiers::none(), MouseButton::Left);
    assert!(delay(cx).bypass);
    assert_eq!(
        ws.read_with(cx, |ws, _| (
            ws.stacked(aux, 0).map(|s| s.1),
            ws.stacked(aux, 1).map(|s| s.1)
        )),
        (Some("Reverb".to_string()), Some("Delay".to_string()))
    );
}

#[gpui::test]
fn the_reverbs_and_delays_pictures_are_dragged(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::Unit;
    let (ws, cx) = open(cx);
    let aux = ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.add_aux();
        });
        Channel::Aux(ws.session.project.auxes.len() - 1)
    });
    let add = |b| MenuAction::AddBuiltin {
        channel: aux,
        builtin: b,
    };
    ws.update(cx, |ws, cx| {
        ws.choose(
            add(Builtin::Tool(pina_engine::song::project::Fx::Reverb)),
            cx,
        )
    });
    cx.run_until_parked();
    let verb = |cx: &VisualTestContext| match ws.read_with(cx, |ws, _| ws.more(aux)[0]) {
        Unit::Reverb(r) => r,
        u => panic!("{u:?}"),
    };
    let was = verb(cx);
    let at = |cx: &mut VisualTestContext, sel: &'static str| cx.debug_bounds(sel).unwrap().center();
    // Its pre-delay later, its decay longer, its damping lower, by their points.
    let p = at(cx, "reverb-handle-predelay");
    drag(cx, p, p + point(px(30.), px(0.)));
    let p = at(cx, "reverb-handle-decay");
    drag(cx, p, p + point(px(60.), px(0.)));
    let p = at(cx, "reverb-handle-damping");
    drag(cx, p, p - point(px(60.), px(0.)));
    let now = verb(cx);
    assert!(now.predelay_ms > was.predelay_ms, "{now:?}");
    assert!(now.decay_s > was.decay_s, "{now:?}");
    assert!(now.damping_hz < was.damping_hz, "{now:?}");
    // The delay: its left time a step later, its feedback up, its low cut higher.
    ws.update(cx, |ws, cx| {
        ws.choose(
            add(Builtin::Tool(pina_engine::song::project::Fx::Delay)),
            cx,
        )
    });
    cx.run_until_parked();
    let delay = |cx: &VisualTestContext| match ws.read_with(cx, |ws, _| ws.more(aux)[1]) {
        Unit::Delay(d) => d,
        u => panic!("{u:?}"),
    };
    let was = delay(cx);
    let p = at(cx, "delay-handle-left");
    drag(cx, p, p + point(px(80.), px(0.)));
    let p = at(cx, "delay-handle-feedback");
    drag(cx, p, p - point(px(0.), px(20.)));
    let p = at(cx, "delay-handle-low-cut");
    drag(cx, p, p + point(px(40.), px(0.)));
    let now = delay(cx);
    assert!(now.beats[0] > was.beats[0], "{now:?}");
    assert!(now.feedback > was.feedback, "{now:?}");
    assert!(now.low_cut_hz > was.low_cut_hz, "{now:?}");
}

#[gpui::test]
fn each_tools_window_turns_and_drags(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::{DeEsser, FilterFx, Fx, Gate, Motion, Shaper, Stereo, Unit};
    let (ws, cx) = open(cx);
    let ch = Channel::Track(0);
    let add = |ws: &Entity<Workspace>, fx, cx: &mut VisualTestContext| {
        ws.update(cx, |ws, cx| {
            let builtin = Builtin::Tool(fx);
            ws.choose(
                MenuAction::AddBuiltin {
                    channel: ch,
                    builtin,
                },
                cx,
            )
        });
        cx.run_until_parked();
    };
    let last = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| *ws.more(ch).last().unwrap());
    let pull = |cx: &mut VisualTestContext, sel: &'static str, by: (f32, f32)| {
        let at = cx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{sel}"))
            .center();
        drag(cx, at, at + point(px(by.0), px(by.1)));
    };

    add(&ws, Fx::Filter, cx);
    pull(cx, "filter-handle", (80.0, -40.0));
    pull(cx, "filter-lfo-handle", (30.0, -30.0));
    pull(cx, "fx-knob-drive", (0.0, -30.0));
    click_on(cx, "filter-sync-1/4", Modifiers::none(), MouseButton::Left);
    let Unit::Filter(f) = last(cx) else { panic!() };
    let was = FilterFx::default();
    assert!(f.cutoff_hz > was.cutoff_hz, "{f:?}");
    assert!(f.resonance > was.resonance, "{f:?}");
    assert!(
        f.lfo_depth > was.lfo_depth && f.lfo_hz > was.lfo_hz,
        "{f:?}"
    );
    assert!(f.drive > was.drive && f.lfo_beats == 1.0, "{f:?}");

    add(&ws, Fx::Motion, cx);
    pull(cx, "motion-handle", (60.0, -20.0));
    let Unit::Motion(m) = last(cx) else { panic!() };
    assert!(m.rate_hz > Motion::default().rate_hz, "{m:?}");
    assert!(m.depth > Motion::default().depth, "{m:?}");
    // A flanger's own settings, what it was mixed at kept.
    click_on(
        cx,
        "motion-style-flanger",
        Modifiers::none(),
        MouseButton::Left,
    );
    let Unit::Motion(flanged) = last(cx) else {
        panic!()
    };
    assert_eq!(flanged.mix, m.mix);
    assert!(flanged.feedback != m.feedback, "{flanged:?}");

    add(&ws, Fx::Shaper, cx);
    pull(cx, "shaper-front", (0.0, -50.0));
    pull(cx, "shaper-tail", (0.0, 50.0));
    let Unit::Shaper(s) = last(cx) else { panic!() };
    assert!(s.attack > 0.3 && s.sustain < -0.3, "{s:?}");
    assert_eq!(
        Shaper {
            attack: 0.0,
            sustain: 0.0,
            ..s
        },
        Shaper::default()
    );

    add(&ws, Fx::Gate, cx);
    pull(cx, "gate-threshold", (0.0, 40.0));
    let Unit::Gate(g) = last(cx) else { panic!() };
    assert!(g.threshold_db < Gate::default().threshold_db, "{g:?}");

    add(&ws, Fx::DeEsser, cx);
    pull(cx, "de-esser-handle", (-40.0, 30.0));
    click_on(
        cx,
        "de-esser-listen-listen",
        Modifiers::none(),
        MouseButton::Left,
    );
    let Unit::DeEsser(d) = last(cx) else { panic!() };
    assert!(d.freq_hz < DeEsser::default().freq_hz, "{d:?}");
    assert!(
        d.threshold_db < DeEsser::default().threshold_db && d.listen,
        "{d:?}"
    );

    add(&ws, Fx::Stereo, cx);
    pull(cx, "stereo-width-high", (0.0, -40.0));
    pull(cx, "stereo-split-low", (40.0, 0.0));
    pull(cx, "stereo-mono-under", (60.0, 0.0));
    click_on(cx, "stereo-swap-on", Modifiers::none(), MouseButton::Left);
    let Unit::Stereo(s) = last(cx) else { panic!() };
    assert!(s.width[2] > 1.0 && s.mono_hz > 20.0, "{s:?}");
    assert!(
        s.crossovers[0] > Stereo::default().crossovers[0] && s.swap,
        "{s:?}"
    );
    // All six stacked, in the order they went on, and undone like any edit.
    assert_eq!(ws.read_with(cx, |ws, _| ws.more(ch).len()), 6);
    ws.update(cx, |ws, cx| ws.undo(cx));
    let Unit::Stereo(back) = last(cx) else {
        panic!()
    };
    assert!(!back.swap, "{back:?}");
}

#[gpui::test]
fn a_tools_knob_can_be_automated_and_modulated(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
        workspace::{Knob, Target},
    };
    use pina_engine::{
        dsp::modulate::ModTarget,
        song::project::{AutoPoint, Fx, Param, Unit},
    };
    let (ws, cx) = open(cx);
    let ch = Channel::Track(0);
    ws.update(cx, |ws, cx| {
        let builtin = Builtin::Tool(Fx::Filter);
        ws.choose(
            MenuAction::AddBuiltin {
                channel: ch,
                builtin,
            },
            cx,
        );
        let cutoff = Param::Tool(0, 0);
        let knob = Target::Stacked(ch, 0, Knob::Param(0));
        assert_eq!(ws.target_param(knob), Some((0, cutoff)));
        assert_eq!(ws.mod_key(knob), Some((0, ModTarget::Tool(0, 0))));
        assert_eq!(ws.param_name(0, cutoff), "Filter: Cutoff");
        assert_eq!(ws.value_text(0, cutoff, 0.5), "632 Hz");
        // Its lane offers every knob, under the effect's name.
        let lanes = ws.auto_params(0, 0);
        let names: Vec<&str> = lanes.iter().map(|i| i.0.as_str()).collect();
        assert!(
            names.contains(&"Filter") && names.contains(&"Env release"),
            "{names:?}"
        );
        // Automated, the knob, the window's picture and what plays all follow it.
        let own = ws.target(knob);
        ws.session.project.tracks[0]
            .automation
            .points_mut(cutoff)
            .push(AutoPoint::new(0, 0.5));
        assert_eq!(ws.target(knob), 0.5);
        assert_ne!(own, 0.5);
        let Some(Unit::Filter(f)) = ws.tool_heard(ch, 0) else {
            panic!()
        };
        assert!((f.cutoff_hz - 632.5).abs() < 1.0, "{f:?}");
    });
}

#[gpui::test]
fn the_last_four_windows_turn_and_drag(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::{Builtin, Channel},
        ui::menu::MenuAction,
    };
    use pina_engine::song::project::{Fx, Multiband, TapeStop, Unit};
    let (ws, cx) = open(cx);
    let ch = Channel::Track(0);
    ws.update(cx, |ws, _| ws.session.project.key = Some("A minor".into()));
    let add = |ws: &Entity<Workspace>, fx, cx: &mut VisualTestContext| {
        ws.update(cx, |ws, cx| {
            let builtin = Builtin::Tool(fx);
            ws.choose(
                MenuAction::AddBuiltin {
                    channel: ch,
                    builtin,
                },
                cx,
            )
        });
        cx.run_until_parked();
    };
    let last = |cx: &VisualTestContext| ws.read_with(cx, |ws, _| *ws.more(ch).last().unwrap());
    let pull = |cx: &mut VisualTestContext, sel: &'static str, by: (f32, f32)| {
        let at = cx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{sel}"))
            .center();
        drag(cx, at, at + point(px(by.0), px(by.1)));
    };
    let click = |cx: &mut VisualTestContext, sel: &'static str| {
        click_on(cx, sel, Modifiers::none(), MouseButton::Left);
    };

    add(&ws, Fx::Multiband, cx);
    pull(cx, "multiband-low", (0.0, 20.0));
    pull(cx, "multiband-split-low", (40.0, 0.0));
    // The middle band in hand: its knobs are the middle's.
    click(cx, "multiband-band-middle");
    pull(cx, "fx-knob-ratio", (0.0, -30.0));
    let Unit::Multiband(m) = last(cx) else {
        panic!()
    };
    let was = Multiband::default();
    assert!(m.threshold_db[0] < was.threshold_db[0], "{m:?}");
    assert!(m.crossovers[0] > was.crossovers[0], "{m:?}");
    assert!(
        m.ratio[1] > was.ratio[1] && m.ratio[0] == was.ratio[0],
        "{m:?}"
    );

    add(&ws, Fx::Pitch, cx);
    pull(cx, "pitch-handle", (120.0, 0.0));
    let Unit::Pitch(p) = last(cx) else { panic!() };
    assert!(p.semitones > 5.0 && p.semitones.fract() == 0.0, "{p:?}");
    click(cx, "pitch-leap--7");
    let Unit::Pitch(p) = last(cx) else { panic!() };
    assert_eq!(p.semitones, -7.0);

    add(&ws, Fx::TapeStop, cx);
    pull(cx, "tape-stop-handle", (-40.0, 0.0));
    click(cx, "tape-stop-stopped");
    let Unit::TapeStop(t) = last(cx) else {
        panic!()
    };
    assert!(t.stop_beats > TapeStop::default().stop_beats, "{t:?}");
    assert_eq!(t.stop, 1.0);

    // It comes on the song's key: A minor has no C#, and clicking the key adds it.
    add(&ws, Fx::Tune, cx);
    let Unit::Tune(t) = last(cx) else { panic!() };
    assert!(t.notes[9] && !t.notes[1], "{:?}", t.notes);
    click(cx, "tune-note-1");
    let Unit::Tune(t) = last(cx) else { panic!() };
    assert!(t.notes[1]);
    click(cx, "tune-notes-every-note");
    let Unit::Tune(t) = last(cx) else { panic!() };
    assert_eq!(t.notes, [true; 12]);
    click(cx, "tune-notes-the-song's-key");
    let Unit::Tune(t) = last(cx) else { panic!() };
    assert!(!t.notes[1]);
}

#[gpui::test]
fn every_built_in_is_in_the_automation_list_and_plays_it(cx: &mut TestAppContext) {
    use crate::{
        mix::strip::Channel,
        workspace::{CompField, Knob, SatField, Target},
    };
    use pina_engine::song::project::{
        AutoPoint, Comp, Instrument, OWN_COMP, OWN_SAT, Param, Sampler, Saturator, Unit, VOICE,
    };
    let (ws, cx) = open(cx);
    let ch = Channel::Track(0);
    ws.update(cx, |ws, _| {
        let t = &mut ws.session.project.tracks[0];
        t.comp = Some(Comp::default());
        t.sat = Some(Saturator::default());
        t.more = vec![Unit::Comp(Comp::default()), Unit::Sat(Saturator::default())];
        t.instrument = Some(Instrument::Sampler(Sampler::default()));
        let items = ws.auto_params(0, 0);
        let names: Vec<&str> = items.iter().map(|i| i.0.as_str()).collect();
        for want in [
            "Compressor",
            "Threshold",
            "Saturator",
            "Band 1 drive",
            "Voice",
            "Cutoff",
        ] {
            assert!(names.contains(&want), "{want}: {names:?}");
        }
        // Stacked ones under their own names, and only bands that exist.
        assert!(
            names.contains(&"Compressor 2") && !names.contains(&"Band 2 drive"),
            "{names:?}"
        );

        // The channel's own compressor: its knob, its name, what it reads, and it follows.
        let knob = Target::Comp(ch, CompField::Threshold);
        let param = Param::Tool(OWN_COMP, 0);
        assert_eq!(ws.target_param(knob), Some((0, param)));
        assert_eq!(ws.param_name(0, param), "Compressor: Threshold");
        assert_eq!(ws.value_text(0, param, 0.5), "-30.0 dB");
        ws.session.project.tracks[0]
            .automation
            .points_mut(param)
            .push(AutoPoint::new(0, 0.5));
        assert_eq!(ws.target(knob), 0.5);

        // The saturator's first drive, from its plain knob and from its band's.
        assert_eq!(
            ws.target_param(Target::Sat(ch, SatField::Drive)),
            Some((0, Param::Tool(OWN_SAT, 0)))
        );
        // A stacked compressor's knob, now it has a table.
        let stacked = Target::Stacked(ch, 0, Knob::Comp(CompField::Ratio));
        assert_eq!(ws.target_param(stacked), Some((0, Param::Tool(0, 1))));
        assert!(ws.mod_key(stacked).is_some());

        // The Sampler's voice.
        let cutoff = Target::Sampler {
            track: 0,
            field: crate::editors::sampler::Field::Cutoff,
        };
        let knob = VOICE.iter().position(|k| k.name == "Cutoff").unwrap() as u8;
        assert_eq!(ws.target_param(cutoff), Some((0, Param::Voice(knob))));
        assert_eq!(ws.param_name(0, Param::Voice(knob)), "Cutoff");
    });
}
