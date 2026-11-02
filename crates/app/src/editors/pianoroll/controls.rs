//! The roll's control bar.

use gpui::{App, Pixels, Window};

use super::*;

/// A note's length as a fraction of a bar of 4/4: "1/16", "3/8", "1/1".
pub(crate) fn length_name(ticks: u64) -> String {
    let whole = PPQ * 4;
    let gcd = |mut a: u64, mut b: u64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let g = gcd(ticks, whole).max(1);
    match whole / g {
        d if d <= 64 => format!("{}/{d}", ticks / g),
        _ => format!("{ticks} ticks"),
    }
}

/// A different seed each time, so humanizing twice isn't the same nudge twice.
pub(crate) fn fresh_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// A toggle with an icon, tinted with the accent while on.
pub(crate) fn chip_button(i: Icon, label: String, on: bool) -> Div {
    chip(on)
        .child(icon(i, 13.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 })))
        .child(label)
}

/// A toggle with words alone.
pub(crate) fn chip_text(label: &str, on: bool) -> Div {
    chip(on).child(label.to_string())
}

pub(crate) fn chip(on: bool) -> Div {
    div()
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .bg(if on {
            alpha(ACCENT, 0.16)
        } else {
            rgb(0x23232a).into()
        })
        .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
        .hover(|s| s.bg(rgb(RAISED)))
}

/// What an editor keeps out of its bar, in its More menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum More {
    /// A list of its own, opened where More's was.
    Chords(Point<Pixels>),
    Ghost(Point<Pixels>),
    Velocity,
    Fit,
    /// A sampler's, by its track.
    Mono(usize),
    Legato(usize),
}

/// The bar's way to what's used less; `on` while something in it is.
pub(crate) fn more_button(
    on: bool,
    open: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    chip(on)
        .flex_none()
        .debug_selector(|| "editor-more".into())
        .child("More")
        .child(icon(Icon::Chevron, 11.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_3 })))
        .on_mouse_down(MouseButton::Left, open)
}

impl Workspace {
    /// What's used less, in the bar's More: chord stamps, ghost notes, colour by velocity, fit.
    /// Chords and ghost notes are lists of their own, opened from here.
    pub(crate) fn roll_more(&self, at: Point<Pixels>) -> Vec<(String, MenuAction)> {
        let stamp = self.roll.stamp;
        let chords = match stamp {
            Stamp::Note => "Chords".to_string(),
            s => format!("Chords: {}", s.name()),
        };
        let shown = self.roll.ghost;
        let ghost = shown.and_then(|g| self.session.project.tracks.iter().find(|t| t.id == g));
        let ghost = ghost.map_or("Ghost notes".to_string(), |t| {
            format!("Ghost notes: {}", t.name)
        });
        let velocity = match self.roll.by_velocity {
            true => "✓ Colour by velocity",
            false => "Colour by velocity",
        };
        vec![
            (format!("{chords} ▸"), MenuAction::More(More::Chords(at))),
            (format!("{ghost} ▸"), MenuAction::More(More::Ghost(at))),
            (velocity.to_string(), MenuAction::More(More::Velocity)),
            ("Fit notes".to_string(), MenuAction::More(More::Fit)),
        ]
    }

    /// The chord stamps to write with.
    fn stamp_menu(&self, at: Point<Pixels>) -> Menu {
        let stamps = Stamp::ALL.iter();
        let item = |s: &Stamp| {
            let tick = if *s == self.roll.stamp { "✓ " } else { "" };
            (format!("{tick}{}", s.name()), MenuAction::Stamp(*s))
        };
        Menu {
            at,
            items: stamps.map(item).collect(),
            ..Default::default()
        }
    }

    /// The tracks whose notes can show behind these.
    fn ghost_menu(&self, at: Point<Pixels>) -> Menu {
        let now = self.roll.ghost;
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![(
            format!("{}Off", tick(now.is_none())),
            MenuAction::Ghost(None),
        )];
        let p = &self.session.project;
        let shown = self.roll_region().map(|at| p.tracks[at.0].id);
        items.extend(
            p.tracks
                .iter()
                .filter(|t| t.instrument.is_some() && Some(t.id) != shown)
                .map(|t| {
                    let label = format!("{}{}", tick(now == Some(t.id)), t.name);
                    (label, MenuAction::Ghost(Some(t.id)))
                }),
        );
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// One of the bar's More items.
    pub fn pick_more(&mut self, item: More, cx: &mut Context<Self>) {
        match item {
            More::Chords(at) => self.menu = Some(self.stamp_menu(at)),
            More::Ghost(at) => self.menu = Some(self.ghost_menu(at)),
            More::Velocity => self.roll.by_velocity ^= true,
            More::Fit => self.fit_roll(),
            More::Mono(t) => self.edit_sampler(t, cx, |s| s.voice.mono ^= true),
            More::Legato(t) => self.edit_sampler(t, cx, |s| {
                s.voice.legato ^= true;
                s.voice.mono |= s.voice.legato;
            }),
        }
        cx.notify();
    }

    /// The piano roll's own controls in the editor's bar: scale, writing in key, chord stamps,
    /// ghost notes, colour by velocity, quantize, fit and zoom.
    pub fn roll_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let key = self.session.project.song_key();
        let scale_label = key.map_or("Scale".to_string(), |k| k.short());
        let at = self.roll_region();
        let grid = self.roll.grid;
        // More lights while something in it is on, so nothing's on unseen.
        let extras =
            self.roll.stamp != Stamp::Note || self.roll.ghost.is_some() || self.roll.by_velocity;
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(
                chip_button(Icon::Scale, scale_label, self.roll.scale)
                    .debug_selector(|| "roll-scale".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.scale ^= true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip_text("In key", self.roll.in_key)
                    .debug_selector(|| "roll-in-key".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.in_key ^= true;
                            if ws.roll.in_key && ws.session.project.song_key().is_none() {
                                ws.status = Some(
                                    "The song has no key yet: set one in the control bar.".into(),
                                );
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                button(
                    format!(
                        "Q {}",
                        crate::workspace::inspector::grid_name(grid).trim_end_matches(" note")
                    ),
                    false,
                )
                .debug_selector(|| "roll-quantize".into())
                .when(at.is_none(), |d| d.opacity(0.4).cursor_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.roll_op(RollOp::Quantize, cx)),
                ),
            )
            .child(more_button(
                extras,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    ws.menu = Some(Menu {
                        at: e.position,
                        items: ws.roll_more(e.position),
                        ..Default::default()
                    });
                    cx.notify();
                }),
            ))
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)))
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(self.slider_bar(Slider::Zoom, cx).flex_1()),
            )
    }
}
