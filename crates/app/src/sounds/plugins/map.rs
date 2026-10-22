//! Map: plugins on keys. The keyboard drawn, each key with what Pina Studio does with it or the
//! plugin it's mapped to; a press of a mapped key (anywhere else in Pina Studio) puts its plugin
//! on the selected track. Press a key, click one, click a card and then a key, or drag a card
//! onto one.

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
        ("backspace", "delete", 1.5),
        ("", "", 0.4),
        ("insert", "help", 1.0),
        ("home", "home", 1.0),
        ("pageup", "page up", 1.0),
    ],
    &[
        ("tab", "tab", 1.5),
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
        ("pagedown", "page down", 1.0),
    ],
    &[
        ("", "caps lock", 1.75),
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
        ("enter", "return", 1.75),
    ],
    &[
        ("", "shift", 2.25),
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
        ("", "shift", 2.25),
        ("", "", 1.4),
        ("up", "↑", 1.0),
    ],
    &[
        ("", "control", 1.25),
        ("", "option", 1.25),
        ("", "command", 1.5),
        ("space", "", 6.0),
        ("", "command", 1.5),
        ("", "option", 1.25),
        ("", "control", 1.25),
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
        if self.view == View::Plugins && self.plugin_page.tab == Tab::Map {
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

    /// The keyboard: each key's cap, what Pina Studio does with it, or its plugin.
    pub(crate) fn plugin_keyboard(&mut self, cx: &mut Context<Self>) -> Div {
        let used = self.keys_in_use(cx);
        let unit = 50.0;
        let armed = self.plugin_page.armed.clone();
        let rows: Vec<Div> = ROWS
            .iter()
            .map(|row| {
                div()
                    .flex()
                    .gap(px(5.))
                    .children(row.iter().map(|&(key, label, w)| {
                        let width = unit * w + 5.0 * (w - 1.0).max(0.0);
                        if key.is_empty() && label.is_empty() {
                            return div().w(px(width)).h(px(unit)).into_any_element();
                        }
                        self.keycap(key, label, width, unit, &used, armed.as_deref(), cx)
                    }))
            })
            .collect();
        div()
            .p(px(18.))
            .rounded(px(16.))
            .bg(rgb(0x18181d))
            .border_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap(px(5.))
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
        let mapped = self
            .catalog
            .keys
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, d)| self.catalog.device(d));
        let pina = used.get(key).filter(|_| !key.is_empty()).cloned();
        let modifier = key.is_empty();
        let is_armed = armed == Some(key) && !modifier;
        let picture = mapped.and_then(|d| {
            let k = &self.catalog.devices[d].key;
            let f = self.plugin_page.facts.get(k)?;
            Some(pictures_dir().join(f.picture.as_ref()?))
        });
        let name = mapped.map(|d| self.catalog.devices[d].name.clone());
        let mut cap = div()
            .relative()
            .w(px(w))
            .h(px(h))
            .rounded(px(7.))
            .overflow_hidden()
            .bg(rgb(if modifier { 0x202027 } else { 0x2a2a32 }))
            .border_1()
            .border_color(if is_armed {
                rgb(ACCENT).into()
            } else {
                alpha(0x000000, 0.4)
            })
            .shadow(vec![gpui::BoxShadow {
                color: alpha(0x000000, 0.5),
                offset: gpui::point(px(0.), px(2.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: false,
            }])
            .p(px(5.))
            .flex()
            .flex_col()
            .justify_between()
            .text_size(px(10.))
            .text_color(rgb(if modifier { TEXT_4 } else { TEXT_2 }));
        if let Some(path) = picture.filter(|p| p.exists()) {
            cap = cap.child(
                img(path)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .opacity(0.85),
            );
        } else if mapped.is_some() {
            cap = cap.bg(alpha(ACCENT, 0.35));
        }
        cap = cap
            .child(
                div()
                    .relative()
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(mapped.is_some(), |d| d.text_color(rgb(0xffffff)))
                    .child(label),
            )
            .children(name.map(|n| {
                div()
                    .relative()
                    .px(px(3.))
                    .rounded(px(3.))
                    .bg(alpha(0x000000, 0.6))
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffffff))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(n)
            }))
            .when(mapped.is_none(), |d| {
                d.children(pina.map(|what| {
                    div()
                        .text_size(px(8.5))
                        .text_color(rgb(0xd9a35a))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(what)
                }))
            });
        if modifier {
            return cap.into_any_element();
        }
        let in_use = used.contains_key(key);
        cap.cursor_pointer()
            .hover(|s| s.border_color(alpha(ACCENT, 0.7)))
            .debug_selector(move || format!("key-{key}"))
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
                    // A card dragged onto the key.
                    if let Some(Drag::Device {
                        device,
                        moved: true,
                        ..
                    }) = ws.drag
                    {
                        ws.drag = None;
                        if !in_use {
                            let k = ws.catalog.devices[device].key.clone();
                            ws.map_key(key, &k);
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
