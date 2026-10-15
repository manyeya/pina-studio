//! A pop-up list of choices: plugins and buses on the channel strips, keys, markers, snap,
//! automation. Long ones narrow down as you type.

use gpui::{Context, MouseButton, Pixels, Point, anchored, deferred, div, prelude::*, px, rgb};
use tempo_engine::project::{
    At, AutoMode, AuxSend, Comp, Content, Eq, Input, Instrument, Param, PatternRow, PluginRef,
    Saturator,
};

use crate::{
    strip::{Builtin, Channel},
    theme::*,
    workspace::{Name, Snap, Workspace},
};

#[derive(Default)]
pub struct Menu {
    pub at: Point<Pixels>,
    pub items: Vec<(String, MenuAction)>,
    /// Typing narrows the items down, for long lists like a plugin's parameters.
    pub searchable: bool,
    pub query: String,
    /// Side by side, after `lead` items across the top: majors beside minors.
    pub columns: usize,
    pub lead: usize,
}

/// Most items a menu draws at once; typing narrows the rest down.
const SHOWN: usize = 250;

impl Menu {
    /// A menu narrowed down by typing.
    pub fn search(at: Point<Pixels>, items: Vec<(String, MenuAction)>) -> Self {
        Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        }
    }

    /// The items to show, by index: those with every word typed in their label. Headings (items
    /// that do nothing) go while searching.
    pub fn shown(&self) -> Vec<usize> {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        (0..self.items.len())
            .filter(|&i| {
                let (label, action) = &self.items[i];
                let label = label.to_lowercase();
                words.is_empty()
                    || (!matches!(action, MenuAction::Nothing)
                        && words.iter().all(|w| label.contains(w)))
            })
            .collect()
    }

    /// The first item that does something, of those shown: what Enter picks.
    pub fn first(&self) -> Option<MenuAction> {
        self.shown()
            .into_iter()
            .map(|i| self.items[i].1.clone())
            .find(|a| !matches!(a, MenuAction::Nothing))
    }
}

#[derive(Clone)]
pub enum MenuAction {
    Nothing,
    Instrument {
        track: usize,
        instrument: Instrument,
    },
    /// Puts a plugin in an insert slot (an `index` past the end adds one), or takes it out.
    Insert {
        channel: Channel,
        index: usize,
        plugin: Option<PluginRef>,
    },
    /// Adds a channel's built-in EQ or compressor, or takes it out.
    AddBuiltin {
        channel: Channel,
        builtin: Builtin,
    },
    RemoveBuiltin {
        channel: Channel,
        builtin: Builtin,
    },
    /// Sends a track to a bus, or to a new aux.
    Send {
        track: usize,
        bus: Option<u32>,
    },
    /// Plays a track into a bus, or the master.
    Output {
        track: usize,
        bus: Option<u32>,
    },
    /// Adds a row for a sound to a pattern.
    AddRow {
        at: At,
        pitch: u8,
    },
    /// Adds a track after the selected one: an instrument track, or an audio track.
    NewTrack {
        instrument: Option<Instrument>,
    },
    /// The song's key, or None to work it out from the notes.
    Key(Option<String>),
    Snap(Snap),
    /// The track whose notes the piano roll shows faintly, by id.
    Ghost(Option<u64>),
    /// Which parameter a track's automation lane shows, by track id.
    AutoParam(u64, Param),
    /// How a track follows its automation.
    AutoMode(usize, AutoMode),
    /// What an audio track records.
    Input {
        track: usize,
        input: Option<Input>,
    },
    /// A send before or after the fader.
    SendPre {
        track: usize,
        index: usize,
        pre: bool,
    },
    RemoveSend {
        track: usize,
        index: usize,
    },
    /// Beats to a bar, and the note that's a beat.
    TimeSignature([u64; 2]),
    /// Opens a text field to name something.
    Rename(Name),
    /// Makes an alias of a region, after it.
    Alias(At),
    /// Makes an alias a region of its own.
    Unlink(At),
    /// A region's playback quantize, in ticks, or none.
    RegionQuantize(At, Option<u64>),
    /// Names a marker, or adds one at a tick.
    NameMarker(usize, String),
    AddMarker(u64, String),
    DeleteMarker(usize),
    Save,
    Export,
}

/// What markers are usually called.
pub const MARKER_NAMES: [&str; 9] = [
    "Intro",
    "Verse",
    "Pre-Chorus",
    "Chorus",
    "Bridge",
    "Breakdown",
    "Drop",
    "Solo",
    "Outro",
];

impl Workspace {
    pub fn choose(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            MenuAction::Nothing => {}
            MenuAction::Instrument { track, instrument } => {
                let t = &self.session.project.tracks[track];
                let audio = t
                    .regions
                    .iter()
                    .any(|r| matches!(r.content, Content::Audio { .. }));
                if t.instrument.is_none() && audio {
                    self.status = Some(format!(
                        "{} has audio on it: instruments go on instrument tracks.",
                        t.name
                    ));
                } else {
                    self.edit(cx, |ws| {
                        ws.session.project.tracks[track].set_instrument(instrument);
                    });
                }
            }
            MenuAction::Insert {
                channel,
                index,
                plugin,
            } => self.edit(cx, |ws| {
                // A track's automation follows its plugins.
                if let Channel::Track(t) = channel {
                    let _ = ws.session.project.tracks[t].set_insert(index, plugin);
                    return;
                }
                let Some(list) = ws.inserts_mut(channel) else {
                    return;
                };
                match plugin {
                    Some(p) if index < list.len() => list[index] = p,
                    Some(p) => list.push(p),
                    None if index < list.len() => drop(list.remove(index)),
                    None => {}
                }
            }),
            // One it has already, it shows; one it hasn't, it gets.
            MenuAction::AddBuiltin { channel, builtin } => {
                self.channel = channel;
                let (eq, comp) = self.strip_dsp(channel);
                let has = match builtin {
                    Builtin::Eq => eq.is_some(),
                    Builtin::Comp => comp.is_some(),
                    Builtin::Sat => self.sat(channel).is_some(),
                };
                if has {
                    self.show_channel(channel, cx);
                } else {
                    self.edit(cx, |ws| match builtin {
                        Builtin::Eq => drop(ws.strip_dsp_mut(channel).0.insert(Eq::default())),
                        Builtin::Comp => drop(ws.strip_dsp_mut(channel).1.insert(Comp::default())),
                        Builtin::Sat => drop(ws.sat_mut(channel).insert(Saturator::default())),
                    })
                }
            }
            MenuAction::RemoveBuiltin { channel, builtin } => self.edit(cx, |ws| match builtin {
                Builtin::Eq => *ws.strip_dsp_mut(channel).0 = None,
                Builtin::Comp => *ws.strip_dsp_mut(channel).1 = None,
                Builtin::Sat => *ws.sat_mut(channel) = None,
            }),
            MenuAction::Input { track, input } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].input = input;
            }),
            MenuAction::SendPre { track, index, pre } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].sends[index].pre = pre;
            }),
            MenuAction::RemoveSend { track, index } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].sends.remove(index);
            }),
            MenuAction::TimeSignature(sig) => {
                self.edit(cx, |ws| ws.session.project.time_signature = sig)
            }
            MenuAction::Rename(what) => self.pending_name = Some(what),
            MenuAction::Alias(at) => {
                let r = self.session.project.region(at);
                let start = r.start + r.length;
                let mut made = None;
                self.edit(cx, |ws| {
                    made = ws.session.project.alias(at, at.0, start).ok()
                });
                match made {
                    Some(alias) => self.region = Some(alias),
                    None => {
                        self.session.cancel();
                        self.status = Some("Only MIDI and pattern regions have aliases.".into());
                    }
                }
            }
            MenuAction::Unlink(at) => self.edit(cx, |ws| ws.session.project.unlink(at)),
            MenuAction::RegionQuantize(at, q) => self.edit(cx, |ws| {
                ws.session.project.tracks[at.0].regions[at.1].quantize = q;
            }),
            MenuAction::Send { track, bus } => self.edit(cx, |ws| {
                let project = &mut ws.session.project;
                let bus = bus.unwrap_or_else(|| project.add_aux());
                project.tracks[track].sends.push(AuxSend {
                    bus,
                    level_db: 0.0,
                    ..Default::default()
                });
            }),
            MenuAction::Output { track, bus } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].output = bus;
            }),
            MenuAction::AddRow { at, pitch } => self.edit(cx, |ws| {
                let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                if let Content::Pattern { rows, .. } = &mut r.content {
                    let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(16);
                    rows.push(PatternRow {
                        pitch,
                        cells: vec![0; steps],
                    });
                }
            }),
            MenuAction::NewTrack { instrument } => self.new_track(instrument, cx),
            MenuAction::Key(key) => self.edit(cx, |ws| ws.session.project.key = key),
            MenuAction::Snap(snap) => self.snap = snap,
            MenuAction::Ghost(id) => self.roll.ghost = id,
            MenuAction::AutoParam(track, param) => {
                self.params.insert(track, param);
                self.show_automation = true;
            }
            MenuAction::AutoMode(track, mode) => self.edit(cx, |ws| {
                ws.session.project.tracks[track].automation.mode = mode;
                ws.writing.clear();
            }),
            MenuAction::NameMarker(i, name) => self.edit(cx, |ws| {
                if let Some(m) = ws.session.project.markers.get_mut(i) {
                    m.name = name;
                }
            }),
            MenuAction::AddMarker(tick, name) => self.edit(cx, |ws| {
                let markers = &mut ws.session.project.markers;
                let at = markers.partition_point(|m| m.tick <= tick);
                markers.insert(at, tempo_engine::project::Marker { tick, name });
            }),
            MenuAction::DeleteMarker(i) => self.edit(cx, |ws| {
                if i < ws.session.project.markers.len() {
                    ws.session.project.markers.remove(i);
                }
            }),
            MenuAction::Save => {
                self.save(cx);
            }
            MenuAction::Export => self.export(cx),
        }
        cx.notify();
    }

    /// Typing into an open searchable menu: narrows it, Enter picks, Escape closes. Returns
    /// whether the key went to the menu.
    pub fn menu_key(&mut self, e: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        let k = &e.keystroke;
        match k.key.as_str() {
            "escape" => self.menu = None,
            "enter" => {
                if let Some(action) = menu.first() {
                    self.choose(action, cx);
                }
            }
            "backspace" if menu.searchable => drop(menu.query.pop()),
            _ if menu.searchable && !k.modifiers.platform && !k.modifiers.control => {
                match &k.key_char {
                    Some(c) => menu.query.push_str(c),
                    None => return false,
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub fn menu_view(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.menu.as_ref()?;
        let shown = menu.shown();
        let more = shown.len().saturating_sub(SHOWN);
        let mut items: Vec<_> = shown
            .into_iter()
            .take(SHOWN)
            .map(|i| (i, &menu.items[i]))
            .map(|(i, (label, action))| {
                let action = action.clone();
                let inert = matches!(action, MenuAction::Nothing);
                div()
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .text_color(rgb(if inert { TEXT_3 } else { TEXT }))
                    .when(!inert, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(rgb(ACCENT)).text_color(rgb(0xffffff)))
                    })
                    .debug_selector(move || format!("menu-{i}"))
                    .child(label.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.choose(action.clone(), cx);
                        }),
                    )
            })
            .collect();
        // In columns: the lead items across the top, the rest split evenly beside each other.
        if menu.columns > 1 && menu.query.is_empty() && items.len() > menu.lead {
            let rest = items.split_off(menu.lead);
            let per = rest.len().div_ceil(menu.columns);
            let mut columns = div().flex().gap(px(4.));
            let mut rest = rest.into_iter();
            for _ in 0..menu.columns {
                let column: Vec<_> = rest.by_ref().take(per).collect();
                columns = columns.child(div().flex_1().flex().flex_col().children(column));
            }
            items.push(columns);
        }
        let list = div()
            .id("menu")
            .min_w(px(200.))
            .max_h(px(440.))
            .overflow_y_scroll()
            .p(px(4.))
            .rounded(px(10.))
            .bg(rgb(0x1c1c22))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .when(menu.searchable, |d| {
                let typed = !menu.query.is_empty();
                d.child(
                    div()
                        .h(px(28.))
                        .mb(px(4.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(6.))
                        .bg(rgb(FIELD))
                        .text_size(px(12.))
                        .child(icon(Icon::Search, 13.).text_color(rgb(TEXT_3)))
                        .child(
                            div()
                                .text_color(rgb(if typed { TEXT } else { TEXT_4 }))
                                .child(if typed {
                                    menu.query.clone()
                                } else {
                                    "Type to search".into()
                                }),
                        ),
                )
            })
            .children(items)
            .when(more > 0, |d| {
                d.child(
                    div()
                        .h(px(26.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(format!("{more} more: type to narrow them down")),
                )
            })
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                ws.menu = None;
                cx.notify();
            }));
        Some(deferred(anchored().position(menu.at).snap_to_window().child(list)).with_priority(1))
    }
}
