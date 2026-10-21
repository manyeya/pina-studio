//! Kits: drums picked to go together, reshuffled, made into a sampler track.

use super::*;

impl Workspace {
    /// Builds a kit from the filters (key, tempo, words, traits); locked slots keep their sound.
    pub fn build_kit(&mut self) {
        let b = &self.browser;
        // Locked sounds stay, and the rest are picked to go with them.
        let keep = b
            .kit
            .iter()
            .filter(|k| k.locked)
            .filter_map(|k| Some(b.sample(k.hit()?.index)?.path.clone()))
            .collect();
        let seed = KitSeed {
            key: self.sample_key().map(|k| k.name()),
            bpm: self.sample_bpm(),
            text: b.text.clone(),
            traits: b.traits.clone(),
            alternatives: 6,
            keep,
            ..Default::default()
        };
        let kit = search::kit(&b.library.samples, &seed);
        let old = std::mem::take(&mut self.browser.kit);
        self.browser.kit = kit
            .slots
            .into_iter()
            .map(
                |slot| match old.iter().find(|o| o.locked && o.slot.role == slot.role) {
                    Some(o) => o.clone(),
                    None => KitPick {
                        slot,
                        choice: 0,
                        locked: false,
                    },
                },
            )
            .collect();
    }

    /// New picks for the slots that aren't locked.
    pub fn shuffle_kit(&mut self, cx: &mut Context<Self>) {
        let mut seed = Instant::now().elapsed().as_nanos() as u64 | 1;
        seed ^= std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        for k in self.browser.kit.iter_mut().filter(|k| !k.locked) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let n = k.options().len().max(1);
            k.choice = (seed % n as u64) as usize;
        }
        cx.notify();
    }

    /// Makes the kit a sampler track: drums on their General MIDI notes (a closed hat choking an
    /// open one), tonal sounds across free notes, in tune.
    pub fn make_kit(&mut self, cx: &mut Context<Self>) {
        let samples = &self.browser.library.samples;
        let picks: Vec<(Category, Sample)> = self
            .browser
            .kit
            .iter()
            .filter_map(|k| Some((k.slot.role, samples.get(k.hit()?.index)?.clone())))
            .collect();
        if picks.is_empty() {
            self.status = Some("Nothing fits this kit: loosen the filters and try again.".into());
            cx.notify();
            return;
        }
        let mut sampler = Sampler::default();
        for (role, s) in &picks {
            // A sound that won't fit is left out; the rest still make the kit.
            let _ = place_zone(&mut sampler, s, Some(*role));
        }
        let n = sampler.zones.len();
        let name = match self.sample_key() {
            Some(k) => format!("Kit in {}", k.name()),
            None => "Kit".to_string(),
        };
        let index = self.new_track_index(false);
        self.session.checkpoint();
        let dir = self.session.dir.clone();
        let add = Command::AddTrack {
            name: name.clone(),
            instrument: Some(Instrument::Sampler(sampler)),
            index: Some(index),
        };
        match commands::apply(&mut self.session.project, &dir, add) {
            Ok(_) => {
                self.select_track(index);
                self.region = None;
                self.changed(cx);
                self.status = Some(format!("Made {name}: {n} sounds on a sampler"));
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    pub(crate) fn kit_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let mut seed: Vec<String> = Vec::new();
        if let Some(k) = self.sample_key() {
            seed.push(k.name());
        }
        if let Some(bpm) = self.sample_bpm() {
            seed.push(format!("{bpm:.0} BPM"));
        }
        if !b.text.is_empty() {
            seed.push(format!("“{}”", b.text));
        }
        seed.extend(b.traits.iter().cloned());
        let cards: Vec<Div> = b
            .kit
            .iter()
            .enumerate()
            .map(|(n, k)| self.kit_card(n, k, cx))
            .collect();
        let header = div()
            .h(px(44.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .child("A kit that goes together"),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .min_w(px(0.))
                    .overflow_hidden()
                    .children(match seed.is_empty() {
                        true => vec![chip("from everything", TEXT_3)],
                        false => seed.into_iter().map(|s| chip(s, ACCENT_TEXT)).collect(),
                    }),
            )
            .child(div().flex_1())
            .child(
                button("New picks", false)
                    .child(icon(Icon::Shuffle, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "shuffle-kit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.shuffle_kit(cx)),
                    ),
            )
            .child(
                primary("Make kit")
                    .h(px(28.))
                    .debug_selector(|| "make-kit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.make_kit(cx)),
                    ),
            );
        let empty = cards.is_empty();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("kit")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .when(empty, |d| {
                        d.flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(TEXT_3))
                            .child("Nothing to build a kit from yet.")
                    })
                    .when(!empty, |d| {
                        d.child(div().grid().grid_cols(3).gap(px(8.)).children(cards))
                    }),
            )
            .child(self.info_line())
    }

    pub(crate) fn kit_card(&self, n: usize, k: &KitPick, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let role = k.slot.role;
        let color = category_color(role);
        let options = k.options().len();
        let hit = k.hit().cloned();
        let s = hit.as_ref().and_then(|h| b.sample(h.index));
        let selected = hit.as_ref().is_some_and(|h| b.selected == Some(h.index));
        let playing = hit
            .as_ref()
            .is_some_and(|h| b.audition.is_some_and(|a| a.0 == h.index));
        let step = |i: Icon, by: i32, cx: &mut Context<Self>| {
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .child(icon(i, 13.).text_color(rgb(TEXT_2)))
                .debug_selector(move || format!("kit-{n}-{by}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let Some(k) = ws.browser.kit.get_mut(n) else {
                            return;
                        };
                        let len = k.options().len().max(1) as i32;
                        k.choice = (k.choice as i32 + by).rem_euclid(len) as usize;
                        if let Some(i) = k.hit().map(|h| h.index) {
                            ws.select_sample(i, true, cx);
                        }
                    }),
                )
        };
        let index = hit.as_ref().map(|h| h.index);
        div()
            .h(px(104.))
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(10.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(if selected {
                alpha(color, 0.8)
            } else {
                rgb(LINE).into()
            })
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(LINE_STRONG)))
            .debug_selector(move || format!("kit-{n}"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(8.)).rounded(px(2.)).bg(rgb(color)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(role.name()),
                    )
                    .when(options > 1, |d| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{}/{options}", k.choice + 1)),
                        )
                    })
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(if k.locked {
                                alpha(ACCENT, 0.2)
                            } else {
                                gpui::transparent_black()
                            })
                            .hover(|s| s.bg(rgb(RAISED)))
                            .debug_selector(move || format!("kit-lock-{n}"))
                            .child(
                                icon(if k.locked { Icon::Lock } else { Icon::Unlock }, 13.)
                                    .text_color(rgb(if k.locked { ACCENT_TEXT } else { TEXT_3 })),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(k) = ws.browser.kit.get_mut(n) {
                                        k.locked = !k.locked;
                                    }
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(match s {
                Some(s) => div()
                    .h(px(24.))
                    .child(wave(s.analysis.waveform.clone(), color, 0.85))
                    .into_any_element(),
                None => div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child(format!(
                        "No {} fits these filters",
                        role.name().to_lowercase()
                    ))
                    .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.map_or(String::new(), |s| match tone_text(&s.analysis) {
                                Some(t) => format!("{} · {t}", s.name),
                                None => s.name.clone(),
                            })),
                    )
                    .when(options > 1, |d| {
                        d.child(step(Icon::Back, -1, cx))
                            .child(step(Icon::Forward, 1, cx))
                    })
                    .when_some(index, |d, i| {
                        d.child(
                            div()
                                .size(px(22.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(11.))
                                .bg(rgb(if playing { ACCENT } else { RAISED }))
                                .cursor_pointer()
                                .debug_selector(move || format!("kit-play-{n}"))
                                .child(icon(Icon::Play, 11.).text_color(rgb(TEXT)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.audition_sample(i, cx);
                                    }),
                                ),
                        )
                    }),
            )
            .when_some(index, |d, i| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.select_sample(i, true, cx);
                        let at = pt(e.position);
                        ws.begin_drag(
                            Drag::Sample {
                                index: i,
                                from: at,
                                at,
                                moved: false,
                            },
                            cx,
                        );
                    }),
                )
            })
    }
}
