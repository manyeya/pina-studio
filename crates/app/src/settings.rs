//! Settings (⌘,), as Logic's: a section at a time from the list down the left, kept for every
//! song in `~/Library/Application Support/tempo/settings.json`. Track headers (which parts show,
//! as Logic's Track Header Components, and how tall new tracks are), regions and the grid (names,
//! contents, colour, bar shading, beat lines), editing (the loop zone at a region's end, writing
//! with a double-click), the transport (following the playhead, going back where play started on
//! stop), automation (its lanes' height, what an automated control does moved while stopped,
//! snapping, thinning, Write turning to Latch, automation going with regions), and the editor's
//! tabs (a hidden one's shortcut still opens it).

use std::path::PathBuf;

use gpui::{Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use serde::{Deserialize, Serialize};

use crate::{
    automation::Stopped,
    theme::*,
    workspace::{Bottom, View, Workspace},
};

/// What a setting's switch does.
type Toggle = Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// The editor's tabs left out, by name.
    pub hidden: Vec<String>,
    pub follow_playhead: bool,
    /// Stopping goes back to where play started, as Logic's "Return to Last Play Position".
    pub return_on_stop: bool,
    /// The track headers' parts.
    pub header_numbers: bool,
    pub header_icons: bool,
    pub header_kind: bool,
    pub header_meter: bool,
    pub header_strip: bool,
    pub header_record: bool,
    /// A track's height, unless it's been dragged.
    pub track_height: f32,
    /// Regions: their names, what's in them, their colour bright (or soft).
    pub region_names: bool,
    pub region_contents: bool,
    pub region_bright: bool,
    /// The grid: every other four bars shaded, a line each beat.
    pub bar_shading: bool,
    pub beat_lines: bool,
    /// The top half of a region's end loops it; a double-click in an empty lane writes a region.
    pub loop_zone: bool,
    pub double_click_writes: bool,
    /// Automation: how tall each lane is; what moving an automated control does while stopped;
    /// points on the grid; what's recorded thinned of points that change nothing; Write
    /// turning to Latch after a pass; a region's automation going with it when it moves.
    pub auto_height: f32,
    pub stopped: Stopped,
    pub snap_automation: bool,
    pub thin_recorded: bool,
    pub write_to_latch: bool,
    pub carry_automation: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hidden: Vec::new(),
            follow_playhead: true,
            return_on_stop: false,
            header_numbers: true,
            header_icons: true,
            header_kind: true,
            header_meter: true,
            header_strip: true,
            header_record: true,
            track_height: LANE_H,
            region_names: true,
            region_contents: true,
            region_bright: false,
            bar_shading: true,
            beat_lines: true,
            loop_zone: true,
            double_click_writes: true,
            auto_height: 64.0,
            stopped: Stopped::Trim,
            snap_automation: true,
            thin_recorded: true,
            write_to_latch: true,
            carry_automation: false,
        }
    }
}

fn file() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/tempo/settings.json")
}

impl Settings {
    /// As kept; as they come in tests, which never read or write the person's.
    pub fn load() -> Self {
        match cfg!(test) {
            true => Settings::default(),
            false => std::fs::read(file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        }
    }

    fn save(&self) {
        if cfg!(test) {
            return;
        }
        let path = file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn shows(&self, tab: Bottom) -> bool {
        !self.hidden.iter().any(|h| h == tab.key())
    }
}

/// The page's sections, down its left.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Section {
    #[default]
    Tracks,
    Regions,
    Editing,
    Transport,
    Automation,
    Tabs,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Tracks,
        Section::Regions,
        Section::Editing,
        Section::Transport,
        Section::Automation,
        Section::Tabs,
    ];

    fn name(self) -> &'static str {
        match self {
            Section::Tracks => "Track headers",
            Section::Regions => "Regions & grid",
            Section::Editing => "Editing",
            Section::Transport => "Transport",
            Section::Automation => "Automation",
            Section::Tabs => "Editor tabs",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::Tracks => Icon::PanelLeft,
            Section::Regions => Icon::Grid,
            Section::Editing => Icon::Pointer,
            Section::Transport => Icon::Play,
            Section::Automation => Icon::Fade,
            Section::Tabs => Icon::PanelBottom,
        }
    }

    fn note(self) -> &'static str {
        match self {
            Section::Tracks => {
                "What each track's header shows, as Logic's Track Header Components; and how tall a track is until you drag it."
            }
            Section::Regions => "How regions and the grid behind them look.",
            Section::Editing => "What the mouse does in the tracks.",
            Section::Transport => "Playing and stopping.",
            Section::Automation => {
                "Volume, pan, mute, sends and plugin parameters over time: the lanes under each track (A), and what moving an automated control does."
            }
            Section::Tabs => {
                "The tabs under the tracks. A hidden one's shortcut still opens it, its tab showing while it's open."
            }
        }
    }
}

impl Bottom {
    /// The editor's tabs, in order.
    pub const ALL: [Bottom; 7] = [
        Bottom::PianoRoll,
        Bottom::Steps,
        Bottom::Sampler,
        Bottom::Modulation,
        Bottom::Score,
        Bottom::Takes,
        Bottom::Loops,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "piano_roll",
            Bottom::Steps => "steps",
            Bottom::Sampler => "sampler",
            Bottom::Modulation => "modulation",
            Bottom::Score => "score",
            Bottom::Takes => "takes",
            Bottom::Loops => "live_loops",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "Piano Roll",
            Bottom::Steps => "Step Sequencer",
            Bottom::Sampler => "Sampler",
            Bottom::Modulation => "Modulation",
            Bottom::Score => "Score",
            Bottom::Takes => "Takes",
            Bottom::Loops => "Live Loops",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Bottom::PianoRoll => "A MIDI region's notes against a keyboard · P",
            Bottom::Steps => "Patterns: the sounds as pads, a row of steps each",
            Bottom::Sampler => "The Sampler's zones, waveform and voice",
            Bottom::Modulation => "LFOs, steps, envelopes and followers, and what they move",
            Bottom::Score => "A MIDI region as notation · N",
            Bottom::Takes => "Takes recorded over each other, to comp",
            Bottom::Loops => "Cells to launch, a column a scene · L",
        }
    }
}

/// A switch's setting, read and written.
type Field = fn(&mut Settings) -> &mut bool;

/// Heights an automation lane can be.
const AUTO_HEIGHTS: [(&str, f32); 3] = [("Small", 44.0), ("Medium", 64.0), ("Large", 96.0)];

/// Heights a track can start at.
const HEIGHTS: [(&str, f32); 4] = [
    ("Small", 52.0),
    ("Medium", LANE_H),
    ("Large", 110.0),
    ("Huge", 160.0),
];

impl Workspace {
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Settings => View::Arrange,
            _ => View::Settings,
        };
        cx.notify();
    }

    fn set_tab_shown(&mut self, tab: Bottom, show: bool, cx: &mut Context<Self>) {
        let shown = Bottom::ALL
            .iter()
            .filter(|&&b| self.settings.shows(b))
            .count();
        if !show && shown <= 1 {
            self.status = Some("One editor tab at least stays.".into());
            cx.notify();
            return;
        }
        self.settings.hidden.retain(|h| h != tab.key());
        if !show {
            self.settings.hidden.push(tab.key().to_string());
        }
        self.settings.save();
        cx.notify();
    }

    fn flip(&mut self, field: Field, cx: &mut Context<Self>) {
        let v = field(&mut self.settings);
        *v = !*v;
        self.settings.save();
        cx.notify();
    }

    pub fn settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let section = self.settings_section;
        let row = |title: &'static str,
                   what: &'static str,
                   on: bool,
                   name: String,
                   f: Toggle,
                   cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(14.))
                .py(px(11.))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(OFF)))
                .debug_selector(move || name.clone())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child(what),
                        ),
                )
                .child(switch(on))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| f(ws, cx)),
                )
        };
        let card = |rows: Vec<Div>| {
            div()
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .overflow_hidden()
                .children(rows)
        };
        // A switch for one of the settings, by its name in the file.
        let s = self.settings.clone();
        let flag = |title: &'static str,
                    what: &'static str,
                    key: &'static str,
                    field: Field,
                    cx: &mut Context<Self>| {
            let on = *field(&mut s.clone());
            row(
                title,
                what,
                on,
                format!("setting-{key}"),
                Box::new(move |ws, cx| ws.flip(field, cx)),
                cx,
            )
        };
        let body: Vec<Div> = match section {
            Section::Tracks => {
                let height = self.settings.track_height;
                let heights = HEIGHTS.map(|(label, h)| {
                    seg(label, (height - h).abs() < 0.5, 26.)
                        .debug_selector(move || format!("setting-height-{label}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.settings.track_height = h;
                                ws.settings.save();
                                cx.notify();
                            }),
                        )
                });
                vec![
                    card(vec![
                        flag(
                            "Numbers",
                            "01, 02… beside each name",
                            "header_numbers",
                            |s| &mut s.header_numbers,
                            cx,
                        ),
                        flag(
                            "Icons",
                            "What each track is, at a glance (click one for its colour and icon)",
                            "header_icons",
                            |s| &mut s.header_icons,
                            cx,
                        ),
                        flag(
                            "What it plays",
                            "Its instrument, under its name",
                            "header_kind",
                            |s| &mut s.header_kind,
                            cx,
                        ),
                        flag(
                            "Level meters",
                            "The LEDs beside M S R",
                            "header_meter",
                            |s| &mut s.header_meter,
                            cx,
                        ),
                        flag(
                            "Fader and pan",
                            "Its volume and pan, under M S R",
                            "header_strip",
                            |s| &mut s.header_strip,
                            cx,
                        ),
                        flag(
                            "Record buttons",
                            "R, to arm a track",
                            "header_record",
                            |s| &mut s.header_record,
                            cx,
                        ),
                    ]),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .px(px(14.))
                        .py(px(12.))
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Track height"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(rgb(TEXT_3))
                                        .child("Every track's, until you drag its bottom edge"),
                                ),
                        )
                        .child(tray().p(px(2.)).rounded(px(8.)).children(heights)),
                ]
            }
            Section::Regions => vec![card(vec![
                flag(
                    "Names",
                    "Each region's name across its top",
                    "region_names",
                    |s| &mut s.region_names,
                    cx,
                ),
                flag(
                    "Contents",
                    "Its notes, steps or waveform drawn inside it",
                    "region_contents",
                    |s| &mut s.region_contents,
                    cx,
                ),
                flag(
                    "Bright colours",
                    "Regions filled in their track's colour more strongly",
                    "region_bright",
                    |s| &mut s.region_bright,
                    cx,
                ),
                flag(
                    "Bar shading",
                    "Every other four bars a shade lighter",
                    "bar_shading",
                    |s| &mut s.bar_shading,
                    cx,
                ),
                flag(
                    "Beat lines",
                    "A line each beat, not just each bar",
                    "beat_lines",
                    |s| &mut s.beat_lines,
                    cx,
                ),
            ])],
            Section::Editing => vec![card(vec![
                flag(
                    "Loop zone",
                    "The top half of a region's end loops it; off, the whole end trims",
                    "loop_zone",
                    |s| &mut s.loop_zone,
                    cx,
                ),
                flag(
                    "Double-click writes",
                    "A double-click in an empty lane makes a MIDI region and opens it in the piano roll",
                    "double_click_writes",
                    |s| &mut s.double_click_writes,
                    cx,
                ),
            ])],
            Section::Transport => vec![card(vec![
                flag(
                    "Follow the playhead",
                    "The tracks page along as the song plays, a screen at a time",
                    "follow",
                    |s| &mut s.follow_playhead,
                    cx,
                ),
                flag(
                    "Return when stopping",
                    "Stopping goes back to where play started, as Logic's Return to Last Play Position",
                    "return_on_stop",
                    |s| &mut s.return_on_stop,
                    cx,
                ),
            ])],
            Section::Automation => {
                // A choice of a few, beside its title.
                let choose = |title: &'static str, what: &'static str, segs: Vec<Div>| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .px(px(14.))
                        .py(px(12.))
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(rgb(TEXT_3))
                                        .child(what),
                                ),
                        )
                        .child(tray().p(px(2.)).rounded(px(8.)).children(segs))
                };
                let height = self.settings.auto_height;
                let heights = AUTO_HEIGHTS
                    .map(|(label, h)| {
                        seg(label, (height - h).abs() < 0.5, 26.)
                            .debug_selector(move || format!("setting-auto-height-{label}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.settings.auto_height = h;
                                    ws.settings.save();
                                    cx.notify();
                                }),
                            )
                    })
                    .into_iter()
                    .collect();
                let stopped = self.settings.stopped;
                let stops = Stopped::ALL
                    .map(|st| {
                        seg(st.name(), st == stopped, 26.)
                            .debug_selector(move || format!("setting-stopped-{}", st.name()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.settings.stopped = st;
                                    ws.settings.save();
                                    cx.notify();
                                }),
                            )
                    })
                    .into_iter()
                    .collect();
                vec![
                    choose(
                        "Lane height",
                        "Each automation lane's, under its track",
                        heights,
                    ),
                    choose(
                        "Moved while stopped",
                        "An automated control moved with the song stopped moves its whole line, sets a point at the playhead, or changes it until you play",
                        stops,
                    ),
                    card(vec![
                        flag(
                            "Snap points",
                            "Points drawn or dragged go to the grid (hold ⌘ to place one freely)",
                            "snap_automation",
                            |s| &mut s.snap_automation,
                            cx,
                        ),
                        flag(
                            "Thin what's recorded",
                            "After a pass, and after the pencil, points that change nothing are taken out",
                            "thin_recorded",
                            |s| &mut s.thin_recorded,
                            cx,
                        ),
                        flag(
                            "Write turns to Latch",
                            "After a pass in Write, the track goes to Latch, so the next doesn't wipe it",
                            "write_to_latch",
                            |s| &mut s.write_to_latch,
                            cx,
                        ),
                        flag(
                            "Automation moves with regions",
                            "Moving or copying a region takes the automation under it along",
                            "carry_automation",
                            |s| &mut s.carry_automation,
                            cx,
                        ),
                    ]),
                ]
            }
            Section::Tabs => vec![card(
                Bottom::ALL
                    .into_iter()
                    .map(|tab| {
                        let on = self.settings.shows(tab);
                        row(
                            tab.name(),
                            tab.what(),
                            on,
                            format!("setting-tab-{}", tab.key()),
                            Box::new(move |ws, cx| ws.set_tab_shown(tab, !on, cx)),
                            cx,
                        )
                    })
                    .collect(),
            )],
        };
        let rail = Section::ALL.map(|s| {
            let on = s == section;
            div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(9.))
                .rounded(px(8.))
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .when(!on, |d| d.text_color(rgb(TEXT_2)).hover(|d| d.bg(rgb(OFF))))
                .debug_selector(move || format!("settings-{}", s.name()))
                .child(icon(s.icon(), 14.).text_color(rgb(if on { TEXT } else { TEXT_3 })))
                .child(s.name())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.settings_section = s;
                        cx.notify();
                    }),
                )
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(
                div()
                    .w(px(232.))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(px(14.))
                    .bg(rgb(SURFACE))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .px(px(8.))
                            .pb(px(14.))
                            .child(icon(Icon::Settings, 17.).text_color(rgb(TEXT_2)))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            ),
                    )
                    .children(rail),
            )
            .child(
                div()
                    .id("settings")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .overflow_y_scroll()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(620.))
                            .py(px(30.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.))
                                            .child(
                                                div()
                                                    .text_size(px(20.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(section.name()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(TEXT_3))
                                                    .child(section.note()),
                                            ),
                                    )
                                    .child(
                                        button("Done", false)
                                            .debug_selector(|| "settings-done".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    ws.view = View::Arrange;
                                                    cx.notify();
                                                }),
                                            ),
                                    ),
                            )
                            .children(body),
                    ),
            )
    }
}
