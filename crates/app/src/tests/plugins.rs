//! The Plugins page: opened from the Browser and back, its filters, a plugin picked and put on a
//! track (Add, double-click, Enter, dragged out onto one), keys, collections, Health and the
//! checkup, and the picker a mixer slot opens.

use pina_engine::song::project::{Format, Which};

use super::*;
use crate::sounds::{
    catalog::Scope,
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
    assert_eq!(cards(&ws, cx), ["AUDelay"]);
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
    assert_eq!(cards(&ws, cx), ["AUDelay"]);
    click_on(
        cx,
        "plugins-format-AU",
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
    cx.simulate_input("delay");
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
