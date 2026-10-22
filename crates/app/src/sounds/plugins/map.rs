//! Keys: plugins on keys of their own. The keyboard drawn, each key with what Pina Studio does
//! with it or the plugin it's on; a press of one (anywhere else in Pina Studio) puts its plugin
//! on the selected track. Pick a plugin and press a key or click one, click a key and then a
//! plugin, or drag a plugin onto a key.

use gpui::{
    AnyElement, Div, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseUpEvent,
    ObjectFit, StyledImage, div, img, px, rgb,
};

use super::*;
use crate::{
    ui::theme::*,
    workspace::{Drag, View},
};

/// The keyboard, row by row: each key's name (as a keystroke says it), its label, and how wide
/// it is (a letter key is 1). Keys with no name are modifiers, not mappable.
pub const ROWS: [&[(&str, &str, f32)]; 6] = [
    &[
        ("escape", "esc", 1.0),
        ("f1", "F1", 1.0),
        ("f2", "F2", 1.0),
        ("f3", "F3", 1.0),
        ("f4", "F4", 1.0),
        ("f5", "F5", 1.0),
        ("f6", "F6", 1.0),
        ("f7", "F7", 1.0),
        ("f8", "F8", 1.0),
        ("f9", "F9", 1.0),
        ("f10", "F10", 1.0),
        ("f11", "F11", 1.0),
        ("f12", "F12", 1.0),
        ("", "", 0.4),
        ("f13", "F13", 1.0),
        ("f14", "F14", 1.0),
        ("f15", "F15", 1.0),
        ("", "", 0.4),
        ("f16", "F16", 1.0),
        ("f17", "F17", 1.0),
        ("f18", "F18", 1.0),
        ("f19", "F19", 1.0),
    ],
    &[
        ("`", "`", 1.0),
        ("1", "1", 1.0),
        ("2", "2", 1.0),
        ("3", "3", 1.0),
        ("4", "4", 1.0),
        ("5", "5", 1.0),
        ("6", "6", 1.0),
        ("7", "7", 1.0),
        ("8", "8", 1.0),
        ("9", "9", 1.0),
        ("0", "0", 1.0),
        ("-", "-", 1.0),
        ("=", "=", 1.0),
        ("backspace", "⌫", 1.5),
        ("", "", 0.4),
        ("insert", "help", 1.0),
        ("home", "home", 1.0),
        ("pageup", "⇞", 1.0),
    ],
    &[
        ("tab", "⇥", 1.5),
        ("q", "Q", 1.0),
        ("w", "W", 1.0),
        ("e", "E", 1.0),
        ("r", "R", 1.0),
        ("t", "T", 1.0),
        ("y", "Y", 1.0),
        ("u", "U", 1.0),
        ("i", "I", 1.0),
        ("o", "O", 1.0),
        ("p", "P", 1.0),
        ("[", "[", 1.0),
        ("]", "]", 1.0),
        ("\\", "\\", 1.0),
        ("", "", 0.4),
        ("delete", "del", 1.0),
        ("end", "end", 1.0),
        ("pagedown", "⇟", 1.0),
    ],
    &[
        ("", "⇪", 1.75),
        ("a", "A", 1.0),
        ("s", "S", 1.0),
        ("d", "D", 1.0),
        ("f", "F", 1.0),
        ("g", "G", 1.0),
        ("h", "H", 1.0),
        ("j", "J", 1.0),
        ("k", "K", 1.0),
        ("l", "L", 1.0),
        (";", ";", 1.0),
        ("'", "'", 1.0),
        ("enter", "↩", 1.75),
    ],
    &[
        ("", "⇧", 2.25),
        ("z", "Z", 1.0),
        ("x", "X", 1.0),
        ("c", "C", 1.0),
        ("v", "V", 1.0),
        ("b", "B", 1.0),
        ("n", "N", 1.0),
        ("m", "M", 1.0),
        (",", ",", 1.0),
        (".", ".", 1.0),
        ("/", "/", 1.0),
        ("", "⇧", 2.25),
        ("", "", 1.4),
        ("up", "↑", 1.0),
    ],
    &[
        ("", "⌃", 1.25),
        ("", "⌥", 1.25),
        ("", "⌘", 1.5),
        ("space", "", 6.0),
        ("", "⌘", 1.5),
        ("", "⌥", 1.25),
        ("", "⌃", 1.25),
        ("", "", 0.4),
        ("left", "←", 1.0),
        ("down", "↓", 1.0),
        ("right", "→", 1.0),
    ],
];

/// A key as it's written on its cap.
pub fn key_label(key: &str) -> String {
    ROWS.iter()
        .flat_map(|r| r.iter())
        .find(|k| k.0 == key)
        .map_or_else(|| key.to_uppercase(), |k| k.1.to_string())
}

/// An action's name as people say it: "TogglePianoRoll" → "Toggle piano roll".
fn spoken(name: &str) -> String {
    let short = name.rsplit("::").next().unwrap_or(name);
    let mut out = String::new();
    for (i, c) in short.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

impl Workspace {
    /// The keys Pina Studio uses on their own (no ⌘, ⌥ or ⌃), and what for.
    pub(crate) fn keys_in_use(&self, cx: &gpui::App) -> HashMap<String, String> {
        let mut used = HashMap::new();
        for b in cx.key_bindings().borrow().bindings() {
            let [k] = b.keystrokes() else { continue };
            let m = &k.modifiers();
            if m.platform || m.control || m.alt || m.shift || m.function {
                continue;
            }
            used.entry(k.key().to_string())
                .or_insert_with(|| spoken(b.action().name()));
        }
        used
    }

    /// Puts a device on a key (in place of what was there), and the key on nothing else.
    pub(crate) fn map_key(&mut self, key: &str, device: &str) {
        let keys = &mut self.catalog.keys;
        keys.retain(|(k, d)| k != key && d != device);
        keys.push((key.to_string(), device.to_string()));
        self.catalog.save();
        let name = self
            .catalog
            .device(device)
            .map_or_else(String::new, |d| self.catalog.devices[d].name.clone());
        self.status = Some(format!(
            "{} puts {name} on the selected track",
            key_label(key)
        ));
    }

    /// A key pressed: if it's mapped (and not in use, nor while the map's open), its plugin goes
    /// on the selected track. Whether it was.
    pub(crate) fn mapped_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let m = &e.keystroke.modifiers;
        if m.modified() || self.naming.is_some() || self.typing.is_some() || self.menu.is_some() {
            return false;
        }
        // On the map, a key pressed is picked to map.
        if self.view == View::Plugins && self.plugin_page.shown == Shown::Keys {
            let key = e.keystroke.key.clone();
            if ROWS
                .iter()
                .flat_map(|r| r.iter())
                .any(|k| k.0 == key && !key.is_empty())
            {
                match self.plugin_page.picked.clone() {
                    Some(device) if !self.keys_in_use(cx).contains_key(&key) => {
                        self.map_key(&key, &device)
                    }
                    _ => self.plugin_page.armed = Some(key),
                }
                cx.notify();
                return true;
            }
            return false;
        }
        let Some(device) = self
            .catalog
            .keys
            .iter()
            .find(|(k, _)| *k == e.keystroke.key)
            .map(|(_, d)| d.clone())
        else {
            return false;
        };
        let Some(d) = self.catalog.device(&device) else {
            return false;
        };
        self.load_plugin_card(d, None, false, cx);
        true
    }

    /// The keyboard: each key's cap, what Pina Studio does with it, or its plugin; as wide as
    /// there's room for.
    pub(crate) fn plugin_keyboard(&mut self, cx: &mut Context<Self>) -> Div {
        const GAP: f32 = 4.0;
        // The widest row: 20.8 keys and 21 gaps, inside the board's padding.
        let room = self.plugin_page.width.get().max(400.0) - 24.0;
        let unit = ((room - GAP * 21.0) / 20.8).clamp(24.0, 46.0);
        let used = self.keys_in_use(cx);
        let armed = self.plugin_page.armed.clone();
        let rows: Vec<Div> = ROWS
            .iter()
            .map(|row| {
                div()
                    .flex()
                    .gap(px(GAP))
                    .children(row.iter().map(|&(key, label, w)| {
                        let width = unit * w + GAP * (w - 1.0).max(0.0);
                        if key.is_empty() && label.is_empty() {
                            return div().w(px(width)).h(px(unit)).into_any_element();
                        }
                        self.keycap(key, label, width, unit, &used, armed.as_deref(), cx)
                    }))
            })
            .collect();
        div()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(GAP))
            .rounded(px(10.))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(LINE))
            .children(rows)
    }

    #[allow(clippy::too_many_arguments)]
    fn keycap(
        &mut self,
        key: &'static str,
        label: &'static str,
        w: f32,
        h: f32,
        used: &HashMap<String, String>,
        armed: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let modifier = key.is_empty();
        if modifier {
            return div()
                .w(px(w))
                .h(px(h))
                .p(px(4.))
                .rounded(px(5.))
                .bg(rgb(BG))
                .border_1()
                .border_color(rgb(LINE_SOFT))
                .text_size(px(9.))
                .text_color(rgb(TEXT_4))
                .child(label)
                .into_any_element();
        }
        let mapped = self
            .catalog
            .keys
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, d)| self.catalog.device(d));
        let pina = used.get(key).cloned();
        let in_use = pina.is_some();
        let is_armed = armed == Some(key);
        let picture = mapped
            .and_then(|d| {
                let f = self.plugin_page.facts.get(&self.catalog.devices[d].key)?;
                Some(pictures_dir().join(f.picture.as_ref()?))
            })
            .filter(|p| p.exists());
        let name = mapped.map(|d| self.catalog.devices[d].name.clone());
        let device_key = mapped.map(|d| self.catalog.devices[d].key.clone());
        let roomy = h >= 38.0;
        let (bg, border) = match (is_armed, mapped.is_some()) {
            (true, _) => (alpha(ACCENT, 0.22), rgb(ACCENT).into()),
            (false, true) => (alpha(ACCENT, 0.16), alpha(ACCENT, 0.7)),
            (false, false) => (rgb(CARD).into(), rgb(LINE).into()),
        };
        let mut cap = div()
            .id(gpui::SharedString::from(format!("key-{key}")))
            .relative()
            .w(px(w))
            .h(px(h))
            .p(px(4.))
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(5.))
            .overflow_hidden()
            .bg(bg)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(LINE_STRONG)))
            .debug_selector(move || format!("key-{key}"));
        if let Some(path) = picture {
            cap = cap.child(
                img(path)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .opacity(0.9),
            );
        }
        let label_el = div().relative().flex().child(
            div()
                .px(px(if mapped.is_some() { 3. } else { 0. }))
                .rounded(px(3.))
                .when(mapped.is_some(), |d| d.bg(alpha(BG, 0.85)))
                .font_family(MONO)
                .text_size(px(9.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(match (mapped.is_some(), in_use) {
                    (true, _) => ACCENT_TEXT,
                    (false, true) => TEXT_4,
                    (false, false) => TEXT_2,
                }))
                .child(label),
        );
        cap = cap.child(label_el).when(roomy, |d| {
            d.children(match (&name, &pina) {
                (Some(n), _) if mapped.is_some() => Some(
                    div()
                        .relative()
                        .px(px(3.))
                        .rounded(px(3.))
                        .bg(alpha(BG, 0.85))
                        .text_size(px(8.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(n.clone()),
                ),
                (_, Some(what)) => Some(
                    div()
                        .text_size(px(8.))
                        .text_color(rgb(TEXT_4))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(what.clone()),
                ),
                _ => None,
            })
        });
        cap.when_some(device_key, |d, k| {
            d.on_hover(cx.listener(move |ws, over: &bool, _, cx| ws.hover_plugin(&k, *over, cx)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                match ws.plugin_page.picked.clone() {
                    Some(device) if !in_use => {
                        ws.map_key(key, &device);
                        ws.plugin_page.armed = None;
                    }
                    Some(_) => {
                        ws.status = Some(format!(
                            "Pina Studio uses {} already: pick another key",
                            key_label(key)
                        ))
                    }
                    None => ws.plugin_page.armed = Some(key.to_string()),
                }
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |ws, _: &MouseUpEvent, _, cx| {
                // A plugin dragged onto the key.
                if let Some(Drag::Device {
                    device,
                    moved: true,
                    ..
                }) = ws.drag
                {
                    ws.drag = None;
                    match in_use {
                        true => {
                            ws.status = Some(format!(
                                "Pina Studio uses {} already: pick another key",
                                key_label(key)
                            ))
                        }
                        false => {
                            let k = ws.catalog.devices[device].key.clone();
                            ws.map_key(key, &k);
                        }
                    }
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, _: &MouseDownEvent, _, cx| {
                ws.catalog.keys.retain(|(k, _)| k != key);
                ws.catalog.save();
                cx.notify();
            }),
        )
        .into_any_element()
    }
}
