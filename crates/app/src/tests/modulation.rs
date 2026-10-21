//! Modulation: modulators added and mapped, Learn, stacks sharing them, their settings automated.

use super::*;

#[gpui::test]
fn modulators_are_added_mapped_by_dragging_and_their_depth_set(cx: &mut TestAppContext) {
    use pina_engine::dsp::modulate::{ModKind, ModTarget};
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.select_track(0);
        ws.show_bottom(Bottom::Modulation, cx);
    });
    cx.run_until_parked();
    let mods = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].modulators.clone())
    };
    click_on(cx, "add-mod-LFO", Modifiers::none(), MouseButton::Left);
    assert!(matches!(mods(cx)[0].kind, ModKind::Lfo { .. }));
    // Its ↗ dragged onto the header's pan knob: the pan follows it, by its usual depth.
    let handle = cx.debug_bounds("mod-0-map").unwrap().center();
    let knob = cx.debug_bounds("track-pan-0").unwrap();
    let dial = point(knob.left() + px(12.), knob.center().y);
    drag(cx, handle, dial);
    let routes: Vec<_> = mods(cx)[0]
        .routes
        .iter()
        .map(|r| (r.target, r.depth))
        .collect();
    assert_eq!(routes, [(ModTarget::Pan, 0.8)]);
    // Its route dragged right: deeper (as far as pan goes, 2).
    let route = cx.debug_bounds("mod-0-route-Pan").unwrap().center();
    drag(cx, route, route + point(px(40.), px(0.)));
    let depth = mods(cx)[0].routes[0].depth;
    assert!((depth - 1.3).abs() < 0.01, "{depth}");
    // Picked, dragging the knob it moves sets its depth, not the pan.
    let pan = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].pan);
    click_on(cx, "mod-card-0", Modifiers::none(), MouseButton::Left);
    drag(cx, dial, dial - point(px(0.), px(40.)));
    let after = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].pan);
    assert_eq!(after, pan);
    assert!(
        mods(cx)[0].routes[0].depth > 1.6,
        "{}",
        mods(cx)[0].routes[0].depth
    );
    // Steps: painted in its picture, the first step set low.
    click_on(cx, "add-mod-Steps", Modifiers::none(), MouseButton::Left);
    let pic = cx.debug_bounds("mod-1-visual").unwrap();
    // Clear of its rounded corner: the first step, at the bottom.
    let low = point(pic.left() + px(10.), pic.bottom() - px(1.5));
    drag(cx, low, low);
    let ModKind::Steps { values, .. } = &mods(cx)[1].kind else {
        panic!("not steps")
    };
    assert_eq!(values[0], 0.0);
    // Taken off: the LFO and its route.
    click_on(cx, "mod-0-remove", Modifiers::none(), MouseButton::Left);
    let left = mods(cx);
    assert_eq!(left.len(), 1);
    assert!(matches!(left[0].kind, ModKind::Steps { .. }));
}

#[gpui::test]
fn a_modulator_learns_the_plugin_parameter_turned_next_and_lists_every_one(
    cx: &mut TestAppContext,
) {
    use pina_engine::{
        dsp::modulate::{ModKind, ModTarget},
        song::commands::{ChannelRef, Command},
        song::project::{Format, PluginRef, Which},
    };
    let (ws, cx) = open(cx);
    // Apple's low-pass filter, on every Mac, in the first track's first insert.
    ws.update(cx, |ws, cx| {
        ws.select_track(0);
        ws.edit(cx, |ws| {
            let insert = Command::SetInsert {
                channel: ChannelRef::Track(0),
                index: 0,
                plugin: Some(PluginRef {
                    format: Format::Au,
                    class: "AULowpass".into(),
                    uid: Some("aufx lpas appl".into()),
                    ..Default::default()
                }),
            };
            let dir = ws.session.dir.clone();
            pina_engine::song::commands::apply(&mut ws.session.project, &dir, insert).unwrap();
        });
        ws.show_bottom(Bottom::Modulation, cx);
    });
    cx.run_until_parked();
    click_on(cx, "add-mod-LFO", Modifiers::none(), MouseButton::Left);
    // Every one of its parameters is offered, named with its plugin.
    let menu = ws.read_with(cx, |ws, _| ws.target_menu(0, 0));
    assert!(
        menu.iter()
            .any(|(label, _)| label == "AULowpass · Cutoff Frequency"),
        "{:?}",
        menu.iter().map(|m| &m.0).collect::<Vec<_>>()
    );
    // Its names are asked of it once, not each frame (an Audio Unit answers one call each).
    ws.read_with(cx, |ws, _| {
        let id = ws.session.project.tracks[0].id;
        let (a, b) = (
            ws.param_names(id, Which::Insert(0)),
            ws.param_names(id, Which::Insert(0)),
        );
        assert!(!a.is_empty() && std::rc::Rc::ptr_eq(&a, &b));
    });
    // Learning: its cutoff turned as if in the plugin's own window, and that's where it goes.
    click_on(cx, "mod-0-learn", Modifiers::none(), MouseButton::Left);
    assert!(ws.read_with(cx, |ws, _| ws.mod_learn.is_some()));
    let id = ws.read_with(cx, |ws, _| ws.session.project.tracks[0].id);
    ws.update(cx, |ws, _| {
        let now = ws.player.param_value(id, Which::Insert(0), 0);
        ws.player
            .set_param(id, Which::Insert(0), 0, if now > 0.5 { 0.2 } else { 0.8 });
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.run_until_parked();
    let routes: Vec<ModTarget> = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].modulators[0]
            .routes
            .iter()
            .map(|r| r.target)
            .collect()
    });
    assert_eq!(routes, [ModTarget::Plugin(Which::Insert(0), 0)]);
    assert!(ws.read_with(cx, |ws, _| ws.mod_learn.is_none()));
    let kind = ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[0].modulators[0].kind.clone()
    });
    assert!(matches!(kind, ModKind::Lfo { .. }));
}

#[gpui::test]
fn an_automation_lane_takes_a_modulator(cx: &mut TestAppContext) {
    use pina_engine::dsp::modulate::ModTarget;
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.select_track(0);
        ws.show_automation = true;
        cx.notify();
    });
    cx.run_until_parked();
    // The keys' volume lane: a new LFO moving it, from its right-click.
    click_on(cx, "auto-lane-0-0", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "New LFO");
    let routes = |cx: &mut VisualTestContext| {
        ws.read_with(cx, |ws, _| {
            let mods = &ws.session.project.tracks[0].modulators;
            mods.iter().map(|m| m.routes.len()).collect::<Vec<_>>()
        })
    };
    assert_eq!(routes(cx), [1]);
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks[0].modulators[0]
            .routes[0]
            .target),
        ModTarget::Volume
    );
    // Ticked in the menu: picking it again lets the volume go.
    click_on(cx, "auto-lane-0-0", Modifiers::none(), MouseButton::Right);
    let ticked = ws.read_with(cx, |ws, _| {
        ws.menu
            .as_ref()
            .is_some_and(|m| m.items.iter().any(|(l, _)| l == "✓ LFO 1"))
    });
    assert!(ticked);
    pick(cx, &ws, "LFO 1");
    assert_eq!(routes(cx), [0]);
}

#[gpui::test]
fn a_stacks_shared_modulator_is_on_every_track_in_it(cx: &mut TestAppContext) {
    use crate::editors::modulation::ModField;
    use pina_engine::dsp::modulate::ModKind;
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.add_stack(&[0, 2]);
        });
        ws.select_track(0);
        ws.show_bottom(Bottom::Modulation, cx);
    });
    cx.run_until_parked();
    click_on(cx, "add-mod-shared", Modifiers::none(), MouseButton::Left);
    pick(cx, &ws, "New Macro");
    let shared = ModKind::Shared { modulator: 0 };
    ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        assert_eq!(p.tracks[0].modulators[0].kind, shared);
        // The drums moved up under the keys.
        assert_eq!(p.tracks[1].modulators[0].kind, shared);
        assert!(p.tracks[2].modulators.is_empty(), "not in the stack");
        assert_eq!(ws.mod_name(1, 0), "Stack 1 · Macro 1");
    });
    // Turned on one track, it's turned on every one.
    ws.update(cx, |ws, _| ws.set_mod_field(0, 0, ModField::Value, 0.7));
    assert!((ws.read_with(cx, |ws, _| ws.mod_field(1, 0, ModField::Value)) - 0.7).abs() < 1e-4);
    // Taken off one track's card: off the stack, and every track in it.
    ws.update(cx, |ws, cx| ws.remove_modulator(1, 0, cx));
    ws.read_with(cx, |ws, _| {
        let p = &ws.session.project;
        assert!(p.auxes.iter().all(|a| a.modulators.is_empty()));
        assert!(p.tracks.iter().all(|t| t.modulators.is_empty()));
    });
}

#[gpui::test]
fn a_modulators_knob_is_automated_from_its_right_click(cx: &mut TestAppContext) {
    use pina_engine::{
        dsp::modulate::ModParam,
        song::project::{AutoPoint, Param},
    };
    let (ws, cx) = open(cx);
    ws.update(cx, |ws, cx| {
        ws.select_track(0);
        ws.show_bottom(Bottom::Modulation, cx);
    });
    cx.run_until_parked();
    click_on(cx, "add-mod-Macro", Modifiers::none(), MouseButton::Left);
    // Right-click its value: Automate it. A lane, a point where it is, and the lane showing.
    click_on(cx, "mod-0-Value", Modifiers::none(), MouseButton::Right);
    pick(cx, &ws, "Automate Value");
    let param = Param::Mod(0, ModParam::Value);
    let (points, lanes, shown) = ws.read_with(cx, |ws, _| {
        (
            ws.session.project.tracks[0]
                .automation
                .points(param)
                .to_vec(),
            ws.auto_lanes(0),
            ws.show_automation,
        )
    });
    assert_eq!(points.len(), 1);
    assert!(lanes.contains(&param) && shown, "{lanes:?}");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.param_name(0, param)),
        "Macro 1: Value"
    );
    // Where its automation has it, its knob shows it.
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            *ws.session.project.tracks[0].automation.points_mut(param) =
                vec![AutoPoint::new(0, 0.6)];
        })
    });
    let shown = ws.read_with(cx, |ws, _| {
        ws.mod_field(0, 0, crate::editors::modulation::ModField::Value)
    });
    assert!((shown - 0.6).abs() < 1e-4, "{shown}");
}
