//! The Agent panel's producer tools: the song at a glance (its genre, key, tempo and length),
//! the path from a genre to a master (each stage lit by what the song has, and a click asking
//! the agent for it), the critic (tempo's notes on the song, each with a fix to ask for), undo
//! for a whole turn, and what's selected, sent along as context.

use std::time::Instant;

use gpui::{
    Animation, AnimationExt, Context, Div, FontWeight, MouseButton, MouseDownEvent, div,
    prelude::*, px, rgb,
};
use serde_json::Value;
use tempo_engine::{arrange, critique, playbook, project::Project};

use crate::{
    genres::genre_color,
    menu::{Menu, MenuAction},
    theme::*,
    workspace::Workspace,
};

/// A note of the critic's, as the panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub level: String,
    pub area: String,
    pub what: String,
    pub at: Option<String>,
    pub fix: String,
}

impl Note {
    fn from_json(v: &Value) -> Option<Note> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Some(Note {
            level: s("level")?,
            area: s("area").unwrap_or_default(),
            what: s("what")?,
            at: s("at"),
            fix: s("fix").unwrap_or_default(),
        })
    }
}

#[derive(Default)]
pub struct Critic {
    pub notes: Vec<Note>,
    /// The song's edit count they were made for.
    pub rev: Option<u64>,
    /// From a render (the mix heard), not only the notes.
    pub heard: bool,
    pub listening: Option<Instant>,
    pub open: bool,
    pub error: Option<String>,
}

/// The stages of making a song, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Genre,
    Chords,
    Loop,
    Arrange,
    Mix,
    Master,
}

pub const STAGES: [Stage; 6] = [
    Stage::Genre,
    Stage::Chords,
    Stage::Loop,
    Stage::Arrange,
    Stage::Mix,
    Stage::Master,
];

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Genre => "Genre",
            Stage::Chords => "Chords",
            Stage::Loop => "Loop",
            Stage::Arrange => "Arrange",
            Stage::Mix => "Mix",
            Stage::Master => "Master",
        }
    }

    /// What the agent does for it, in a few words.
    pub fn about(self) -> &'static str {
        match self {
            Stage::Genre => "pick the genre its parts and mix aim at",
            Stage::Chords => "find a progression and set the chord track",
            Stage::Loop => "write an 8-bar loop on real sounds",
            Stage::Arrange => "lay the loop out as a song, with transitions",
            Stage::Mix => "mix it by part, then fix what the critic hears",
            Stage::Master => "master it to the genre's loudness",
        }
    }

    /// Whether the song has it, as far as tempo can tell.
    pub fn done(self, p: &Project) -> bool {
        let sounding = p.tracks.iter().filter(|t| !t.regions.is_empty()).count();
        match self {
            Stage::Genre => p.genre.is_some(),
            // A chord track, or chords its parts play.
            Stage::Chords => {
                !p.chords.is_empty() || {
                    let end = p.end().min(16 * p.bar());
                    tempo_engine::compose::heard_chords(p, 0, end).len() >= 2
                }
            }
            Stage::Loop => sounding >= 2,
            Stage::Arrange => p.markers.len() >= 3 || p.end() >= 24 * p.bar(),
            Stage::Mix => {
                p.auxes.iter().any(|a| a.name == "Verb")
                    || p.tracks
                        .iter()
                        .filter(|t| t.eq.is_some() || t.comp.is_some())
                        .count()
                        >= 2
            }
            Stage::Master => p
                .master_inserts
                .iter()
                .any(|i| i.class.to_lowercase().contains("limit")),
        }
    }

    /// What to ask the agent for, for this song.
    pub fn ask(self, p: &Project) -> String {
        let genre = p
            .genre
            .as_deref()
            .and_then(playbook::find)
            .map_or("its genre".to_string(), |b| b.name);
        let key = p.song_key().map_or("its key".into(), |k| k.name());
        match self {
            Stage::Genre => format!(
                "Pick a genre for this song from tempo's playbooks (it's {:.0} BPM in {key}) and set it, with a tempo and key that suit it. Say why you picked it.",
                p.tempo
            ),
            Stage::Chords => format!(
                "Write this song's chords: find a progression that suits {genre} with the theory tool, and set_chords for 8 bars. Say what you chose and why."
            ),
            Stage::Loop => format!(
                "Make an 8-bar loop for this {genre} song: drums, bass and chords, and a lead if the genre has one, on real sounds (the sounds tool, not the sine synth), written with the write commands. Then critique quick and fix what it says."
            ),
            Stage::Arrange => format!(
                "Arrange the loop into a full song with the {genre} playbook's sections, with transitions into each section, and vary the hook so it lifts. Then critique quick."
            ),
            Stage::Mix => "Mix the song: the mix tool, then critique with a render, and fix every mix issue it finds.".into(),
            Stage::Master => format!(
                "Master the song for {genre}, then critique once more and tell me its loudness and what's left to do."
            ),
        }
    }
}

impl Workspace {
    /// The critic's notes on the song as it is now, from its notes and layout (instant). A
    /// render's notes stay until the song changes.
    pub fn refresh_critic(&mut self) {
        if self.chat.critic.rev == Some(self.edits) || self.chat.critic.listening.is_some() {
            return;
        }
        let issues = critique::critique(&self.session.project, None);
        self.chat.critic.notes = issues
            .iter()
            .filter_map(|i| serde_json::to_value(i).ok())
            .filter_map(|v| Note::from_json(&v))
            .collect();
        self.chat.critic.heard = false;
        self.chat.critic.rev = Some(self.edits);
        self.chat.critic.error = None;
    }

    /// The critic with a render: the mix heard too. In a process of its own (`tempo critique`),
    /// which asks this window for the song.
    fn listen_critic(&mut self, cx: &mut Context<Self>) {
        if self.chat.critic.listening.is_some() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let dir = self.session.dir.clone();
        let rev = self.edits;
        self.chat.critic.listening = Some(Instant::now());
        self.chat.critic.open = true;
        cx.notify();
        let heard = cx.background_spawn(async move {
            let out = std::process::Command::new(exe)
                .arg("critique")
                .arg(&dir)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
            let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            Ok(v["issues"]
                .as_array()
                .map(|a| a.iter().filter_map(Note::from_json).collect::<Vec<_>>())
                .unwrap_or_default())
        });
        cx.spawn(async move |this, cx| {
            let heard = heard.await;
            this.update(cx, |ws, cx| {
                ws.chat.critic.listening = None;
                match heard {
                    Ok(notes) => {
                        ws.chat.critic.notes = notes;
                        ws.chat.critic.heard = true;
                        ws.chat.critic.rev = Some(rev);
                        ws.chat.critic.error = None;
                    }
                    Err(e) => {
                        ws.chat.critic.error = Some(match e.is_empty() {
                            true => "The render didn't finish.".into(),
                            false => e,
                        })
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// What's selected, said for the agent: the region (its track and bars), else the track.
    pub fn selection_note(&self) -> Option<(String, String)> {
        let p = &self.session.project;
        let bar = p.bar();
        if let Some((t, r)) = self.region
            && let Some(region) = p.tracks.get(t).and_then(|tr| tr.regions.get(r))
        {
            let (a, b) = (
                region.start / bar + 1,
                (region.start + region.length).div_ceil(bar),
            );
            let name = &p.tracks[t].name;
            let bars = if a >= b {
                format!("bar {a}")
            } else {
                format!("bars {a}–{b}")
            };
            return Some((
                format!("{name} · {bars}"),
                format!("track {t} \"{name}\", its region {r} ({bars})"),
            ));
        }
        let t = p.tracks.get(self.track)?;
        Some((
            t.name.clone(),
            format!("track {} \"{}\" ({})", self.track, t.name, arrange::role(t)),
        ))
    }

    /// Puts the song back as it was before the agent's last turn, as one step undo can take
    /// back.
    pub fn undo_turn(&mut self, cx: &mut Context<Self>) {
        let Some(before) = self.chat.before.take() else {
            return;
        };
        self.session.checkpoint();
        self.session.project = before;
        self.region = None;
        self.changed(cx);
        self.status =
            Some("The song is back as it was before that turn (⌘Z to put it back).".into());
        cx.notify();
    }

    /// The genres to choose from for the song, and the page to edit them.
    fn genre_menu(&self, at: gpui::Point<gpui::Pixels>) -> Menu {
        let now = self.session.project.genre.clone();
        let mut items: Vec<(String, MenuAction)> = playbook::load()
            .0
            .into_iter()
            .map(|b| {
                let mark = if now.as_deref() == Some(b.id.as_str()) {
                    "✓ "
                } else {
                    "    "
                };
                (
                    format!("{mark}{}  ·  {:.0}–{:.0}", b.name, b.tempo.0, b.tempo.1),
                    MenuAction::Genre(Some(b.id)),
                )
            })
            .collect();
        if now.is_some() {
            items.push(("    No genre".into(), MenuAction::Genre(None)));
        }
        items.push(("    Edit playbooks…".into(), MenuAction::Playbooks(now)));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// Under the panel's header: the song at a glance, and the path from a genre to a master.
    pub fn agent_context(&mut self, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let busy = self.chat.busy;
        let genre = p.genre.as_deref().and_then(playbook::find);
        let color = genre.as_ref().map_or(TEXT_3, |g| genre_color(&g.id));
        let bars = p.end().div_ceil(p.bar());
        let key = p.song_key().map_or("no key".to_string(), |k| k.short());
        // The genre, a chip that picks another; the rest of the song in a quiet line beside it.
        let chip = div()
            .id("agent-genre")
            .flex_none()
            .h(px(26.))
            .pl(px(8.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(13.))
            .border_1()
            .border_color(alpha(color, if genre.is_some() { 0.45 } else { 0.25 }))
            .cursor_pointer()
            .hover(|d| d.bg(alpha(color, 0.1)))
            .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if genre.is_some() { TEXT } else { TEXT_3 }))
                    .child(
                        genre
                            .as_ref()
                            .map_or("Pick a genre".to_string(), |g| g.name.clone()),
                    ),
            )
            .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
            .debug_selector(|| "agent-genre".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    ws.menu = Some(ws.genre_menu(e.position));
                    cx.notify();
                }),
            );
        let facts = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .justify_end()
            .font_family(MONO)
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .whitespace_nowrap()
            .child(format!("{key} · {:.0} bpm · {bars} bars", p.tempo));

        // The path: a segment a stage, lit when the song has it; the first it hasn't, breathing.
        let done: Vec<bool> = STAGES.iter().map(|s| s.done(p)).collect();
        let next = done.iter().position(|d| !d);
        let segments: Vec<_> = STAGES
            .iter()
            .enumerate()
            .map(|(i, &stage)| {
                let (is_done, is_next) = (done[i], next == Some(i));
                let bar = div()
                    .h(px(4.))
                    .rounded(px(2.))
                    .map(|d| match (is_done, is_next) {
                        (true, _) => d.bg(rgb(AGENT)),
                        (false, true) => d.bg(alpha(AGENT, 0.35)),
                        _ => d.bg(rgb(LINE_STRONG)),
                    });
                let bar = match is_next && !busy {
                    true => bar
                        .with_animation(
                            ("path-next", i),
                            Animation::new(std::time::Duration::from_millis(1600))
                                .repeat()
                                .with_easing(gpui::pulsating_between(0.4, 1.0)),
                            |d, t| d.opacity(t),
                        )
                        .into_any_element(),
                    false => bar.into_any_element(),
                };
                let ask = stage.ask(p);
                div()
                    .id(("stage", i))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .when(!busy, |d| d.cursor_pointer())
                    .group("stage")
                    .child(bar)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .text_size(px(10.5))
                            .text_color(rgb(match (is_done, is_next) {
                                (true, _) => TEXT_2,
                                (_, true) => AGENT_TEXT,
                                _ => TEXT_4,
                            }))
                            .when(!busy, |d| d.hover(|d| d.text_color(rgb(TEXT))))
                            .whitespace_nowrap()
                            .child(stage.name()),
                    )
                    .debug_selector(move || format!("stage-{}", stage.name().to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if !ws.chat.busy {
                                ws.ask_agent(ask.clone(), cx);
                            }
                        }),
                    )
            })
            .collect();
        // What's next, said, with a button to ask for it.
        let next_line = next.map(|i| {
            let stage = STAGES[i];
            let ask = stage.ask(p);
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(AGENT_TEXT))
                                .child(format!("Next: {}", stage.name())),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .line_height(px(15.))
                                .text_color(rgb(TEXT_3))
                                .child(sentence(stage.about())),
                        ),
                )
                .child(
                    div()
                        .id("path-ask")
                        .flex_none()
                        .h(px(22.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .rounded(px(11.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .when(busy, |d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_4)))
                        .when(!busy, |d| {
                            d.bg(rgb(AGENT))
                                .text_color(rgb(BG))
                                .cursor_pointer()
                                .hover(|d| d.opacity(0.85))
                        })
                        .child("Ask")
                        .child(icon(Icon::Forward, 10.).text_color(rgb(if busy {
                            TEXT_4
                        } else {
                            BG
                        })))
                        .debug_selector(|| "path-ask".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if !ws.chat.busy {
                                    ws.ask_agent(ask.clone(), cx);
                                }
                            }),
                        ),
                )
        });
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(11.))
            .px(px(12.))
            .pt(px(10.))
            .pb(px(11.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(chip)
                    .child(facts),
            )
            .child(div().flex().gap(px(3.)).children(segments))
            .children(next_line)
    }

    /// The critic, in the composer's toolbar: a dot in the color of the worst it found, and
    /// how much; it opens its notes above.
    pub fn critic_chip(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        self.refresh_critic();
        let c = &self.chat.critic;
        let count = |l: &str| c.notes.iter().filter(|n| n.level == l).count();
        let (fixes, improves) = (count("fix"), count("improve"));
        let (dot, text) = match (c.listening, fixes, improves) {
            (Some(_), ..) => (ACCENT, "Listening…".to_string()),
            (None, 0, 0) => (METER_LOW, "Sounds right".to_string()),
            (None, 0, i) => (METER_MID, format!("{i} to improve")),
            (None, f, _) => (METER_HIGH, format!("{f} to fix")),
        };
        let open = c.open;
        let chip = div()
            .id("critic-chip")
            .flex_none()
            .h(px(26.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .text_size(px(11.5))
            .text_color(rgb(if open { TEXT } else { TEXT_2 }))
            .when(open, |d| d.bg(rgb(RAISED)))
            .hover(|d| d.bg(rgb(RAISED)))
            .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(dot)))
            .child(text)
            .debug_selector(|| "critic-head".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.chat.critic.open ^= true;
                    cx.notify();
                }),
            );
        // Listening: it breathes until the render's heard.
        match c.listening.is_some() {
            true => chip
                .with_animation(
                    "critic-listening",
                    Animation::new(std::time::Duration::from_millis(1200))
                        .repeat()
                        .with_easing(gpui::pulsating_between(0.5, 1.0)),
                    |d, t| d.opacity(t),
                )
                .into_any_element(),
            false => chip.into_any_element(),
        }
    }

    /// The critic's notes, in a sheet over the composer while it's open: each with a fix to ask
    /// for, Fix all, and Listen for what a render hears.
    pub fn critic_sheet(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.chat.critic.open {
            return None;
        }
        let c = &self.chat.critic;
        let busy = self.chat.busy;
        let count = |l: &str| c.notes.iter().filter(|n| n.level == l).count();
        let (fixes, improves) = (count("fix"), count("improve"));
        let color = |l: &str| match l {
            "fix" => METER_HIGH,
            "improve" => METER_MID,
            _ => TEXT_4,
        };
        let ghost = |label: &'static str| {
            div()
                .flex_none()
                .h(px(24.))
                .px(px(9.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_color(rgb(TEXT_2))
                .when(busy, |d| d.opacity(0.4))
                .when(!busy, |d| {
                    d.cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                })
                .child(label)
        };
        let ordered: Vec<Note> = c
            .notes
            .iter()
            .filter(|n| n.level == "fix")
            .chain(c.notes.iter().filter(|n| n.level == "improve"))
            .chain(c.notes.iter().filter(|n| n.level == "note"))
            .cloned()
            .collect();
        let rows: Vec<_> = ordered
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let ask = format!(
                    "Fix this, from tempo's critic ({}): {}{}. Its advice: {} Make the change, then critique quick to check it.",
                    n.area,
                    n.what,
                    n.at.as_ref().map_or(String::new(), |a| format!(" ({a})")),
                    n.fix
                );
                div()
                    .flex()
                    .gap(px(10.))
                    .px(px(14.))
                    .py(px(9.))
                    .when(i > 0, |d| d.border_t_1().border_color(rgb(LINE_SOFT)))
                    .child(div().w(px(3.)).flex_none().rounded(px(2.)).bg(rgb(color(&n.level))))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .line_height(px(17.))
                                    .text_color(rgb(TEXT))
                                    .child(sentence(&n.what)),
                            )
                            .when_some(n.at.clone(), |d, at| {
                                d.child(
                                    div()
                                        .font_family(MONO)
                                        .text_size(px(10.5))
                                        .text_color(rgb(TEXT_3))
                                        .child(at),
                                )
                            })
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(px(16.))
                                    .text_color(rgb(TEXT_3))
                                    .child(n.fix.clone()),
                            ),
                    )
                    .when(n.level != "note", |d| {
                        d.child(
                            ghost("Fix")
                                .id(("critic-fix", i))
                                .debug_selector(move || format!("critic-fix-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        if !ws.chat.busy {
                                            ws.ask_agent(ask.clone(), cx);
                                        }
                                    }),
                                ),
                        )
                    })
            })
            .collect();
        let all = {
            let wrong: Vec<String> = ordered
                .iter()
                .filter(|n| n.level != "note")
                .map(|n| {
                    let at = n.at.as_ref().map_or(String::new(), |a| format!(" ({a})"));
                    format!("- {}{at}: {}", n.what, n.fix)
                })
                .collect();
            format!(
                "Fix what tempo's critic found, the worst first, then critique again to check:\n{}",
                wrong.join("\n")
            )
        };
        let summary = match (fixes, improves) {
            (0, 0) => "Nothing wrong".to_string(),
            (0, i) => format!("{i} to improve"),
            (f, 0) => format!("{f} to fix"),
            (f, i) => format!("{f} to fix, {i} to improve"),
        };
        let heard = c.heard;
        let error = c.error.clone();
        let listening = c.listening;
        Some(
            div().flex_none().px(px(10.)).pt(px(6.)).child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(14.))
                            .py(px(10.))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("Critic"))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(TEXT_3))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(match (listening, heard) {
                                                (Some(t), _) => format!("Rendering to listen… {}s", t.elapsed().as_secs()),
                                                (None, true) => format!("{summary}, heard in a render"),
                                                (None, false) => format!("{summary}, from the notes"),
                                            }),
                                    ),
                            )
                            .child(
                                ghost(if heard { "Listen again" } else { "Listen to the mix" })
                                    .id("critic-listen")
                                    .debug_selector(|| "critic-listen".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.listen_critic(cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("critic-close")
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(7.))
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(RAISED)))
                                    .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_3)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            ws.chat.critic.open = false;
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("critic-notes")
                            .max_h(px(300.))
                            .overflow_y_scroll()
                            .children(rows)
                            .when(ordered.is_empty() && listening.is_none(), |d| {
                                d.child(
                                    div()
                                        .px(px(14.))
                                        .py(px(14.))
                                        .text_size(px(12.))
                                        .line_height(px(17.))
                                        .text_color(rgb(TEXT_3))
                                        .child("Nothing wrong in the notes or the layout. Listen to the mix for what a render hears: levels, balance, loudness."),
                                )
                            })
                            .when_some(error, |d, e| {
                                d.child(
                                    div()
                                        .px(px(14.))
                                        .py(px(8.))
                                        .text_size(px(11.5))
                                        .text_color(rgb(METER_HIGH))
                                        .child(e),
                                )
                            }),
                    )
                    .when(fixes + improves > 0, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .px(px(14.))
                                .py(px(9.))
                                .border_t_1()
                                .border_color(rgb(LINE))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_4))
                                        .child("The agent fixes them, then checks again"),
                                )
                                .child(
                                    div()
                                        .id("critic-fix-all")
                                        .h(px(26.))
                                        .px(px(11.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(8.))
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .when(busy, |d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_4)))
                                        .when(!busy, |d| {
                                            d.bg(rgb(TEXT)).text_color(rgb(BG)).cursor_pointer().hover(|d| d.bg(rgb(0xffffff)))
                                        })
                                        .child(format!("Fix all {}", fixes + improves))
                                        .debug_selector(|| "critic-fix-all".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, _, _, cx| {
                                                if !ws.chat.busy {
                                                    ws.ask_agent(all.clone(), cx);
                                                }
                                            }),
                                        ),
                                ),
                        )
                    }),
            ),
        )
    }
}

/// A note's words as a sentence: its first letter capital.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
