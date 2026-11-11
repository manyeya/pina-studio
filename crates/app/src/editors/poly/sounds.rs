//! The Sounds page: the whole library a page of its own. Down the left where to look (all of
//! it, the factory's, those made here, liked, played lately) and every pack, searched; in the
//! middle the sounds, searched and narrowed by part and by what they sound like, sorted; on the
//! right the one picked, all of it: its pack, its words, its macros, what's in it; loaded,
//! heard, moved or taken out. And the sound in the synth as saved: Save lights when it's
//! changed, Compare plays it as saved, Undo all goes back to it.

use super::*;
use library::{CHARACTER, Entry, FACTORY, Library, PARTS};

/// Where the library looks.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Place {
    #[default]
    Everything,
    Factory,
    Mine,
    Liked,
    Recent,
    Pack(String),
}

/// How the list is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Sort {
    #[default]
    Newest,
    Name,
    Part,
    Pack,
}

impl Sort {
    fn name(self) -> &'static str {
        match self {
            Sort::Newest => "Newest first",
            Sort::Name => "By name",
            Sort::Part => "By part",
            Sort::Pack => "By pack",
        }
    }
}

/// The page's own state, and the sound as saved.
pub struct LibView {
    pub place: Place,
    pub part: Option<&'static str>,
    pub words: Vec<&'static str>,
    pub sort: Sort,
    /// The sound shown on the right, by its key.
    pub picked: Option<String>,
    pub find: Entity<TextInput>,
    pub query: String,
    pub pack_find: Entity<TextInput>,
    pub pack_query: String,
    /// The keys as the list last showed them: the title's arrows step through these.
    pub shown: Vec<String>,
    /// The sound as it was loaded or saved: changed from it, Save lights.
    pub saved: Option<Poly>,
    /// Playing it as saved, the changes kept aside.
    pub comparing: bool,
    /// Taken out lately, to bring back.
    pub taken: Vec<Entry>,
    pub dialog: Option<saving::Dialog>,
    /// Save, waiting while a pack's made for it.
    pub pending: Option<saving::Dialog>,
}

impl LibView {
    pub fn new(cx: &mut Context<Workspace>) -> (LibView, Vec<Subscription>) {
        let find = cx.new(|cx| {
            TextInput::search("Search names, packs, makers and what they sound like", cx)
        });
        let pack_find = cx.new(|cx| TextInput::search("Find a pack", cx));
        let subs = vec![
            cx.observe(&find, |ws, input, cx| {
                let text = input.read(cx).text().trim().to_lowercase();
                if let Some(v) = ws.poly_view.as_mut().filter(|v| v.lib.query != text) {
                    v.lib.query = text;
                    cx.notify();
                }
            }),
            cx.observe(&pack_find, |ws, input, cx| {
                let text = input.read(cx).text().trim().to_lowercase();
                if let Some(v) = ws.poly_view.as_mut().filter(|v| v.lib.pack_query != text) {
                    v.lib.pack_query = text;
                    cx.notify();
                }
            }),
        ];
        (
            LibView {
                place: Place::Everything,
                part: None,
                words: Vec::new(),
                sort: Sort::Newest,
                picked: None,
                find,
                query: String::new(),
                pack_find,
                pack_query: String::new(),
                shown: Vec::new(),
                saved: None,
                comparing: false,
                taken: Vec::new(),
                dialog: None,
                pending: None,
            },
            subs,
        )
    }
}

/// A cover's colours, by its seed.
const PALETTE: [(u32, u32); 8] = [
    (0x5b8cff, 0xa78bfa),
    (0x38bdf8, 0x34d399),
    (0xf472b6, 0x7c3aed),
    (0xc08457, 0x3f3f46),
    (0x22d3ee, 0xf472b6),
    (0xa3e635, 0x0d9488),
    (0xfde68a, 0xf472b6),
    (0x1e3a8a, 0x38bdf8),
];

/// A seed from a name: a pack has a cover of its own without anyone painting one.
pub(crate) fn seed_of(name: &str) -> u32 {
    name.bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
}

/// A pack's cover: two colours and waves drawn from its seed (the factory's grey).
pub(crate) fn cover(seed: Option<u32>, lines: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (a, c) = match seed {
                Some(s) => PALETTE[s as usize % PALETTE.len()],
                None => (0x2b2b33, 0x55555f),
            };
            window.paint_quad(
                gpui::fill(
                    b,
                    gpui::linear_gradient(
                        135.,
                        gpui::linear_color_stop(rgb(a), 0.),
                        gpui::linear_color_stop(rgb(c), 1.),
                    ),
                )
                .corner_radii(px(4.)),
            );
            let s = seed.unwrap_or(1) as f32;
            for k in 0..7 {
                let fr = 1.0 + ((s as u32 / 3 + k * 3) % 5) as f32 * 0.3;
                let (ph, amp) = (s * 1.7 + k as f32 * 0.7, 0.07 + k as f32 * 0.022);
                let at = 0.16 + k as f32 * 0.11;
                polyline(
                    window,
                    b,
                    (0..=40).map(|i| {
                        let x = i as f32 / 40.0;
                        (x, at + (x * std::f32::consts::TAU * fr + ph).sin() * amp)
                    }),
                    lines,
                    gpui::white().opacity(0.5),
                );
            }
        },
    )
    .size_full()
}

/// A day, as the list says it: today, yesterday, or its date.
fn day(secs: u64) -> String {
    if secs == 0 {
        return "–".into();
    }
    let (days, today) = (secs / 86_400, library::now() / 86_400);
    match today.saturating_sub(days) {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => {
            // The civil date from days since 1970.
            let z = days as i64 + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{d} {}", MONTHS[(m - 1) as usize])
        }
    }
}

/// The sounds `place` holds, narrowed by part, words and search, sorted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn listed<'a>(
    lib: &'a Library,
    place: &Place,
    part: Option<&str>,
    words: &[&str],
    query: &str,
    sort: Sort,
    liked: &[String],
    recent: &[String],
) -> Vec<&'a Entry> {
    let q: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut list: Vec<&Entry> = lib
        .sounds
        .iter()
        .filter(|e| match place {
            Place::Everything => true,
            Place::Factory => e.factory(),
            Place::Mine => {
                !e.factory()
                    && e.pack
                        .as_deref()
                        .and_then(|p| lib.pack(p))
                        .is_none_or(|p| !p.imported)
            }
            Place::Liked => liked.contains(&e.key),
            Place::Recent => recent.contains(&e.key),
            Place::Pack(p) => e.pack.as_deref() == Some(p.as_str()),
        })
        .filter(|e| part.is_none_or(|p| e.sound.part.eq_ignore_ascii_case(p)))
        .filter(|e| words.iter().all(|w| e.sound.words.iter().any(|x| x == w)))
        .filter(|e| {
            let all = format!(
                "{} {} {} {} {} {}",
                e.sound.name,
                e.sound.part,
                e.sound.about,
                e.sound.by,
                e.pack.as_deref().unwrap_or_default(),
                e.sound.words.join(" ")
            )
            .to_lowercase();
            q.iter().all(|w| all.contains(w))
        })
        .collect();
    match (place, sort) {
        // Played lately: the latest first, whatever the sort.
        (Place::Recent, _) => list.sort_by_key(|e| recent.iter().position(|r| *r == e.key)),
        // A pack in its own order, as its maker put it.
        (Place::Pack(p), Sort::Newest) => {
            let order = lib.pack(p).map(|p| p.order.clone()).unwrap_or_default();
            list.sort_by_key(|e| {
                order
                    .iter()
                    .position(|o| *o == e.sound.name)
                    .unwrap_or(usize::MAX)
            });
        }
        (_, Sort::Newest) => list.sort_by_key(|e| std::cmp::Reverse(e.sound.made)),
        (_, Sort::Name) => list.sort_by_key(|e| e.sound.name.to_lowercase()),
        (_, Sort::Part) => {
            list.sort_by_key(|e| (e.sound.part.clone(), e.sound.name.to_lowercase()))
        }
        (_, Sort::Pack) => list.sort_by_key(|e| (e.pack.clone(), e.sound.name.to_lowercase())),
    }
    list
}

impl Workspace {
    /// The library, read from its folder the first time it's wanted.
    pub(crate) fn library(&mut self) -> &mut Library {
        let dir = self.sounds_dir.clone();
        self.library.get_or_insert_with(|| Library::read(&dir))
    }

    /// The sound with `key` into track `t`'s synth: one edit; remembered as played lately.
    pub(crate) fn load_sound(&mut self, t: usize, key: &str, cx: &mut Context<Self>) {
        let Some(e) = self.library().find(key).cloned() else {
            return;
        };
        let mut poly = e.sound.poly.clone();
        poly.patch = e.key.clone();
        self.edit(cx, |ws| {
            if let Some(p) = ws.poly_mut(t) {
                *p = poly.clone();
            }
        });
        if let Some(v) = self.poly_view.as_mut() {
            v.lib.saved = Some(poly);
            v.lib.comparing = false;
            v.lib.picked = Some(e.key.clone());
        }
        let recent = &mut self.settings.recent_sounds;
        recent.retain(|r| *r != e.key);
        recent.insert(0, e.key);
        recent.truncate(50);
        self.settings.save();
        self.compare_heard();
    }

    /// The sound as saved (or loaded), from the library or the factory, for what's in `p`.
    pub(crate) fn as_saved(&mut self, p: &Poly) -> Option<Poly> {
        let key = match p.patch.is_empty() {
            true => "Init".to_string(),
            false => p.patch.clone(),
        };
        let mut poly = self.library().find(&key)?.sound.poly.clone();
        poly.patch = p.patch.clone();
        Some(poly)
    }

    /// Whether the synth's sound has changed from how it was saved. Another sound in it (one
    /// loaded, or an undo past loading): what it's measured against follows.
    pub(crate) fn sound_changed(&mut self, p: &Poly) -> bool {
        let stale = self
            .poly_view
            .as_ref()
            .is_some_and(|v| v.lib.saved.as_ref().is_none_or(|s| s.patch != p.patch));
        if stale {
            let saved = self.as_saved(p);
            if let Some(v) = self.poly_view.as_mut() {
                v.lib.saved = saved;
                v.lib.comparing = false;
            }
        }
        self.poly_view
            .as_ref()
            .and_then(|v| v.lib.saved.as_ref())
            .is_some_and(|s| s != p)
    }

    /// The engine plays the sound as saved while comparing, else as it is.
    pub(crate) fn compare_heard(&mut self) {
        let project = self.engine_project().into_owned();
        self.status = self.player.sync(&project).err();
    }

    /// Something done to the library (or the sound as saved).
    pub(crate) fn sound_op(&mut self, t: usize, op: SoundOp, cx: &mut Context<Self>) {
        match op {
            SoundOp::Load(key) => self.load_sound(t, &key, cx),
            SoundOp::Compare => {
                if let Some(v) = self.poly_view.as_mut() {
                    v.lib.comparing ^= true;
                }
                self.compare_heard();
            }
            SoundOp::UndoAll => {
                let Some(saved) = self.poly_view.as_ref().and_then(|v| v.lib.saved.clone()) else {
                    return;
                };
                self.edit(cx, |ws| {
                    if let Some(p) = ws.poly_mut(t) {
                        *p = saved.clone();
                    }
                });
                if let Some(v) = self.poly_view.as_mut() {
                    v.lib.comparing = false;
                }
                self.compare_heard();
            }
            SoundOp::Copy => {
                if let Some(p) = self.poly(t)
                    && let Ok(json) = serde_json::to_string(p)
                {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
                    self.status = Some("The sound's copied: paste it into any Synth-0".into());
                }
            }
            SoundOp::Paste => {
                let pasted = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text())
                    .and_then(|t| serde_json::from_str::<Poly>(&t).ok());
                match pasted {
                    Some(poly) => self.edit(cx, |ws| {
                        if let Some(p) = ws.poly_mut(t) {
                            *p = poly.clone();
                        }
                    }),
                    None => {
                        self.status =
                            Some("There's no sound to paste: copy one from a Synth-0 first".into())
                    }
                }
            }
            SoundOp::Show => {
                if let Some(v) = self.poly_view.as_mut() {
                    v.page = PolyPage::Sounds;
                    v.lib.picked =
                        self.session
                            .project
                            .tracks
                            .get(t)
                            .and_then(|tr| match &tr.instrument {
                                Some(pina_engine::song::project::Instrument::Poly(p)) => {
                                    Some(p.patch.clone())
                                }
                                _ => None,
                            });
                }
            }
            SoundOp::Move(key, to) => {
                let done = self.library().move_sound(&key, to.as_deref());
                match done {
                    Ok(new) => {
                        self.renamed_sound(t, &key, &new);
                        self.status = Some(format!(
                            "Moved to {}",
                            to.as_deref().unwrap_or("no pack: just yours")
                        ));
                    }
                    Err(e) => self.status = Some(e),
                }
            }
            SoundOp::TakeOut(key) => {
                let done = self.library().remove(&key);
                match done {
                    Ok(e) => {
                        self.status = Some(format!(
                            "{} taken out: Bring it back is under the list",
                            e.sound.name
                        ));
                        if let Some(v) = self.poly_view.as_mut() {
                            v.lib.picked = None;
                            v.lib.taken.push(e);
                        }
                    }
                    Err(e) => self.status = Some(e),
                }
            }
            SoundOp::BringBack => {
                let Some(e) = self.poly_view.as_mut().and_then(|v| v.lib.taken.pop()) else {
                    return;
                };
                let done = self.library().bring_back(e);
                if let Err(e) = done {
                    self.status = Some(e);
                }
            }
        }
        cx.notify();
    }

    /// A sound's key changed (moved): the synth playing it follows.
    fn renamed_sound(&mut self, t: usize, from: &str, to: &str) {
        if let Some(p) = self.poly_mut(t).filter(|p| p.patch == from) {
            p.patch = to.to_string();
        }
        if let Some(v) = self.poly_view.as_mut() {
            if let Some(s) = v.lib.saved.as_mut().filter(|s| s.patch == from) {
                s.patch = to.to_string();
            }
            if v.lib.picked.as_deref() == Some(from) {
                v.lib.picked = Some(to.to_string());
            }
        }
    }

    /// The Sounds page.
    pub(crate) fn poly_sounds_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        // Read now, if not yet.
        self.library();
        let Some(v) = self.poly_view.as_ref() else {
            return div();
        };
        let lib = self.library.as_ref().expect("read");
        let (liked, recent) = (&self.settings.liked_sounds, &self.settings.recent_sounds);
        let lv = &v.lib;
        let list = listed(
            lib, &lv.place, lv.part, &lv.words, &lv.query, lv.sort, liked, recent,
        );
        let keys: Vec<String> = list.iter().map(|e| e.key.clone()).collect();
        let picked = lv
            .picked
            .as_ref()
            .and_then(|k| lib.find(k))
            .or_else(|| lib.find(&p.patch))
            .cloned();
        let left = self.lib_places(t, cx);
        let middle = self.lib_list(t, p, &list, cx);
        let right = picked.map(|e| self.lib_detail(t, &e, cx));
        if let Some(v) = self.poly_view.as_mut() {
            v.lib.shown = keys;
        }
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(left)
            .child(middle)
            .children(right)
    }

    /// Down the left: where to look, then every pack, searched; new, and brought in.
    fn lib_places(&self, t: usize, cx: &mut Context<Self>) -> Div {
        let v = &self.poly_view.as_ref().expect("open").lib;
        let lib = self.library.as_ref().expect("read");
        let (liked, recent) = (&self.settings.liked_sounds, &self.settings.recent_sounds);
        let count =
            |place: &Place| listed(lib, place, None, &[], "", Sort::Name, liked, recent).len();
        let place = |icon_: Icon, name: &'static str, to: Place, cx: &mut Context<Self>| {
            let on = v.place == to;
            let n = count(&to);
            let sel = format!("poly-lib-place-{}", name.to_lowercase().replace(' ', "-"));
            div()
                .h(px(32.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(7.))
                .text_size(px(13.))
                .cursor_pointer()
                .bg(rgb(if on { RAISED } else { 0x111115 }))
                .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                .when(on, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || sel.clone())
                .child(icon(icon_, 13.).text_color(rgb(TEXT_3)))
                .child(div().flex_1().child(name))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(n.to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.lib.place = to.clone();
                        }
                        cx.notify();
                    }),
                )
        };
        let packs: Vec<Div> = std::iter::once((FACTORY.to_string(), "Pina".to_string(), None))
            .chain(lib.packs.iter().map(|p| {
                let by = match p.imported {
                    true => format!(
                        "{} · brought in",
                        if p.by.is_empty() { "someone" } else { &p.by }
                    ),
                    false => "yours".to_string(),
                };
                (
                    p.name.clone(),
                    by,
                    Some(if p.cover == 0 {
                        seed_of(&p.name)
                    } else {
                        p.cover
                    }),
                )
            }))
            .filter(|(name, by, _)| {
                let all = format!("{name} {by}").to_lowercase();
                v.pack_query.split_whitespace().all(|w| all.contains(w))
            })
            .map(|(name, by, seed)| {
                let to = Place::Pack(name.clone());
                let on = v.place == to;
                let n = lib.count(&name);
                let sel = format!("poly-lib-pack-{name}");
                div()
                    .h(px(40.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .bg(rgb(if on { 0x24242b } else { 0x111115 }))
                    .when(on, |d| d.border_l_2().border_color(rgb(ACCENT)))
                    .hover(|d| d.bg(rgb(RAISED)))
                    .debug_selector(move || sel.clone())
                    .child(div().size(px(26.)).flex_none().child(cover(seed, 1.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(name.clone()),
                            )
                            .child(div().text_size(px(10.5)).text_color(rgb(TEXT_3)).child(by)),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(n.to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(v) = ws.poly_view.as_mut() {
                                v.lib.place = to.clone();
                            }
                            cx.notify();
                        }),
                    )
            })
            .collect();
        div()
            .w(px(232.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(0x111115))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(place(Icon::Grid, "Everything", Place::Everything, cx))
                    .child(place(Icon::Samples, "Factory", Place::Factory, cx))
                    .child(place(Icon::Pencil, "Made by me", Place::Mine, cx))
                    .child(place(Icon::Heart, "Liked", Place::Liked, cx))
                    .child(place(Icon::Rescan, "Played lately", Place::Recent, cx)),
            )
            .child(div().h(px(1.)).mx(px(10.)).bg(rgb(LINE)))
            .child(
                div()
                    .px(px(14.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_3))
                            .child("PACKS"),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child((lib.packs.len() + 1).to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        crate::mix::eq::icon_button(Icon::Plus, "poly-lib-new-pack").on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, window, cx| {
                                cx.stop_propagation();
                                ws.open_pack_dialog(None, Some(window), cx);
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .px(px(10.))
                    .pb(px(6.))
                    .child(crate::pages::genres::field_box(
                        Some(v.pack_find.clone()),
                        "poly-lib-pack-search",
                    )),
            )
            .child(
                div()
                    .id("poly-lib-packs")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(packs),
            )
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        button("Bring in a pack…", false)
                            .h(px(30.))
                            .text_size(px(12.))
                            .debug_selector(|| "poly-lib-import".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.import_pack(t, cx);
                                }),
                            ),
                    )
                    .child(label("A pack file from anyone with Pina")),
            )
    }

    /// The middle: search, part and words, the order; the sounds.
    fn lib_list(&self, t: usize, p: &Poly, list: &[&Entry], cx: &mut Context<Self>) -> Div {
        let v = &self.poly_view.as_ref().expect("open").lib;
        let lib = self.library.as_ref().expect("read");
        let liked = &self.settings.liked_sounds;
        let chip = |name: &'static str, on: bool, sel: String| {
            div()
                .h(px(26.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(13.))
                .text_size(px(12.))
                .cursor_pointer()
                .border_1()
                .when(on, |d| {
                    d.bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .border_color(rgb(TEXT))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!on, |d| {
                    d.bg(rgb(CARD))
                        .text_color(rgb(TEXT_2))
                        .border_color(rgb(LINE))
                        .hover(|d| d.bg(rgb(RAISED)))
                })
                .debug_selector(move || sel.clone())
                .child(name)
        };
        let in_place = listed(
            lib,
            &v.place,
            None,
            &[],
            &v.query,
            Sort::Name,
            liked,
            &self.settings.recent_sounds,
        );
        let parts: Vec<Div> = std::iter::once(None)
            .chain(PARTS.iter().copied().map(Some))
            .map(|part| {
                let on = v.part == part;
                let n = in_place
                    .iter()
                    .filter(|e| part.is_none_or(|p| e.sound.part.eq_ignore_ascii_case(p)))
                    .count();
                chip(
                    part.unwrap_or("All"),
                    on,
                    format!("poly-lib-part-{}", part.unwrap_or("all").to_lowercase()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .opacity(0.6)
                        .child(match part {
                            Some(_) => n.to_string(),
                            None => String::new(),
                        }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.lib.part = part;
                        }
                        cx.notify();
                    }),
                )
            })
            .collect();
        let words: Vec<Div> = CHARACTER
            .iter()
            .map(|&w| {
                let on = v.words.contains(&w);
                chip(w, on, format!("poly-lib-word-{}", w.to_lowercase())).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            match v.lib.words.iter().position(|x| *x == w) {
                                Some(i) => drop(v.lib.words.remove(i)),
                                None => v.lib.words.push(w),
                            }
                        }
                        cx.notify();
                    }),
                )
            })
            .collect();
        let sort = v.sort;
        let order = self.poly_menu(
            "poly-lib-sort".into(),
            sort.name().into(),
            move |_| {
                [Sort::Newest, Sort::Name, Sort::Part, Sort::Pack]
                    .map(|s| {
                        (
                            s.name().to_string(),
                            MenuAction::Poly(t, PolyOp::LibSort(s)),
                        )
                    })
                    .to_vec()
            },
            false,
            cx,
        );
        const COLS: [f32; 7] = [34., 0., 74., 0., 0., 78., 44.];
        let grid = |d: Div| d.flex().items_center().gap(px(10.));
        let head = grid(div())
            .px(px(14.))
            .py(px(6.))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(LINE))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .child(div().w(px(COLS[0])))
            .child(div().flex_1().min_w(px(0.)).child("Name"))
            .child(div().w(px(COLS[2])).child("Part"))
            .child(div().w(px(150.)).child("Sounds like"))
            .child(div().w(px(140.)).child("Pack"))
            .child(div().w(px(COLS[5])).child("Added"))
            .child(div().w(px(COLS[6])));
        let now = p.patch.clone();
        let picked = v.picked.clone();
        let demo = v_demo(self);
        let rows: Vec<gpui::Stateful<Div>> =
            list.iter()
                .map(|e| {
                    let key = e.key.clone();
                    let loved = liked.contains(&key);
                    let on =
                        picked.as_deref() == Some(key.as_str()) || (picked.is_none() && now == key);
                    let pack = e.pack.clone();
                    let seed = pack.as_deref().filter(|p| *p != FACTORY).map(|p| {
                        lib.pack(p).map_or(seed_of(p), |i| {
                            if i.cover == 0 { seed_of(p) } else { i.cover }
                        })
                    });
                    let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
                    grid(div())
                        .id(gpui::SharedString::from(format!("poly-sound-{key}")))
                        .h(px(46.))
                        .flex_none()
                        .px(px(14.))
                        .border_b_1()
                        .border_color(rgb(0x1d1d23))
                        .cursor_pointer()
                        .when(on, |d| {
                            d.bg(rgb(0x1f2333)).border_l_2().border_color(rgb(ACCENT))
                        })
                        .when(!on, |d| d.hover(|d| d.bg(rgb(CARD))))
                        .debug_selector({
                            let k = key.clone();
                            move || format!("poly-sound-{k}")
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                if let Some(v) = ws.poly_view.as_mut() {
                                    v.lib.picked = Some(k1.clone());
                                }
                                // A double-click, or a sound clicked while stepping, loads it.
                                if e.click_count == 2 || ws.settings.load_as_step {
                                    ws.load_sound(t, &k1, cx);
                                }
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .w(px(COLS[0]))
                                .flex()
                                .justify_center()
                                .debug_selector({
                                    let k = key.clone();
                                    move || format!("poly-like-{k}")
                                })
                                .child(
                                    icon(
                                        if loved {
                                            Icon::HeartFilled
                                        } else {
                                            Icon::Heart
                                        },
                                        14.,
                                    )
                                    .text_color(rgb(if loved { PINK } else { TEXT_4 })),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        let list = &mut ws.settings.liked_sounds;
                                        match list.iter().position(|l| *l == k2) {
                                            Some(i) => drop(list.remove(i)),
                                            None => list.push(k2.clone()),
                                        }
                                        ws.settings.save();
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(e.sound.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_3))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(e.sound.about.clone()),
                                ),
                        )
                        .child(
                            div()
                                .w(px(COLS[2]))
                                .text_size(px(12.))
                                .text_color(rgb(TEXT_2))
                                .child(e.sound.part.clone()),
                        )
                        .child(
                            div()
                                .w(px(150.))
                                .flex()
                                .gap(px(4.))
                                .overflow_hidden()
                                .children(e.sound.words.iter().take(2).map(|w| {
                                    div()
                                        .h(px(20.))
                                        .px(px(7.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(10.))
                                        .bg(rgb(FIELD))
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_2))
                                        .whitespace_nowrap()
                                        .child(w.clone())
                                })),
                        )
                        .child(
                            div()
                                .w(px(140.))
                                .flex()
                                .items_center()
                                .gap(px(7.))
                                .child(div().size(px(16.)).flex_none().child(cover(seed, 0.6)))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(TEXT_2))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(pack.unwrap_or_else(|| "No pack".into())),
                                ),
                        )
                        .child(
                            div()
                                .w(px(COLS[5]))
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(day(e.sound.made)),
                        )
                        .child(
                            div().w(px(COLS[6])).flex().justify_center().child(
                                div()
                                    .size(px(26.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(13.))
                                    .border_1()
                                    .border_color(rgb(LINE_STRONG))
                                    .text_color(rgb(TEXT_3))
                                    .hover(|d| {
                                        d.bg(rgb(ACCENT))
                                            .border_color(rgb(ACCENT))
                                            .text_color(rgb(0xffffff))
                                    })
                                    .debug_selector({
                                        let k = key.clone();
                                        move || format!("poly-hear-{k}")
                                    })
                                    .child(icon(Icon::Play, 10.).text_color(rgb(TEXT_2)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.load_sound(t, &k3, cx);
                                            if !demo
                                                && ws
                                                    .poly_view
                                                    .as_ref()
                                                    .is_some_and(|v| v.demo.is_none())
                                            {
                                                ws.poly_demo(cx);
                                            }
                                        }),
                                    ),
                            ),
                        )
                })
                .collect();
        let none = rows.is_empty();
        let step = self.settings.load_as_step;
        // A pack open: what it is, and the way to share it or change it.
        let header = match &v.place {
            Place::Pack(name) if name != FACTORY => lib.pack(name).map(|info| {
                let (n1, n2) = (name.clone(), name.clone());
                let seed = if info.cover == 0 {
                    seed_of(name)
                } else {
                    info.cover
                };
                div()
                    .flex()
                    .gap(px(16.))
                    .p(px(14.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .size(px(84.))
                            .flex_none()
                            .rounded(px(10.))
                            .overflow_hidden()
                            .child(cover(Some(seed), 1.2)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(name.clone()),
                            )
                            .child(label(format!(
                                "{} · {} sounds{}",
                                match (info.imported, info.by.is_empty()) {
                                    (true, false) => format!("by {}, brought in", info.by),
                                    (true, true) => "brought in".to_string(),
                                    (false, _) => "yours".to_string(),
                                },
                                lib.count(name),
                                if info.made > 0 {
                                    format!(" · started {}", day(info.made))
                                } else {
                                    String::new()
                                }
                            )))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(info.about.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .pt(px(4.))
                                    .child(
                                        crate::sounds::browser::primary("Share the pack…")
                                            .h(px(30.))
                                            .px(px(12.))
                                            .text_size(px(12.))
                                            .debug_selector(|| "poly-pack-share".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |ws, _, _, cx| {
                                                    cx.stop_propagation();
                                                    ws.share_pack(n1.clone(), cx);
                                                }),
                                            ),
                                    )
                                    .child(
                                        button("Name, cover and words", false)
                                            .h(px(30.))
                                            .px(px(12.))
                                            .text_size(px(12.))
                                            .debug_selector(|| "poly-pack-edit".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |ws, _, window, cx| {
                                                    cx.stop_propagation();
                                                    ws.open_pack_dialog(
                                                        Some(n2.clone()),
                                                        Some(window),
                                                        cx,
                                                    );
                                                }),
                                            ),
                                    ),
                            ),
                    )
            }),
            _ => None,
        };
        let taken = v.taken.last().map(|e| e.sound.name.clone());
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(14.))
                    .pt(px(12.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .items_center()
                            .child(div().flex_1().child(crate::pages::genres::field_box(Some(v.find.clone()), "poly-sound-search")))
                            .child(div().w(px(140.)).child(order)),
                    )
                    .child(div().flex().flex_wrap().gap(px(4.)).items_center().child(div().w(px(70.)).child(label("Part"))).children(parts))
                    .child(div().flex().flex_wrap().gap(px(4.)).items_center().child(div().w(px(70.)).child(label("Sounds like"))).children(words)),
            )
            .children(header)
            .child(head)
            .child(
                div()
                    .id("poly-sound-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(none, |d| d.child(div().p(px(20.)).child(label("Nothing here sounds like that")))),
            )
            .child(
                div()
                    .h(px(38.))
                    .flex_none()
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .whitespace_nowrap()
                    .child(label("Double-click loads · ▶ hears it · the title's arrows step through this list"))
                    .child(div().flex_1())
                    .when_some(taken, |d, name| {
                        d.child(
                            button(format!("Bring back {name}"), false)
                                .h(px(24.))
                                .text_size(px(11.5))
                                .debug_selector(|| "poly-lib-bring-back".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.sound_op(t, SoundOp::BringBack, cx);
                                    }),
                                ),
                        )
                    })
                    .child(label("Load as I click"))
                    .child(
                        switch(step)
                            .debug_selector(|| "poly-lib-step".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.settings.load_as_step ^= true;
                                    ws.settings.save();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
    }

    /// On the right: the sound picked, all of it.
    fn lib_detail(&self, t: usize, e: &Entry, cx: &mut Context<Self>) -> Div {
        let lib = self.library.as_ref().expect("read");
        let s = &e.sound;
        let pack = e.pack.clone();
        let seed = pack.as_deref().filter(|p| *p != FACTORY).map(|p| {
            lib.pack(p).map_or(
                seed_of(p),
                |i| if i.cover == 0 { seed_of(p) } else { i.cover },
            )
        });
        let in_pack = pack.as_deref().map_or(0, |p| lib.count(p));
        let own = !e.factory();
        let macros: Vec<String> = s
            .poly
            .macros
            .iter()
            .enumerate()
            .map(|(i, m)| {
                if m.is_empty() {
                    format!("M{}", i + 1)
                } else {
                    m.clone()
                }
            })
            .collect();
        let effects: Vec<String> = s.poly.fx.iter().map(rack_name).collect();
        let inside = [
            (
                "Tables",
                s.poly
                    .tables
                    .iter()
                    .filter(|t| !t.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" → "),
            ),
            ("Moves", format!("{} routes", s.poly.routes.len())),
            (
                "Effects",
                if effects.is_empty() {
                    "none of its own".to_string()
                } else {
                    effects.join(" → ")
                },
            ),
        ];
        let key = e.key.clone();
        let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
        let packs: Vec<String> = lib.packs.iter().map(|p| p.name.clone()).collect();
        let move_to = self.poly_menu(
            "poly-lib-move".into(),
            "Move to pack".into(),
            move |_| {
                std::iter::once((
                    "No pack: just mine".to_string(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::Move(k3.clone(), None))),
                ))
                .chain(packs.iter().map(|p| {
                    (
                        p.clone(),
                        MenuAction::Poly(
                            t,
                            PolyOp::Sound(SoundOp::Move(k3.clone(), Some(p.clone()))),
                        ),
                    )
                }))
                .collect()
            },
            false,
            cx,
        );
        div()
            .w(px(318.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(0x111115))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .relative()
                    .h(px(112.))
                    .flex_none()
                    .child(cover(seed, 1.0))
                    .child(
                        div()
                            .absolute()
                            .left(px(14.))
                            .bottom(px(10.))
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0xffffff))
                            .child(format!(
                                "{} · {in_pack} sounds",
                                pack.clone().unwrap_or_else(|| "No pack".into())
                            )),
                    ),
            )
            .child(
                div()
                    .id("poly-lib-detail")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(19.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(s.name.clone()),
                            )
                            .child(label(format!(
                                "{} · {}{}",
                                s.part,
                                match s.by.as_str() {
                                    "" => "made by you".to_string(),
                                    by => format!("made by {by}"),
                                },
                                if s.made > 0 {
                                    format!(" · {}", day(s.made))
                                } else {
                                    String::new()
                                }
                            ))),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(18.))
                            .text_color(rgb(TEXT_2))
                            .child(s.about.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(5.))
                            .children(s.words.iter().map(|w| {
                                div()
                                    .h(px(22.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(11.))
                                    .bg(rgb(FIELD))
                                    .text_size(px(11.5))
                                    .child(w.clone())
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_3))
                                    .child("MACROS"),
                            )
                            .child(div().grid().grid_cols(4).gap(px(4.)).children(
                                macros.into_iter().map(|m| {
                                    div()
                                        .h(px(24.))
                                        .px(px(4.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(6.))
                                        .bg(rgb(CARD))
                                        .border_1()
                                        .border_color(rgb(LINE))
                                        .text_size(px(11.))
                                        .text_color(rgb(VIOLET))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(m)
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_3))
                                    .child("IN IT"),
                            )
                            .children(inside.into_iter().map(|(k, v)| {
                                div()
                                    .flex()
                                    .gap(px(10.))
                                    .text_size(px(12.))
                                    .child(
                                        div()
                                            .w(px(64.))
                                            .flex_none()
                                            .text_color(rgb(TEXT_3))
                                            .child(k),
                                    )
                                    .child(div().text_color(rgb(TEXT_2)).child(v))
                            })),
                    ),
            )
            .child(
                div()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(
                                crate::sounds::browser::primary("Load")
                                    .flex_1()
                                    .h(px(36.))
                                    .debug_selector(|| "poly-lib-load".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.load_sound(t, &k1, cx);
                                        }),
                                    ),
                            )
                            .child(
                                button("Hear", false)
                                    .h(px(36.))
                                    .px(px(14.))
                                    .debug_selector(|| "poly-lib-hear".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.load_sound(t, &k2, cx);
                                            if ws
                                                .poly_view
                                                .as_ref()
                                                .is_some_and(|v| v.demo.is_none())
                                            {
                                                ws.poly_demo(cx);
                                            }
                                        }),
                                    ),
                            ),
                    )
                    .when(own, |d| {
                        let k = key.clone();
                        d.child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(div().flex_1().child(move_to))
                                .child(
                                    button("Take out", false)
                                        .h(px(28.))
                                        .px(px(10.))
                                        .text_size(px(12.))
                                        .debug_selector(|| "poly-lib-take-out".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.sound_op(t, SoundOp::TakeOut(k.clone()), cx);
                                            }),
                                        ),
                                ),
                        )
                    }),
            )
    }
}

/// Whether the demo plays.
fn v_demo(ws: &Workspace) -> bool {
    ws.poly_view.as_ref().is_some_and(|v| v.demo.is_some())
}

/// Something done to the library, or the sound as saved.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundOp {
    Load(String),
    Compare,
    UndoAll,
    Copy,
    Paste,
    Show,
    Move(String, Option<String>),
    TakeOut(String),
    BringBack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_read_as_dates() {
        assert_eq!(day(0), "–");
        assert_eq!(day(library::now()), "Today");
        // 1 Oct 2026, noon.
        assert_eq!(day(1_790_856_000), "1 Oct");
    }
}
