//! The palette: sounds the person picks for the agent to make the song with. Picked with the
//! button on each sound in the Browser (the Agent panel's "+ Sounds" opens it to pick from), or
//! dropped in from the Finder onto the Agent panel. They show in the Agent panel's message box,
//! to hear, give a part, or take out; they're kept in the song, and the agent is told them with
//! the next message after they change (and can ask for them with its `palette` tool).

use std::path::{Path, PathBuf};

use gpui::{Context, Div, FontWeight, MouseButton, MouseDownEvent, div, prelude::*, px, rgb};
use tempo_engine::{
    project::{Palette, PaletteSound},
    samples::{Category, Kind, Sample, note_name},
};

use crate::{
    browser::{category_color, wave},
    catalog::Scope,
    menu::{Menu, MenuAction},
    theme::*,
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

/// What a palette item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaletteOp {
    /// A sound's part (None: the agent decides).
    Role(usize, Option<&'static str>),
    Remove(usize),
    Only,
    Clear,
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
        let n = self.session.project.palette.sounds.len();
        self.status = Some(match had {
            true => format!("{name} taken out of the agent's sounds"),
            false => format!("{name} is for the agent · {n} picked"),
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
        self.edit(cx, |ws| {
            let palette = &mut ws.session.project.palette;
            match op {
                PaletteOp::Role(i, role) => {
                    if let Some(s) = palette.sounds.get_mut(i) {
                        s.role = role.map(str::to_string);
                    }
                }
                PaletteOp::Remove(i) => {
                    if i < palette.sounds.len() {
                        palette.sounds.remove(i);
                    }
                }
                PaletteOp::Only => palette.only ^= true,
                PaletteOp::Clear => *palette = Palette::default(),
            }
        });
    }

    /// The Browser's sounds, opened to pick some for the agent; Done goes back.
    pub fn pick_sounds(&mut self, cx: &mut Context<Self>) {
        self.picking_sounds = true;
        self.browse(Scope::Sounds, cx);
        cx.notify();
    }

    pub fn done_picking(&mut self, cx: &mut Context<Self>) {
        self.picking_sounds = false;
        self.show_browser = false;
        self.show_agent = true;
        cx.notify();
    }

    /// The palette, as the agent is told it with the next message: when it's changed since
    /// it was last told (all of it, or that it's been cleared).
    pub fn palette_note(&mut self) -> Option<String> {
        let palette = self.session.project.palette.clone();
        let heard = self.chat.palette_heard.replace(palette.clone());
        if heard.as_ref() == Some(&palette) || (heard.is_none() && palette.sounds.is_empty()) {
            return None;
        }
        if palette.sounds.is_empty() {
            return Some(
                "(The person took all the sounds out of their palette: choose your own.)".into(),
            );
        }
        let only = match palette.only {
            true => "; make the song with these and no others",
            false => "; start with these, then fill what's missing",
        };
        let lines: Vec<String> = palette
            .sounds
            .iter()
            .map(|s| about(s, self.library_sound(&s.path).map(|(_, s)| s)))
            .collect();
        Some(format!(
            "(Sounds the person picked for this song{only}. Put each in with samples_put, for its \
             part; `palette` says which are in the song already.)\n{}",
            lines.join("\n")
        ))
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

    fn role_menu(&mut self, i: usize, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let now = self
            .session
            .project
            .palette
            .sounds
            .get(i)
            .and_then(|s| s.role.clone());
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

    /// In the Agent panel's message box: the sounds picked for the agent, each to hear (click),
    /// give a part (its tag) or take out; "only these", and more to pick.
    pub fn palette_card(&self, cx: &mut Context<Self>) -> Option<Div> {
        let palette = &self.session.project.palette;
        if palette.sounds.is_empty() {
            return None;
        }
        let chips = palette.sounds.iter().enumerate().map(|(i, sound)| {
            let found = self.library_sound(&sound.path).map(|(_, s)| s);
            let color = found.map_or(TEXT_3, |s| category_color(s.analysis.category));
            let name = sound
                .path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let path = sound.path.clone();
            let role = sound.role.clone();
            div()
                .id(("palette-sound", i))
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
                .cursor_pointer()
                .hover(|d| d.border_color(rgb(TEXT_4)))
                .debug_selector(move || format!("palette-sound-{i}"))
                .child(match found {
                    Some(s) => div().w(px(24.)).h(px(14.)).flex_none().child(wave(
                        s.analysis.waveform.clone(),
                        color,
                        0.9,
                    )),
                    None => div()
                        .size(px(14.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(Icon::Wave, 12.).text_color(rgb(color))),
                })
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
                        .hover(|d| d.bg(alpha(AGENT, 0.26)))
                        .debug_selector(move || format!("palette-role-{i}"))
                        .child(role.as_deref().map_or("any".into(), capital))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.role_menu(i, e.position, cx);
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
                        .hover(|d| d.bg(rgb(LINE_STRONG)))
                        .debug_selector(move || format!("palette-remove-{i}"))
                        .child(icon(Icon::Close, 9.).text_color(rgb(TEXT_3)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.palette_op(PaletteOp::Remove(i), cx);
                            }),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.hear_palette(&path, cx)),
                )
        });
        let only = palette.only;
        let n = palette.sounds.len();
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
                        .child(icon(Icon::Wave, 12.).text_color(rgb(AGENT)))
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_2))
                                .child(format!("Sounds to use · {n}")),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
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
                                .debug_selector(|| "palette-more".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.pick_sounds(cx)),
                                ),
                        ),
                )
                // A list: five show, the rest scroll.
                .child(
                    div()
                        .id("palette-sounds")
                        .max_h(px(5.0 * 30.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .children(chips),
                ),
        )
    }

    /// With nothing picked: a quiet line atop the message box, to start picking.
    pub fn palette_start(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.session.project.palette.sounds.is_empty() {
            return None;
        }
        Some(
            div().flex().px(px(8.)).pt(px(7.)).child(
                div()
                    .h(px(22.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                    .child(icon(Icon::Wave, 11.).text_color(rgb(AGENT)))
                    .child("Pick sounds for the agent")
                    .debug_selector(|| "palette-start".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.pick_sounds(cx)),
                    ),
            ),
        )
    }

    /// Over the Browser while picking for the agent: how many, and back to it.
    pub fn picking_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.picking_sounds {
            return None;
        }
        let n = self.session.project.palette.sounds.len();
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
                                .child("Picking sounds for the agent"),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(match n {
                                    0 => "＋ on a sound gives it to the agent".to_string(),
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
            .when(!on && !self.picking_sounds, |d| {
                d.opacity(0.0)
                    .group_hover(group.to_string(), |s| s.opacity(1.0))
            })
            .debug_selector(move || format!("give-agent-{name}"))
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
                    ws.give_agent(path.clone(), cx);
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
