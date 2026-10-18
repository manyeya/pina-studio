//! tempo's look: colors, type, sizes, icons and the small controls every panel uses. Dark,
//! quiet surfaces in a few close greys; color is kept for what's playing (tracks, meters) and for
//! what's on (the accent).

use std::borrow::Cow;

use gpui::{
    Div, FontWeight, Hsla, PathBuilder, Pixels, Point, Svg, canvas, div, point, prelude::*, px,
    rgb, svg,
};

// Surfaces, from the window back to the controls on it.
pub const BG: u32 = 0x0c0c0f;
pub const SURFACE: u32 = 0x131317;
pub const CARD: u32 = 0x1a1a1f;
pub const STRIP: u32 = 0x18181d;
pub const WELL: u32 = 0x0f0f13;
pub const FIELD: u32 = 0x24242b;
pub const RAISED: u32 = 0x2b2b33;
pub const OFF: u32 = 0x1e1e25;
pub const LANE: u32 = 0x0e0e12;
pub const LANE_ALT: u32 = 0x121217;
pub const HEADERS: u32 = 0x101014;
pub const HEADER_SELECTED: u32 = 0x1b1b21;
pub const LINE: u32 = 0x26262d;
pub const LINE_SOFT: u32 = 0x1c1c22;
pub const LINE_STRONG: u32 = 0x34343d;

// Text, from loudest to quietest. The quietest is for decoration only.
pub const TEXT: u32 = 0xf4f4f6;
pub const TEXT_2: u32 = 0xd4d4da;
pub const TEXT_3: u32 = 0x8a8a95;
pub const TEXT_4: u32 = 0x55555f;

pub const ACCENT: u32 = 0x5b8cff;
pub const ACCENT_TEXT: u32 = 0xa5c0ff;
pub const RECORD: u32 = 0xff4d4d;
pub const SOLO: u32 = 0xffc53d;
pub const METER_LOW: u32 = 0x3ddc97;
pub const METER_MID: u32 = 0xffc53d;
pub const METER_HIGH: u32 = 0xff5c5c;
/// The agent's color: its cursor, its outline on what it's editing, its avatar.
pub const AGENT: u32 = 0xc6f432;
pub const AGENT_TEXT: u32 = 0xd9f99d;
pub const MASTER: u32 = 0xf4f4f6;
pub const BUS: u32 = 0xc4c4cc;

// Track colors, picked by what a track is called (as Logic suggests one), else in turn.
pub const CORAL: u32 = 0xff7a59;
pub const AMBER: u32 = 0xffc53d;
pub const MINT: u32 = 0x3ddc97;
pub const SKY: u32 = 0x38bdf8;
pub const VIOLET: u32 = 0xa78bfa;
pub const PINK: u32 = 0xf472b6;
pub const SLATE: u32 = 0x94a3b8;
pub const TEAL: u32 = 0x2dd4bf;

pub const CONTROL_BAR_H: f32 = 52.0;
pub const STATUS_H: f32 = 24.0;
pub const INSPECTOR_W: f32 = 248.0;
pub const HEADER_W: f32 = 344.0;
pub const MARKERS_H: f32 = 22.0;
pub const RULER_H: f32 = 26.0;
/// The top of each column beside the ruler (the headers' tools, a side panel's tabs or title),
/// so their lines meet the ruler's.
pub const TOP_H: f32 = MARKERS_H + RULER_H;
/// A track's lane as it opens, and how short and tall one can be dragged.
pub const LANE_H: f32 = 76.0;
pub const MIN_LANE_H: f32 = 40.0;
pub const MAX_LANE_H: f32 = 240.0;
/// An automation lane, under its track.
pub const AUTO_H: f32 = 44.0;
pub const EDITOR_H: f32 = 296.0;
/// The shortest the editor can be dragged.
pub const EDITOR_MIN_H: f32 = 160.0;
pub const EDITOR_BAR_H: f32 = 40.0;
pub const PANEL_W: f32 = 360.0;

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

/// Geist and Geist Mono (SIL Open Font License, `assets/fonts/OFL.txt`), for the text system.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Geist-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf")),
        // Agents' logos, modisa's (see ModisaMarks.NOTICE).
        Cow::Borrowed(include_bytes!("../assets/fonts/ModisaMarks.ttf")),
    ]
}

/// `c` lifted toward white by `t` (0 to 1): text on a tint of the same color.
pub fn tint(c: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let v = ((c >> shift) & 0xff) as f32;
        ((v + (255.0 - v) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

pub fn alpha(c: u32, a: f32) -> Hsla {
    Hsla::from(rgb(c)).opacity(a)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    Play,
    Stop,
    ToStart,
    Record,
    Cycle,
    Metronome,
    PanelLeft,
    PanelBottom,
    Mixer,
    Grid,
    Chevron,
    Plus,
    Minus,
    Close,
    Pointer,
    Pencil,
    Scissors,
    Eraser,
    Magnet,
    Agent,
    Keys,
    Wave,
    Bus,
    Output,
    More,
    Scale,
    Ghost,
    Export,
    Plug,
    Search,
    Link,
    Loop,
    Samples,
    Lock,
    Unlock,
    Folder,
    Rescan,
    Back,
    Forward,
    Shuffle,
    Expand,
    PanelRight,
    Send,
    Check,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Icon::Play => {
                r##"<path fill="#000" d="M8 5.6v12.8a1 1 0 0 0 1.53.85l10.2-6.4a1 1 0 0 0 0-1.7L9.53 4.75A1 1 0 0 0 8 5.6z"/>"##
            }
            Icon::Stop => r##"<rect x="6.5" y="6.5" width="11" height="11" rx="2"/>"##,
            Icon::ToStart => r##"<path d="M6 5v14M18 6.5l-8.5 5.5 8.5 5.5z"/>"##,
            Icon::Record => r##"<circle fill="#000" stroke="none" cx="12" cy="12" r="5.5"/>"##,
            Icon::Cycle => {
                r##"<path d="M17 3l3 3-3 3M4 11V9.5A3.5 3.5 0 0 1 7.5 6H20M7 21l-3-3 3-3M20 13v1.5a3.5 3.5 0 0 1-3.5 3.5H4"/>"##
            }
            Icon::Metronome => r##"<path d="M10 3.5h4l4.5 17h-13zM12 16l4.5-8.5M7.5 16h9"/>"##,
            Icon::PanelLeft => {
                r##"<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M9.5 4.5v15"/>"##
            }
            Icon::PanelBottom => {
                r##"<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M3.5 13.5h17"/>"##
            }
            Icon::Mixer => {
                r##"<path d="M6 4v5M6 13v7M12 4v11M12 19v1M18 4v2M18 10v10"/><circle cx="6" cy="11" r="2"/><circle cx="12" cy="17" r="2"/><circle cx="18" cy="8" r="2"/>"##
            }
            Icon::Grid => {
                r##"<rect x="4" y="4" width="7" height="7" rx="1.5"/><rect x="13" y="4" width="7" height="7" rx="1.5"/><rect x="4" y="13" width="7" height="7" rx="1.5"/><rect x="13" y="13" width="7" height="7" rx="1.5"/>"##
            }
            Icon::Chevron => r##"<path d="M6 9l6 6 6-6"/>"##,
            Icon::Plus => r##"<path d="M6 12h12M12 6v12"/>"##,
            Icon::Minus => r##"<path d="M6 12h12"/>"##,
            Icon::Close => r##"<path d="M7 7l10 10M17 7L7 17"/>"##,
            Icon::Pointer => r##"<path d="M5 3.5l13 7.2-5.6 1.6-2.4 5.3z"/>"##,
            Icon::Pencil => r##"<path d="M15.5 4.5l4 4L8 20H4v-4z"/>"##,
            Icon::Scissors => {
                r##"<circle cx="6.5" cy="17.5" r="2.5"/><circle cx="17.5" cy="17.5" r="2.5"/><path d="M8.3 15.7L18 4M15.7 15.7L6 4"/>"##
            }
            Icon::Eraser => {
                r##"<path d="M9 20h11M4.6 14.6l9.5-9.5a2 2 0 0 1 2.8 0l2 2a2 2 0 0 1 0 2.8L11 18.8a2 2 0 0 1-1.4.6H8.4a2 2 0 0 1-1.4-.6l-2.4-2.4a1.2 1.2 0 0 1 0-1.8z"/>"##
            }
            Icon::Magnet => r##"<path d="M6 4v7a6 6 0 0 0 12 0V4M6 8h4M14 8h4"/>"##,
            Icon::Agent => r##"<path stroke-width="2.25" d="M5 7l5 5-5 5M12.5 17.5H19"/>"##,
            Icon::Keys => r##"<path d="M4 5h16v14H4zM8.5 5v8M15.5 5v8M12 13v6"/>"##,
            Icon::Wave => r##"<path d="M3 12h2M7 8v8M10 5v14M13 9v6M16 7v10M19 11v2"/>"##,
            Icon::Bus => r##"<path d="M4 7h16M4 12h16M4 17h16M8 4v6M16 9v6M11 14v6"/>"##,
            Icon::Output => r##"<path d="M4 12h11M11 7l5 5-5 5M19 5v14"/>"##,
            Icon::More => {
                r##"<circle fill="#000" stroke="none" cx="6" cy="12" r="1.6"/><circle fill="#000" stroke="none" cx="12" cy="12" r="1.6"/><circle fill="#000" stroke="none" cx="18" cy="12" r="1.6"/>"##
            }
            Icon::Scale => r##"<path d="M4 7h16M4 12h16M4 17h10"/>"##,
            Icon::Ghost => {
                r##"<rect stroke-dasharray="3 2.5" x="3.5" y="7" width="17" height="10" rx="2.5"/>"##
            }
            Icon::Export => r##"<path d="M12 4v11M7.5 10.5L12 15l4.5-4.5M5 19h14"/>"##,
            Icon::Plug => {
                r##"<path d="M9 3.5v4M15 3.5v4M6.5 7.5h11v3a5.5 5.5 0 0 1-11 0zM12 16v4.5"/>"##
            }
            Icon::Search => r##"<circle cx="10.5" cy="10.5" r="6"/><path d="M15 15l5 5"/>"##,
            Icon::Link => {
                r##"<path d="M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1"/>"##
            }
            Icon::Loop => {
                r##"<path d="M4 12a5 5 0 0 1 5-5h9M15 4l3 3-3 3M20 12a5 5 0 0 1-5 5H6M9 20l-3-3 3-3"/>"##
            }
            Icon::Samples => {
                r##"<path d="M3.5 12h1.5M7.5 8.5v7M11 5.5v13"/><circle cx="16.5" cy="11" r="3.5"/><path d="M19 13.5l2.5 2.5"/>"##
            }
            Icon::Lock => {
                r##"<rect x="5" y="10.5" width="14" height="9.5" rx="2"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5"/>"##
            }
            Icon::Unlock => {
                r##"<rect x="5" y="10.5" width="14" height="9.5" rx="2"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 6.8-1.2"/>"##
            }
            Icon::Folder => {
                r##"<path d="M3.5 7.5a2 2 0 0 1 2-2h3.8l2 2.2h7.2a2 2 0 0 1 2 2v7.8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>"##
            }
            Icon::Rescan => r##"<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4.5v4h-4"/>"##,
            Icon::Back => r##"<path d="M14.5 6l-6 6 6 6"/>"##,
            Icon::Forward => r##"<path d="M9.5 6l6 6-6 6"/>"##,
            Icon::Send => r##"<path stroke-width="2.25" d="M12 19V5.5M6.5 11L12 5.5 17.5 11"/>"##,
            Icon::Check => r##"<path stroke-width="2.25" d="M5 12.5l4.5 4.5L19 7.5"/>"##,
            Icon::Expand => {
                r##"<path d="M14 4.5h5.5V10M10 19.5H4.5V14M19.5 4.5l-6 6M4.5 19.5l6-6"/>"##
            }
            Icon::PanelRight => {
                r##"<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M14.5 4.5v15"/>"##
            }
            Icon::Shuffle => {
                r##"<path d="M4 7h3.5c4.5 0 4.5 10 9 10H20M17.5 14.5L20 17l-2.5 2.5M4 17h3.5c1.6 0 2.6-1.3 3.4-3M13.1 10c.8-1.7 1.8-3 3.4-3H20M17.5 4.5L20 7l-2.5 2.5"/>"##
            }
        }
    }

    fn svg(self) -> &'static [u8] {
        // Built once per icon and kept: the SVG renderer caches by the bytes' hash.
        use std::sync::OnceLock;
        static CACHE: [OnceLock<Vec<u8>>; 64] = [const { OnceLock::new() }; 64];
        CACHE[self as usize].get_or_init(|| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
                self.path()
            )
            .into_bytes()
        })
    }
}

/// An icon, `size` pixels square, in the text color (set it with `text_color`).
pub fn icon(i: Icon, size: f32) -> Svg {
    svg().data(i.svg()).size(px(size)).flex_none()
}

/// A tray of segmented buttons, like the transport and the tool picker.
pub fn tray() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(3.))
        .bg(rgb(BG))
        .border_1()
        .border_color(rgb(LINE))
        .rounded(px(10.))
}

/// A text button in a tray: raised when on.
pub fn seg(label: impl Into<gpui::SharedString>, on: bool, h: f32) -> Div {
    div()
        .h(px(h))
        .px(px(if h >= 28.0 { 12. } else { 10. }))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(if h >= 28.0 { 7. } else { 6. }))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .bg(rgb(if on { RAISED } else { BG }))
        .text_color(rgb(if on { TEXT } else { TEXT_3 }))
        .when(!on, |d| d.hover(|s| s.bg(rgb(OFF)).text_color(rgb(TEXT_2))))
        .child(label.into())
}

/// An icon button in a tray, `h` square: raised when on.
pub fn icon_seg(i: Icon, on: bool, h: f32) -> Div {
    div()
        .size(px(h))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(if h >= 28.0 { 7. } else { 6. }))
        .cursor_pointer()
        .bg(rgb(if on { RAISED } else { BG }))
        .text_color(rgb(if on { TEXT } else { TEXT_3 }))
        .when(!on, |d| d.hover(|s| s.bg(rgb(OFF)).text_color(rgb(TEXT_2))))
        .child(
            icon(i, if h >= 28.0 { 16. } else { 14. }).text_color(rgb(if on {
                TEXT
            } else {
                TEXT_3
            })),
        )
}

/// A button with a border and a chevron, that opens a menu.
pub fn dropdown(label: impl Into<gpui::SharedString>, h: f32) -> Div {
    div()
        .h(px(h))
        .pl(px(8.))
        .pr(px(5.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(RAISED))
        .bg(rgb(CARD))
        .text_color(rgb(TEXT))
        .text_size(px(12.))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.border_color(rgb(LINE_STRONG)))
        .child(
            div()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .child(label.into()),
        )
        .child(icon(Icon::Chevron, 12.).text_color(rgb(TEXT_3)))
}

/// A plain bordered button.
pub fn button(label: impl Into<gpui::SharedString>, on: bool) -> Div {
    div()
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(7.))
        .border_1()
        .border_color(rgb(if on { RAISED } else { LINE }))
        .bg(rgb(if on { RAISED } else { CARD }))
        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        .child(label.into())
}

/// A small square toggle, like M, S and R on a track: lit in its color when on.
pub fn toggle(label: &'static str, on: bool, on_bg: u32, on_text: u32, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .bg(rgb(if on { on_bg } else { OFF }))
        .text_color(rgb(if on { on_text } else { TEXT_3 }))
        .when(!on, |d| {
            d.hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
        })
        .child(label)
}

/// An on/off switch.
pub fn switch(on: bool) -> Div {
    div()
        .relative()
        .w(px(28.))
        .h(px(16.))
        .flex_none()
        .rounded(px(8.))
        .cursor_pointer()
        .bg(rgb(if on { ACCENT } else { RAISED }))
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 14. } else { 2. }))
                .size(px(12.))
                .rounded(px(6.))
                .bg(rgb(if on { 0xffffff } else { TEXT_3 })),
        )
}

/// A small label over a section: "Region", "Smart Controls".
pub fn section_title(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(s.into())
}

/// A chip of colored text on a tint of its color.
pub fn chip(s: impl Into<gpui::SharedString>, color: u32) -> Div {
    div()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(alpha(color, 0.14))
        .text_color(rgb(color))
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .child(s.into())
}

/// A knob: an arc around a disc, the arc lit up to the value. `value` is 0 to 1, or -1 to 1
/// from the top when `bipolar`.
pub fn knob(value: f32, size: f32, color: u32, bipolar: bool) -> impl IntoElement {
    let thick = if size >= 30.0 { 3.0 } else { 2.5 };
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = f32::from(b.size.width.min(b.size.height)) / 2.0 - thick / 2.0 - 0.5;
            let at = |deg: f32, rr: f32| -> Point<Pixels> {
                let a = deg.to_radians();
                point(c.x + px(rr * a.cos()), c.y + px(rr * a.sin()))
            };
            let arc = |from: f32, to: f32| {
                let mut p = PathBuilder::stroke(px(thick));
                p.move_to(at(from, r));
                // Arcs over 180° in two halves, so the flags never have to choose.
                let mid = (from + to) / 2.0;
                p.arc_to(point(px(r), px(r)), px(0.), false, to > from, at(mid, r));
                p.arc_to(point(px(r), px(r)), px(0.), false, to > from, at(to, r));
                p.build().ok()
            };
            if let Some(track) = arc(135.0, 405.0) {
                window.paint_path(track, rgb(RAISED));
            }
            let (from, to) = if bipolar {
                (270.0, 270.0 + value.clamp(-1.0, 1.0) * 135.0)
            } else {
                (135.0, 135.0 + value.clamp(0.0, 1.0) * 270.0)
            };
            if (to - from).abs() > 0.5
                && let Some(lit) = arc(from.min(to), from.max(to))
            {
                window.paint_path(lit, rgb(color));
            }
            let body = r * 0.68;
            let mut disc = PathBuilder::fill();
            disc.move_to(point(c.x + px(body), c.y));
            disc.arc_to(
                point(px(body), px(body)),
                px(0.),
                false,
                false,
                point(c.x - px(body), c.y),
            );
            disc.arc_to(
                point(px(body), px(body)),
                px(0.),
                false,
                false,
                point(c.x + px(body), c.y),
            );
            disc.close();
            if let Ok(p) = disc.build() {
                window.paint_path(p, rgb(RAISED));
            }
            let mut pointer = PathBuilder::stroke(px(if size >= 30.0 { 2.0 } else { 1.75 }));
            pointer.move_to(at(to, r * 0.18));
            pointer.line_to(at(to, r * 0.6));
            if let Ok(p) = pointer.build() {
                window.paint_path(p, rgb(TEXT));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// "−3.2", "+6.0", "−∞": a level as the strips show it, with a true minus.
pub fn db_text(db: f32) -> String {
    if db <= -100.0 {
        "−∞".to_string()
    } else if db.abs() < 0.05 {
        "0.0".to_string()
    } else if db < 0.0 {
        format!("−{:.1}", -db)
    } else {
        format!("+{db:.1}")
    }
}
