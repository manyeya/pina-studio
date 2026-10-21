//! Small pieces the browser draws with: waveforms, chips, headings, facts and their text.

use super::*;

/// One of the Similar list's aspects, by its place in `ASPECTS`.
pub fn aspect(a: &mut Aspects, i: usize) -> &mut f32 {
    match i {
        0 => &mut a.timbre,
        1 => &mut a.pitch,
        2 => &mut a.envelope,
        _ => &mut a.spectrum,
    }
}

/// A category's color: drums by what they are (kicks, snares, metals, percussion), the rest as
/// tracks are colored (bass amber, keys mint, leads sky, pads violet).
pub fn category_color(c: Category) -> u32 {
    use Category::*;
    match c {
        Kick | Drums => CORAL,
        Snare | Clap | Rim | Snap => PINK,
        HatClosed | HatOpen | Ride | Crash | Shaker | Tambourine => TEAL,
        Tom | Perc => ORANGE,
        Bass808 | Bass => AMBER,
        Keys | Chord | Pluck | Bell | Guitar => MINT,
        Lead | Synth | Brass | Woodwind => SKY,
        Pad | Strings => VIOLET,
        Vocal => FUCHSIA,
        Fx | Reverse | Texture | Foley => SLATE,
        Other => GREY,
    }
}

/// The map's legend: each color, and what it stands for.
pub(crate) const LEGEND: [(u32, &str); 10] = [
    (CORAL, "Kicks"),
    (PINK, "Snares"),
    (TEAL, "Hats"),
    (ORANGE, "Perc"),
    (AMBER, "Bass"),
    (MINT, "Keys"),
    (SKY, "Leads"),
    (VIOLET, "Pads"),
    (FUCHSIA, "Vocals"),
    (SLATE, "FX"),
];

pub(crate) fn seconds_text(s: f32) -> String {
    match s {
        s if s < 1.0 => format!("{s:.2} s"),
        s if s < 10.0 => format!("{s:.1} s"),
        s => format!("{s:.0} s"),
    }
}

pub(crate) fn note_text(p: &Pitch) -> String {
    match p.cents.round() as i32 {
        0 => note_name(p.note),
        c if c < 0 => format!("{} −{}¢", note_name(p.note), -c),
        c => format!("{} +{c}¢", note_name(p.note)),
    }
}

/// Its key, or its note.
pub(crate) fn tone_text(a: &Analysis) -> Option<String> {
    a.key
        .as_ref()
        .map(|k| k.name.clone())
        .or_else(|| a.pitch.as_ref().map(note_text))
}

/// "+3", "−5", "−0.2": semitones, with a true minus.
pub(crate) fn semis_text(s: f32) -> String {
    let s = (s * 10.0).round() / 10.0;
    let body = match s.fract() == 0.0 {
        true => format!("{}", s.abs() as i32),
        false => format!("{:.1}", s.abs()),
    };
    match s {
        s if s < 0.0 => format!("−{body}"),
        _ => format!("+{body}"),
    }
}

pub(crate) fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::OneShot => "One-shot",
        Kind::Loop => "Loop",
    }
}

/// A sample's outline, mirrored about its middle, filling `b`.
pub(crate) fn paint_wave(points: &[u8], b: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let (x0, w) = (f32::from(b.origin.x), f32::from(b.size.width));
    let h = f32::from(b.size.height);
    let mid = f32::from(b.origin.y) + h / 2.0;
    let at = |i: usize, side: f32| {
        let v = (points[i] as f32 / 255.0 * h / 2.0).max(0.5);
        point(px(x0 + i as f32 / (n - 1) as f32 * w), px(mid - side * v))
    };
    let mut path = PathBuilder::fill();
    path.move_to(at(0, 1.0));
    for i in 1..n {
        path.line_to(at(i, 1.0));
    }
    for i in (0..n).rev() {
        path.line_to(at(i, -1.0));
    }
    path.close();
    if let Ok(p) = path.build() {
        window.paint_path(p, color);
    }
}

/// A sample's outline as bars, the part already heard brighter.
pub(crate) fn paint_bars(
    points: &[u8],
    b: Bounds<Pixels>,
    color: u32,
    heard: Option<f32>,
    window: &mut Window,
) {
    let n = points.len().max(1);
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let pitch = w / n as f32;
    let bar = (pitch - 1.0).max(1.0);
    for (i, &v) in points.iter().enumerate() {
        let x = i as f32 * pitch;
        let bh = (v as f32 / 255.0 * h).max(1.5);
        let lit = heard.is_some_and(|t| x / w <= t);
        let c = match (heard.is_some(), lit) {
            (false, _) => alpha(color, 0.85),
            (true, true) => alpha(tint(color, 0.25), 1.0),
            (true, false) => alpha(color, 0.45),
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    point(b.origin.x + px(x), b.origin.y + px((h - bh) / 2.0)),
                    size(px(bar), px(bh)),
                ),
                c,
            )
            .corner_radii(px(1.)),
        );
    }
}

/// A row's waveform, in its category's color.
pub fn wave(points: Vec<u8>, color: u32, strength: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| paint_wave(&points, b, alpha(color, strength), window),
    )
    .size_full()
}

/// A filter chip: a color dot, its name, and how many it would find.
pub(crate) fn filter_chip(
    label: impl Into<SharedString>,
    count: Option<usize>,
    on: bool,
    dot: Option<u32>,
) -> Div {
    div()
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(6.))
        .border_1()
        .border_color(if on {
            alpha(ACCENT, 0.7)
        } else {
            rgb(LINE).into()
        })
        .bg(if on {
            alpha(ACCENT, 0.16)
        } else {
            rgb(CARD).into()
        })
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
        .whitespace_nowrap()
        .cursor_pointer()
        .when(!on, |d| {
            d.hover(|s| s.bg(rgb(OFF)).border_color(rgb(LINE_STRONG)))
        })
        .when_some(dot, |d, c| {
            d.child(div().size(px(6.)).flex_none().rounded(px(3.)).bg(rgb(c)))
        })
        .child(label.into())
        .when_some(count, |d, n| {
            d.child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_3 }))
                    .child(n.to_string()),
            )
        })
}

/// A label over a part of a column, with something on its right.
pub(crate) fn heading(label: &'static str) -> Div {
    div()
        .h(px(22.))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(label.to_uppercase())
}

/// A small text link, like "Clear".
pub(crate) fn link(label: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_3))
        .cursor_pointer()
        .hover(|s| s.text_color(rgb(TEXT)))
        .child(label)
}

/// The main action's button: filled with the accent.
pub(crate) fn primary(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(30.))
        .px(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(8.))
        .bg(rgb(ACCENT))
        .text_color(rgb(0xffffff))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(tint(ACCENT, 0.12))))
        .child(label.into())
}

/// A fact about a sample: what it is, and its value.
pub(crate) fn fact(label: &'static str, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .min_w(px(0.))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .child(label),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(value.into()),
        )
}
