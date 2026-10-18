//! The side panel, left of the tracks. Inspector: the selected region's settings, Smart
//! Controls for its track, and the track's and the master's channel strips. Browser: instruments
//! and effects to drop on the selected track. Agent: how to connect one, and what it did.

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, SharedString, div, prelude::*, px, rgb,
};
use tempo_engine::{
    music::NoteEdit,
    player::Which,
    plugin::PluginParam,
    project::{Content, Flex, Instrument, PPQ, Project},
};

use crate::{
    menu::{Menu, MenuAction},
    strip::{self, Channel, Layout},
    theme::*,
    workspace::{Bottom, CompField, Drag, Name, Side, Target, Workspace, pt},
};

/// "17 1 1 1": a position as bars, beats, sixteenths and ticks, from 1.
pub fn position(p: &Project, tick: u64) -> String {
    let [bar, beat, div, tick] = p.bbt(tick);
    format!("{} {} {} {}", bar + 1, beat + 1, div + 1, tick + 1)
}

/// "4 0 0 0": a length as bars, beats, sixteenths and ticks.
pub fn length(p: &Project, ticks: u64) -> String {
    let [bars, beats, divs, ticks] = p.bbt(ticks);
    format!("{bars} {beats} {divs} {ticks}")
}

/// A row of the inspector: a label, and what it's set to.
fn row(label: &'static str, value: impl IntoElement) -> Div {
    div()
        .h(px(26.))
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_color(rgb(TEXT_3)).child(label))
        .child(value)
}

fn mono(s: impl Into<SharedString>) -> Div {
    div()
        .font_family(MONO)
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_2))
        .child(s.into())
}

/// The parameters of an instrument plugin worth having at hand: its filter's cutoff and
/// resonance, and its amp's attack and release, as Logic's Smart Controls map them; else the
/// first ones it has.
// ponytail: a name match; plugins with their own smart-control maps would do better.
pub fn smart_params(params: &[PluginParam]) -> Vec<(u32, String)> {
    let wants: [(&str, &[&str], &[&str]); 4] = [
        ("Cutoff", &["cutoff", "cut off"], &["filter", "vcf", "1"]),
        ("Resonance", &["resonance", "reso"], &["filter", "vcf", "1"]),
        ("Attack", &["attack"], &["amp", "vca", "env 1", "eg"]),
        ("Release", &["release"], &["amp", "vca", "env 1", "eg"]),
    ];
    let mut chosen: Vec<(u32, String)> = Vec::new();
    for (label, words, near) in wants {
        let best = params
            .iter()
            .enumerate()
            .filter(|(_, p)| !chosen.iter().any(|c| c.0 == p.id))
            .filter_map(|(i, p)| {
                let name = p.name.to_lowercase();
                words.iter().any(|w| name.contains(w)).then(|| {
                    let context = near.iter().filter(|w| name.contains(*w)).count();
                    (context as i64 * 1000 - i as i64, p)
                })
            })
            .max_by_key(|(score, _)| *score);
        if let Some((_, p)) = best {
            chosen.push((p.id, label.to_string()));
        }
    }
    for p in params {
        if chosen.len() >= 4 {
            break;
        }
        if !chosen.iter().any(|c| c.0 == p.id) {
            chosen.push((p.id, p.name.clone()));
        }
    }
    chosen
}

impl Workspace {
    pub fn inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |label: &'static str, side: Side, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 26.)
                .flex_1()
                .debug_selector(move || format!("side-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.side = side;
                        cx.notify();
                    }),
                )
        };
        let tabs = tray()
            .rounded(px(9.))
            .child(tab(
                "Inspector",
                Side::Inspector,
                self.side == Side::Inspector,
                cx,
            ))
            .child(tab(
                "Browser",
                Side::Browser,
                self.side == Side::Browser,
                cx,
            ));
        let body = match self.side {
            Side::Inspector => self.inspector_tab(cx).into_any_element(),
            Side::Browser => self.device_browser(cx).into_any_element(),
        };
        div()
            .w(px(self.side_w()))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_none()
                    .h(px(TOP_H))
                    .px(px(12.))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(tabs),
            )
            .child(
                div()
                    .id("inspector")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(body),
            )
            // Its right edge: drag it wider or narrower; double-click for its usual width.
            .relative()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(-3.))
                    .w(px(6.))
                    .h_full()
                    .cursor_col_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .when(
                        matches!(self.drag, Some(Drag::Width { side: true, .. })),
                        |d| d.bg(alpha(ACCENT, 0.6)),
                    )
                    .debug_selector(|| "side-edge".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                match ws.side {
                                    Side::Browser => ws.browser_w = INSPECTOR_W + 52.0,
                                    _ => ws.inspector_w = INSPECTOR_W,
                                }
                                cx.notify();
                                return;
                            }
                            let width = ws.side_w();
                            ws.begin_drag(
                                Drag::Width {
                                    side: true,
                                    x: f32::from(e.position.x),
                                    width,
                                },
                                cx,
                            );
                        }),
                    ),
            )
    }

    fn inspector_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let has_track = !self.session.project.tracks.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.region_section(cx))
            .when(has_track, |d| d.child(self.smart_controls(cx)))
            .child(self.channel_section(cx))
    }

    fn region_section(&mut self, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let Some(at) = self.region else {
            return div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(section_title("Region"))
                .child(
                    div()
                        .text_color(rgb(TEXT_3))
                        .line_height(px(17.))
                        .child("Select a region to name it, loop it, or change how it plays."),
                );
        };
        let track = &project.tracks[at.0];
        let region = &track.regions[at.1];
        let color = self.color(at.0);
        let kind = match &region.content {
            Content::Midi { .. } => "MIDI",
            Content::Pattern { .. } => "Pattern",
            Content::Audio { .. } => "Audio",
            Content::Takes { .. } => "Takes",
        };
        let aliases = region.link.map(|l| {
            project
                .tracks
                .iter()
                .flat_map(|t| &t.regions)
                .filter(|r| r.link == Some(l))
                .count()
        });
        let header = div()
            .h(px(22.))
            .flex()
            .items_center()
            .justify_between()
            .child(section_title("Region"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(div().size(px(8.)).rounded(px(2.)).bg(rgb(color)))
                    .when(aliases.is_some(), |d| {
                        d.child(icon(Icon::Link, 13.).text_color(rgb(TEXT_2)))
                    })
                    .child(format!("{} · {kind}", track.name)),
            );
        // Its name: click to type one.
        let name = match self.name_field(Name::Region(at)) {
            Some(field) => div().w(px(150.)).child(field),
            None => {
                let named = !region.name.is_empty();
                mono(if named {
                    region.name.clone()
                } else {
                    "Unnamed".into()
                })
                .when(!named, |d| d.text_color(rgb(TEXT_4)))
                .max_w(px(150.))
                .overflow_hidden()
                .whitespace_nowrap()
                .cursor_text()
                .debug_selector(|| "region-name".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        ws.start_naming(Name::Region(at), window, cx)
                    }),
                )
            }
        };
        let looped = region.looped;
        let mut d = div()
            .flex()
            .flex_col()
            .child(header)
            .child(row("Name", name))
            .child(row("Position", mono(position(project, region.start))))
            .child(row("Length", mono(length(project, region.length))))
            .child(row(
                "Loop",
                switch(looped)
                    .debug_selector(|| "region-loop".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit(cx, |ws| {
                                ws.session.project.tracks[at.0].regions[at.1].looped ^= true
                            })
                        }),
                    ),
            ));
        // What MIDI plays, its notes untouched: as Logic's region parameters.
        if region.content.is_midi() && !matches!(region.content, Content::Takes { .. }) {
            let q = region.quantize;
            let signed = |v: i8| match v {
                0 => "0".to_string(),
                v if v > 0 => format!("+{v}"),
                v => format!("−{}", -(v as i16)),
            };
            d = d
                .child(row(
                    "Quantize",
                    dropdown(q.map_or("Off".into(), grid_name), 22.)
                        .debug_selector(|| "region-quantize".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                let items = [None, Some(PPQ), Some(PPQ / 2), Some(PPQ / 4)]
                                    .into_iter()
                                    .chain([Some(PPQ / 8)])
                                    .map(|g| {
                                        let label = g.map_or("Off".into(), grid_name);
                                        (label, MenuAction::RegionQuantize(at, g))
                                    })
                                    .collect();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items,
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                ))
                .child(row(
                    "Transpose",
                    stepper(signed(region.transpose), at, Field::Transpose, cx),
                ))
                .child(row(
                    "Velocity",
                    stepper(signed(region.velocity), at, Field::Velocity, cx),
                ))
                .when(!region.plain(), |d| {
                    d.child(row(
                        "",
                        button("Apply to notes", false)
                            .debug_selector(|| "region-apply".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.apply_region_settings(at, cx)),
                            ),
                    ))
                })
                .child(row(
                    "Alias",
                    match aliases {
                        Some(n) => button(format!("Make unique · {n} linked"), false)
                            .debug_selector(|| "region-unlink".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.choose(MenuAction::Unlink(at), cx)
                                }),
                            ),
                        None => button("Make alias", false)
                            .debug_selector(|| "region-alias".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.choose(MenuAction::Alias(at), cx)
                                }),
                            ),
                    },
                ));
        }
        match &region.content {
            Content::Midi { notes, .. } => {
                let range = match (
                    notes.iter().map(|n| n.pitch).min(),
                    notes.iter().map(|n| n.pitch).max(),
                ) {
                    (Some(lo), Some(hi)) => {
                        format!("{}–{}", self.note_name(lo), self.note_name(hi))
                    }
                    _ => "—".into(),
                };
                d = d
                    .child(row("Range", mono(range)))
                    .child(row("Notes", mono(format!("{}", notes.len()))));
            }
            Content::Pattern { step, rows, .. } => {
                let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
                d = d
                    .child(row(
                        "Steps",
                        mono(format!("{steps} × {}", grid_name(*step))),
                    ))
                    .child(row("Sounds", mono(format!("{}", rows.len()))));
            }
            Content::Audio {
                file,
                stretch,
                flex,
                follow,
                formants,
                warps,
                ..
            } => {
                let (formants, warped) = (*formants, warps.len());
                let name = file
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                let (follow, stretch) = (*follow, *stretch);
                let signed = |v: i8| match v {
                    0 => "0".to_string(),
                    v if v > 0 => format!("+{v}"),
                    v => format!("−{}", -(v as i16)),
                };
                d = d
                    .child(row(
                        "File",
                        mono(name)
                            .max_w(px(140.))
                            .overflow_hidden()
                            .whitespace_nowrap(),
                    ))
                    // How it's stretched, as Logic's Flex mode or Ableton's warp mode.
                    .child(row(
                        "Flex",
                        dropdown(flex.name(), 22.)
                            .debug_selector(|| "region-flex".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    let items = Flex::ALL
                                        .into_iter()
                                        .map(|f| {
                                            (f.name().to_string(), MenuAction::RegionFlex(at, f))
                                        })
                                        .collect();
                                    ws.menu = Some(Menu {
                                        at: e.position,
                                        items,
                                        ..Default::default()
                                    });
                                    cx.notify();
                                }),
                            ),
                    ))
                    .child(row("Stretch", mono(format!("×{stretch:.3}"))))
                    .child(row(
                        "Follow tempo",
                        switch(follow)
                            .debug_selector(|| "region-follow".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit(cx, |ws| {
                                        let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                                        if let Content::Audio { follow, .. } = &mut r.content {
                                            *follow ^= true;
                                        }
                                    })
                                }),
                            ),
                    ))
                    .child(row(
                        "Transpose",
                        stepper(signed(region.transpose), at, Field::Transpose, cx),
                    ))
                    // A voice moved in pitch sounds like itself, not a chipmunk.
                    .child(row(
                        "Keep formants",
                        switch(formants)
                            .debug_selector(|| "region-formants".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit(cx, |ws| {
                                        let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                                        if let Content::Audio { formants, .. } = &mut r.content {
                                            *formants ^= true;
                                        }
                                    })
                                }),
                            ),
                    ))
                    .when(warped > 0, |d| {
                        d.child(row(
                            "Warp markers",
                            button(format!("Clear {warped}"), false)
                                .debug_selector(|| "region-unwarp".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.edit(cx, |ws| {
                                            let r =
                                                &mut ws.session.project.tracks[at.0].regions[at.1];
                                            if let Content::Audio { warps, .. } = &mut r.content {
                                                warps.clear();
                                            }
                                        })
                                    }),
                                ),
                        ))
                    });
            }
            Content::Takes { takes, .. } => {
                d = d
                    .child(row("Takes", mono(format!("{}", takes.len()))))
                    .child(row(
                        "",
                        button("Open takes", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.show_bottom(Bottom::Takes, cx)),
                        ),
                    ));
            }
        }
        d
    }

    fn smart_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.track;
        let track = &self.session.project.tracks[t];
        let id = track.id;
        let ch = Channel::Track(t);
        let color = self.color(t);
        // An instrument plugin's own parameters, else the channel's EQ and compressor.
        let (source, knobs): (String, Vec<(Target, String, f32, String)>) = match &track.instrument
        {
            Some(Instrument::Plugin(r)) => {
                let key = format!("{}#{}", r.path.display(), r.class);
                let stale = self
                    .smart
                    .get(&id)
                    .is_none_or(|(k, list)| *k != key || list.is_empty());
                if stale {
                    let picked = smart_params(&self.player.params(id, Which::Instrument));
                    self.smart.insert(id, (key, picked));
                }
                let list = self.smart[&id].1.clone();
                let knobs = list
                    .into_iter()
                    .map(|(param, label)| {
                        let target = Target::Param { track: id, param };
                        let v = self.target(target);
                        let text = self
                            .player
                            .param_text(id, Which::Instrument, param, v as f64);
                        (target, label, v, text)
                    })
                    .collect();
                (r.class.clone(), knobs)
            }
            _ => {
                let (eq, comp) = self.strip_dsp(ch);
                let eq = eq.unwrap_or_default();
                let knob = |target: Target, label: &str, text: String| {
                    (target, label.to_string(), self.target(target), text)
                };
                let knobs = vec![
                    knob(
                        Target::LowCut(ch),
                        "Low Cut",
                        if eq.low_cut <= 20.0 {
                            "Off".into()
                        } else {
                            strip::hz(eq.low_cut)
                        },
                    ),
                    knob(
                        Target::Gain(ch, 2),
                        "Presence",
                        format!("{} dB", db_text(eq.high_mid.gain_db)),
                    ),
                    knob(
                        Target::Gain(ch, 3),
                        "Air",
                        format!("{} dB", db_text(eq.high_shelf.gain_db)),
                    ),
                    knob(
                        Target::Comp(ch, CompField::Threshold),
                        "Threshold",
                        comp.map_or("Off".into(), |c| {
                            format!("{:.0} dB", c.threshold_db).replace('-', "−")
                        }),
                    ),
                ];
                ("Channel Strip".into(), knobs)
            }
        };
        let tiles = knobs
            .into_iter()
            .enumerate()
            .map(|(i, (target, label, v, text))| {
                div()
                    .h(px(48.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(6.))
                    .pr(px(8.))
                    .rounded(px(8.))
                    .bg(rgb(CARD))
                    .cursor_ns_resize()
                    .hover(|s| s.bg(rgb(OFF)))
                    .debug_selector(move || format!("smart-{i}"))
                    .child(knob(v, 34., color, false))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .min_w(px(0.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(label),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(text),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            let value = ws.target(target);
                            let y = pt(e.position).y;
                            ws.begin_drag(Drag::Knob { target, y, value }, cx);
                        }),
                    )
            });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(section_title("Smart Controls"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .max_w(px(120.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(source),
                    ),
            )
            .child(div().grid().grid_cols(2).gap(px(6.)).children(tiles))
    }

    fn channel_section(&mut self, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let has_track = !project.tracks.is_empty();
        let output = project.tracks.get(self.track).map(|t| {
            let to = t
                .output
                .and_then(|b| project.auxes.iter().find(|a| a.bus == b))
                .map_or("Stereo Out".to_string(), |a| a.name.clone());
            format!("→ {to}")
        });
        let t = self.track;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(section_title("Channel"))
                    .when_some(output, |d, label| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .px(px(6.))
                                .h(px(22.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(OFF)))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_2))
                                .child(label)
                                .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
                                .debug_selector(move || format!("output-{t}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        ws.menu = Some(ws.output_menu(t, e.position));
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .child({
                // Side by side, row for row.
                let (mut effects, mut sends) = strip::rows(self, Channel::Master);
                if has_track {
                    let (e, s) = strip::rows(self, Channel::Track(self.track));
                    (effects, sends) = (effects.max(e), sends.max(s));
                }
                let layout = Layout::Compact { effects, sends };
                div()
                    .flex()
                    .gap(px(8.))
                    .when(has_track, |d| {
                        d.child(strip::strip(self, Channel::Track(self.track), layout, cx))
                    })
                    .child(strip::strip(self, Channel::Master, layout, cx))
            })
    }
}

/// "1/16": a grid in ticks, as a note value.
pub fn grid_name(ticks: u64) -> String {
    match ticks {
        t if t == PPQ * 4 => "1 bar".into(),
        t if t == PPQ => "1/4 note".into(),
        t if t == PPQ / 2 => "1/8 note".into(),
        t if t == PPQ / 4 => "1/16 note".into(),
        t if t == PPQ / 8 => "1/32 note".into(),
        t => format!("{t} ticks"),
    }
}

/// A region playback setting a stepper changes.
#[derive(Clone, Copy)]
enum Field {
    Transpose,
    Velocity,
}

/// − value +: steps a region's transpose (a semitone) or velocity (one) down or up; ⌥ steps an
/// octave, or ten.
fn stepper(
    value: String,
    at: tempo_engine::project::At,
    field: Field,
    cx: &mut Context<Workspace>,
) -> Div {
    let side = |i: Icon, sign: i8, cx: &mut Context<Workspace>| {
        let name = match field {
            Field::Transpose => "transpose",
            Field::Velocity => "velocity",
        };
        div()
            .debug_selector(move || format!("{name}{sign:+}"))
            .w(px(22.))
            .h(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .text_color(rgb(TEXT_3))
            .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
            .child(icon(i, 12.).text_color(rgb(TEXT_3)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let r = &ws.session.project.tracks[at.0].regions[at.1];
                    let (now, big, limit) = match field {
                        Field::Transpose => (r.transpose, 12, 96),
                        Field::Velocity => (r.velocity, 10, 99),
                    };
                    let by = if e.modifiers.alt { big } else { 1 } * sign;
                    let to = (now as i16 + by as i16).clamp(-limit, limit) as i8;
                    ws.edit(cx, |ws| {
                        let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                        match field {
                            Field::Transpose => r.transpose = to,
                            Field::Velocity => r.velocity = to,
                        }
                    });
                }),
            )
    };
    div()
        .h(px(22.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(RAISED))
        .bg(rgb(CARD))
        .child(side(Icon::Minus, -1, cx))
        .child(
            div()
                .min_w(px(52.))
                .flex()
                .justify_center()
                .font_family(MONO)
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .child(value),
        )
        .child(side(Icon::Plus, 1, cx))
}

impl Workspace {
    /// Writes a region's playback settings into its notes, as Logic's "Apply Quantization
    /// Settings Destructively" and "Normalize Region Parameters" do, and clears them.
    pub fn apply_region_settings(&mut self, at: tempo_engine::project::At, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
            let played = r.clone();
            match &mut r.content {
                Content::Midi { notes, .. } => {
                    for n in notes.iter_mut() {
                        *n = played.played(n);
                    }
                }
                // A pattern is on its grid already: it takes the transpose and velocity.
                Content::Pattern { rows, .. } => {
                    for row in rows.iter_mut() {
                        row.pitch =
                            (row.pitch as i16 + played.transpose as i16).clamp(0, 127) as u8;
                        for c in row.cells.iter_mut().filter(|c| **c > 0) {
                            *c = (*c as i16 + played.velocity as i16).clamp(1, 127) as u8;
                        }
                    }
                }
                _ => return,
            }
            (r.transpose, r.velocity, r.quantize) = (0, 0, None);
        });
    }

    /// Edits a MIDI region's notes in one go: the selected note alone where that makes sense.
    pub fn edit_notes(
        &mut self,
        at: tempo_engine::project::At,
        edit: NoteEdit,
        cx: &mut Context<Self>,
    ) {
        // The notes picked, else all of them.
        let only = self.picked.clone();
        self.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut(at) {
                edit.apply(notes, &only);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: u32, name: &str) -> PluginParam {
        PluginParam {
            id,
            name: name.into(),
            value: 0.5,
            ..Default::default()
        }
    }

    #[test]
    fn smart_controls_find_the_filter_and_the_amp() {
        let params = [
            p(1, "A Osc 1 Pitch"),
            p(2, "A Filter 1 Cutoff"),
            p(3, "A Filter 2 Cutoff"),
            p(4, "A Filter 1 Resonance"),
            p(5, "A Filter EG Attack"),
            p(6, "A Amp EG Attack"),
            p(7, "A Amp EG Release"),
        ];
        let picked: Vec<u32> = smart_params(&params).iter().map(|c| c.0).collect();
        assert_eq!(picked, [2, 4, 6, 7]);
        // Without names to go on, the first four.
        let plain: Vec<u32> = smart_params(&[p(9, "Gain"), p(8, "Mix")])
            .iter()
            .map(|c| c.0)
            .collect();
        assert_eq!(plain, [9, 8]);
        let mut p = Project::new(48_000);
        assert_eq!(position(&p, PPQ * 4 * 16 + PPQ), "17 2 1 1");
        assert_eq!(length(&p, PPQ * 4 * 4), "4 0 0 0");
        p.time_signature = [6, 8];
        assert_eq!(position(&p, PPQ * 3 + PPQ / 2), "2 2 1 1");
    }
}
