//! What a turn did to the song: worked out from how it was and how it is, so it's true whatever
//! the agent says it did. Each line is somewhere to go, and the card it makes has a way to hear
//! it and a way to take it back.

use pina_engine::song::project::{Project, Region, Track};
use serde::{Deserialize, Serialize};

use super::*;

/// Lines shown before the rest are folded.
const SHOWN: usize = 6;

/// One thing a turn changed: in words, on which track, from which bar.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub what: String,
    #[serde(default)]
    pub track: Option<String>,
    #[serde(default)]
    pub bar: Option<u64>,
}

fn notes(regions: &[Region]) -> usize {
    regions
        .iter()
        .filter_map(|r| r.content.notes(r.length))
        .map(|n| n.len())
        .sum()
}

/// The first bar two tracks' regions part ways at.
fn first_bar(a: &[Region], b: &[Region], bar: u64) -> Option<u64> {
    let start = |r: &Region| r.start;
    let differ = b.iter().filter(|r| !a.contains(r)).map(start);
    differ
        .chain(a.iter().filter(|r| !b.contains(r)).map(start))
        .min()
        .map(|t| t / bar.max(1) + 1)
}

/// What's different about a track, in a line a piece.
fn track_changes(was: &Track, is: &Track, bar: u64, out: &mut Vec<Change>) {
    let mut add = |what: String, at: Option<u64>| {
        out.push(Change {
            what,
            track: Some(is.name.clone()),
            bar: at,
        })
    };
    if was.name != is.name {
        add(format!("Renamed {} to {}", was.name, is.name), None);
    }
    if was.regions != is.regions {
        let at = first_bar(&was.regions, &is.regions, bar);
        let (a, b) = (was.regions.len(), is.regions.len());
        let (x, y) = (notes(&was.regions), notes(&is.regions));
        let many = |n: usize, one: &str| match n {
            1 => format!("1 {one}"),
            n => format!("{n} {one}s"),
        };
        add(
            match (b.cmp(&a), y.cmp(&x)) {
                (std::cmp::Ordering::Greater, _) => {
                    format!("{}: {} added", is.name, many(b - a, "region"))
                }
                (std::cmp::Ordering::Less, _) => {
                    format!("{}: {} taken out", is.name, many(a - b, "region"))
                }
                (_, std::cmp::Ordering::Greater) => {
                    format!("{}: {} added", is.name, many(y - x, "note"))
                }
                (_, std::cmp::Ordering::Less) => {
                    format!("{}: {} taken out", is.name, many(x - y, "note"))
                }
                _ => format!("{}: rewritten", is.name),
            },
            at,
        );
    }
    if was.instrument != is.instrument {
        add(format!("{}: new sound", is.name), None);
    }
    let effects = [
        ("EQ", was.eq != is.eq),
        ("compressor", was.comp != is.comp),
        ("saturator", was.sat != is.sat),
        ("limiter", was.limit != is.limit),
        ("effects", was.inserts != is.inserts || was.more != is.more),
    ]
    .into_iter()
    .filter(|e| e.1)
    .map(|e| e.0)
    .collect::<Vec<_>>();
    if !effects.is_empty() {
        add(format!("{}: {}", is.name, effects.join(", ")), None);
    }
    if was.volume_db != is.volume_db {
        add(
            format!("{}: level {:+.1} dB", is.name, is.volume_db - was.volume_db),
            None,
        );
    }
    if was.pan != is.pan {
        add(format!("{}: panned", is.name), None);
    }
    if was.modulators != is.modulators {
        add(format!("{}: modulation", is.name), None);
    }
    if was.automation != is.automation {
        add(format!("{}: automation", is.name), None);
    }
}

/// What's different between how a song was and how it is.
pub(crate) fn changes(was: &Project, is: &Project) -> Vec<Change> {
    let bar = is.bar();
    let mut out = Vec::new();
    let song = |what: String| Change {
        what,
        track: None,
        bar: None,
    };
    if was.tempo != is.tempo {
        out.push(song(format!("Tempo {:.0} to {:.0}", was.tempo, is.tempo)));
    }
    // The key set, not the one its notes suggest: new notes aren't a change of key.
    if was.key != is.key
        && let Some(k) = &is.key
    {
        out.push(song(format!("Key now {k}")));
    }
    if was.time_signature != is.time_signature {
        let [n, d] = is.time_signature;
        out.push(song(format!("Meter now {n}/{d}")));
    }
    for t in &is.tracks {
        match was.tracks.iter().find(|o| o.id == t.id) {
            Some(old) => track_changes(old, t, bar, &mut out),
            None => out.push(Change {
                what: match t.regions.len() {
                    0 => format!("New track: {}", t.name),
                    1 => format!("New track: {}, 1 region", t.name),
                    n => format!("New track: {}, {n} regions", t.name),
                },
                track: Some(t.name.clone()),
                bar: t.regions.iter().map(|r| r.start / bar.max(1) + 1).min(),
            }),
        }
    }
    for t in &was.tracks {
        if !is.tracks.iter().any(|o| o.id == t.id) {
            out.push(song(format!("Took out {}", t.name)));
        }
    }
    let master = was.master_volume_db != is.master_volume_db
        || was.master_inserts != is.master_inserts
        || was.master_eq != is.master_eq;
    if master {
        out.push(song("Master changed".to_string()));
    }
    out
}

impl Workspace {
    /// What changed, once a turn's over: a card, if anything did.
    pub(crate) fn note_changes(&mut self) {
        let Some(before) = &self.chat.before else {
            return;
        };
        let lines = changes(before, &self.session.project);
        if !lines.is_empty() {
            self.chat.items.push(Item::Changes { lines });
        }
    }

    /// From the bar a card's change starts on; the track it's on picked, by name, if it's there.
    fn go_to_change(&mut self, track: Option<&str>, bar: Option<u64>, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let t = track.and_then(|n| p.tracks.iter().position(|t| t.name == n));
        if let Some(b) = bar {
            self.locate_tick(b.saturating_sub(1) * p.bar(), cx);
        }
        if let Some(t) = t {
            self.select_track(t);
        }
        cx.notify();
    }

    /// The card: what changed, a line each to go to, and to hear it or take it back. Taking it
    /// back is the last turn's, while the song as it was is still to hand.
    pub(crate) fn changes_card(
        &self,
        n: usize,
        lines: &[Change],
        last: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let opened = self.chat.opened.contains(&n);
        let hidden = lines.len().saturating_sub(SHOWN);
        let shown = if opened {
            lines.len()
        } else {
            SHOWN.min(lines.len())
        };
        let from = lines.iter().filter_map(|l| l.bar).min();
        let undo = last
            && !self.chat.busy
            && self
                .chat
                .before
                .as_ref()
                .is_some_and(|b| *b != self.session.project);
        let rows = lines[..shown].iter().enumerate().map(|(k, l)| {
            let (track, bar) = (l.track.clone(), l.bar);
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(11.))
                .py(px(4.))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || format!("change-{n}-{k}"))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(rgb(TEXT_2))
                        .child(l.what.clone()),
                )
                .children(bar.map(|b| {
                    div()
                        .flex_none()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_4))
                        .child(format!("bar {b}"))
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.go_to_change(track.as_deref(), bar, cx);
                    }),
                )
        });
        let action = |id: &'static str, i: Icon, label: &'static str| {
            div()
                .h(px(26.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || id.into())
                .child(icon(i, 11.).text_color(rgb(TEXT_3)))
                .child(label)
        };
        div()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .debug_selector(move || format!("changes-{n}"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .px(px(11.))
                    .pt(px(9.))
                    .pb(px(5.))
                    .child(icon(Icon::Pencil, 12.).text_color(rgb(AGENT_TEXT)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child("What changed"),
                    ),
            )
            .children(rows)
            .when(hidden > 0, |d| {
                d.child(
                    div()
                        .px(px(11.))
                        .py(px(4.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .cursor_pointer()
                        .hover(|d| d.text_color(rgb(TEXT)))
                        .debug_selector(move || format!("changes-more-{n}"))
                        .child(match opened {
                            true => "Show fewer".to_string(),
                            false => format!("{hidden} more"),
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.toggle_opened(n, cx);
                            }),
                        ),
                )
            })
            .child(
                div()
                    .mt(px(4.))
                    .px(px(4.))
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(action("hear-changes", Icon::Play, "Hear it").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let tick =
                                from.unwrap_or(1).saturating_sub(1) * ws.session.project.bar();
                            ws.locate_tick(tick, cx);
                            if !ws.player.playing() {
                                ws.play_stop(cx);
                            }
                        }),
                    ))
                    .when(undo, |d| {
                        d.child(
                            action("undo-turn", Icon::Back, "Undo this turn").on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.undo_turn(cx);
                                }),
                            ),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pina_engine::song::project::{Content, Note, PPQ};

    fn midi(start: u64, pitches: &[u8]) -> Region {
        Region {
            start,
            length: PPQ * 4,
            content: Content::Midi {
                notes: pitches
                    .iter()
                    .map(|&pitch| Note {
                        start: 0,
                        length: PPQ,
                        pitch,
                        velocity: 100,
                        ..Default::default()
                    })
                    .collect(),
                controls: Vec::new(),
            },
            ..Default::default()
        }
    }

    fn song() -> Project {
        let mut p = Project::new(48_000);
        p.tracks = vec![Track {
            id: 1,
            name: "Bass".into(),
            regions: vec![midi(0, &[40])],
            ..Default::default()
        }];
        p
    }

    fn said(was: &Project, is: &Project) -> Vec<String> {
        changes(was, is).into_iter().map(|c| c.what).collect()
    }

    #[test]
    fn nothing_changed_says_nothing() {
        assert!(changes(&song(), &song()).is_empty());
    }

    #[test]
    fn a_change_says_what_and_where() {
        let was = song();
        let mut is = song();
        is.tempo = was.tempo + 4.0;
        is.tracks[0].regions.push(midi(PPQ * 16, &[42]));
        is.tracks[0].volume_db = -3.0;
        is.tracks.push(Track {
            id: 2,
            name: "Lead".into(),
            regions: vec![midi(PPQ * 32, &[60])],
            ..Default::default()
        });
        let c = changes(&was, &is);
        let words: Vec<&str> = c.iter().map(|c| &c.what[..]).collect();
        assert_eq!(
            words[1..],
            [
                "Bass: 1 region added",
                "Bass: level -3.0 dB",
                "New track: Lead, 1 region"
            ]
        );
        assert!(words[0].starts_with("Tempo"));
        assert_eq!(c[1].bar, Some(5), "where the region it added starts");
        assert_eq!(c[3].bar, Some(9));
        assert_eq!(c[3].track.as_deref(), Some("Lead"));
    }

    #[test]
    fn notes_rewritten_in_a_region_count_as_notes() {
        let was = song();
        let mut is = song();
        is.tracks[0].regions[0] = midi(0, &[40, 43, 47]);
        assert_eq!(said(&was, &is), ["Bass: 2 notes added"]);
        let mut same = song();
        same.tracks[0].regions[0] = midi(0, &[41]);
        assert_eq!(said(&was, &same), ["Bass: rewritten"]);
    }

    #[test]
    fn a_track_taken_out_says_so() {
        let was = song();
        let mut is = song();
        is.tracks.clear();
        assert_eq!(said(&was, &is), ["Took out Bass"]);
    }
}
