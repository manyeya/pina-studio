//! What's found, as pictures: a plugin's picture of its own window (or, until it has one, a face
//! drawn for it) with its format on it and its name, kind and maker under it; laid out in
//! columns, each card as tall as its picture.

use gpui::{AnyElement, Div, FontWeight, MouseButton, ObjectFit, StyledImage, div, img, px, rgb};

use super::*;
use crate::{
    sounds::catalog::Target,
    ui::{looks::hsl, menu::MenuAction, theme::*},
};

/// The narrowest a card is drawn, the gap between them, and the height of the name under one.
pub(crate) const CARD_MIN: f32 = 180.0;
pub(crate) const CARD_GAP: f32 = 14.0;
const LABEL_H: f32 = 34.0;

/// How many columns go across `width`, and how wide each is.
pub(crate) fn columns(width: f32, min: f32, gap: f32) -> (usize, f32) {
    let n = (((width + gap) / (min + gap)) as usize).max(1);
    (n, ((width - gap * (n - 1) as f32) / n as f32).max(40.0))
}

/// A colour for a maker, the same each time.
pub fn maker_hue(vendor: &str) -> f32 {
    let h = vendor
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    (h % 360) as f32
}

impl Workspace {
    /// A device's picture height over its width; a drawn face's, without one.
    pub(crate) fn picture_ratio(&self, key: &str) -> f32 {
        match self.plugin_page.facts.get(key) {
            Some(f) if f.picture.is_some() && f.w > 0 => (f.h as f32 / f.w as f32).clamp(0.2, 2.0),
            _ => 0.62,
        }
    }

    /// A device's picture, `w` by `h`: its own window's, or a face drawn for it.
    pub(crate) fn card_art(&self, d: &Device, w: f32, h: f32) -> AnyElement {
        let picture = self
            .plugin_page
            .facts
            .get(&d.key)
            .and_then(|f| f.picture.clone())
            .map(|name| pictures_dir().join(name))
            .filter(|p| p.exists());
        match picture {
            Some(path) => img(path)
                .w(px(w))
                .h(px(h))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => face(d, w, h).into_any_element(),
        }
    }

    /// A card: its picture with its format on it, and its name, kind and maker under it.
    pub(crate) fn plugin_card(
        &self,
        card: &Card,
        pos: usize,
        w: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let d = card.device();
        let dev = &self.catalog.devices[d];
        let page = &self.plugin_page;
        let h = (w * self.picture_ratio(&dev.key)).round();
        let selected = page.picked.as_deref() == Some(dev.key.as_str())
            && page.preset.as_ref() == super::page::card_preset(card);
        let badge = match (card, dev.own.is_some()) {
            (Card::Preset(..), _) => "Preset".to_string(),
            (_, true) => "Built in".into(),
            _ => dev.badges().first().copied().unwrap_or("").to_string(),
        };
        let failed = page.facts.get(&dev.key).is_some_and(|f| f.failed.is_some());
        let mapped = self
            .catalog
            .keys
            .iter()
            .find(|(_, k)| *k == dev.key)
            .map(|(key, _)| key_label(key));
        let (title, under) = match card {
            Card::Preset(_, p) => (p.name.clone(), dev.name.clone()),
            Card::Device(_) => (
                dev.name.clone(),
                match (subcategory(dev), dev.own) {
                    (Some(k), None) => format!("{k} · {}", dev.vendor),
                    (Some(k), Some(_)) => format!("{k} · Pina Studio"),
                    (None, None) => dev.vendor.clone(),
                    (None, Some(_)) => "Pina Studio".to_string(),
                },
            ),
        };
        let key = dev.key.clone();
        let color = dev.cat.color();
        let art = self.card_art(dev, w, h);
        let tag = |text: String, color: u32| chip(text, color).bg(alpha(BG, 0.85));
        div()
            .id(("plugin-card", pos))
            .w(px(w))
            .flex()
            .flex_col()
            .gap(px(6.))
            .cursor_pointer()
            .debug_selector(move || format!("plugin-card-{key}"))
            .child(
                div()
                    .relative()
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(if selected { ACCENT } else { LINE }))
                    .when(!selected, |d| d.hover(|s| s.border_color(rgb(LINE_STRONG))))
                    .child(div().when(failed, |d| d.opacity(0.45)).child(art))
                    .child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .right(px(6.))
                            .flex()
                            .gap(px(4.))
                            .children(mapped.map(|k| tag(k, ACCENT_TEXT)))
                            .when(!badge.is_empty(), |d| d.child(tag(badge, TEXT_2))),
                    )
                    .when(failed, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(6.))
                                .bottom(px(6.))
                                .child(tag("Won't load".into(), RECORD)),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(LABEL_H))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .size(px(6.))
                                    .flex_none()
                                    .rounded(px(3.))
                                    .bg(rgb(color)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(under),
                            ),
                    ),
            )
            .on_hover({
                let key = self.catalog.devices[d].key.clone();
                cx.listener(move |ws, over: &bool, _, cx| ws.hover_plugin(&key, *over, cx))
            })
            .on_mouse_down(MouseButton::Left, self.plugin_press(card.clone(), cx))
            .on_mouse_down(MouseButton::Right, self.plugin_menu_press(d, cx))
    }

    /// A device put on the selected track (or a new one), its window opening.
    pub(crate) fn load_plugin_card(
        &mut self,
        d: usize,
        preset: Option<pina_engine::song::project::Preset>,
        new_track: bool,
        cx: &mut Context<Self>,
    ) {
        let key = self.catalog.devices[d].key.clone();
        *self.catalog.uses.entry(key).or_default() += 1;
        self.catalog.save();
        let target = match new_track || self.session.project.tracks.is_empty() {
            true => Target::NewTrack,
            false => Target::Track(self.track),
        };
        let name = self.catalog.devices[d].name.clone();
        let status = self.status.clone();
        self.use_device(d, target, preset, cx);
        // Said where it went, unless putting it there had something else to say.
        if self.status == status
            && let Some(t) = self.session.project.tracks.get(self.track)
        {
            self.status = Some(format!("{name} is on {}", t.name));
        }
        cx.notify();
    }

    /// The cards in columns, each going in the shortest so far.
    pub(crate) fn plugin_grid(&mut self, cards: Vec<Card>, cx: &mut Context<Self>) -> Div {
        let width = self.plugin_page.width.get();
        let width = if width > 0.0 {
            width
        } else {
            (self.viewport_w - self.header_w - 340.0).max(400.0)
        };
        let (n, w) = columns(width, CARD_MIN, CARD_GAP);
        let mut cols: Vec<(f32, Vec<gpui::Stateful<Div>>)> =
            (0..n).map(|_| (0.0, Vec::new())).collect();
        for (pos, card) in cards.iter().enumerate() {
            let key = &self.catalog.devices[card.device()].key;
            let h = w * self.picture_ratio(key) + LABEL_H + CARD_GAP;
            let el = self.plugin_card(card, pos, w, cx);
            let shortest = cols.iter_mut().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            shortest.0 += h;
            shortest.1.push(el);
        }
        div()
            .flex()
            .gap(px(CARD_GAP))
            .children(cols.into_iter().map(|(_, c)| {
                div()
                    .w(px(w))
                    .flex()
                    .flex_col()
                    .gap(px(CARD_GAP))
                    .children(c)
            }))
    }

    /// A card's menu: where to put it, its collections, its key, its picture, its files.
    pub(crate) fn card_menu(&self, d: usize) -> Vec<(String, MenuAction)> {
        let dev = &self.catalog.devices[d];
        let op = |label: String, op: PluginOp| (label, MenuAction::Plugins(op));
        let track = self
            .session
            .project
            .tracks
            .get(self.track)
            .map_or("the Selected Track".to_string(), |t| t.name.clone());
        let mut items = vec![
            op(format!("Add to {track}"), PluginOp::Load(d, false)),
            op(
                "Add on a New Track\t⌥-click".into(),
                PluginOp::Load(d, true),
            ),
            (String::new(), MenuAction::Nothing),
            ("Add to Collection".into(), MenuAction::Nothing),
        ];
        for (i, c) in self.catalog.collections.iter().enumerate() {
            let has = c.devices.contains(&dev.key);
            items.push(op(
                format!("{}  {}", if has { "✓" } else { " " }, c.name),
                PluginOp::ToggleCollection(d, i),
            ));
        }
        items.push(op(
            "  New Collection…".into(),
            PluginOp::NewCollection(Some(d)),
        ));
        items.push((String::new(), MenuAction::Nothing));
        let mapped = self.catalog.keys.iter().find(|(_, k)| *k == dev.key);
        items.push(match mapped {
            Some((k, _)) => op(
                format!("Unmap from {}", key_label(k)),
                PluginOp::Unmap(k.clone()),
            ),
            None => op("Map to a Key…".into(), PluginOp::MapCard(d)),
        });
        let fav = self.catalog.favourites.contains(&dev.key);
        items.push(op(
            if fav { "Unfavourite" } else { "Favourite" }.into(),
            PluginOp::Favourite(d),
        ));
        if dev.own.is_none() {
            items.push((String::new(), MenuAction::Nothing));
            items.push(op("Take Its Picture Again".into(), PluginOp::Retake(d)));
            for p in &dev.formats {
                items.push(op(
                    format!("Show {} in Finder", p.format.name()),
                    PluginOp::Reveal(p.path.clone()),
                ));
            }
        }
        items
    }
}

/// A face for a device without a picture yet: its maker's colour, its name, a few knobs.
fn face(d: &Device, w: f32, h: f32) -> Div {
    let hue = if d.own.is_some() {
        222.0
    } else {
        maker_hue(&d.vendor)
    };
    let (bg, edge, ink) = (
        hsl(hue, 0.28, 0.17),
        hsl(hue, 0.3, 0.3),
        hsl(hue, 0.55, 0.72),
    );
    let knob = |size: f32, turn: f32| {
        let r = size / 2.0;
        let a = (turn * 270.0 - 225.0).to_radians();
        div()
            .relative()
            .size(px(size))
            .rounded(px(r))
            .border_2()
            .border_color(rgb(edge))
            .bg(rgb(hsl(hue, 0.25, 0.12)))
            .child(
                div()
                    .absolute()
                    .left(px(r + a.cos() * (r - 5.0) - 2.0))
                    .top(px(r + a.sin() * (r - 5.0) - 2.0))
                    .size(px(4.))
                    .rounded(px(2.))
                    .bg(rgb(ink)),
            )
    };
    let seed = d
        .name
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let knobs = (0..4).map(|i| {
        knob(
            ((w / 7.0).clamp(18.0, 34.0)).round(),
            ((seed >> (i * 5)) % 10) as f32 / 10.0,
        )
    });
    div()
        .w(px(w))
        .h(px(h))
        .flex()
        .flex_col()
        .justify_between()
        .p(px(12.))
        .bg(rgb(bg))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(9.5))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(ink))
                .child(d.vendor.to_uppercase()),
        )
        .child(
            div()
                .text_size(px((w / 11.0).clamp(14.0, 22.0)))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(0xf4f4f6))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(d.name.clone()),
        )
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .children(knobs)
                .child(d.cat.glyph(16.).text_color(rgb(ink))),
        )
}
