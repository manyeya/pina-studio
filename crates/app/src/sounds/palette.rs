//! The palette: sounds, instruments and effects the person picks for the agent to make the song
//! with. Picked with the ＋ on each in the Browser (the Agent panel opens it to pick from), or,
//! for sounds, dropped in from the Finder onto the Agent panel. They show in the Agent panel's
//! message box, to hear, give a part, or take out; they're kept in the song, and the agent is
//! told them with the next message after they change (and can ask for them with its `palette`
//! tool).

use std::path::{Path, PathBuf};

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, Stateful, div, prelude::*, px, rgb,
};
use pina_engine::{
    samples::{Category, Kind, Sample, note_name},
    song::project::{Palette, PaletteDevice, PaletteSound},
};

use crate::{
    sounds::browser::{category_color, wave},
    sounds::catalog::{Device, Scope},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::Workspace,
};

/// The parts a sound can be given, in the order its menu lists them.
pub const ROLES: [&str; 16] = [
    "kick",
    "snare",
    "clap",
    "hats",
    "perc",
    "drum loop",
    "808",
    "bass",
    "chords",
    "keys",
    "pad",
    "lead",
    "pluck",
    "vocal",
    "fx",
    "texture",
];

/// The part a sound sounds like it's for.
pub fn role_for(c: Category) -> Option<&'static str> {
    use Category::*;
    Some(match c {
        Kick => "kick",
        Snare | Rim | Snap => "snare",
        Clap => "clap",
        HatClosed | HatOpen | Ride | Crash | Shaker | Tambourine => "hats",
        Tom | Perc | Foley => "perc",
        Drums => "drum loop",
        Bass808 => "808",
        Bass => "bass",
        Chord | Strings | Brass | Guitar => "chords",
        Keys | Bell => "keys",
        Pad => "pad",
        Lead | Synth | Woodwind => "lead",
        Pluck => "pluck",
        Vocal => "vocal",
        Fx | Reverse => "fx",
        Texture => "texture",
        Other => return None,
    })
}

/// Something in the palette, by its place in its list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Sound(usize),
    Device(usize),
}

/// What a palette item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaletteOp {
    /// Its part (None: the agent decides).
    Role(Item, Option<&'static str>),
    Remove(Item),
    Only,
    Clear,
    /// More to pick, from the Browser's sounds, instruments or effects.
    Pick(Scope),
}

/// A Browser device as the palette keeps it.
pub fn palette_device(d: &Device) -> PaletteDevice {
    PaletteDevice {
        name: d.name.clone(),
        instrument: d.instrument(),
        own: d.key.strip_prefix("own:").map(str::to_string),
        plugin: match d.own {
            Some(_) => None,
            None => d.formats.first().map(|p| p.plugin_ref()),
        },
        role: None,
    }
}

/// How the agent puts a device in: a plugin as its commands take it, Pina Studio's own by its command.
pub fn how_to_use(d: &PaletteDevice) -> String {
    if let Some(r) = &d.plugin {
        return format!(
            "\"plugin\": {}",
            serde_json::to_string(r).unwrap_or_default()
        );
    }
    match d.own.as_deref() {
        Some("synth") => "Pina Studio's own: \"instrument\": \"synth\"",
        Some("drums") => "Pina Studio's own: \"instrument\": \"drums\"",
        Some("sampler") => "Pina Studio's own: samples_put puts sounds on it",
        Some("eq") => "Pina Studio's own: set_eq",
        Some("comp") => "Pina Studio's own: set_comp",
        Some("sat") => "Pina Studio's own: set_saturator",
        _ => "",
    }
    .into()
}

/// A device as the agent is told it: its part, its name, how to put it in.
fn about_device(d: &PaletteDevice) -> String {
    let part = d.role.as_deref().unwrap_or(match d.instrument {
        true => "any part",
        false => "anywhere",
    });
    format!("- {part}: \"{}\" — {}", d.name, how_to_use(d))
}

/// A sound as the agent is told it: what it is, its note or key and tempo, where it is.
fn about(sound: &PaletteSound, s: Option<&Sample>) -> String {
    let name = sound
        .path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let part = sound.role.as_deref().unwrap_or("any part");
    let what = s.map(|s| {
        let a = &s.analysis;
        let mut bits = vec![match a.kind {
            Kind::Loop => format!("{} loop", a.category.name().to_lowercase()),
            Kind::OneShot => format!("{} one-shot", a.category.name().to_lowercase()),
        }];
        if let Some(k) = &a.key {
            bits.push(k.name.clone());
        } else if let Some(p) = a.pitch {
            bits.push(note_name(p.note));
        }
        if let Some(bpm) = a.bpm {
            bits.push(format!("{bpm:.0} BPM"));
        }
        if let Some(bars) = a.bars {
            bits.push(format!("{bars:.0} bars"));
        }
        format!(" ({})", bits.join(", "))
    });
    format!(
        "- {part}: \"{name}\"{} — {}",
        what.unwrap_or_default(),
        sound.path.display()
    )
}

impl Workspace {
    /// A sound in the library, by its file: where it is in the list, and it.
    fn library_sound(&self, path: &Path) -> Option<(usize, &Sample)> {
        let samples = &self.browser.library.samples;
        let i = samples.iter().position(|s| s.path == path)?;
        Some((i, &samples[i]))
    }

    pub fn in_palette(&self, path: &Path) -> bool {
        self.session.project.palette.has(path)
    }

    /// The Browser's device a picked one is, if it's installed.
    fn catalog_device(&self, pd: &PaletteDevice) -> Option<&Device> {
        self.catalog
            .devices
            .iter()
            .find(|d| match (&pd.plugin, &pd.own) {
                (Some(r), _) => d
                    .formats
                    .iter()
                    .any(|f| f.class == r.class && f.path == r.path),
                (None, Some(own)) => d.key == format!("own:{own}"),
                _ => false,
            })
    }

    fn picked(&self) -> usize {
        let p = &self.session.project.palette;
        p.sounds.len() + p.devices.len()
    }

    /// A Browser device given to the agent, or (given already) taken back.
    pub fn give_agent_device(&mut self, d: usize, cx: &mut Context<Self>) {
        let Some(pd) = self.catalog.devices.get(d).map(palette_device) else {
            return;
        };
        let had = self.session.project.palette.has_device(&pd);
        let name = pd.name.clone();
        self.edit(cx, |ws| {
            let devices = &mut ws.session.project.palette.devices;
            match had {
                true => devices.retain(|x| !x.same(&pd)),
                false => devices.push(pd),
            }
        });
        self.status = Some(match had {
            true => format!("{name} taken out of what the agent uses"),
            false => format!("{name} is for the agent · {} picked", self.picked()),
        });
        cx.notify();
    }

    /// A sound given to the agent, or (given already) taken back.
    pub fn give_agent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let role = self
            .library_sound(&path)
            .and_then(|(_, s)| role_for(s.analysis.category))
            .map(str::to_string);
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let had = self.in_palette(&path);
        self.edit(cx, |ws| {
            let sounds = &mut ws.session.project.palette.sounds;
            match had {
                true => sounds.retain(|s| s.path != path),
                false => sounds.push(PaletteSound { path, role }),
            }
        });
        self.status = Some(match had {
            true => format!("{name} taken out of the agent's sounds"),
            false => format!("{name} is for the agent · {} picked", self.picked()),
        });
        cx.notify();
    }

    /// Audio files dropped on the Agent panel: for the agent, each for what it sounds like.
    pub fn give_agent_files(&mut self, files: &[PathBuf], cx: &mut Context<Self>) {
        let new: Vec<PaletteSound> = files
            .iter()
            .filter(|f| !self.in_palette(f))
            .map(|f| PaletteSound {
                path: f.clone(),
                role: self
                    .library_sound(f)
                    .and_then(|(_, s)| role_for(s.analysis.category))
                    .map(str::to_string),
            })
            .collect();
        let n = new.len();
        self.edit(cx, |ws| ws.session.project.palette.sounds.extend(new));
        self.status = Some(format!(
            "{n} sound{} for the agent",
            if n == 1 { "" } else { "s" }
        ));
        cx.notify();
    }

    pub fn palette_op(&mut self, op: PaletteOp, cx: &mut Context<Self>) {
        if let PaletteOp::Pick(scope) = op {
            return self.pick_for_agent(scope, cx);
        }
        self.edit(cx, |ws| {
            let palette = &mut ws.session.project.palette;
            match op {
                PaletteOp::Role(Item::Sound(i), role) => {
                    if let Some(s) = palette.sounds.get_mut(i) {
                        s.role = role.map(str::to_string);
                    }
                }
                PaletteOp::Role(Item::Device(i), role) => {
                    if let Some(d) = palette.devices.get_mut(i) {
                        d.role = role.map(str::to_string);
                    }
                }
                PaletteOp::Remove(Item::Sound(i)) => {
                    if i < palette.sounds.len() {
                        palette.sounds.remove(i);
                    }
                }
                PaletteOp::Remove(Item::Device(i)) => {
                    if i < palette.devices.len() {
                        palette.devices.remove(i);
                    }
                }
                PaletteOp::Only => palette.only ^= true,
                PaletteOp::Clear => *palette = Palette::default(),
                PaletteOp::Pick(_) => {}
            }
        });
    }

    /// The Browser's sounds, instruments or effects, opened to pick some for the agent; Done
    /// goes back.
    pub fn pick_for_agent(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.picking = true;
        self.browse(scope, cx);
    }

    fn pick_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let items = [
            ("Sounds…", Scope::Sounds),
            ("Instruments…", Scope::Instruments),
            ("Effects…", Scope::Effects),
        ]
        .map(|(label, s)| (label.to_string(), MenuAction::Palette(PaletteOp::Pick(s))))
        .into();
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    pub fn done_picking(&mut self, cx: &mut Context<Self>) {
        self.picking = false;
        self.show_browser = false;
        self.show_agent = true;
        cx.notify();
    }

    /// The palette, as the agent is told it with the next message: when it's changed since
    /// it was last told (all of it, or that it's been cleared).
    pub fn palette_note(&mut self) -> Option<String> {
        let palette = self.session.project.palette.clone();
        let heard = self.chat.palette_heard.replace(palette.clone());
        if heard.as_ref() == Some(&palette) || (heard.is_none() && palette.none()) {
            return None;
        }
        if palette.none() {
            return Some(
                "(The person took everything out of their palette: choose your own sounds, \
                 instruments and effects.)"
                    .into(),
            );
        }
        let only = match palette.only {
            true => "; make the song with these, and no others of each kind they picked",
            false => "; start with these, then fill what's missing",
        };
        let mut lines = vec![format!(
            "(What the person picked for this song{only}. `palette` says which are in the song \
             already.)"
        )];
        if !palette.sounds.is_empty() {
            lines.push("Sounds, each put in with samples_put for its part:".into());
            lines.extend(
                palette
                    .sounds
                    .iter()
                    .map(|s| about(s, self.library_sound(&s.path).map(|(_, s)| s))),
            );
        }
        for (instrument, title) in [
            (true, "Instruments, for set_instrument or add_track:"),
            (
                false,
                "Effects, for set_insert (Pina Studio's own with their own command):",
            ),
        ] {
            let these: Vec<String> = palette
                .devices
                .iter()
                .filter(|d| d.instrument == instrument)
                .map(about_device)
                .collect();
            if !these.is_empty() {
                lines.push(title.into());
                lines.extend(these);
            }
        }
        Some(lines.join("\n"))
    }

    fn hear_palette(&mut self, path: &Path, cx: &mut Context<Self>) {
        match self.library_sound(path).map(|(i, _)| i) {
            Some(i) => self.audition_sample(i, cx),
            None => {
                if let Err(e) = self.player.audition(path, 0.0, 0.0) {
                    self.status = Some(e);
                }
                cx.notify();
            }
        }
    }

    fn role_menu(&mut self, i: Item, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let palette = &self.session.project.palette;
        let now = match i {
            Item::Sound(i) => palette.sounds.get(i).and_then(|s| s.role.clone()),
            Item::Device(i) => palette.devices.get(i).and_then(|d| d.role.clone()),
        };
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![(
            format!("{}Any part (the agent decides)", tick(now.is_none())),
            MenuAction::Palette(PaletteOp::Role(i, None)),
        )];
        items.push((String::new(), MenuAction::Nothing));
        items.extend(ROLES.iter().map(|&r| {
            (
                format!("{}{}", tick(now.as_deref() == Some(r)), capital(r)),
                MenuAction::Palette(PaletteOp::Role(i, Some(r))),
            )
        }));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A line of the palette's list: what it is, its name, its part (its tag's menu) and its ×.
    fn palette_row(
        &self,
        item: Item,
        lead: Div,
        name: String,
        role: Option<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (id, tag) = match item {
            Item::Sound(i) => (i, "palette"),
            Item::Device(i) => (i, "palette-device"),
        };
        div()
            .id((tag, id))
            .flex_none()
            .h(px(26.))
            .w_full()
            .pl(px(6.))
            .pr(px(3.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(rgb(RAISED))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .child(lead)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(18.))
                    .px(px(5.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(alpha(AGENT, if role.is_some() { 0.16 } else { 0.06 }))
                    .text_size(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if role.is_some() { AGENT_TEXT } else { TEXT_3 }))
                    .cursor_pointer()
                    .hover(|d| d.bg(alpha(AGENT, 0.26)))
                    .debug_selector(move || format!("{tag}-role-{id}"))
                    .child(role.as_deref().map_or("any".into(), capital))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.role_menu(item, e.position, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .size(px(18.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(LINE_STRONG)))
                    .debug_selector(move || format!("{tag}-remove-{id}"))
                    .child(icon(Icon::Close, 9.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.palette_op(PaletteOp::Remove(item), cx);
                        }),
                    ),
            )
    }

    /// In the Agent panel's message box: the sounds, instruments and effects picked for the
    /// agent, each to give a part (its tag) or take out, a sound to hear (click); "only these",
    /// and more to pick.
    pub fn palette_card(&self, cx: &mut Context<Self>) -> Option<Div> {
        let palette = &self.session.project.palette;
        if palette.none() {
            return None;
        }
        let sounds = palette.sounds.iter().enumerate().map(|(i, sound)| {
            let found = self.library_sound(&sound.path).map(|(_, s)| s);
            let color = found.map_or(TEXT_3, |s| category_color(s.analysis.category));
            let name = sound
                .path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let path = sound.path.clone();
            let lead = match found {
                Some(s) => div().w(px(24.)).h(px(14.)).flex_none().child(wave(
                    s.analysis.waveform.clone(),
                    color,
                    0.9,
                )),
                None => div()
                    .w(px(24.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(Icon::Wave, 12.).text_color(rgb(color))),
            };
            self.palette_row(Item::Sound(i), lead, name, sound.role.clone(), cx)
                .cursor_pointer()
                .hover(|d| d.border_color(rgb(TEXT_4)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.hear_palette(&path, cx)),
                )
        });
        let sounds: Vec<_> = sounds.collect();
        // Instruments, then effects.
        let mut order: Vec<usize> = (0..palette.devices.len()).collect();
        order.sort_by_key(|&i| !palette.devices[i].instrument);
        let devices: Vec<_> = order
            .into_iter()
            .map(|i| {
                let d = &palette.devices[i];
                let cat = self.catalog_device(d).map(|d| d.cat);
                let color = cat.map_or(TEXT_3, |c| c.color());
                let lead = div()
                    .w(px(24.))
                    .h(px(18.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .bg(alpha(color, 0.14))
                    .child(match cat {
                        Some(c) => c.glyph(12.).text_color(rgb(color)),
                        None => icon(Icon::Plug, 11.).text_color(rgb(color)),
                    });
                self.palette_row(Item::Device(i), lead, d.name.clone(), d.role.clone(), cx)
            })
            .collect();
        let only = palette.only;
        let n = self.picked();
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(7.))
                .px(px(10.))
                .pt(px(9.))
                .pb(px(9.))
                .border_b_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(icon(Icon::Agent, 12.).text_color(rgb(AGENT)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_2))
                                .child(format!("For the agent · {n}")),
                        )
                        .child(
                            div()
                                .flex_none()
                                .px(px(4.))
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(TEXT_2)))
                                .debug_selector(|| "palette-clear".into())
                                .child("Clear")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.palette_op(PaletteOp::Clear, cx)),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .h(px(22.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .rounded(px(6.))
                                .border_1()
                                .cursor_pointer()
                                .text_size(px(10.5))
                                .when(only, |d| {
                                    d.bg(alpha(AGENT, 0.16))
                                        .border_color(alpha(AGENT, 0.5))
                                        .text_color(rgb(AGENT_TEXT))
                                })
                                .when(!only, |d| {
                                    d.border_color(rgb(LINE_STRONG))
                                        .text_color(rgb(TEXT_3))
                                        .hover(|d| d.text_color(rgb(TEXT_2)))
                                })
                                .when(only, |d| {
                                    d.child(icon(Icon::Check, 10.).text_color(rgb(AGENT)))
                                })
                                .child("Only these")
                                .debug_selector(|| "palette-only".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.palette_op(PaletteOp::Only, cx)),
                                ),
                        )
                        .child(
                            icon_seg(Icon::Plus, false, 22.)
                                .flex_none()
                                .debug_selector(|| "palette-more".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        ws.pick_menu(e.position, cx)
                                    }),
                                ),
                        ),
                )
                // A list: five show, the rest scroll.
                .child(
                    div()
                        .id("palette-list")
                        .max_h(px(5.0 * 30.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children(sounds)
                        .children(devices),
                ),
        )
    }

    /// With nothing picked: a quiet line atop the message box, to start picking sounds,
    /// instruments or effects.
    pub fn palette_start(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.session.project.palette.none() {
            return None;
        }
        let pick = |label: &'static str, scope: Scope, cx: &mut Context<Self>| {
            div()
                .h(px(22.))
                .px(px(5.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .child(label)
                .debug_selector(move || format!("palette-start-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.pick_for_agent(scope, cx)),
                )
        };
        let dot = || div().text_size(px(11.)).text_color(rgb(TEXT_4)).child("·");
        Some(
            div()
                .flex()
                .items_center()
                .px(px(8.))
                .pt(px(7.))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .pl(px(6.))
                        .pr(px(1.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child(icon(Icon::Agent, 11.).text_color(rgb(AGENT)))
                        .child("Pick"),
                )
                .child(pick("sounds", Scope::Sounds, cx))
                .child(dot())
                .child(pick("instruments", Scope::Instruments, cx))
                .child(dot())
                .child(pick("effects", Scope::Effects, cx)),
        )
    }

    /// Over the Browser while picking for the agent: how many, and back to it.
    pub fn picking_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.picking {
            return None;
        }
        let n = self.picked();
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(8.))
                .bg(alpha(AGENT, 0.09))
                .border_b_1()
                .border_color(alpha(AGENT, 0.3))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(AGENT_TEXT))
                                .child("Picking for the agent"),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(match n {
                                    0 => {
                                        "＋ on a sound, instrument or effect gives it to the agent"
                                            .to_string()
                                    }
                                    1 => "1 picked".to_string(),
                                    n => format!("{n} picked"),
                                }),
                        ),
                )
                .child(
                    div()
                        .h(px(26.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .bg(rgb(AGENT))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(BG))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(AGENT_TEXT)))
                        .debug_selector(|| "picking-done".into())
                        .child("Done")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.done_picking(cx)),
                        ),
                ),
        )
    }

    /// On a sound in the Browser: ＋ gives it to the agent; ✓, given, takes it back. Shown on
    /// hover, while picking, or once given.
    pub fn agent_button(&self, path: &Path, group: &str, cx: &mut Context<Self>) -> Div {
        let on = self.in_palette(path);
        let path = path.to_path_buf();
        let name = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.give_toggle(
            on,
            format!("give-agent-{name}"),
            group,
            cx,
            move |ws, cx| ws.give_agent(path.clone(), cx),
        )
    }

    /// The same on an instrument or effect in the Browser.
    pub fn device_agent_button(&self, d: usize, group: &str, cx: &mut Context<Self>) -> Div {
        let dev = &self.catalog.devices[d];
        let on = self
            .session
            .project
            .palette
            .has_device(&palette_device(dev));
        self.give_toggle(
            on,
            format!("give-agent-{}", dev.key),
            group,
            cx,
            move |ws, cx| ws.give_agent_device(d, cx),
        )
    }

    fn give_toggle(
        &self,
        on: bool,
        id: String,
        group: &str,
        cx: &mut Context<Self>,
        give: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div()
            .size(px(22.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(11.))
            .cursor_pointer()
            .when(on, |d| d.bg(rgb(AGENT)))
            .when(!on, |d| {
                d.border_1()
                    .border_color(alpha(AGENT, 0.5))
                    .hover(|d| d.bg(alpha(AGENT, 0.18)))
            })
            .when(!on && !self.picking, |d| {
                d.opacity(0.0)
                    .group_hover(group.to_string(), |s| s.opacity(1.0))
            })
            .debug_selector(move || id.clone())
            .child(
                icon(if on { Icon::Check } else { Icon::Plus }, 11.).text_color(rgb(if on {
                    BG
                } else {
                    AGENT
                })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    give(ws, cx);
                }),
            )
    }
}

impl Workspace {
    /// Under a picked sound: "For the agent", lit once it is.
    pub fn give_button(&self, path: PathBuf, h: f32, cx: &mut Context<Self>) -> Div {
        let on = self.in_palette(&path);
        div()
            .h(px(h))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .when(on, |d| {
                d.bg(alpha(AGENT, 0.16))
                    .border_1()
                    .border_color(alpha(AGENT, 0.5))
                    .text_color(rgb(AGENT_TEXT))
            })
            .when(!on, |d| {
                d.bg(rgb(RAISED))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .text_color(rgb(TEXT_2))
                    .hover(|d| d.border_color(alpha(AGENT, 0.6)))
            })
            .debug_selector(|| "give-agent".into())
            .child(
                icon(if on { Icon::Check } else { Icon::Plus }, 12.).text_color(rgb(if on {
                    AGENT
                } else {
                    TEXT_3
                })),
            )
            .child("Agent")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.give_agent(path.clone(), cx)),
            )
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
