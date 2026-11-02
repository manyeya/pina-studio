//! A kit of MIDI: a file for each part of a song, chosen to go together, all in one key. Hear them
//! together and change them as they play: step through a part's alternatives, mute one, lock those
//! to keep and ask for new picks that follow them. Add to Song puts them on new tracks, going round
//! together; a card dragged to the tracks takes just that part.

use pina_engine::samples::midi::{self, MidiKitSeed};

use super::*;

/// The whole kit being heard, where a file's index would be.
pub(crate) const KIT_HEARD: usize = usize::MAX;

impl Workspace {
    /// Builds a kit from the search, in the key the results keep to, else the song's. Locked parts
    /// stay and the rest are chosen to go with them; what's muted stays muted.
    pub fn build_midi_kit(&mut self) {
        let b = &self.browser;
        let path = |k: &MidiPick| Some(b.library.midi.get(k.hit()?.index)?.path.clone());
        let keep = b
            .midi_kit
            .iter()
            .filter(|k| k.locked)
            .filter_map(|k| Some((k.slot.role.clone(), path(k)?)))
            .collect();
        let key = self
            .sample_key()
            .or_else(|| self.session.project.song_key());
        let seed = MidiKitSeed {
            key: key.map(|k| k.name()),
            tempo: self.sample_bpm(),
            words: b.text.clone(),
            alternatives: 6,
            keep,
            variation: b.midi_variation,
            ..Default::default()
        };
        let kit = midi::kit(&b.library.midi, &seed);
        let old = std::mem::take(&mut self.browser.midi_kit);
        let was = |role: &str| old.iter().find(|o| o.slot.role == role);
        self.browser.midi_kit_key = kit.key;
        self.browser.midi_kit = kit
            .slots
            .into_iter()
            .map(|mut slot| {
                let before = was(&slot.role);
                slot.on = before.map_or(slot.on, |o| o.slot.on);
                MidiPick {
                    slot,
                    choice: 0,
                    locked: before.is_some_and(|o| o.locked),
                }
            })
            .collect();
    }

    /// Another kit that holds together, for what isn't locked.
    pub fn new_midi_picks(&mut self, cx: &mut Context<Self>) {
        self.browser.midi_variation += 1;
        self.build_midi_kit();
        self.kit_changed(cx);
    }

    /// A part changed: if the kit's being heard, it's heard as it now is.
    fn kit_changed(&mut self, cx: &mut Context<Self>) {
        if self.kit_heard() {
            self.hear_midi_kit(cx);
        }
        cx.notify();
    }

    pub(crate) fn kit_heard(&self) -> bool {
        self.midi_heard(KIT_HEARD)
    }

    /// The parts that play, each file and how it's moved.
    fn kit_parts(&self) -> Vec<(String, Arc<pina_engine::song::midi_file::MidiFile>, i32)> {
        let on = self.browser.midi_kit.iter().filter(|k| k.slot.on);
        on.filter_map(|k| {
            let h = k.hit()?;
            Some((capital(&k.slot.role), self.midi_file(h.index)?, h.transpose))
        })
        .collect()
    }

    /// The parts that play heard together, going round until the longest ends.
    pub fn hear_midi_kit(&mut self, cx: &mut Context<Self>) {
        let b = &self.browser;
        let on = b.midi_kit.iter().filter(|k| k.slot.on);
        let names: Vec<String> = on
            .filter_map(|k| {
                Some(
                    b.library
                        .midi
                        .get(k.hit()?.index)?
                        .path
                        .display()
                        .to_string(),
                )
            })
            .collect();
        let parts = self.kit_parts();
        if parts.is_empty() {
            self.status = Some("Every part is muted.".into());
            cx.notify();
            return;
        }
        let name = PathBuf::from(format!("kit:{}", names.join("|")));
        let files = parts.into_iter().map(|(_, f, s)| (f, s)).collect();
        self.hear_midi(KIT_HEARD, name, files, cx);
    }

    /// The parts that play into the song at the bar the playhead's in, on new tracks, going round
    /// together. The song keeps the tempo they were heard at.
    pub fn make_midi_kit(&mut self, cx: &mut Context<Self>) {
        let parts = self.kit_parts();
        if parts.is_empty() {
            self.status = Some("Every part is muted: nothing to add.".into());
            cx.notify();
            return;
        }
        let bar = self.session.project.bar().max(1);
        let tick = self.playhead() / bar * bar;
        let key = self.browser.midi_kit_key.clone();
        let mut made = Vec::new();
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            let (tempo, tempos, had_key) = (p.tempo, p.tempos.clone(), p.key.is_some());
            for (_, file, shift) in &parts {
                made.extend(p.import_midi(file, tick, None, *shift));
            }
            (p.tempo, p.tempos) = (tempo, tempos);
            if !had_key && p.key.is_none() {
                p.key = key.clone();
            }
            p.loop_together(&made);
        });
        if let Some(&t) = made.first() {
            self.select_track(t);
        }
        let names: Vec<String> = parts.into_iter().map(|p| p.0).collect();
        let at = tick / bar + 1;
        self.status = Some(match &key {
            Some(k) => format!("{} on new tracks at bar {at}, in {k}", names.join(", ")),
            None => format!("{} on new tracks at bar {at}", names.join(", ")),
        });
        cx.notify();
    }

    pub(crate) fn midi_kit_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let mut seed: Vec<String> = Vec::new();
        if let Some(k) = &b.midi_kit_key {
            seed.push(k.clone());
        }
        if let Some(bpm) = self.sample_bpm() {
            seed.push(format!("{bpm:.0} BPM"));
        }
        if !b.text.is_empty() {
            seed.push(format!("“{}”", b.text));
        }
        let heard = self.kit_heard();
        let cards: Vec<Div> = b
            .midi_kit
            .iter()
            .enumerate()
            .map(|(n, k)| self.midi_kit_card(n, k, cx))
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
                    .child("Parts that go together"),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .min_w(px(0.))
                    .overflow_hidden()
                    .children(seed.into_iter().map(|s| chip(s, ACCENT_TEXT))),
            )
            .child(div().flex_1())
            .child(
                button(if heard { "Stop" } else { "Hear together" }, heard)
                    .flex_none()
                    .child(
                        icon(if heard { Icon::Stop } else { Icon::Play }, 11.)
                            .text_color(rgb(if heard { TEXT } else { TEXT_3 })),
                    )
                    .flex_row_reverse()
                    .debug_selector(|| "midi-kit-hear".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| match heard {
                            true => ws.stop_audition(cx),
                            false => ws.hear_midi_kit(cx),
                        }),
                    ),
            )
            .child(
                button("New picks", false)
                    .flex_none()
                    .child(icon(Icon::Shuffle, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "midi-kit-new".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.new_midi_picks(cx)),
                    ),
            )
            .child(
                primary("Add to song")
                    .h(px(28.))
                    .flex_none()
                    .debug_selector(|| "midi-kit-add".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.make_midi_kit(cx)),
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
                    .id("midi-kit")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .when(empty, |d| {
                        d.flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(TEXT_3))
                            .child("Add a folder with MIDI files to build a kit from them.")
                    })
                    .when(!empty, |d| {
                        d.child(div().grid().grid_cols(3).gap(px(8.)).children(cards))
                    }),
            )
            .child(self.midi_info_line())
    }

    fn midi_kit_card(&self, n: usize, k: &MidiPick, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let role = k.slot.role.clone();
        let color = role_hue(&role);
        let options = k.options().len();
        let hit = k.hit().cloned();
        let m = hit.as_ref().and_then(|h| b.library.midi.get(h.index));
        let index = hit.as_ref().map(|h| h.index);
        let selected = index.is_some() && index == b.midi_selected;
        let playing = index.is_some_and(|i| self.midi_heard(i));
        let on = k.slot.on;
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
                .debug_selector(move || format!("midi-kit-{n}-{by}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let Some(k) = ws.browser.midi_kit.get_mut(n) else {
                            return;
                        };
                        let len = k.options().len().max(1) as i32;
                        k.choice = (k.choice as i32 + by).rem_euclid(len) as usize;
                        let index = k.hit().map(|h| h.index);
                        // Heard together, it changes in the kit; else it's heard on its own.
                        match (ws.kit_heard(), index) {
                            (true, _) => ws.kit_changed(cx),
                            (false, Some(i)) => ws.select_midi(i, true, cx),
                            _ => {}
                        }
                    }),
                )
        };
        // A part's switch: what it changes is heard at once if that's the part being muted.
        let toggle = |i: Icon, lit: bool, name: String, flip: fn(&mut MidiPick), cx: &mut _| {
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(if lit {
                    alpha(ACCENT, 0.2)
                } else {
                    gpui::transparent_black()
                })
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .debug_selector(move || name)
                .child(icon(i, 13.).text_color(rgb(if lit { ACCENT_TEXT } else { TEXT_3 })))
                .on_mouse_down(
                    MouseButton::Left,
                    Context::listener(cx, move |ws: &mut Workspace, _, _, cx| {
                        cx.stop_propagation();
                        let Some(k) = ws.browser.midi_kit.get_mut(n) else {
                            return;
                        };
                        let was = k.slot.on;
                        flip(k);
                        match k.slot.on != was {
                            true => ws.kit_changed(cx),
                            false => cx.notify(),
                        }
                    }),
                )
        };
        let line = m.map_or(String::new(), |m| {
            let key = m.study.key.as_deref().and_then(Key::parse);
            match (key, hit.as_ref().map_or(0, |h| h.transpose)) {
                (Some(k), 0) => format!("{} · {}", m.name, k.short()),
                (Some(k), t) => format!("{} · {} {t:+}", m.name, k.short()),
                (None, _) => m.name.clone(),
            }
        });
        div()
            .h(px(116.))
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
            .when(!on, |d| d.opacity(0.5))
            .debug_selector(move || format!("midi-kit-{n}"))
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
                            .child(capital(&role)),
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
                    .child(toggle(
                        Icon::Speaker,
                        on,
                        format!("midi-kit-on-{n}"),
                        |k| k.slot.on = !k.slot.on,
                        cx,
                    ))
                    .child(toggle(
                        if k.locked { Icon::Lock } else { Icon::Unlock },
                        k.locked,
                        format!("midi-kit-lock-{n}"),
                        |k| k.locked = !k.locked,
                        cx,
                    )),
            )
            .child(match index {
                Some(i) => div()
                    .h(px(34.))
                    .rounded(px(5.))
                    .bg(alpha(color, 0.08))
                    .overflow_hidden()
                    .child(piano_roll(self.midi_file(i).map(|f| outline(&f)), color))
                    .into_any_element(),
                None => div()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child(format!("No {role} fits these filters"))
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
                            .child(line),
                    )
                    .when(options > 1, |d| {
                        d.child(step(Icon::Back, -1, cx))
                            .child(step(Icon::Forward, 1, cx))
                    })
                    .when_some(index, |d, i| {
                        d.child(
                            play_dot(playing, color, 22.)
                                .debug_selector(move || format!("midi-kit-play-{n}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        match playing {
                                            true => ws.stop_audition(cx),
                                            false => ws.select_midi(i, true, cx),
                                        }
                                    }),
                                ),
                        )
                    }),
            )
            .when_some(index, |d, i| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.pick_up_midi(i, pt(e.position), cx);
                    }),
                )
            })
    }
}
