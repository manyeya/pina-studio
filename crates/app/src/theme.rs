//! Colors, sizes and the small controls every panel uses. Dark, like Logic.

use gpui::{Div, FontWeight, Hsla, PathBuilder, canvas, div, point, prelude::*, px, rgb};

pub const BG: u32 = 0x1c1c1e;
pub const PANEL: u32 = 0x2a2a2d;
pub const PANEL_DARK: u32 = 0x222224;
pub const BORDER: u32 = 0x0e0e0f;
pub const HEADER: u32 = 0x2f2f33;
pub const HEADER_SELECTED: u32 = 0x404046;
pub const LANE: u32 = 0x252527;
pub const LANE_SELECTED: u32 = 0x2c2c30;
pub const GRID_BAR: u32 = 0x3b3b40;
pub const GRID_BEAT: u32 = 0x2e2e32;
pub const BUTTON: u32 = 0x45454b;
pub const TEXT: u32 = 0xdedee2;
pub const TEXT_DIM: u32 = 0x8e8e94;
pub const MIDI: u32 = 0x4fae5a;
pub const AUDIO: u32 = 0x3f86e0;
pub const AUX: u32 = 0xb07ad6;
pub const MUTE: u32 = 0x3f95ee;
pub const SOLO: u32 = 0xf0c63c;
pub const PLAYING: u32 = 0x4cd964;
pub const RECORD: u32 = 0xff453a;
pub const AUTO_VOLUME: u32 = 0xffd60a;
pub const AUTO_PAN: u32 = 0x64d2ff;
pub const LCD: u32 = 0x0f1012;
pub const LCD_TEXT: u32 = 0xd6e8ff;
pub const METER_LOW: u32 = 0x4cd964;
pub const METER_MID: u32 = 0xffcc00;
pub const METER_HIGH: u32 = 0xff3b30;

pub const CONTROL_BAR_H: f32 = 56.0;
pub const INSPECTOR_W: f32 = 184.0;
pub const HEADER_W: f32 = 210.0;
pub const RULER_H: f32 = 28.0;
pub const LANE_H: f32 = 64.0;
pub const STRIP_W: f32 = 82.0;
pub const FADER_H: f32 = 150.0;
pub const MIXER_H: f32 = 440.0;
pub const ROLL_H: f32 = 300.0;

pub const MONO: &str = "Menlo";

/// A track's color: green for instruments, blue for audio, as in Logic.
pub fn track_color(instrument: bool) -> u32 {
    if instrument { MIDI } else { AUDIO }
}

/// A small square toggle, like Logic's M and S buttons.
pub fn toggle(label: &'static str, on: bool, on_color: u32) -> Div {
    div()
        .w(px(20.))
        .h(px(16.))
        .rounded(px(3.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .bg(rgb(if on { on_color } else { BUTTON }))
        .text_color(rgb(if on { 0x111111 } else { TEXT }))
        .child(label)
}

/// A text button.
pub fn button(label: &'static str, on: bool) -> Div {
    div()
        .px_2()
        .h(px(22.))
        .rounded(px(4.))
        .flex()
        .items_center()
        .text_size(px(11.))
        .bg(rgb(if on { 0x5a5a62 } else { BUTTON }))
        .text_color(rgb(TEXT))
        .child(label)
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Stop,
    ToStart,
    Record,
}

/// A transport icon, drawn rather than taken from a font.
pub fn icon(icon: Icon, color: impl Into<Hsla>) -> impl IntoElement {
    let color = color.into();
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (x, y) = (b.origin.x, b.origin.y);
            let (w, h) = (b.size.width, b.size.height);
            let mut p = PathBuilder::fill();
            match icon {
                Icon::Play => {
                    p.move_to(point(x + w * 0.2, y));
                    p.line_to(point(x + w * 0.95, y + h * 0.5));
                    p.line_to(point(x + w * 0.2, y + h));
                    p.close();
                }
                Icon::Stop => {
                    p.add_polygon(
                        &[
                            point(x + w * 0.1, y + h * 0.1),
                            point(x + w * 0.9, y + h * 0.1),
                            point(x + w * 0.9, y + h * 0.9),
                            point(x + w * 0.1, y + h * 0.9),
                        ],
                        true,
                    );
                }
                Icon::ToStart => {
                    p.add_polygon(
                        &[
                            point(x, y),
                            point(x + w * 0.18, y),
                            point(x + w * 0.18, y + h),
                            point(x, y + h),
                        ],
                        true,
                    );
                    p.move_to(point(x + w, y));
                    p.line_to(point(x + w * 0.25, y + h * 0.5));
                    p.line_to(point(x + w, y + h));
                    p.close();
                }
                Icon::Record => {
                    let (c, r) = (point(x + w * 0.5, y + h * 0.5), w * 0.45);
                    p.move_to(point(c.x + r, c.y));
                    p.arc_to(point(r, r), px(0.), false, false, point(c.x - r, c.y));
                    p.arc_to(point(r, r), px(0.), false, false, point(c.x + r, c.y));
                    p.close();
                }
            }
            if let Ok(path) = p.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(12.))
}
