//! A pop-up list of choices: plugins and buses on the channel strips, keys, markers, snap.

use gpui::{Context, MouseButton, Pixels, Point, anchored, deferred, div, prelude::*, px, rgb};
use tempo_engine::{
    music::NoteEdit,
    project::{At, AuxSend, Comp, Content, Eq, Instrument, PatternRow, PluginRef},
};

use crate::{
    strip::{Builtin, Channel},
    theme::*,
    workspace::{Snap, Workspace},
};

pub struct Menu {
    pub at: Point<Pixels>,
    pub items: Vec<(String, MenuAction)>,
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
    /// Quantizes the selected region to a grid (ticks).
    Quantize(u64),
    Snap(Snap),
    /// The track whose notes the piano roll shows faintly, by id.
    Ghost(Option<u64>),
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
                        ws.session.project.tracks[track].instrument = Some(instrument);
                    });
                }
            }
            MenuAction::Insert {
                channel,
                index,
                plugin,
            } => self.edit(cx, |ws| {
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
            MenuAction::AddBuiltin { channel, builtin } => {
                self.channel = channel;
                self.edit(cx, |ws| {
                    let (eq, comp) = ws.strip_dsp_mut(channel);
                    match builtin {
                        Builtin::Eq => {
                            eq.get_or_insert_with(Eq::default);
                        }
                        Builtin::Comp => {
                            comp.get_or_insert_with(Comp::default);
                        }
                    }
                })
            }
            MenuAction::RemoveBuiltin { channel, builtin } => self.edit(cx, |ws| {
                let (eq, comp) = ws.strip_dsp_mut(channel);
                match builtin {
                    Builtin::Eq => *eq = None,
                    Builtin::Comp => *comp = None,
                }
            }),
            MenuAction::Send { track, bus } => self.edit(cx, |ws| {
                let project = &mut ws.session.project;
                let bus = bus.unwrap_or_else(|| project.add_aux());
                project.tracks[track]
                    .sends
                    .push(AuxSend { bus, level_db: 0.0 });
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
            MenuAction::Quantize(grid) => {
                self.roll.grid = grid;
                if let Some(at) = self.region {
                    let edit = NoteEdit::Quantize {
                        grid,
                        strength: self.roll.strength,
                        swing: self.roll.swing,
                    };
                    self.edit_notes(at, edit, cx);
                }
            }
            MenuAction::Snap(snap) => self.snap = snap,
            MenuAction::Ghost(id) => self.roll.ghost = id,
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

    pub fn menu_view(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.menu.as_ref()?;
        let items: Vec<_> = menu
            .items
            .iter()
            .enumerate()
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
            .children(items)
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                ws.menu = None;
                cx.notify();
            }));
        Some(deferred(anchored().position(menu.at).snap_to_window().child(list)).with_priority(1))
    }
}
