//! Keeping sounds: the sound's bar in the synth's title (what it is, changed or not, Save,
//! Compare, Undo all, and a menu of the rest), Save (its name, part, words, line, pack), packs
//! made and changed, shared as one file and brought in.

use super::*;
use gpui::Focusable;
use library::{CHARACTER, FACTORY, PARTS, PackInfo, SavedSound};
use sounds::{Place, SoundOp, cover, seed_of};

/// A dialog over the synth.
pub enum Dialog {
    Save(SaveForm),
    Pack(PackForm),
}

/// Save's fields.
pub struct SaveForm {
    pub name: Entity<TextInput>,
    pub about: Entity<TextInput>,
    pub word: Entity<TextInput>,
    pub part: &'static str,
    pub words: Vec<String>,
    pub pack: Option<String>,
    /// Saving over this one of the person's own (its key); else a new one.
    pub over: Option<String>,
    /// Where it started, said in the dialog.
    pub from: String,
}

/// A pack's fields: a new one, or (`edit`) one that's there.
pub struct PackForm {
    pub name: Entity<TextInput>,
    pub about: Entity<TextInput>,
    pub cover: u32,
    pub edit: Option<String>,
    /// A sound to start it with, by its key.
    pub with: Option<String>,
}

/// The covers offered for a pack: its own, from its name, and five more.
fn covers(name: &str) -> [u32; 6] {
    let s = seed_of(name);
    std::array::from_fn(|k| s.wrapping_add(k as u32 * 3))
}

impl Workspace {
    /// The key and entry of the sound the synth plays, if the library has it.
    fn now_sound(&mut self, t: usize) -> Option<library::Entry> {
        let key = self.poly(t)?.patch.clone();
        let key = if key.is_empty() {
            "Init".to_string()
        } else {
            key
        };
        self.library().find(&key).cloned()
    }

    /// Save: over the person's own as it is, else (or `as_new`) the dialog for a new one.
    pub(crate) fn save_sound(
        &mut self,
        t: usize,
        as_new: bool,
        window: Option<&mut gpui::Window>,
        cx: &mut Context<Self>,
    ) {
        let now = self.now_sound(t);
        let own = now.clone().filter(|e| !e.factory());
        if let (Some(e), false) = (own.clone(), as_new) {
            let mut s = e.sound.clone();
            s.poly = self.poly(t).cloned().unwrap_or(s.poly);
            s.made = library::now();
            return self.write_sound(t, s, e.pack.clone(), cx);
        }
        let (name, part, words, about, pack, from) = match &now {
            Some(e) => (
                format!("{} 2", e.sound.name),
                e.sound.part.clone(),
                e.sound.words.clone(),
                e.sound.about.clone(),
                e.pack
                    .clone()
                    .filter(|p| p != FACTORY)
                    .or(self.settings.last_pack.clone()),
                format!(
                    "started from {} ({})",
                    e.sound.name,
                    e.pack.clone().unwrap_or_else(|| "no pack".into())
                ),
            ),
            None => (
                "New sound".into(),
                "Lead".into(),
                Vec::new(),
                String::new(),
                self.settings.last_pack.clone(),
                String::new(),
            ),
        };
        let part = PARTS
            .iter()
            .copied()
            .find(|p| p.eq_ignore_ascii_case(&part))
            .unwrap_or("Lead");
        let name = cx.new(|cx| TextInput::form(&name, "What it's called", cx));
        let about_field =
            cx.new(|cx| TextInput::writing("What it's like, for whoever finds it", 3, cx));
        about_field.update(cx, |a, cx| a.set_text(&about, cx));
        let about = about_field;
        let word = cx.new(|cx| TextInput::search("+ your own word", cx));
        // A word typed and confirmed joins the words.
        cx.subscribe(&word, |ws, input, e: &crate::ui::text::TextEvent, cx| {
            if let crate::ui::text::TextEvent::Confirm(w) = e {
                let w = w.trim().to_string();
                if let Some(Dialog::Save(f)) =
                    ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                    && !w.is_empty()
                    && !f.words.contains(&w)
                {
                    f.words.push(w);
                }
                input.update(cx, |i, cx| i.set_text("", cx));
                cx.notify();
            }
        })
        .detach();
        if let Some(window) = window {
            window.focus(&name.read(cx).focus_handle(cx), cx);
        }
        if let Some(v) = self.poly_view.as_mut() {
            v.lib.dialog = Some(Dialog::Save(SaveForm {
                name,
                about,
                word,
                part,
                words,
                pack,
                over: None,
                from,
            }));
        }
        cx.notify();
    }

    /// Save's fields kept: the sound written into its pack, the synth now playing it.
    pub(crate) fn confirm_save(&mut self, t: usize, cx: &mut Context<Self>) {
        let Some(Dialog::Save(f)) = self.poly_view.as_mut().and_then(|v| v.lib.dialog.take())
        else {
            return;
        };
        let name = f.name.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.status = Some("A sound needs a name".into());
            if let Some(v) = self.poly_view.as_mut() {
                v.lib.dialog = Some(Dialog::Save(f));
            }
            return cx.notify();
        }
        // A new one never quietly replaces another of its name in the pack.
        let mut unique = name.clone();
        let mut n = 2;
        while self
            .library()
            .find(&format!("{}/{unique}", f.pack.clone().unwrap_or_default()))
            .is_some()
        {
            unique = format!("{name} {n}");
            n += 1;
        }
        let Some(poly) = self.poly(t).cloned() else {
            return;
        };
        let sound = SavedSound {
            name: unique,
            part: f.part.to_string(),
            words: f.words.clone(),
            about: f.about.read(cx).text().trim().to_string(),
            by: self.settings.maker.clone(),
            made: library::now(),
            poly,
        };
        self.settings.last_pack = f.pack.clone();
        self.settings.save();
        self.write_sound(t, sound, f.pack, cx);
    }

    /// `sound` into `pack`: on disk, in the library, and the synth playing it as saved.
    pub(crate) fn write_sound(
        &mut self,
        t: usize,
        mut sound: SavedSound,
        pack: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let key = format!("{}/{}", pack.clone().unwrap_or_default(), sound.name);
        sound.poly.patch = key.clone();
        let name = sound.name.clone();
        let written = self.library().save(sound.clone(), pack.as_deref());
        match written {
            Ok(_) => {
                self.edit(cx, |ws| {
                    if let Some(p) = ws.poly_mut(t) {
                        p.patch = key.clone();
                    }
                });
                let saved = self.poly(t).cloned();
                if let Some(v) = self.poly_view.as_mut() {
                    v.lib.saved = saved;
                    v.lib.comparing = false;
                    v.lib.picked = Some(key);
                }
                self.status = Some(format!(
                    "Saved {name} {}",
                    match pack {
                        Some(p) => format!("into {p}"),
                        None => "with your sounds".to_string(),
                    }
                ));
            }
            Err(e) => self.status = Some(format!("Couldn't save the sound: {e}")),
        }
        cx.notify();
    }

    /// What an agent asks of the library: as `cli::sounds` does on the files, here so the
    /// window and its library follow; saving and loading as Save and Load do (one edit each).
    pub(crate) fn agent_sounds(
        &mut self,
        args: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        use serde_json::{Value, json};
        if let Some(done) = crate::cli::sounds::library_op(self.library(), args) {
            cx.notify();
            return done;
        }
        let t = args
            .get("track")
            .and_then(Value::as_u64)
            .map(|t| t as usize)
            .ok_or("`track`: which track's Synth-0")?;
        let poly = self
            .poly(t)
            .cloned()
            .ok_or_else(|| format!("track {t} doesn't play Synth-0"))?;
        match args.get("op").and_then(Value::as_str).unwrap_or("list") {
            "save" => {
                let (sound, pack) = crate::cli::sounds::to_save(args, poly, &self.settings.maker)?;
                let key = format!("{}/{}", pack.clone().unwrap_or_default(), sound.name);
                self.write_sound(t, sound, pack, cx);
                match self.library().find(&key) {
                    Some(_) => Ok(json!({ "saved": key })),
                    None => Err(self
                        .status
                        .clone()
                        .unwrap_or_else(|| "it wasn't saved".into())),
                }
            }
            "load" => {
                let key = args
                    .get("key")
                    .and_then(Value::as_str)
                    .ok_or("`key`: a sound as `list` gives it")?
                    .to_string();
                if self.library().find(&key).is_none() {
                    return Err(format!("no sound {key}: `list` gives every key"));
                }
                self.load_sound(t, &key, cx);
                Ok(json!({ "loaded": key, "track": t }))
            }
            other => Err(format!(
                "no op {other}: list, save, load, make_pack or move"
            )),
        }
    }

    /// The pack dialog: a new pack (starting with the sound picked, if `with`), or `edit`.
    pub(crate) fn open_pack_dialog(
        &mut self,
        edit: Option<String>,
        window: Option<&mut gpui::Window>,
        cx: &mut Context<Self>,
    ) {
        let info = edit
            .as_deref()
            .and_then(|p| self.library().pack(p).cloned());
        let (name, about, seed) = match &info {
            Some(i) => (
                i.name.clone(),
                i.about.clone(),
                if i.cover == 0 {
                    seed_of(&i.name)
                } else {
                    i.cover
                },
            ),
            None => ("New pack".to_string(), String::new(), seed_of("New pack")),
        };
        let with = self
            .poly_view
            .as_ref()
            .and_then(|v| v.lib.picked.clone())
            .filter(|_| edit.is_none());
        let name = cx.new(|cx| TextInput::form(&name, "What it's called", cx));
        let about_field =
            cx.new(|cx| TextInput::writing("What's in it, for whoever opens it", 3, cx));
        about_field.update(cx, |a, cx| a.set_text(&about, cx));
        let about = about_field;
        if let Some(window) = window {
            window.focus(&name.read(cx).focus_handle(cx), cx);
        }
        if let Some(v) = self.poly_view.as_mut() {
            v.lib.dialog = Some(Dialog::Pack(PackForm {
                name,
                about,
                cover: seed,
                edit,
                with,
            }));
        }
        cx.notify();
    }

    pub(crate) fn confirm_pack(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Pack(f)) = self.poly_view.as_mut().and_then(|v| v.lib.dialog.take())
        else {
            return;
        };
        let name = f.name.read(cx).text().trim().to_string();
        let about = f.about.read(cx).text().trim().to_string();
        let maker = self.settings.maker.clone();
        let done = match &f.edit {
            Some(old) => {
                let lib = self.library();
                let mut info = lib.pack(old).cloned().unwrap_or_default();
                info.about = about;
                info.cover = f.cover;
                lib.set_pack(info).and_then(|_| match name != *old {
                    true => lib.rename_pack(old, &name),
                    false => Ok(name.clone()),
                })
            }
            None => {
                let made = self.library().make_pack(PackInfo {
                    name,
                    by: maker,
                    about,
                    cover: f.cover,
                    ..Default::default()
                });
                // Started with the sound picked: a copy of it in.
                if let (Ok(pack), Some(key)) = (&made, &f.with)
                    && let Some(e) = self.library().find(key).cloned()
                {
                    let _ = self.library().save(e.sound, Some(pack));
                }
                made
            }
        };
        match done {
            Ok(pack) => {
                if let Some(v) = self.poly_view.as_mut() {
                    v.lib.place = Place::Pack(pack.clone());
                }
                self.status = Some(format!("{pack}: ready"));
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// A pack shared: one file, where the person says.
    pub(crate) fn share_pack(&mut self, pack: String, cx: &mut Context<Self>) {
        let bytes = match self.library().export(&pack) {
            Ok(b) => b,
            Err(e) => {
                self.status = Some(e);
                return cx.notify();
            }
        };
        let dir = dirs_desktop();
        let picked = cx.prompt_for_new_path(&dir, Some(&format!("{pack}.pinapack")));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let written = std::fs::write(&path, &bytes).map_err(|e| e.to_string());
            this.update(cx, |ws, cx| {
                ws.status = Some(match written {
                    Ok(()) => format!("{pack} is one file now: {}", path.display()),
                    Err(e) => format!("Couldn't write the pack: {e}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Packs brought in: files picked, each its own pack.
    pub(crate) fn import_pack(&mut self, _t: usize, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Bring in".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| ws.import_files(&paths, cx)).ok();
        })
        .detach();
    }

    /// Pack files (or folders of them) dropped or picked: each in as a pack of its own.
    pub(crate) fn import_files(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let files: Vec<std::path::PathBuf> = paths
            .iter()
            .flat_map(|p| match p.is_dir() {
                true => std::fs::read_dir(p)
                    .map(|d| d.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default(),
                false => vec![p.clone()],
            })
            .filter(|p| p.extension().is_some_and(|x| x == "pinapack"))
            .collect();
        let mut said = Vec::new();
        let mut last = None;
        for f in files {
            let got = std::fs::read(&f)
                .map_err(|e| e.to_string())
                .and_then(|b| self.library().import(&b));
            match got {
                Ok((name, n)) => {
                    said.push(format!("{name}: {n} sounds in"));
                    last = Some(name);
                }
                Err(e) => said.push(format!("{}: {e}", f.display())),
            }
        }
        if let (Some(v), Some(p)) = (self.poly_view.as_mut(), last) {
            v.lib.place = Place::Pack(p);
        }
        self.status = Some(match said.is_empty() {
            true => "No pack files there: they end in .pinapack".into(),
            false => said.join(" · "),
        });
        cx.notify();
    }

    /// The sound's bar: where it's from and what it's called, changed or not; the arrows
    /// through the list; Save, Compare, Undo all; and the rest in its menu.
    pub(crate) fn sound_bar(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let changed = self.sound_changed(p);
        let wide = self.poly_size().0 >= 1300.0;
        let comparing = self.poly_view.as_ref().is_some_and(|v| v.lib.comparing);
        let on_page = self
            .poly_view
            .as_ref()
            .is_some_and(|v| v.page == PolyPage::Sounds);
        let key = if p.patch.is_empty() {
            "Init".to_string()
        } else {
            p.patch.clone()
        };
        let entry = self.library().find(&key).cloned();
        let (pack, name) = match &entry {
            Some(e) => (e.pack.clone(), e.sound.name.clone()),
            None => (None, key.rsplit('/').next().unwrap_or(&key).to_string()),
        };
        let seed = pack.as_deref().filter(|p| *p != FACTORY).map(|p| {
            self.library
                .as_ref()
                .and_then(|l| l.pack(p))
                .map_or(
                    seed_of(p),
                    |i| if i.cover == 0 { seed_of(p) } else { i.cover },
                )
        });
        // The arrows step through the list as the library last showed it, else the factory's.
        let shown = self
            .poly_view
            .as_ref()
            .map(|v| v.lib.shown.clone())
            .unwrap_or_default();
        let list: Vec<String> = match shown.contains(&key) {
            true => shown,
            false => pina_engine::song::project::POLY_PATCHES
                .iter()
                .map(|p| p.0.to_string())
                .collect(),
        };
        let here = list.iter().position(|k| *k == key);
        let step = |by: isize, cx: &mut Context<Self>| {
            let n = list.len().max(1) as isize;
            let to = list
                .get((here.map_or(-1, |h| h as isize) + by).rem_euclid(n) as usize)
                .cloned();
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(CARD)))
                .debug_selector(move || {
                    if by < 0 {
                        "poly-patch-back"
                    } else {
                        "poly-patch-next"
                    }
                    .into()
                })
                .child(
                    icon(if by < 0 { Icon::Back } else { Icon::Forward }, 14.)
                        .text_color(rgb(TEXT_2)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(to) = &to {
                            ws.load_sound(t, to, cx);
                        }
                    }),
                )
        };
        let menu = move |_: &Workspace| -> Vec<(String, MenuAction)> {
            vec![
                ("Save".into(), MenuAction::Poly(t, PolyOp::SaveSound(false))),
                (
                    "Save as a new sound…".into(),
                    MenuAction::Poly(t, PolyOp::SaveSound(true)),
                ),
                (
                    "Show in the library".into(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::Show)),
                ),
                (
                    "Copy the sound".into(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::Copy)),
                ),
                (
                    "Paste a sound".into(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::Paste)),
                ),
                (
                    "Start from Init".into(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::Load("Init".into()))),
                ),
                (
                    "Undo all changes".into(),
                    MenuAction::Poly(t, PolyOp::Sound(SoundOp::UndoAll)),
                ),
            ]
        };
        let bar_button = |label: &'static str, lit: bool, sel: &'static str| {
            button(label, lit)
                .h(px(30.))
                .px(px(11.))
                .text_size(px(12.5))
                .whitespace_nowrap()
                .debug_selector(move || sel.into())
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(step(-1, cx))
            .child(
                div()
                    .h(px(34.))
                    .w(px(232.))
                    // In a narrow window it gives way, rather than push the title's end off.
                    .flex_shrink(1.)
                    .min_w(px(150.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(8.))
                    .bg(rgb(if on_page { RAISED } else { 0x1f1f26 }))
                    .border_1()
                    .border_color(rgb(if on_page { ACCENT } else { LINE }))
                    .cursor_pointer()
                    .hover(|d| d.border_color(rgb(LINE_STRONG)))
                    .debug_selector(|| "poly-patch".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            // The chevron: the menu; else the library, or back from it.
                            if e.click_count == 1
                                && let Some(v) = ws.poly_view.as_mut()
                            {
                                v.page = match v.page {
                                    PolyPage::Sounds => PolyPage::Synth,
                                    _ => PolyPage::Sounds,
                                };
                            }
                            cx.notify();
                        }),
                    )
                    .child(div().size(px(18.)).flex_none().child(cover(seed, 0.6)))
                    .child(label(format!(
                        "{} /",
                        pack.clone().unwrap_or_else(|| "Mine".into())
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .when(changed, |d| {
                        d.child(
                            div()
                                .size(px(6.))
                                .flex_none()
                                .rounded(px(3.))
                                .bg(rgb(TEXT))
                                .debug_selector(|| "poly-patch-changed".into()),
                        )
                    })
                    .child(
                        div()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|d| d.bg(rgb(FIELD)))
                            .debug_selector(|| "poly-patch-menu".into())
                            .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_2)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.menu = Some(Menu {
                                        at: e.position,
                                        items: menu(ws),
                                        ..Default::default()
                                    });
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(step(1, cx))
            .child(bar_button("Save", changed, "poly-save").on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, window, cx| {
                    cx.stop_propagation();
                    ws.save_sound(t, false, Some(window), cx);
                }),
            ))
            .when(changed || comparing, |d| {
                d.child(
                    // A/B: the sound as saved, and back.
                    bar_button("A/B", comparing, "poly-compare")
                        .px(px(8.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.sound_op(t, SoundOp::Compare, cx);
                            }),
                        ),
                )
                // Narrow, it's in the sound's menu only: the title's end stays in the window.
                .when(wide, |d| {
                    d.child(
                        bar_button("Undo all", false, "poly-undo-all").on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.sound_op(t, SoundOp::UndoAll, cx);
                            }),
                        ),
                    )
                })
            })
    }

    /// The dialog open, over the synth: Save, or a pack's.
    pub(crate) fn sound_dialog(
        &mut self,
        t: usize,
        p: &Poly,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let v = self.poly_view.as_ref()?;
        let body = match v.lib.dialog.as_ref()? {
            Dialog::Save(f) => self.save_dialog(t, p, f, cx),
            Dialog::Pack(f) => self.pack_dialog(f, cx),
        };
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.5))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.lib.dialog = None;
                        }
                        cx.notify();
                    }),
                )
                .child(
                    body.occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                ),
        )
    }

    fn save_dialog(
        &self,
        t: usize,
        p: &Poly,
        f: &SaveForm,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let lib = self.library.as_ref();
        let seg_part = |name: &'static str, cx: &mut Context<Self>| {
            seg(name, f.part == name, 28.)
                .flex_1()
                .debug_selector(move || format!("poly-save-part-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(Dialog::Save(f)) =
                            ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                        {
                            f.part = name;
                        }
                        cx.notify();
                    }),
                )
        };
        let own_words: Vec<String> = f
            .words
            .iter()
            .filter(|w| !CHARACTER.contains(&w.as_str()))
            .cloned()
            .collect();
        let word_chip = |w: String, on: bool, cx: &mut Context<Self>| {
            let sel = format!("poly-save-word-{}", w.to_lowercase());
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(13.))
                .text_size(px(12.))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!on, |d| {
                    d.bg(rgb(CARD))
                        .text_color(rgb(TEXT_2))
                        .border_1()
                        .border_color(rgb(LINE))
                })
                .debug_selector(move || sel.clone())
                .child(w.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(Dialog::Save(f)) =
                            ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                        {
                            match f.words.iter().position(|x| *x == w) {
                                Some(i) => drop(f.words.remove(i)),
                                None => f.words.push(w.clone()),
                            }
                        }
                        cx.notify();
                    }),
                )
        };
        let mut chips: Vec<Div> = CHARACTER
            .iter()
            .map(|w| word_chip(w.to_string(), f.words.iter().any(|x| x == w), cx))
            .collect();
        chips.extend(own_words.into_iter().map(|w| word_chip(w, true, cx)));
        let packs: Vec<(Option<String>, usize, Option<u32>)> = std::iter::once((None, 0, None))
            .chain(lib.into_iter().flat_map(|l| {
                l.packs.iter().filter(|p| !p.imported).map(|p| {
                    (
                        Some(p.name.clone()),
                        l.count(&p.name),
                        Some(if p.cover == 0 {
                            seed_of(&p.name)
                        } else {
                            p.cover
                        }),
                    )
                })
            }))
            .collect();
        let pack_rows: Vec<Div> = packs
            .into_iter()
            .map(|(name, n, seed)| {
                let on = f.pack == name;
                let shown = name.clone().unwrap_or_else(|| "No pack: just mine".into());
                let sel = format!("poly-save-pack-{shown}");
                div()
                    .h(px(32.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(rgb(0x24242b)).border_l_2().border_color(rgb(ACCENT))
                    })
                    .hover(|d| d.bg(rgb(RAISED)))
                    .debug_selector(move || sel.clone())
                    .child(div().size(px(20.)).flex_none().child(cover(seed, 0.6)))
                    .child(div().flex_1().child(shown))
                    .when(name.is_some(), |d| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(n.to_string()),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(Dialog::Save(f)) =
                                ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                            {
                                f.pack = name.clone();
                            }
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let keeps = format!(
            "{} tables, {} routes, its macros and their names, the voice, the arp{}",
            p.tables.iter().filter(|t| !t.is_empty()).count(),
            p.routes.len(),
            match p.fx.is_empty() {
                true => ".".to_string(),
                false => format!(
                    ", and its effects: {}.",
                    p.fx.iter().map(rack_name).collect::<Vec<_>>().join(" → ")
                ),
            }
        );
        let field = |label_: &'static str, child: Div| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(label(label_))
                .child(child)
        };
        div()
            .id("poly-save-dialog")
            .w(px(660.))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .overflow_hidden()
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(18.))
                    .pt(px(16.))
                    .pb(px(6.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Save the sound"),
                    )
                    .child(div().flex_1())
                    .child(label(f.from.clone())),
            )
            .child(
                div()
                    .px(px(18.))
                    .pt(px(10.))
                    .pb(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(field(
                        "Name",
                        crate::pages::genres::field_box(Some(f.name.clone()), "poly-save-name"),
                    ))
                    .child(field(
                        "What it's for",
                        tray()
                            .p(px(2.))
                            .rounded(px(8.))
                            .flex()
                            .children(PARTS.map(|p| seg_part(p, cx))),
                    ))
                    .child(field(
                        "What it sounds like: the words search finds it by",
                        div().flex().flex_wrap().gap(px(5.)).children(chips).child(
                            div().w(px(150.)).child(crate::pages::genres::field_box(
                                Some(f.word.clone()),
                                "poly-save-word",
                            )),
                        ),
                    ))
                    .child(field(
                        "About it",
                        crate::pages::genres::field_box(Some(f.about.clone()), "poly-save-about"),
                    ))
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(12.))
                            .child(field(
                                "Into the pack",
                                div()
                                    .p(px(4.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .rounded(px(9.))
                                    .bg(rgb(WELL))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .children(pack_rows)
                                    .child(
                                        div()
                                            .h(px(30.))
                                            .px(px(8.))
                                            .flex()
                                            .items_center()
                                            .text_size(px(12.5))
                                            .text_color(rgb(ACCENT))
                                            .cursor_pointer()
                                            .debug_selector(|| "poly-save-new-pack".into())
                                            .child("+ New pack…")
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, window, cx| {
                                                    cx.stop_propagation();
                                                    // Kept: the pack's dialog, then back to Save in it.
                                                    let save = ws
                                                        .poly_view
                                                        .as_mut()
                                                        .and_then(|v| v.lib.dialog.take());
                                                    ws.open_pack_dialog(None, Some(window), cx);
                                                    if let Some(v) = ws.poly_view.as_mut() {
                                                        v.lib.pending = save;
                                                    }
                                                }),
                                            ),
                                    ),
                            ))
                            .child(field(
                                "It keeps",
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(rgb(TEXT_2))
                                    .child(keeps),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(18.))
                    .py(px(12.))
                    .bg(rgb(SURFACE))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(label(match f.over.is_some() {
                        true => "Saves over it".to_string(),
                        false => "The factory's sounds stay as they are: this makes a new one"
                            .to_string(),
                    }))
                    .child(div().flex_1())
                    .child(
                        button("Cancel", false)
                            .h(px(34.))
                            .px(px(14.))
                            .debug_selector(|| "poly-save-cancel".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(v) = ws.poly_view.as_mut() {
                                        v.lib.dialog = None;
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        crate::sounds::browser::primary("Save")
                            .h(px(34.))
                            .px(px(18.))
                            .debug_selector(|| "poly-save-confirm".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.confirm_save(t, cx);
                                }),
                            ),
                    ),
            )
    }

    fn pack_dialog(&self, f: &PackForm, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let name_now = f.name.read(cx).text().to_string();
        let seeds = covers(&name_now);
        let picked = f.cover;
        let choices: Vec<Div> = std::iter::once(picked)
            .chain(seeds.into_iter().filter(|s| *s != picked))
            .take(6)
            .enumerate()
            .map(|(k, s)| {
                let on = s == picked;
                div()
                    .size(px(60.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(on, |d| d.border_2().border_color(rgb(ACCENT)))
                    .debug_selector(move || format!("poly-pack-cover-{k}"))
                    .child(cover(Some(s), 1.0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(Dialog::Pack(f)) =
                                ws.poly_view.as_mut().and_then(|v| v.lib.dialog.as_mut())
                            {
                                f.cover = s;
                            }
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let with = f.with.clone();
        let editing = f.edit.is_some();
        div()
            .id("poly-pack-dialog")
            .w(px(460.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .rounded(px(14.))
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(if editing { "The pack" } else { "New pack" }),
            )
            .child(crate::pages::genres::field_box(
                Some(f.name.clone()),
                "poly-pack-name",
            ))
            .child(crate::pages::genres::field_box(
                Some(f.about.clone()),
                "poly-pack-about",
            ))
            .child(label("Its cover, drawn from its name; or pick another"))
            .child(div().flex().gap(px(8.)).children(choices))
            .when_some(with.filter(|_| !editing), |d, key| {
                d.child(label(format!(
                    "Starts with a copy of {}",
                    key.rsplit('/').next().unwrap_or(&key)
                )))
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("Cancel", false)
                            .h(px(32.))
                            .px(px(12.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(v) = ws.poly_view.as_mut() {
                                        v.lib.dialog = v.lib.pending.take();
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        crate::sounds::browser::primary(if editing {
                            "Keep it"
                        } else {
                            "Make the pack"
                        })
                        .h(px(32.))
                        .px(px(14.))
                        .debug_selector(|| "poly-pack-confirm".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.confirm_pack(cx);
                                // Back to Save, into the pack just made.
                                let pending =
                                    ws.poly_view.as_mut().and_then(|v| v.lib.pending.take());
                                if let Some(Dialog::Save(mut f)) = pending {
                                    if let Some(Place::Pack(p)) =
                                        ws.poly_view.as_ref().map(|v| v.lib.place.clone())
                                    {
                                        f.pack = Some(p);
                                    }
                                    if let Some(v) = ws.poly_view.as_mut() {
                                        v.lib.dialog = Some(Dialog::Save(f));
                                    }
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            )
    }
}

/// Where a shared pack's file is offered to go first: the Desktop, else home.
fn dirs_desktop() -> std::path::PathBuf {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    match home.join("Desktop").is_dir() {
        true => home.join("Desktop"),
        false => home,
    }
}
