//! Health: how much room the plugins take and how much can be had back (copies of a plugin in
//! formats other than the one Pina Studio uses, plugins that won't load, plugins this Mac can't
//! run without Rosetta), what the songs use and miss, and what's never been in a song. Each set
//! seen in full in the library; copies moved to the Trash.

use std::path::PathBuf;

use gpui::{
    AnyElement, Div, FontWeight, MouseButton, MouseDownEvent, ObjectFit, StyledImage, div, img, px,
    rgb,
};

use super::*;
use crate::ui::{
    menu::{Menu, MenuAction},
    theme::*,
};

/// What Health finds, worked out from the facts.
pub(crate) struct Checkup {
    pub total: u64,
    /// The files that could go: copies, plugins that won't load, those needing Rosetta.
    pub spare: Vec<PathBuf>,
    pub spare_size: u64,
    pub copies: Vec<usize>,
    pub copies_size: u64,
    pub wont_load: Vec<usize>,
    pub rosetta: Vec<usize>,
    pub in_songs: Vec<usize>,
    pub never: Vec<usize>,
    pub never_size: u64,
}

impl Workspace {
    pub(crate) fn checkup(&self) -> Checkup {
        let page = &self.plugin_page;
        let file = |p: &PathBuf| page.files.get(p).cloned().unwrap_or_default();
        let mut c = Checkup {
            total: 0,
            spare: Vec::new(),
            spare_size: 0,
            copies: Vec::new(),
            copies_size: 0,
            wont_load: Vec::new(),
            rosetta: Vec::new(),
            in_songs: Vec::new(),
            never: Vec::new(),
            never_size: 0,
        };
        let mut counted = std::collections::HashSet::new();
        for (i, d) in self.catalog.devices.iter().enumerate() {
            if d.own.is_some() {
                continue;
            }
            for p in &d.formats {
                if counted.insert(p.path.clone()) {
                    c.total += file(&p.path).size;
                }
            }
            let spare = |p: &PathBuf, c: &mut Checkup| {
                let f = file(p);
                if !f.system && !c.spare.contains(p) {
                    c.spare_size += f.size;
                    c.spare.push(p.clone());
                }
            };
            if d.formats.len() > 1 {
                c.copies.push(i);
                for p in &d.formats[1..] {
                    // A bundle holding several plugins (a suite) isn't a copy of this one.
                    c.copies_size += file(&p.path).size;
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::WontLoad) {
                c.wont_load.push(i);
                for p in &d.formats {
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::NeedsRosetta) {
                c.rosetta.push(i);
                for p in d.formats.iter().filter(|p| file(&p.path).intel_only) {
                    spare(&p.path, &mut c);
                }
            }
            if self.in_set(d, HealthSet::InSongs) {
                c.in_songs.push(i);
            }
            if self.in_set(d, HealthSet::NeverInASong) {
                c.never.push(i);
                c.never_size += self.device_size(d);
            }
        }
        // A file another plugin is used from (a suite in one bundle) stays.
        let kept: std::collections::HashSet<&PathBuf> = self
            .catalog
            .devices
            .iter()
            .filter(|d| {
                d.own.is_none()
                    && !self.in_set(d, HealthSet::WontLoad)
                    && !self.in_set(d, HealthSet::NeedsRosetta)
            })
            .filter_map(|d| d.formats.first().map(|p| &p.path))
            .collect();
        c.spare.retain(|p| !kept.contains(p));
        c.spare_size = c.spare.iter().map(|p| file(p).size).sum();
        let uses = page
            .songs
            .as_ref()
            .map(|s| s.used.clone())
            .unwrap_or_default();
        c.in_songs.sort_by_key(|&i| {
            std::cmp::Reverse(uses.get(&self.catalog.devices[i].key).copied().unwrap_or(0))
        });
        c.never
            .sort_by_key(|&i| std::cmp::Reverse(self.device_size(&self.catalog.devices[i])));
        c
    }

    /// A row of small pictures of some devices, a count on each if given, and how many more.
    fn thumbs(&self, list: &[usize], counts: Option<&HashMap<String, usize>>) -> Div {
        let shown: Vec<AnyElement> = list
            .iter()
            .take(5)
            .map(|&d| {
                let dev = &self.catalog.devices[d];
                let (w, h) = (148.0, 84.0);
                let picture = self
                    .plugin_page
                    .facts
                    .get(&dev.key)
                    .and_then(|f| f.picture.clone())
                    .map(|n| pictures_dir().join(n))
                    .filter(|p| p.exists());
                let art: AnyElement = match picture {
                    Some(p) => img(p)
                        .w(px(w))
                        .h(px(h))
                        .object_fit(ObjectFit::Cover)
                        .into_any_element(),
                    None => div()
                        .w(px(w))
                        .h(px(h))
                        .p(px(8.))
                        .flex()
                        .items_end()
                        .bg(rgb(crate::ui::looks::hsl(
                            super::cards::maker_hue(&dev.vendor),
                            0.28,
                            0.17,
                        )))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(dev.name.clone())
                        .into_any_element(),
                };
                div()
                    .relative()
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(7.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .child(art)
                    .children(counts.and_then(|c| c.get(&dev.key)).map(|n| {
                        div()
                            .absolute()
                            .right(px(6.))
                            .bottom(px(6.))
                            .px(px(6.))
                            .rounded(px(5.))
                            .bg(alpha(0x000000, 0.7))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{n}"))
                    }))
                    .into_any_element()
            })
            .collect();
        let more = list.len().saturating_sub(5);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .children(shown)
            .when(more > 0, |d| {
                d.child(
                    div()
                        .pl(px(4.))
                        .text_size(px(14.))
                        .text_color(rgb(TEXT_3))
                        .child(format!("+{more}")),
                )
            })
    }

    fn health_card(title: &'static str) -> Div {
        div()
            .p(px(20.))
            .rounded(px(14.))
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
    }

    fn see_all(&self, label: String, set: HealthSet, cx: &mut Context<Self>) -> Div {
        crate::ui::theme::button(label, false)
            .h(px(32.))
            .px(px(16.))
            .text_size(px(13.))
            .debug_selector(move || format!("health-{set:?}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    let page = &mut ws.plugin_page;
                    page.set = Some(set);
                    (page.tab, page.cat, page.sub, page.maker, page.collection) =
                        (Tab::Library, None, None, None, None);
                    page.source = Source::All;
                    // A set holds effects and instruments both: shown as whichever it has more.
                    let (fx, inst): (Vec<bool>, Vec<bool>) = ws
                        .catalog
                        .devices
                        .iter()
                        .filter(|d| ws.in_set(d, set))
                        .map(|d| d.instrument())
                        .partition(|i| !*i);
                    ws.plugin_page.kind = if inst.len() > fx.len() {
                        Kind::Instruments
                    } else {
                        Kind::Effects
                    };
                    cx.notify();
                }),
            )
    }

    pub(crate) fn health_page(&mut self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let c = self.checkup();
        let songs = self.plugin_page.songs.clone();
        let looking = songs.is_none() || self.plugin_page.files.is_empty();
        let big = |n: String, unit: &'static str| {
            div()
                .flex()
                .items_end()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(64.))
                        .font_weight(FontWeight::BOLD)
                        .line_height(px(64.))
                        .child(n),
                )
                .child(
                    div()
                        .pb(px(8.))
                        .text_size(px(26.))
                        .font_weight(FontWeight::BOLD)
                        .child(unit),
                )
        };
        let size = size_text(c.spare_size);
        let (n, unit) = size.split_once(' ').map_or((size.clone(), ""), |(a, b)| {
            (
                a.to_string(),
                if b == "GB" {
                    "GB"
                } else if b == "MB" {
                    "MB"
                } else {
                    "KB"
                },
            )
        });
        let share = if c.total > 0 {
            c.spare_size as f32 / c.total as f32
        } else {
            0.0
        };
        let stat = |n: usize, words: String| {
            div()
                .flex()
                .items_end()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(38.))
                        .font_weight(FontWeight::BOLD)
                        .line_height(px(40.))
                        .child(format!("{n}")),
                )
                .child(
                    div()
                        .pb(px(5.))
                        .text_size(px(14.))
                        .text_color(rgb(TEXT_2))
                        .child(words),
                )
        };
        let song_count = songs.as_ref().map_or(0, |s| s.songs);
        let missing: Vec<(String, Vec<String>)> = songs
            .as_ref()
            .map(|s| {
                let mut m: Vec<_> = s.missing.clone().into_iter().collect();
                m.sort();
                m
            })
            .unwrap_or_default();
        let missing_songs: std::collections::HashSet<&String> =
            missing.iter().flat_map(|(_, s)| s).collect();
        let used = songs.as_ref().map(|s| s.used.clone()).unwrap_or_default();
        let missing_items: Vec<(String, MenuAction)> = missing
            .iter()
            .map(|(name, s)| (format!("{name}  ·  {}", s.join(", ")), MenuAction::Nothing))
            .collect();
        let spare = c.spare.clone();
        let spare_size = c.spare_size;
        let songs_card = Self::health_card("Your songs")
            .child(self.thumbs(&c.in_songs, Some(&used)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(stat(
                        c.in_songs.len(),
                        format!("plugins in your {song_count} songs"),
                    ))
                    .child(self.see_all(
                        format!("See all {}", c.in_songs.len()),
                        HealthSet::InSongs,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(stat(
                        missing.len(),
                        format!("plugins missing from {} songs", missing_songs.len()),
                    ))
                    .when(!missing.is_empty(), |d| {
                        d.child(
                            crate::ui::theme::button(format!("See the {}", missing.len()), false)
                                .h(px(32.))
                                .px(px(16.))
                                .text_size(px(13.))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        ws.menu = Some(Menu {
                                            at: e.position,
                                            items: missing_items.clone(),
                                            ..Default::default()
                                        });
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child("The songs in your Songs folder, looked through without opening them."),
            );
        let rosetta = Self::health_card("Needs Rosetta")
            .child(stat(
                c.rosetta.len(),
                "plugins have no code for Apple silicon: Pina Studio can't load them".into(),
            ))
            .when(!c.rosetta.is_empty(), |d| {
                d.child(self.thumbs(&c.rosetta, None)).child(self.see_all(
                    format!("See all {}", c.rosetta.len()),
                    HealthSet::NeedsRosetta,
                    cx,
                ))
            });
        let never = Self::health_card("Never in a song")
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(38.))
                            .font_weight(FontWeight::BOLD)
                            .line_height(px(40.))
                            .child(size_text(c.never_size)),
                    )
                    .child(
                        div()
                            .pb(px(5.))
                            .text_size(px(14.))
                            .text_color(rgb(TEXT_2))
                            .child(format!(
                                "{} plugins in none of your {song_count} songs",
                                c.never.len()
                            )),
                    ),
            )
            .when(!c.never.is_empty(), |d| {
                d.child(self.thumbs(&c.never, None)).child(self.see_all(
                    format!("See all {}", c.never.len()),
                    HealthSet::NeverInASong,
                    cx,
                ))
            });
        let copies = Self::health_card("In more than one format")
            .child(stat(c.copies.len(), format!("plugins installed as more than one format · {} in the formats Pina Studio doesn't use", size_text(c.copies_size))))
            .when(!c.copies.is_empty(), |d| {
                d.child(self.thumbs(&c.copies, None)).child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.see_all(format!("See all {}", c.copies.len()), HealthSet::Copies, cx)),
                )
            });
        let wont = Self::health_card("Won't load")
            .child(stat(
                c.wont_load.len(),
                "plugins failed to open when their pictures were taken".into(),
            ))
            .when(!c.wont_load.is_empty(), |d| {
                let keys: Vec<String> = c
                    .wont_load
                    .iter()
                    .map(|&i| self.catalog.devices[i].key.clone())
                    .collect();
                d.child(self.thumbs(&c.wont_load, None)).child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.see_all(
                            format!("See all {}", c.wont_load.len()),
                            HealthSet::WontLoad,
                            cx,
                        ))
                        .child(
                            crate::ui::theme::button("Try Again", false)
                                .h(px(32.))
                                .px(px(16.))
                                .text_size(px(13.))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        for k in &keys {
                                            if let Some(f) = ws.plugin_page.facts.get_mut(k) {
                                                f.failed = None;
                                            }
                                        }
                                        ws.take_plugin_pictures(Some(keys.clone()), cx);
                                    }),
                                ),
                        ),
                )
            });
        div()
            .id("plugin-health")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .p(px(28.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(10.))
                            .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child("Library health"))
                            .when(looking, |d| d.child(div().text_size(px(13.)).text_color(rgb(TEXT_3)).child("Looking through your plugins and songs…"))),
                    )
                    .child(
                        crate::ui::theme::button("← Back to Library", false)
                            .h(px(32.))
                            .px(px(14.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.plugin_page.tab = Tab::Library;
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(big(n, unit))
                    .child(
                        div()
                            .text_size(px(17.))
                            .text_color(rgb(TEXT_2))
                            .child(format!("you can get back · {} of plugins on this Mac", size_text(c.total))),
                    )
                    .child(
                        div()
                            .h(px(8.))
                            .rounded(px(4.))
                            .bg(rgb(RAISED))
                            .child(div().h_full().w(gpui::relative(share.clamp(0.01, 1.0))).rounded(px(4.)).bg(rgb(0xf4f4f6))),
                    )
                    .when(!spare.is_empty(), |d| {
                        d.child(
                            div().flex().child(
                                crate::ui::theme::button(format!("Move {} files to the Trash…", spare.len()), false)
                                    .h(px(32.))
                                    .px(px(16.))
                                    .text_size(px(13.))
                                    .debug_selector(|| "health-trash".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                            let mut items: Vec<(String, MenuAction)> = spare
                                                .iter()
                                                .map(|p| (format!("  {}", p.display()), MenuAction::Nothing))
                                                .collect();
                                            items.insert(0, ("These go to the Trash (you can put them back from there):".into(), MenuAction::Nothing));
                                            items.push((String::new(), MenuAction::Nothing));
                                            items.push((
                                                format!("Move {} files ({}) to the Trash", spare.len(), size_text(spare_size)),
                                                MenuAction::Plugins(PluginOp::Trash(spare.clone())),
                                            ));
                                            ws.menu = Some(Menu { at: e.position, items, ..Default::default() });
                                            cx.notify();
                                        }),
                                    ),
                            ),
                        )
                    }),
            )
            .child(songs_card)
            .child(div().flex().gap(px(20.)).child(div().flex_1().child(rosetta)).child(div().flex_1().child(never)))
            .child(div().flex().gap(px(20.)).child(div().flex_1().child(copies)).child(div().flex_1().child(wont)))
    }
}
