//! MIDI files in the browser: the library's, found by the words searched, the part picked and
//! the key the results keep to (else the song's, with Play in Key on); each row drawn as a little
//! piano roll, with its part, key, tempo, bars and progression. A click hears it as it would be
//! in the song (at its tempo, moved into its key, on the instruments bringing it in would give
//! it), in time with the song while it plays; a drag puts it on the tracks: one part onto an
//! instrument track, a region there; else a track a part. Picked, its whole study shows. What
//! the song's fit leaves is what's in its key and near its tempo, or would be with Play in Key.

use pina_engine::{
    samples::midi::{self, MidiHit, MidiQuery},
    song::midi_file::{self, MidiFile},
    song::project::{Content, Note, Region},
};

use super::*;

/// The parts a MIDI file can be for, as the chips offer them.
pub const ROLES: [&str; 9] = [
    "drums", "bass", "chords", "pad", "pluck", "keys", "lead", "arp", "fx",
];

/// A part's colour, as the sounds of its kind have theirs.
pub fn role_hue(role: &str) -> u32 {
    category_color(match role {
        "drums" => Category::Kick,
        "bass" => Category::Bass,
        "chords" | "keys" => Category::Keys,
        "pad" => Category::Pad,
        "pluck" | "arp" => Category::Pluck,
        "lead" => Category::Lead,
        "fx" => Category::Fx,
        _ => Category::Other,
    })
}

/// The map's legend: each part's colour, those that share one together.
pub(crate) fn midi_legend() -> [(u32, &'static str); 7] {
    [
        (role_hue("drums"), "Drums"),
        (role_hue("bass"), "Bass"),
        (role_hue("chords"), "Chords & keys"),
        (role_hue("pad"), "Pads"),
        (role_hue("pluck"), "Plucks & arps"),
        (role_hue("lead"), "Leads"),
        (role_hue("fx"), "FX"),
    ]
}

/// What a file is, in a few words.
pub(crate) fn midi_line(m: &midi::MidiSample) -> String {
    let s = &m.study;
    let mut line = capital(&s.role);
    if let Some(k) = s.key.as_deref().and_then(Key::parse) {
        line = format!("{line} · {}", k.short());
    }
    if let Some(t) = s.tempo {
        line = format!("{line} · {t:.0}");
    }
    line
}

pub(crate) fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

impl Workspace {
    /// The library's MIDI file `index`, read once and kept, its drum parts on drums.
    pub(crate) fn midi_file(&self, index: usize) -> Option<Arc<MidiFile>> {
        let m = self.browser.library.midi.get(index)?;
        let mut kept = self.browser.midi_files.borrow_mut();
        kept.entry(m.path.clone())
            .or_insert_with(|| {
                let mut f = midi_file::open(&m.path).ok()?;
                midi::mark_drums(&mut f, &m.study);
                Some(Arc::new(f))
            })
            .clone()
    }

    /// The MIDI files that fit what's searched: its words, the part picked, a box on the map, and
    /// the key and tempo the results keep to (a file that would be moved into the key with Play
    /// in Key on fits it). Play in Key alone leaves them all, those in the song's key first.
    pub fn find_midi(&mut self) {
        let song_key = self.session.project.song_key();
        let b = &self.browser;
        let (keep, tempo, moved) = (self.sample_key(), self.sample_bpm(), b.play_in_key);
        let key = keep.or(song_key.filter(|_| moved));
        let found = |role: Option<String>| {
            let q = MidiQuery {
                words: b.text.clone(),
                role,
                key: key.map(|k| k.name()),
                tempo,
                limit: Some(usize::MAX),
            };
            let mut hits = midi::find(&b.library.midi, &q);
            hits.retain(|h| midi::fits(&b.library.midi[h.index], keep, tempo, moved));
            if let Some(area) = &b.area {
                hits.retain(|h| area.contains(&h.index));
            }
            hits
        };
        let pool = found(None);
        let mut counts: HashMap<String, usize> = HashMap::new();
        for h in &pool {
            let parts = &b.library.midi[h.index].study.parts;
            for role in ROLES.iter().filter(|r| parts.iter().any(|p| p.role == **r)) {
                *counts.entry(role.to_string()).or_insert(0) += 1;
            }
        }
        let hits = match &b.midi_role {
            Some(role) => found(Some(role.clone())),
            None => pool,
        };
        let view = b.view;
        let b = &mut self.browser;
        b.midi_hits = hits;
        b.midi_counts = counts;
        b.seen = (song_key, self.session.project.tempo);
        if view == Results::Kit {
            self.build_midi_kit();
        }
    }

    /// Shows MIDI files, or samples again. A box on the map held the other kind.
    pub fn show_midi(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.browser.midi != on {
            self.browser.area = None;
        }
        self.browser.midi = on;
        self.refilter(cx);
    }

    /// A part picked, or let go.
    pub fn toggle_midi_role(&mut self, role: &str, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        b.midi_role = match b.midi_role.as_deref() == Some(role) {
            true => None,
            false => Some(role.to_string()),
        };
        self.refilter(cx);
    }

    /// A chip for each part the library's MIDI files play, with how many have one.
    pub(crate) fn role_chips(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let b = &self.browser;
        ROLES
            .iter()
            .filter_map(|&role| {
                let n = b.midi_counts.get(role).copied().unwrap_or(0);
                let on = b.midi_role.as_deref() == Some(role);
                (n > 0 || on).then(|| {
                    filter_chip(capital(role), Some(n), on, Some(role_hue(role)))
                        .debug_selector(move || format!("midi-role-{role}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.toggle_midi_role(role, cx)),
                        )
                })
            })
            .collect()
    }

    /// Semitones to move a MIDI file by, heard or brought in: into the key the results keep to
    /// (else the song's) with Play in Key on. Drums never move. On the Kit page a kit's parts are
    /// all in its key, whatever Play in Key says.
    pub(crate) fn midi_transpose(&self, index: usize) -> i32 {
        let b = &self.browser;
        let Some(m) = b.library.midi.get(index) else {
            return 0;
        };
        if self.view == View::Samples && b.midi && b.view == Results::Kit {
            let parts = b.midi_kit.iter().flat_map(MidiPick::options);
            if let Some(h) = parts.into_iter().find(|h| h.index == index) {
                return h.transpose;
            }
        }
        let to = self
            .sample_key()
            .or_else(|| self.session.project.song_key());
        match (
            b.play_in_key && !m.study.all_drums(),
            m.study.key.as_deref(),
        ) {
            (true, Some(key)) => match (Key::parse(key), to) {
                (Some(from), Some(to)) => midi::transpose_to(&from, &to),
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Picks a MIDI file (and hears it): its study shows.
    pub fn select_midi(&mut self, index: usize, hear: bool, cx: &mut Context<Self>) {
        self.browser.midi_selected = Some(index);
        if hear {
            self.audition_midi(index, cx);
        }
        cx.notify();
    }

    /// Up and down: the next MIDI file in the results, heard as it's picked.
    pub fn step_midi(&mut self, by: i32, cx: &mut Context<Self>) {
        let b = &self.browser;
        let order: Vec<usize> = match (b.view, self.view) {
            (Results::Kit, View::Samples) => b
                .midi_kit
                .iter()
                .filter_map(|k| Some(k.hit()?.index))
                .collect(),
            _ => b.midi_hits.iter().map(|h| h.index).collect(),
        };
        if order.is_empty() {
            return;
        }
        let at = b
            .midi_selected
            .and_then(|i| order.iter().position(|&o| o == i));
        let next = match at {
            Some(at) => (at as i32 + by).clamp(0, order.len() as i32 - 1) as usize,
            None if by > 0 => 0,
            None => order.len() - 1,
        };
        b.list.scroll_to_item(next, ScrollStrategy::Nearest);
        b.panel_list.scroll_to_item(next, ScrollStrategy::Nearest);
        self.select_midi(order[next], true, cx);
    }

    /// Hears a MIDI file as it would play in the song: at its tempo, moved into its key (with
    /// Play in Key on), on the instruments bringing it in would give it; in time with the song,
    /// over and over, while it plays. Made off the UI thread and kept.
    pub fn audition_midi(&mut self, index: usize, cx: &mut Context<Self>) {
        let (Some(m), Some(file)) = (self.browser.library.midi.get(index), self.midi_file(index))
        else {
            self.status = Some("That MIDI file couldn't be read.".into());
            cx.notify();
            return;
        };
        let semis = self.midi_transpose(index);
        let name = m.path.clone();
        self.hear_midi(index, name, vec![(file, semis)], cx);
    }

    /// `files`, moved, heard together as `index` (a file's, or the kit's), made in the background
    /// once and kept by `name` and how they're moved.
    pub(crate) fn hear_midi(
        &mut self,
        index: usize,
        name: PathBuf,
        files: Vec<(Arc<MidiFile>, i32)>,
        cx: &mut Context<Self>,
    ) {
        let tempo = self.session.project.tempo;
        let moved = files.iter().map(|f| f.1).sum::<i32>() as f32;
        let key = (name, tempo.to_bits(), moved.to_bits());
        self.browser.wanted = None;
        self.browser.midi_wanted = Some(index);
        self.chat.wanted = None;
        if let Some(data) = self.browser.heard.get(&key).cloned() {
            self.play_midi(index, data, cx);
            return;
        }
        let rate = self.player.rate();
        let dir = self.session.dir.clone();
        let made = cx.background_spawn(async move {
            let files: Vec<(&MidiFile, i32)> = files.iter().map(|(f, s)| (&**f, *s)).collect();
            let [l, r] = midi_file::render_all(&files, tempo, rate, &dir)?;
            Ok::<_, String>(Arc::new(pina_engine::audio::engine::AudioData::new(vec![
                l, r,
            ])))
        });
        cx.spawn(async move |ws, cx| {
            let made = made.await;
            ws.update(cx, |ws, cx| match made {
                Ok(data) => {
                    if ws.browser.heard.len() >= 64 {
                        ws.browser.heard.clear();
                    }
                    ws.browser.heard.insert(key, data.clone());
                    if ws.browser.midi_wanted == Some(index) {
                        ws.play_midi(index, data, cx);
                    }
                }
                Err(e) => {
                    ws.status = Some(e);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn play_midi(
        &mut self,
        index: usize,
        data: Arc<pina_engine::audio::engine::AudioData>,
        cx: &mut Context<Self>,
    ) {
        let seconds = data.channels()[0].len() as f32 / self.player.rate() as f32;
        match self.player.audition_synced(data, 0.0) {
            Ok(()) => {
                self.browser.audition = None;
                self.chat.hearing = None;
                self.browser.midi_playing = Some((index, Instant::now(), seconds));
                self.browser.looping = self.player.playing();
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// Whether MIDI file `index` is being heard.
    pub(crate) fn midi_heard(&self, index: usize) -> bool {
        let b = &self.browser;
        b.midi_playing.is_some_and(|(i, since, seconds)| {
            i == index && (b.looping || since.elapsed().as_secs_f32() < seconds)
        })
    }

    /// A file picked on the map or a kit: it's heard, and a drag takes it to the tracks.
    pub(crate) fn pick_up_midi(&mut self, index: usize, at: Point<f32>, cx: &mut Context<Self>) {
        self.select_midi(index, true, cx);
        let drag = Drag::Midi {
            index,
            shift: self.midi_transpose(index),
            from: at,
            at,
            moved: false,
        };
        self.begin_drag(drag, cx);
    }

    /// The pointer over a dot on the map. Making a preview takes a moment, so it's heard once
    /// the pointer rests there, not for every dot it crosses.
    pub(crate) fn hover_midi(&mut self, dot: Option<usize>, cx: &mut Context<Self>) {
        if dot == self.browser.midi_hovered {
            return;
        }
        self.browser.midi_hovered = dot;
        cx.notify();
        let Some(i) = dot else { return };
        let rest = cx.background_executor().timer(Duration::from_millis(140));
        cx.spawn(async move |ws, cx| {
            rest.await;
            ws.update(cx, |ws, cx| {
                if ws.browser.midi_hovered == Some(i) {
                    ws.audition_midi(i, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Over MIDI results: how many, and what they're kept to.
    pub(crate) fn midi_head(&self, cx: &mut Context<Self>) -> Div {
        let n = self.browser.midi_hits.len();
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .whitespace_nowrap()
            .child(match (self.browser.library.midi.len(), n) {
                (0, _) => "No MIDI files yet: add a folder that has some".to_string(),
                (_, 0) => "No MIDI file fits that".to_string(),
                (_, n) => format!("{n} MIDI files · best fit first"),
            })
            .children(self.area_chip(cx))
            .child(div().flex_1())
            .children(self.preview_stop(cx))
    }

    /// Under the map: the hovered (else picked) file in a line, and why it's there.
    pub(crate) fn midi_info_line(&self) -> Div {
        let b = &self.browser;
        let shown = b
            .midi_hovered
            .or(b.midi_selected)
            .and_then(|i| Some((i, b.library.midi.get(i)?)));
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .bg(rgb(SURFACE))
            .text_size(px(11.))
            .whitespace_nowrap()
            .overflow_hidden()
            .children(shown.map(|(i, m)| {
                let why = b.midi_hits.iter().find(|h| h.index == i);
                let why = why.map(|h| h.why.join(", ")).unwrap_or_default();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgb(role_hue(&m.study.role))),
                    )
                    .child(
                        div()
                            .text_color(rgb(TEXT_2))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(m.study.summary.clone()),
                    )
                    .when(!why.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(rgb(ACCENT_TEXT))
                                .child(format!("Why: {why}")),
                        )
                    })
            }))
            .when(shown.is_none(), |d| {
                d.text_color(rgb(TEXT_4)).child(
                    "Near dots play alike · rest on one to hear it · drag it onto the tracks",
                )
            })
    }

    /// The results as MIDI files: how many, and a row each.
    pub(crate) fn midi_list(&mut self, compact: bool, cx: &mut Context<Self>) -> Div {
        let n = self.browser.midi_hits.len();
        let head = self.midi_head(cx);
        let list = uniform_list(
            if compact {
                "midi-panel-list"
            } else {
                "midi-list"
            },
            n,
            cx.processor(move |ws, range: Range<usize>, _, cx| {
                range
                    .filter_map(|pos| {
                        let hit = ws.browser.midi_hits.get(pos)?.clone();
                        Some(ws.midi_row(pos, &hit, compact, cx))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(if compact {
            &self.browser.panel_list
        } else {
            &self.browser.list
        })
        .flex_1();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(head)
            .child(list)
    }

    /// A MIDI file's row: its notes drawn, its name and folder (or progression), its part, key
    /// (and the move into the song's), tempo and bars. A click hears it, a double click puts it
    /// in the song, a drag puts it where it's dropped.
    pub(crate) fn midi_row(
        &self,
        pos: usize,
        hit: &MidiHit,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let b = &self.browser;
        let index = hit.index;
        let m = &b.library.midi[index];
        let s = &m.study;
        let color = role_hue(&s.role);
        let selected = b.midi_selected == Some(index);
        let playing = self.midi_heard(index);
        let roll = self.midi_file(index).map(|f| outline(&f));
        let mono = |text: String, w: f32| {
            div()
                .w(px(w))
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(text)
        };
        // The move that's heard, which is what a row should say.
        let key = match (
            s.key.as_deref().and_then(Key::parse),
            self.midi_transpose(index),
        ) {
            (Some(k), 0) => k.short(),
            (Some(k), t) => format!("{} {t:+}", k.short()),
            (None, _) => "–".into(),
        };
        let below = match (compact, s.progression.is_empty()) {
            (true, false) => s.progression.clone(),
            (false, false) => format!("{} · {}", m.folder, s.progression),
            _ => m.folder.clone(),
        };
        div()
            .id(("midi-row", pos))
            .group("midi-row")
            .debug_selector(move || format!("midi-row-{pos}"))
            .w_full()
            .h(px(if compact { PANEL_ROW_H } else { ROW_H }))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .when(selected, |d| d.bg(rgb(RAISED)))
            .when(!selected, |d| d.hover(|d| d.bg(rgb(OFF))))
            .child(
                play_dot(playing, color, 20.)
                    .debug_selector(move || format!("midi-play-{pos}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            match playing {
                                true => ws.stop_audition(cx),
                                false => ws.select_midi(index, true, cx),
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(if compact { 56. } else { WAVE_W }))
                    .h(px(28.))
                    .flex_none()
                    .rounded(px(4.))
                    .bg(alpha(color, 0.08))
                    .overflow_hidden()
                    .child(piano_roll(roll, color)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(TEXT))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(m.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(below),
                    ),
            )
            .when(!compact, |d| d.child(chip(capital(&s.role), color)))
            .child(mono(key, if compact { 46. } else { KEY_W }))
            .when(!compact, |d| {
                d.child(mono(
                    s.tempo.map_or("–".into(), |t| format!("{t:.0}")),
                    BPM_W,
                ))
                .child(mono(
                    match s.bars {
                        1 => "1 bar".to_string(),
                        n => format!("{n} bars"),
                    },
                    LENGTH_W,
                ))
            })
            .child(self.midi_give_button(&m.path, cx))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        ws.place_midi(index, ws.midi_transpose(index), None, cx);
                        return;
                    }
                    ws.pick_up_midi(index, pt(e.position), cx);
                }),
            )
    }

    /// A MIDI file being dragged: out of the page, the tracks come back to drop it on.
    pub fn drag_midi(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let mut out = false;
        if let Some(Drag::Midi {
            from, at, moved, ..
        }) = &mut self.drag
        {
            *at = p;
            let far = (p.x - from.x).hypot(p.y - from.y);
            *moved |= far > 4.0;
            out = far > 40.0;
        }
        if out && self.view == View::Samples {
            self.view = View::Arrange;
            // With the Agent panel open it stays, to be dropped on as well as the tracks.
            self.show_browser = !self.show_agent;
        }
        cx.notify();
    }

    /// A MIDI file let go of over the tracks: it goes where it's dropped.
    pub fn drop_midi(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        let Some(Drag::Midi {
            index,
            shift,
            at,
            moved: true,
            ..
        }) = drag
        else {
            return;
        };
        if self.over_agent(at) {
            if let Some(path) = self.browser.library.midi.get(index).map(|m| m.path.clone()) {
                self.attach_midi(&[path], cx);
            }
        } else if self.drop_target(at).is_some() {
            self.place_midi(index, shift, Some(at), cx);
        }
    }

    /// The library's MIDI file `index` into the song, moved `shift`: where it's dropped (`at`, a
    /// window point), else at the bar the playhead's in, on new tracks.
    pub fn place_midi(
        &mut self,
        index: usize,
        shift: i32,
        at: Option<Point<f32>>,
        cx: &mut Context<Self>,
    ) {
        let (Some(file), Some(m)) = (self.midi_file(index), self.browser.library.midi.get(index))
        else {
            return;
        };
        let name = m.name.clone();
        self.put_midi(&file, &name, shift, at, cx);
    }

    /// Where a MIDI file dropped at `at` goes: onto the track under it, if that has an
    /// instrument that plays it (drums on a kit, notes on anything else) and the file is one
    /// part; else new tracks under it (at the end, off the tracks).
    fn midi_target(
        &self,
        file: &MidiFile,
        at: Option<Point<f32>>,
    ) -> (Option<usize>, Option<usize>) {
        let drums = file.parts.iter().all(|p| p.is_drums());
        let plays = |t: usize| {
            let i = self.session.project.tracks[t].instrument.as_ref();
            file.parts.len() == 1 && i.is_some_and(|i| i.is_kit() == drums)
        };
        match at.and_then(|p| self.drop_target(p)) {
            Some(Drop::Zone(t, _) | Drop::Under(t, _) | Drop::Pad(t)) if plays(t) => {
                (Some(t), None)
            }
            Some(Drop::Audio(t, _) | Drop::Zone(t, _) | Drop::Under(t, _) | Drop::Pad(t)) => {
                (None, Some(t + 1))
            }
            _ => (None, None),
        }
    }

    /// A MIDI file into the song, moved `shift` semitones (drums never), where it's dropped or
    /// else at the bar the playhead's in: onto the track it's dropped on if that plays it, else
    /// a new track a part.
    pub(crate) fn put_midi(
        &mut self,
        file: &MidiFile,
        name: &str,
        shift: i32,
        at: Option<Point<f32>>,
        cx: &mut Context<Self>,
    ) {
        let bar = self.session.project.bar().max(1);
        let tick = match at.filter(|&p| self.drop_target(p).is_some()) {
            Some(p) => self.snap(self.tick_at(p.x)),
            None => self.playhead() / bar * bar,
        };
        let (onto, below) = self.midi_target(file, at);
        let mut made = Vec::new();
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            match onto {
                Some(t) => {
                    let part = &file.parts[0];
                    let up = if part.is_drums() { 0 } else { shift };
                    p.tracks[t].regions.push(Region {
                        start: tick,
                        length: file.end().div_ceil(bar).max(1) * bar,
                        name: name.to_string(),
                        content: Content::Midi {
                            notes: part
                                .notes
                                .iter()
                                .map(|n| Note {
                                    pitch: (n.pitch as i32 + up).clamp(0, 127) as u8,
                                    ..*n
                                })
                                .collect(),
                            controls: part.controls.clone(),
                        },
                        ..Default::default()
                    });
                    made.push(t);
                }
                None => made = p.import_midi(file, tick, below, shift),
            }
        });
        if let Some(&t) = made.first() {
            self.select_track(t);
        }
        let moved = match shift {
            0 => String::new(),
            s => format!(", moved {s:+} into the key"),
        };
        let tracks = &self.session.project.tracks;
        self.status = Some(match (onto, made.len()) {
            (Some(t), _) => format!("{name} on {}{moved}", tracks[t].name),
            (None, 1) => format!("{name} on a new track{moved}"),
            (None, n) => format!("{name} on {n} new tracks{moved}"),
        });
        cx.notify();
    }

    /// MIDI files from the Finder, dropped on the tracks: each where it's dropped, as from the
    /// library (into the song's key with Play in Key on, if its key can be told).
    pub(crate) fn drop_midi_files(
        &mut self,
        files: &[&PathBuf],
        at: Point<f32>,
        cx: &mut Context<Self>,
    ) {
        for path in files {
            let mut file = match midi_file::open(path) {
                Ok(f) => f,
                Err(e) => {
                    self.status = Some(e);
                    continue;
                }
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let study = midi::study(&file, &name);
            midi::mark_drums(&mut file, &study);
            let shift = match (
                self.browser.play_in_key && study.role != "drums",
                study.key.as_deref().and_then(Key::parse),
                self.session.project.song_key(),
            ) {
                (true, Some(from), Some(to)) => midi::transpose_to(&from, &to),
                _ => 0,
            };
            self.put_midi(&file, &name, shift, Some(at), cx);
        }
        cx.notify();
    }

    /// The MIDI file being dragged, by the pointer, saying where it would go.
    pub fn midi_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Midi {
            index,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let m = self.browser.library.midi.get(index)?;
        let file = self.midi_file(index)?;
        let color = role_hue(&m.study.role);
        let hint = match self.over_agent(at) {
            true => "To the Studio Agent".to_string(),
            false => self.midi_hint(&file, at),
        };
        Some(midi_ghost_at(at, &file, m.name.clone(), color, hint))
    }

    /// Where a MIDI file let go of at `at` would go.
    pub(crate) fn midi_hint(&self, file: &MidiFile, at: Point<f32>) -> String {
        let parts = file.parts.len();
        match (self.drop_target(at), self.midi_target(file, Some(at))) {
            (None, _) => "Drop it on the tracks".to_string(),
            (_, (Some(t), _)) => format!("On {}", self.session.project.tracks[t].name),
            _ if parts == 1 => "On a new track".to_string(),
            _ => format!("On {parts} new tracks"),
        }
    }

    fn add_midi_button(&self, index: usize, cx: &mut Context<Self>) -> Div {
        primary(match self.midi_transpose(index) {
            0 => "Add to song".to_string(),
            s => format!("Add to song ({s:+})"),
        })
        .w_full()
        .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
        .flex_row_reverse()
        .debug_selector(|| "midi-add".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                ws.place_midi(index, ws.midi_transpose(index), None, cx)
            }),
        )
    }

    /// The compact browser's foot, a MIDI file picked: what it is, its notes, and a way in.
    pub(crate) fn midi_footer(&self, cx: &mut Context<Self>) -> Option<Div> {
        let index = self.browser.midi_selected?;
        let m = self.browser.library.midi.get(index)?;
        let color = role_hue(&m.study.role);
        Some(
            div()
                .flex_none()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_t_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(chip(capital(&m.study.role), color))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(m.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(m.study.summary.clone()),
                )
                .child(
                    div()
                        .h(px(40.))
                        .rounded(px(6.))
                        .bg(alpha(color, 0.08))
                        .overflow_hidden()
                        .child(piano_roll(
                            self.midi_file(index).map(|f| outline(&f)),
                            color,
                        )),
                )
                .child(self.add_midi_button(index, cx)),
        )
    }

    /// The picked MIDI file's study, beside the results: what it is, its key, tempo, length and
    /// progression, each part and how it plays, and putting it in the song.
    pub(crate) fn midi_details(&mut self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let column = div()
            .w(px(DETAILS_W))
            .flex_none()
            .h_full()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .flex()
            .flex_col();
        let Some(index) = b.midi_selected.filter(|&i| i < b.library.midi.len()) else {
            return column.child(
                div()
                    .p(px(16.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child("Pick a MIDI file to see what it plays."),
            );
        };
        let m = &b.library.midi[index];
        let s = &m.study;
        let color = role_hue(&s.role);
        let fact = |name: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(9.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(name.to_uppercase()),
                )
                .child(div().font_family(MONO).text_size(px(12.5)).child(value))
        };
        let parts = s.parts.iter().map(|p| {
            let hue = role_hue(&p.role);
            let steps = p.steps.clone();
            div()
                .p(px(10.))
                .rounded(px(9.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(chip(capital(&p.role), hue))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(11.5))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(match &p.instrument {
                                    Some(i) => format!("{} · {i}", p.name),
                                    None => p.name.clone(),
                                }),
                        ),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(format!(
                            "{} notes, {}–{} · {:.1} at once · {:.1} a bar · {:.0}% off the \
                             eighths · swing {:.0} · velocity {}–{}",
                            p.notes,
                            p.low,
                            p.high,
                            p.polyphony,
                            p.density,
                            p.syncopation * 100.0,
                            p.swing,
                            p.velocity[0],
                            p.velocity[1],
                        )),
                )
                // How often each step of the bar starts a note.
                .child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .children(steps.into_iter().enumerate().map(move |(k, v)| {
                            div()
                                .flex_1()
                                .h(px(12.))
                                .rounded(px(2.))
                                .bg(alpha(hue, 0.1 + 0.85 * v.clamp(0.0, 1.0)))
                                .when(k % 4 == 0, |d| d.border_t_2().border_color(rgb(TEXT_4)))
                        })),
                )
                .children(p.drums.iter().map(|(name, row)| {
                    div()
                        .flex()
                        .gap(px(8.))
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_color(rgb(TEXT_3))
                                .overflow_hidden()
                                .child(name.clone()),
                        )
                        .child(div().text_color(rgb(TEXT_2)).child(row.clone()))
                }))
                .children(p.contour.as_ref().map(|c| {
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(format!(
                            "Melody {}: {:.0}% steps, {:.0}% leaps, over {} semitones",
                            c.direction,
                            c.steps * 100.0,
                            c.leaps * 100.0,
                            c.span
                        ))
                }))
        });
        let body = div()
            .id("midi-details")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(m.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(m.folder.clone()),
                    ),
            )
            .child(
                div()
                    .h(px(76.))
                    .rounded(px(8.))
                    .bg(alpha(color, 0.08))
                    .overflow_hidden()
                    .child(piano_roll(
                        self.midi_file(index).map(|f| outline(&f)),
                        color,
                    )),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(s.summary.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(18.))
                    .child(fact("Key", s.key.clone().unwrap_or("–".into())))
                    .child(fact(
                        "Tempo",
                        s.tempo.map_or("–".into(), |t| format!("{t:.0}")),
                    ))
                    .child(fact(
                        "Meter",
                        format!("{}/{}", s.time_signature[0], s.time_signature[1]),
                    ))
                    .child(fact("Bars", s.bars.to_string()))
                    .child(fact("Loops every", format!("{} bars", s.loop_bars))),
            )
            .when(!s.progression.is_empty(), |d| {
                d.child(fact("Progression", s.progression.clone())).child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(s.chords.join(" · ")),
                )
            })
            .children(parts);
        column.child(body).child(
            div()
                .flex_none()
                .p(px(12.))
                .border_t_1()
                .border_color(rgb(LINE))
                .child(self.add_midi_button(index, cx)),
        )
    }
}

/// A file's notes for drawing: each as its start and end, shares of the file's length, and its
/// pitch; and the lowest and highest pitch.
/// A MIDI file being carried, by the pointer.
pub(crate) fn midi_ghost_at(
    at: Point<f32>,
    file: &MidiFile,
    name: String,
    color: u32,
    hint: String,
) -> Div {
    div()
        .absolute()
        .left(px(at.x + 14.))
        .top(px(at.y + 10.))
        .w(px(230.))
        .p(px(8.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(9.))
        .bg(rgb(0x1c1c22))
        .border_1()
        .border_color(alpha(color, 0.7))
        .shadow_lg()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(56.))
                        .h(px(20.))
                        .flex_none()
                        .child(piano_roll(Some(outline(file)), color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name),
                ),
        )
        .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(hint))
}

pub(crate) type Outline = (Vec<(f32, f32, u8)>, u8, u8);

pub(crate) fn outline(f: &MidiFile) -> Outline {
    let end = f.end().max(1) as f32;
    let (mut lo, mut hi) = (127u8, 0u8);
    let mut notes = Vec::new();
    for n in f.parts.iter().flat_map(|p| &p.notes) {
        let a = n.start.max(0) as f32 / end;
        notes.push((a, (a + n.length as f32 / end).min(1.0), n.pitch));
        (lo, hi) = (lo.min(n.pitch), hi.max(n.pitch));
    }
    (notes, lo, hi.max(lo))
}

/// A little piano roll: each note a bar across its time, up by its pitch.
pub(crate) fn piano_roll(roll: Option<Outline>, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let Some((notes, lo, hi)) = &roll else {
                return;
            };
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let rows = (hi - lo).max(11) as f32 + 1.0;
            let row = (h - 4.0) / rows;
            for &(a, z, pitch) in notes {
                let y = h - 2.0 - (pitch - lo + 1) as f32 * row;
                window.paint_quad(fill(
                    Bounds::new(
                        point(b.origin.x + px(a * w), b.origin.y + px(y)),
                        size(px(((z - a) * w).max(1.5)), px(row.max(1.5))),
                    ),
                    alpha(color, 0.9),
                ));
            }
        },
    )
    .size_full()
}
