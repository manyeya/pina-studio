//! Settings (⌘,): what the window shows, kept for every song in
//! `~/Library/Application Support/tempo/settings.json`. The editor's tabs (a hidden one's
//! shortcut still opens it, its tab showing while it's open), and whether the tracks page along
//! with the playhead as the song plays.

use std::path::PathBuf;

use gpui::{Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use serde::{Deserialize, Serialize};

use crate::{
    theme::*,
    workspace::{Bottom, View, Workspace},
};

/// What a setting's switch does.
type Toggle = Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    /// The editor's tabs left out, by name.
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default = "yes")]
    pub follow_playhead: bool,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hidden: Vec::new(),
            follow_playhead: true,
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

    pub fn settings_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let section = |title: &'static str, note: &'static str, rows: Vec<Div>| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .when(!note.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(note),
                            )
                        }),
                )
                .child(
                    div()
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .overflow_hidden()
                        .children(rows),
                )
        };
        let tabs: Vec<Div> = Bottom::ALL
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
            .collect();
        let follow = self.settings.follow_playhead;
        let playing = vec![row(
            "Follow the playhead",
            "The tracks page along as the song plays, a screen at a time",
            follow,
            "setting-follow".into(),
            Box::new(move |ws, cx| {
                ws.settings.follow_playhead = !follow;
                ws.settings.save();
                cx.notify();
            }),
            cx,
        )];
        div()
            .id("settings")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(BG))
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(640.))
                    .py(px(36.))
                    .flex()
                    .flex_col()
                    .gap(px(28.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(icon(Icon::Settings, 20.).text_color(rgb(TEXT_2)))
                            .child(div().flex_1().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Settings"))
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
                    .child(section(
                        "Editor tabs",
                        "The tabs under the tracks. A hidden one's shortcut still opens it, its tab showing while it's open.",
                        tabs,
                    ))
                    .child(section("While playing", "", playing)),
            )
    }
}
