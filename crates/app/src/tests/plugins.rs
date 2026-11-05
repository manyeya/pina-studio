//! The Plugins page: opened from the Browser and back, its filters, a plugin picked and put on a
//! track (Add, double-click, Enter, dragged out onto one), keys, collections, Health and the
//! checkup, and the picker a mixer slot opens.

use pina_engine::song::project::{Format, Which};

use super::*;
use crate::sounds::{
    catalog::{Cat, Scope},
    plugins::{Fact, FileFacts, PluginOp, Shown, subcategory},
};

const DELAY: &str = "plugin:apple/audelay";
const GRAPHIC: &str = "plugin:apple/augraphiceq";
const BELL: &str = "plugin:maker/bell fm";

fn plugin(format: Format, path: &str, class: &str, vendor: &str, uid: Option<&str>) -> PluginInfo {
    PluginInfo {
        format,
        path: path.into(),
        class: class.into(),
        uid: uid.map(String::from),
        vendor: vendor.into(),
        instrument: class == "Bell FM",
        categories: Vec::new(),
    }
}

/// The demo with a few plugins installed: Apple's delay and graphic EQ (one bundle), a synth in
/// two formats, and a suite, one of whose plugins won't load.
fn open_plugins(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (ws, cx) = open(cx);
    let apple = "/System/Library/Components/CoreAudio.component";
    ws.update(cx, |ws, cx| {
        ws.plugins = Some(vec![
            plugin(
                Format::Au,
                apple,
                "AUDelay",
                "Apple",
                Some("aufx dely appl"),
            ),
            plugin(
                Format::Au,
                apple,
                "AUGraphicEQ",
                "Apple",
                Some("aufx greq appl"),
            ),
            plugin(Format::Vst3, "/P/Bell.vst3", "Bell FM", "Maker", None),
            plugin(Format::Au, "/P/Bell.component", "Bell FM", "Maker", None),
            plugin(Format::Vst3, "/P/Suite.vst3", "Suite Half", "Maker", None),
            plugin(Format::Vst3, "/P/Suite.vst3", "Suite Whole", "Maker", None),
            plugin(Format::Vst3, "/P/Broken.vst3", "Broken", "Maker", None),
        ]);
        ws.catalog_plugins();
        ws.track = 1;
        cx.notify();
    });
    cx.run_until_parked();
    (ws, cx)
}

/// The names on the page's cards, in order.
fn cards(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    ws.update(cx, |ws, cx| {
        let cards = ws.plugin_cards(cx);
        cards
            .iter()
            .map(|c| ws.catalog.devices[c.device()].name.clone())
            .collect()
    })
}

/// A click as a hand makes one: a frame drawn between pressing and letting go.
fn press(cx: &mut VisualTestContext, name: &'static str) {
    let at = cx
        .debug_bounds(name)
        .unwrap_or_else(|| panic!("no {name} on screen"))
        .center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn device(ws: &Entity<Workspace>, cx: &VisualTestContext, key: &str) -> usize {
    ws.read_with(cx, |ws, _| ws.catalog.device(key).expect(key))
}

fn inserts(ws: &Entity<Workspace>, cx: &VisualTestContext, t: usize) -> Vec<String> {
    ws.read_with(cx, |ws, _| {
        ws.session.project.tracks[t]
            .inserts
            .iter()
            .map(|p| p.class.clone())
            .collect()
    })
}

#[gpui::test]
fn the_browser_expands_into_the_plugins_page_and_back(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    ws.update(cx, |ws, cx| {
        ws.show_browser = true;
        ws.catalog.scope = Scope::Effects;
        cx.notify();
    });
    cx.run_until_parked();
    // Typed in the Browser, then expanded: the page has the same search and tab.
    let field = ws.read_with(cx, |ws, _| ws.catalog.search.clone());
    field.update(cx, |i, cx| i.set_text("echo", cx));
    click_on(cx, "plugins-page", Modifiers::none(), MouseButton::Left);
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Plugins);
    assert_eq!(cards(&ws, cx), ["AUDelay", "Delay"]);
    field.update(cx, |i, cx| i.set_text("", cx));
    // What each is, within its category.
    let kinds: Vec<_> = [DELAY, GRAPHIC, BELL]
        .map(|k| {
            let d = device(&ws, cx, k);
            ws.read_with(cx, |ws, _| subcategory(&ws.catalog.devices[d]))
        })
        .into();
    assert_eq!(kinds, [Some("Echo"), Some("Graphic"), Some("FM")]);
    // A category, then Audio Units turned off: the delay's only format.
    click_on(
        cx,
        "plugins-cat-Delay",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(cards(&ws, cx), ["AUDelay", "Delay"]);
    click_on(
        cx,
        "plugins-format-AU",
        Modifiers::none(),
        MouseButton::Left,
    );
    // Pina Studio's own has no format to filter out; the plugins alone, nothing's left.
    assert_eq!(cards(&ws, cx), ["Delay"]);
    click_on(
        cx,
        "plugins-source-Plugins",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(cards(&ws, cx).is_empty());
    click_on(cx, "plugins-loosen", Modifiers::none(), MouseButton::Left);
    assert!(cards(&ws, cx).len() > 2);
    // Instruments: the synth, once for its two formats; Pina Studio's own have none of it.
    click_on(
        cx,
        "plugins-kind-Instruments",
        Modifiers::none(),
        MouseButton::Left,
    );
    let synths = cards(&ws, cx);
    assert_eq!(synths.iter().filter(|n| *n == "Bell FM").count(), 1);
    assert!(!synths.contains(&"AUDelay".to_string()));
    click_on(
        cx,
        "plugins-source-Built in",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert!(!cards(&ws, cx).contains(&"Bell FM".to_string()));
    // Compact: the tracks, the Browser beside them on the page's tab.
    click_on(cx, "plugins-compact", Modifiers::none(), MouseButton::Left);
    let (view, browser, scope) =
        ws.read_with(cx, |ws, _| (ws.view, ws.show_browser, ws.catalog.scope));
    assert_eq!(
        (view, browser, scope),
        (View::Arrange, true, Scope::Instruments)
    );
}

#[gpui::test]
fn a_plugin_picked_goes_on_a_track(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    cx.simulate_keystrokes("ctrl-alt-d");
    // A click picks it: its details, nothing added yet.
    press(cx, "plugin-card-plugin:apple/audelay");
    let picked = ws.read_with(cx, |ws, _| ws.plugin_page.picked.clone());
    assert_eq!(picked.as_deref(), Some(DELAY));
    assert!(inserts(&ws, cx, 1).is_empty());
    // Add puts it on the selected track; so does a double-click, and Enter.
    press(cx, "plugin-add");
    assert_eq!(inserts(&ws, cx, 1), ["AUDelay"]);
    let card = cx
        .debug_bounds("plugin-card-plugin:apple/augraphiceq")
        .unwrap()
        .center();
    double_click(cx, card);
    assert_eq!(inserts(&ws, cx, 1), ["AUDelay", "AUGraphicEQ"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        inserts(&ws, cx, 1),
        ["AUDelay", "AUGraphicEQ", "AUGraphicEQ"]
    );
    // The arrows pick the next; On a New Track makes one for it.
    cx.simulate_keystrokes("up");
    let picked = ws.read_with(cx, |ws, _| ws.plugin_page.picked.clone());
    assert_eq!(picked.as_deref(), Some(DELAY));
    let tracks = ws.read_with(cx, |ws, _| ws.session.project.tracks.len());
    press(cx, "plugin-add-new");
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.session.project.tracks.len()),
        tracks + 1
    );
    assert_eq!(
        ws.read_with(cx, |ws, _| ws.catalog.uses.get(DELAY).copied()),
        Some(2)
    );
}

#[gpui::test]
fn a_plugin_dragged_out_of_the_page_lands_on_a_track(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    cx.simulate_keystrokes("ctrl-alt-d");
    let card = cx
        .debug_bounds("plugin-card-plugin:apple/audelay")
        .unwrap()
        .center();
    cx.simulate_mouse_down(card, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    // Out of the page: the tracks come back, the Browser beside them.
    cx.simulate_mouse_move(
        card + point(px(120.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Arrange);
    let lead = spot(&ws, cx, 960, 1);
    cx.simulate_mouse_move(lead, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(lead, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(inserts(&ws, cx, 1), ["AUDelay"]);
}

#[gpui::test]
fn keys_put_plugins_on_the_track(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    cx.simulate_keystrokes("ctrl-alt-d");
    click_on(
        cx,
        "plugins-view-Keys",
        Modifiers::none(),
        MouseButton::Left,
    );
    // A key, then a plugin.
    click_on(cx, "key-f13", Modifiers::none(), MouseButton::Left);
    press(cx, "plugin-card-plugin:apple/audelay");
    // A plugin, then a key pressed.
    press(cx, "plugin-card-plugin:apple/augraphiceq");
    cx.simulate_keystrokes("f14");
    let keys = ws.read_with(cx, |ws, _| ws.catalog.keys.clone());
    assert_eq!(
        keys,
        [("f13".into(), DELAY.into()), ("f14".into(), GRAPHIC.into())]
    );
    // A plugin's on one key: put on another, it moves.
    cx.simulate_keystrokes("f15");
    let keys = ws.read_with(cx, |ws, _| ws.catalog.keys.clone());
    assert_eq!(
        keys,
        [("f13".into(), DELAY.into()), ("f15".into(), GRAPHIC.into())]
    );
    // A plugin dragged onto a key: on Keys, the page stays.
    let card = cx
        .debug_bounds("plugin-card-plugin:apple/audelay")
        .unwrap()
        .center();
    let key = cx.debug_bounds("key-f16").unwrap().center();
    drag(cx, card, key);
    assert_eq!(ws.read_with(cx, |ws, _| ws.view), View::Plugins);
    let keys = ws.read_with(cx, |ws, _| ws.catalog.keys.clone());
    assert_eq!(
        keys,
        [("f15".into(), GRAPHIC.into()), ("f16".into(), DELAY.into())]
    );
    // Away from the page, the key puts it on the selected track; nothing went on while mapping.
    cx.simulate_keystrokes("ctrl-alt-d");
    assert!(inserts(&ws, cx, 1).is_empty());
    cx.simulate_keystrokes("f16");
    assert_eq!(inserts(&ws, cx, 1), ["AUDelay"]);
}

#[gpui::test]
fn health_and_collections_narrow_the_page(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    ws.update(cx, |ws, _| {
        let failed = Fact {
            failed: Some("it stopped while opening".into()),
            ..Default::default()
        };
        ws.plugin_page
            .facts
            .insert("plugin:maker/broken".into(), failed);
    });
    cx.simulate_keystrokes("ctrl-alt-d");
    click_on(
        cx,
        "plugins-health-WontLoad",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(cards(&ws, cx), ["Broken"]);
    click_on(
        cx,
        "plugins-health-WontLoad",
        Modifiers::none(),
        MouseButton::Left,
    );
    let (delay, graphic) = (device(&ws, cx, DELAY), device(&ws, cx, GRAPHIC));
    ws.update(cx, |ws, cx| {
        ws.plugin_op(PluginOp::NewCollection(Some(delay)), cx);
        ws.plugin_page.ask = None;
        ws.plugin_op(PluginOp::ToggleCollection(graphic, 0), cx);
        ws.plugin_op(PluginOp::ToggleCollection(delay, 0), cx);
    });
    let c = ws.read_with(cx, |ws, _| ws.catalog.collections[0].clone());
    assert_eq!(
        (c.name.as_str(), c.devices),
        ("Collection 1", vec![GRAPHIC.to_string()])
    );
    click_on(
        cx,
        "plugins-collection-0",
        Modifiers::none(),
        MouseButton::Left,
    );
    assert_eq!(cards(&ws, cx), ["AUGraphicEQ"]);
    ws.update(cx, |ws, cx| ws.plugin_op(PluginOp::DeleteCollection(0), cx));
    assert!(ws.read_with(cx, |ws, _| ws.catalog.collections.is_empty()));
    assert!(ws.read_with(cx, |ws, _| ws.plugin_page.shown == Shown::Grid));
}

#[gpui::test]
fn a_browser_row_clicked_goes_on_the_track(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    ws.update(cx, |ws, cx| {
        ws.show_browser = true;
        ws.catalog.scope = crate::sounds::catalog::Scope::Effects;
        cx.notify();
    });
    cx.run_until_parked();
    press(cx, "device-plugin:apple/augraphiceq");
    let inserts = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].inserts.clone());
    assert_eq!(
        inserts.last().map(|p| p.class.as_str()),
        Some("AUGraphicEQ")
    );
    assert!(ws.read_with(cx, |ws, _| ws.drag.is_none()));
}

#[gpui::test]
fn the_checkup_finds_what_can_go(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    // The checkup: what's spare is a second format, and a file only plugins that won't load
    // use. A bundle a plugin that loads is in stays; macOS's own are never spare.
    let checkup = ws.update(cx, |ws, _| {
        let size = |size| FileFacts {
            size,
            ..Default::default()
        };
        let files = &mut ws.plugin_page.files;
        files.insert("/P/Bell.vst3".into(), size(100));
        files.insert("/P/Bell.component".into(), size(50));
        files.insert("/P/Suite.vst3".into(), size(30));
        files.insert("/P/Broken.vst3".into(), size(7));
        let failed = Fact {
            failed: Some("it stopped while opening".into()),
            ..Default::default()
        };
        for key in [DELAY, "plugin:maker/suite half", "plugin:maker/broken"] {
            ws.plugin_page.facts.insert(key.into(), failed.clone());
        }
        let bell = &ws.catalog.devices[ws.catalog.device(BELL).unwrap()];
        (ws.checkup(), bell.formats[1].path.clone())
    });
    let (c, second) = checkup;
    assert_eq!(c.total, 187);
    let mut spare = c.spare.clone();
    let mut want = vec![PathBuf::from("/P/Broken.vst3"), second.clone()];
    spare.sort();
    want.sort();
    assert_eq!(spare, want);
    let second = if second.ends_with("Bell.vst3") {
        100
    } else {
        50
    };
    assert_eq!(c.spare_size, 7 + second);
    assert_eq!(c.wont_load.len(), 3);
}

#[gpui::test]
fn a_slot_opens_the_picker(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    ws.update_in(cx, |ws, window, cx| {
        ws.open_mini(
            Channel::Track(1),
            Which::Insert(0),
            point(px(240.), px(300.)),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    // Typed into, the first found goes in with Enter.
    cx.simulate_input("audelay");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let inserts = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].inserts.clone());
    assert_eq!(inserts.first().map(|p| p.class.as_str()), Some("AUDelay"));
    assert!(ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_none()));
    // Clicked, a row goes in too.
    ws.update_in(cx, |ws, window, cx| {
        ws.open_mini(
            Channel::Track(1),
            Which::Insert(1),
            point(px(240.), px(300.)),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    cx.simulate_input("graphic");
    cx.run_until_parked();
    press(cx, "mini-0");
    let inserts = ws.read_with(cx, |ws, _| ws.session.project.tracks[1].inserts.clone());
    assert_eq!(
        inserts.get(1).map(|p| p.class.as_str()),
        Some("AUGraphicEQ")
    );
}

#[gpui::test]
fn the_slots_picker_narrows_by_where_from_kind_maker_star_and_recent(cx: &mut TestAppContext) {
    use crate::mix::strip::Channel;
    let (ws, cx) = open_plugins(cx);
    cx.simulate_keystrokes("x");
    let rows = |cx: &mut VisualTestContext| {
        ws.update(cx, |ws, cx| {
            let rows = ws.mini_rows(cx);
            rows.into_iter()
                .map(|d| ws.catalog.devices[d].clone())
                .collect::<Vec<_>>()
        })
    };
    let open_slot = |cx: &mut VisualTestContext| {
        let n = ws.read_with(cx, |ws, _| ws.inserts(Channel::Track(1)).len());
        let slot: &'static str = Box::leak(format!("slot-Track(1)-Insert({n})").into_boxed_str());
        press(cx, slot);
        assert!(ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_some()));
    };
    open_slot(cx);
    let all = rows(cx);
    assert!(
        all.iter().all(|d| !d.instrument()),
        "a slot for effects lists effects"
    );
    assert!(all.iter().any(|d| d.own.is_some()) && all.iter().any(|d| d.own.is_none()));

    // Where from: only plugins, then only what's built in.
    press(cx, "mini-from-Plugins");
    assert!(rows(cx).iter().all(|d| d.own.is_none()));
    press(cx, "mini-from-Built in");
    let built_in = rows(cx);
    assert!(built_in.iter().all(|d| d.own.is_some()));
    assert_eq!(built_in.len(), 16, "every built-in effect");

    // A category, then a kind in it.
    press(cx, "mini-cat-Dynamics");
    let dynamics = rows(cx);
    assert!(dynamics.len() > 1 && dynamics.iter().all(|d| d.cat == Cat::Dynamics));
    let kind = subcategory(&dynamics[0]).expect("a dynamics kind");
    let chip: &'static str = Box::leak(format!("mini-kind-{kind}").into_boxed_str());
    press(cx, chip);
    let kinds = rows(cx);
    assert!(kinds.len() < dynamics.len());
    assert!(kinds.iter().all(|d| subcategory(d) == Some(kind)));

    // Clear lets go of all of it.
    press(cx, "mini-clear");
    assert_eq!(rows(cx).len(), all.len());

    // A maker.
    press(cx, "mini-maker-Apple");
    let apple: Vec<String> = rows(cx).into_iter().map(|d| d.key).collect();
    assert_eq!(apple.len(), 2);
    assert!(apple.contains(&DELAY.to_string()) && apple.contains(&GRAPHIC.to_string()));

    // Starred from its row, then only the starred.
    press(cx, "mini-star-0");
    let starred = rows(cx)[0].key.clone();
    assert!(ws.read_with(cx, |ws, _| ws.catalog.favourites.contains(&starred)));
    assert!(
        ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_some()),
        "starring keeps it open"
    );
    press(cx, "mini-clear");
    press(cx, "mini-starred");
    assert_eq!(
        rows(cx).iter().map(|d| d.key.clone()).collect::<Vec<_>>(),
        [starred]
    );

    // Nothing found says so, and lets go of what narrowed it.
    cx.simulate_input("zzzz");
    cx.run_until_parked();
    assert!(rows(cx).is_empty());
    press(cx, "mini-clear-all");
    assert_eq!(rows(cx).len(), all.len());

    // Past the rows that show, the keyboard's pick is scrolled to, and goes in.
    for _ in 0..8 {
        cx.simulate_keystrokes("down");
    }
    cx.run_until_parked();
    assert!(cx.debug_bounds("mini-8").is_some(), "scrolled to");
    assert!(
        cx.debug_bounds("mini-0").is_none(),
        "only the rows that show are drawn"
    );
    let eighth = rows(cx)[8].key.clone();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(ws.read_with(cx, |ws, _| ws.plugin_page.mini.is_none()));

    // What went in last is Recent, first.
    open_slot(cx);
    press(cx, "mini-recent");
    assert_eq!(rows(cx)[0].key, eighth);
}

#[gpui::test]
fn the_same_plugin_goes_on_twice(cx: &mut TestAppContext) {
    let (ws, cx) = open_plugins(cx);
    ws.update(cx, |ws, cx| {
        ws.show_browser = true;
        ws.catalog.scope = crate::sounds::catalog::Scope::Effects;
        cx.notify();
    });
    cx.run_until_parked();
    press(cx, "device-plugin:apple/augraphiceq");
    cx.run_until_parked();
    press(cx, "device-plugin:apple/augraphiceq");
    cx.run_until_parked();
    let (inserts, loaded, status) = ws.read_with(cx, |ws, _| {
        let t = &ws.session.project.tracks[1];
        let loaded = (0..t.inserts.len())
            .filter(|&i| {
                ws.player
                    .plugin(t.id, pina_engine::audio::player::Which::Insert(i))
                    .is_some()
            })
            .count();
        (t.inserts.len(), loaded, ws.status.clone())
    });
    assert_eq!((inserts, loaded), (2, 2), "{status:?}");
}

#[gpui::test]
fn plugins_added_and_taken_out_leave_the_others_running_as_they_are(cx: &mut TestAppContext) {
    use pina_engine::audio::player::Which;
    let (ws, cx) = open_plugins(cx);
    ws.update(cx, |ws, cx| {
        ws.show_browser = true;
        ws.catalog.scope = crate::sounds::catalog::Scope::Effects;
        cx.notify();
    });
    cx.run_until_parked();
    press(cx, "device-plugin:apple/augraphiceq");
    cx.run_until_parked();
    // Turned in its own window (not in the song): a band down.
    let (id, param) = ws.read_with(cx, |ws, _| {
        let id = ws.session.project.tracks[1].id;
        let p = ws.player.plugin(id, Which::Insert(0)).unwrap();
        let param = p.params()[0].id;
        p.set_param(param, 0.123);
        (id, param)
    });
    let value = |cx: &VisualTestContext, at: usize| {
        ws.read_with(cx, |ws, _| {
            ws.player
                .plugin(id, Which::Insert(at))
                .map(|p| p.param_value(param))
        })
    };
    // Another of it, then a different one before them both, then that one out again.
    press(cx, "device-plugin:apple/augraphiceq");
    cx.run_until_parked();
    assert!(
        (value(cx, 0).unwrap() - 0.123).abs() < 1e-3,
        "{:?}",
        value(cx, 0)
    );
    ws.update(cx, |ws, cx| {
        let delay = pina_engine::song::project::PluginRef {
            format: pina_engine::song::project::Format::Au,
            class: "AUDelay".into(),
            uid: Some("aufx dely appl".into()),
            ..Default::default()
        };
        ws.edit(cx, |ws| {
            let mut chain = ws.session.project.tracks[1].chain_mut();
            chain.set_insert(2, Some(delay)).unwrap();
            let n = chain.inserts.len();
            let order = pina_engine::song::project::chain(chain.order, n, chain.more.len());
            // The plugin just added: the last one, which goes in before the limiter's place.
            let added = order
                .iter()
                .rposition(|f| *f == pina_engine::song::project::Fx::Plugin)
                .unwrap();
            chain.move_fx(added, 3).unwrap();
        });
    });
    cx.run_until_parked();
    assert!(
        (value(cx, 1).unwrap() - 0.123).abs() < 1e-3,
        "{:?}",
        value(cx, 1)
    );
    ws.update(cx, |ws, cx| {
        ws.edit(cx, |ws| {
            ws.session.project.tracks[1].set_insert(0, None).unwrap()
        })
    });
    cx.run_until_parked();
    assert!(
        (value(cx, 0).unwrap() - 0.123).abs() < 1e-3,
        "{:?}",
        value(cx, 0)
    );
    let names = ws.read_with(cx, |ws, _| {
        (0..2)
            .map(|i| {
                ws.player
                    .plugin(id, Which::Insert(i))
                    .map(|p| p.name().to_string())
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(names.len(), 2);
}

#[gpui::test]
fn an_audio_unit_instrument_is_switched_to_mpe_from_its_slot(cx: &mut TestAppContext) {
    use pina_engine::song::project::{Instrument, PluginRef};
    let (ws, cx) = open(cx);
    let instrument = |format| {
        Instrument::Plugin(PluginRef {
            format,
            class: "Keys".into(),
            uid: Some("aumu keys pina".into()),
            ..Default::default()
        })
    };
    let mpe = |cx: &VisualTestContext| {
        ws.read_with(cx, |ws, _| match &ws.session.project.tracks[0].instrument {
            Some(Instrument::Plugin(r)) => r.mpe,
            _ => false,
        })
    };
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].instrument = Some(instrument(Format::Au));
        cx.notify();
    });
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    click_on(
        cx,
        "slot-Track(0)-Instrument",
        Modifiers::none(),
        MouseButton::Right,
    );
    pick(cx, &ws, "MPE: a channel per note");
    assert!(mpe(cx));
    cx.simulate_keystrokes("cmd-z");
    assert!(!mpe(cx));
    // A VST3 hears each note's expression anyway: nothing to switch.
    ws.update(cx, |ws, cx| {
        ws.session.project.tracks[0].instrument = Some(instrument(Format::Vst3));
        cx.notify();
    });
    cx.run_until_parked();
    let items = ws.read_with(cx, |ws, _| ws.mpe_item(0));
    assert!(items.is_none());
}
