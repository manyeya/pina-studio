//! How tracks look on screen: their icons, drawn; their colours, each a shade of its family's;
//! and the picker that chooses them (click a track's icon). What a track plays decides both
//! (`tempo_engine::look`), from its samples' and audio's analysis, done here in the background.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::OnceLock,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, Subscription,
    Svg, anchored, deferred, div, prelude::*, px, rgb, svg,
};
use tempo_engine::{
    look::{self, Family, Look, Source},
    samples::Category,
};

use crate::{text::TextInput, theme::*, workspace::Workspace};

/// An icon's shape, on the 24-unit grid every tempo icon is drawn on. Snare, piano, keys, the
/// guitars, bass, pad, bells, vocals, mic, fx, vinyl, texture, foley, audio, midi, loop,
/// reference, click and the marks are Lucide's (ISC: assets/LUCIDE-LICENSE); the rest are drawn
/// to match.
fn shape(id: &str) -> &'static str {
    match id {
        "kick" => {
            r##"<circle cx="12" cy="11" r="7.5"/><circle cx="12" cy="11" r="2.4"/><path d="M6.6 16.3 5 21M17.4 16.3 19 21"/>"##
        }
        "snare" => {
            r##"<path d="m2 2 8 8"/><path d="m22 2-8 8"/><ellipse cx="12" cy="9" rx="10" ry="5"/><path d="M7 13.4v7.9"/><path d="M12 14v8"/><path d="M17 13.4v7.9"/><path d="M2 9v8a10 5 0 0 0 20 0V9"/>"##
        }
        "clap" => {
            r##"<path d="M11.4 9.2 9.6 6.4a1.6 1.6 0 0 0-2.7 1.7l.3.5-.9-1.4a1.6 1.6 0 0 0-2.7 1.7l3.2 5.1c1.7 2.7 4.4 4.3 6.7 3.6"/><path d="M12.6 9.2l1.8-2.8a1.6 1.6 0 0 1 2.7 1.7l-.3.5.9-1.4a1.6 1.6 0 0 1 2.7 1.7l-3.2 5.1c-1.7 2.7-4.4 4.3-6.7 3.6M12 9.2v4"/><path d="M12 2.5v1.8M8.4 3.2l.8 1.5M15.6 3.2l-.8 1.5"/>"##
        }
        "hihat" => {
            r##"<path d="M4.5 8.5c0-1.4 3.4-2.5 7.5-2.5s7.5 1.1 7.5 2.5zM4.5 11c0 1.4 3.4 2.5 7.5 2.5s7.5-1.1 7.5-2.5zM12 13.5V21M12 3.5V6M8.5 21h7"/>"##
        }
        "cymbal" => {
            r##"<path d="M2.8 10.2 21.2 6.8M5 9.8c1.6 1.3 4 1.8 7 1.3s5.3-1.8 6.6-3.6M10.8 8.2c.4-1.3 1.8-1.9 2.9-1.2M12 11.3V21M9 21h6"/>"##
        }
        "tom" => {
            r##"<ellipse cx="12" cy="8.5" rx="8" ry="3"/><path d="M4 8.5v6.5c0 1.7 3.6 3 8 3s8-1.3 8-3V8.5M8 11.3v6.2M16 11.3v6.2"/>"##
        }
        "drums" => {
            r##"<circle cx="9.5" cy="15" r="6"/><circle cx="9.5" cy="15" r="1.8"/><path d="M14.5 5.8l7-1.6M18.4 4.9V21M15.5 21h6"/>"##
        }
        "sticks" => {
            r##"<path d="M4 20 18.5 5.5M20 20 5.5 5.5"/><circle cx="19.2" cy="4.8" r="1.4"/><circle cx="4.8" cy="4.8" r="1.4"/>"##
        }
        "conga" => {
            r##"<ellipse cx="12" cy="5" rx="5.5" ry="2"/><path d="M6.5 5c0 4.5 1.8 6.5 2.2 9.6.3 2.4-.4 4.4-.4 6.4h7.4c0-2-.7-4-.4-6.4.4-3.1 2.2-5.1 2.2-9.6"/>"##
        }
        "shaker" => {
            r##"<ellipse cx="9.2" cy="9.2" rx="5.2" ry="6" transform="rotate(-45 9.2 9.2)"/><path d="M13.2 13.2 20 20M17 3.2l1-1M20.8 7l1-1M19.4 4.6l1.4-.4"/>"##
        }
        "tambourine" => {
            r##"<circle cx="12" cy="12" r="8"/><circle cx="12" cy="4" r="1.5"/><circle cx="20" cy="12" r="1.5"/><circle cx="12" cy="20" r="1.5"/><circle cx="4" cy="12" r="1.5"/>"##
        }
        "cowbell" => r##"<path d="M8 4h8l3 15H5zM11 4V2.5h2V4M9.5 19v1.5M14.5 19v1.5"/>"##,
        "808" => {
            r##"<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5.2"/><circle cx="12" cy="12" r="1.6"/>"##
        }
        "bass" => {
            r##"<path d="M19 11h.01"/><path d="M19 6h.01"/><path d="M5 8c0-4 4-4 4-4 6 0 6 6 6 6 0 7-10 11-10 11"/><circle cx="7" cy="8" r="2"/>"##
        }
        "synth-bass" => {
            r##"<path d="M2.5 10c2.3 0 2.5 6 4.75 6S9.7 10 12 10s2.5 6 4.75 6S19.2 10 21.5 10M3 20.5h18"/>"##
        }
        "piano" => {
            r##"<path d="M10 13v4"/><path d="M14 13v4"/><path d="M18 13v4"/><path d="M2 13h20"/><path d="M22 11.5A3.5 3.5 0 0018.5 8a3.52 3.52 0 01-3.173-2A7 7 0 002 9v10a2 2 0 002 2h16a2 2 0 002-2z"/><path d="M6 13v4"/>"##
        }
        "keys" => {
            r##"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M6 8h4"/><path d="M14 8h.01"/><path d="M18 8h.01"/><path d="M2 12h20"/><path d="M6 12v4"/><path d="M10 12v4"/><path d="M14 12v4"/><path d="M18 12v4"/>"##
        }
        "organ" => {
            r##"<path d="M4.5 20.5V10.2a1.75 1.75 0 0 1 3.5 0v10.3M10.25 20.5V5.7a1.75 1.75 0 0 1 3.5 0v14.8M16 20.5v-7.8a1.75 1.75 0 0 1 3.5 0v7.8M2.5 20.5h19"/>"##
        }
        "chords" => {
            r##"<ellipse cx="9" cy="18.2" rx="3.2" ry="2.3" transform="rotate(-20 9 18.2)"/><ellipse cx="9" cy="14" rx="3.2" ry="2.3" transform="rotate(-20 9 14)"/><ellipse cx="9" cy="9.8" rx="3.2" ry="2.3" transform="rotate(-20 9 9.8)"/><path d="M12.1 3v14.5M12.1 3c2.5.6 4.5 2 4.5 4.5"/>"##
        }
        "guitar" => {
            r##"<path d="m11.9 12.1 4.514-4.514"/><path d="M20.1 2.3a1 1 0 0 0-1.4 0l-1.114 1.114A2 2 0 0 0 17 4.828v1.344a2 2 0 0 1-.586 1.414A2 2 0 0 1 17.828 7h1.344a2 2 0 0 0 1.414-.586L21.7 5.3a1 1 0 0 0 0-1.4z"/><path d="m6 16 2 2"/><path d="M8.23 9.85A3 3 0 0 1 11 8a5 5 0 0 1 5 5 3 3 0 0 1-1.85 2.77l-.92.38A2 2 0 0 0 12 18a4 4 0 0 1-4 4 6 6 0 0 1-6-6 4 4 0 0 1 4-4 2 2 0 0 0 1.85-1.23z"/>"##
        }
        "guitar-electric" => {
            r##"<path d="m12.172 11.828 4.242-4.242"/><path d="M16.414 7.586A2 2 0 0017 6.172V5a2 2 0 011.177-1.823L20.59 2.09a1 1 0 011.32 1.32l-1.088 2.412A2 2 0 0119 7h-1a2 2 0 00-1.586.586"/><path d="m6 16 2 2"/><path d="M8.23 9.85a3 3 0 011.28-1.454 1 1 0 011.496.815 4 4 0 003.783 3.783 1 1 0 01.815 1.496 3 3 0 01-1.454 1.28l-.92.38A2 2 0 0012 18a4 4 0 01-4 4 6 6 0 01-6-6 4 4 0 014-4 2 2 0 001.85-1.23z"/>"##
        }
        "strings" => {
            r##"<path d="M13 2v6.5M11.7 2h2.6"/><path d="M13 8.5c-2.4 0-4 1.1-4 2.9 0 1.1.9 1.7.9 2.5S8 15.3 8 17.6c0 2.5 2.3 3.9 5 3.9s5-1.4 5-3.9c0-2.3-1.9-2.9-1.9-3.7s.9-1.4.9-2.5c0-1.8-1.6-2.9-4-2.9zM11.4 14.6v2.6M14.6 14.6v2.6M13 18.5v2"/><path d="M4 21 7 3.5"/>"##
        }
        "harp" => {
            r##"<path d="M6 21V3c4 1 6 4 7 7s3.5 5.5 6.5 6.5V21zM9 6.5V21M12 9.5V21M15 14V21"/>"##
        }
        "brass" => {
            r##"<path d="M2.5 11.5H14L21 7v10l-7-4.5H2.5zM6 11.5V8.2M9 11.5V8.2M12 11.5V8.2M5.5 12.5v3h8v-3"/>"##
        }
        "sax" => {
            r##"<path d="M5 3.5h4.5l1 1.5v9a4.25 4.25 0 0 0 8.5 0v-3M16.4 6.5l5.2 1.5M16.4 6.5 19 11M21.6 8 19 11"/><circle cx="10.5" cy="8" r=".9"/><circle cx="10.5" cy="11.3" r=".9"/>"##
        }
        "flute" => {
            r##"<path d="M3.4 18.8 18.8 3.4M5.2 20.6 20.6 5.2M3.4 18.8l1.8 1.8M18.8 3.4l1.8 1.8M9.2 14.8h.01M11.6 12.4h.01M14 10h.01"/>"##
        }
        "synth" => {
            r##"<rect x="2.5" y="4.5" width="19" height="15" rx="2"/><path d="M2.5 12.5h19M7 12.5v7M12 12.5v7M17 12.5v7"/><circle cx="6.5" cy="8.5" r="1.3"/><circle cx="11" cy="8.5" r="1.3"/><path d="M15 8.5h3.5"/>"##
        }
        "lead" => r##"<path d="M2.5 17 9 6.5V17l6.5-10.5V17L21.5 7"/>"##,
        "arp" => {
            r##"<path d="M3 19.5h4v-4h4v-4h4v-4h5"/><circle cx="5" cy="17" r=".8"/><circle cx="9" cy="13" r=".8"/><circle cx="13" cy="9" r=".8"/><circle cx="17" cy="5" r=".8"/>"##
        }
        "pluck" => r##"<path d="M2.5 12h2.5L7 4.5l2.2 15 1.8-11 1.6 7 1.3-4 1.1 2.2.9-.7h5.6"/>"##,
        "pad" => r##"<path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>"##,
        "bells" => {
            r##"<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"##
        }
        "mallets" => {
            r##"<rect x="3" y="6" width="3" height="12" rx="1"/><rect x="7.5" y="7" width="3" height="10" rx="1"/><rect x="12" y="8" width="3" height="8" rx="1"/><rect x="16.5" y="9" width="3" height="6" rx="1"/><path d="M15 3.5 21 2.5"/>"##
        }
        "vocals" => {
            r##"<path d="m11 7.601-5.994 8.19a1 1 0 0 0 .1 1.298l.817.818a1 1 0 0 0 1.314.087L15.09 12"/><path d="M16.5 21.174C15.5 20.5 14.372 20 13 20c-2.058 0-3.928 2.356-6 2-2.072-.356-2.775-3.369-1.5-4.5"/><circle cx="16" cy="7" r="5"/>"##
        }
        "mic" => {
            r##"<path d="M12 19v3"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><rect x="9" y="2" width="6" height="13" rx="3"/>"##
        }
        "choir" => {
            r##"<circle cx="12" cy="7" r="2.6"/><circle cx="5.3" cy="9.6" r="2.1"/><circle cx="18.7" cy="9.6" r="2.1"/><path d="M7.4 20.5a4.6 4.6 0 0 1 9.2 0M1.5 19a4 4 0 0 1 6.4-3.2M22.5 19a4 4 0 0 0-6.4-3.2"/>"##
        }
        "chop" => {
            r##"<path d="M3.5 10.5v3M6.5 8v8M9.5 6v12M14.5 7v10M17.5 9v6M20.5 11v2M12 3v2M12 8v2M12 13v2M12 18v2"/>"##
        }
        "fx" => {
            r##"<path d="M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z"/><path d="M20 2v4"/><path d="M22 4h-4"/><circle cx="4" cy="20" r="2"/>"##
        }
        "riser" => {
            r##"<path d="M2.5 19.5c7 0 11-3.5 13.8-12.5"/><path d="M12.3 8.2 16.4 6.3l1.9 4.1"/><path d="M2.5 19.5h19"/>"##
        }
        "impact" => {
            r##"<path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M5.6 18.4l2.8-2.8M15.6 8.4l2.8-2.8"/><circle cx="12" cy="12" r="2.2"/>"##
        }
        "reverse" => r##"<path d="M2.5 12c7 0 11.5-6 19-7v14c-7.5-1-12-7-19-7z"/>"##,
        "vinyl" => {
            r##"<circle cx="12" cy="12" r="10"/><path d="M6 12c0-1.7.7-3.2 1.8-4.2"/><circle cx="12" cy="12" r="2"/><path d="M18 12c0 1.7-.7 3.2-1.8 4.2"/>"##
        }
        "texture" => {
            r##"<path d="M2 12q2.5 2 5 0t5 0 5 0 5 0"/><path d="M2 19q2.5 2 5 0t5 0 5 0 5 0"/><path d="M2 5q2.5 2 5 0t5 0 5 0 5 0"/>"##
        }
        "foley" => {
            r##"<path d="M4 16v-2.38C4 11.5 2.97 10.5 3 8c.03-2.72 1.49-6 4.5-6C9.37 2 10 3.8 10 5.5c0 3.11-2 5.66-2 8.68V16a2 2 0 1 1-4 0Z"/><path d="M20 20v-2.38c0-2.12 1.03-3.12 1-5.62-.03-2.72-1.49-6-4.5-6C14.63 6 14 7.8 14 9.5c0 3.11 2 5.66 2 8.68V20a2 2 0 1 0 4 0Z"/><path d="M16 17h4"/><path d="M4 13h4"/>"##
        }
        "audio" => {
            r##"<path d="M2 13a2 2 0 0 0 2-2V7a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0V4a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0v-4a2 2 0 0 1 2-2"/>"##
        }
        "midi" => r##"<path d="m19 6-5.2-3.9a.5.5 0 0 0-.8.4V18"/><circle cx="9" cy="18" r="4"/>"##,
        "sampler" => {
            r##"<rect x="3.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.8"/>"##
        }
        "loop" => {
            r##"<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>"##
        }
        "reference" => {
            r##"<path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3"/>"##
        }
        "click" => {
            r##"<path d="M12 11.4V9.1"/><path d="m12 17 6.59-6.59"/><path d="m15.05 5.7-.218-.691a3 3 0 0 0-5.663 0L4.418 19.695A1 1 0 0 0 5.37 21h13.253a1 1 0 0 0 .951-1.31L18.45 16.2"/><circle cx="20" cy="9" r="2"/>"##
        }
        "star" => {
            r##"<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>"##
        }
        "heart" => {
            r##"<path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/>"##
        }
        "flame" => {
            r##"<path d="M12 3q1 4 4 6.5t3 5.5a1 1 0 0 1-14 0 5 5 0 0 1 1-3 1 1 0 0 0 5 0c0-2-1.5-3-1.5-5q0-2 2.5-4"/>"##
        }
        "bolt" => {
            r##"<path d="M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z"/>"##
        }
        _ => r##"<circle cx="12" cy="12" r="8"/>"##,
    }
}

/// A track icon by its id, `size` pixels square, coloured by `text_color`.
pub fn glyph(id: &str, size: f32) -> Svg {
    static CACHE: OnceLock<HashMap<&'static str, Vec<u8>>> = OnceLock::new();
    let all = CACHE.get_or_init(|| {
        look::ICONS
            .iter()
            .map(|i| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
                    shape(i.id)
                );
                (i.id, svg.into_bytes())
            })
            .collect()
    });
    let bytes: &'static [u8] = all.get(id).or_else(|| all.get("audio")).unwrap();
    svg().data(bytes).size(px(size)).flex_none()
}

/// A colour from hue (degrees), saturation and lightness (0 to 1).
pub fn hsl(h: f32, s: f32, l: f32) -> u32 {
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = match t {
            t if t < 1.0 / 6.0 => p + (q - p) * 6.0 * t,
            t if t < 0.5 => q,
            t if t < 2.0 / 3.0 => p + (q - p) * (2.0 / 3.0 - t) * 6.0,
            _ => p,
        };
        (v.clamp(0.0, 1.0) * 255.0).round() as u32
    };
    (channel(h + 1.0 / 3.0) << 16) | (channel(h) << 8) | channel(h - 1.0 / 3.0)
}

/// A colour's hue, saturation and lightness.
fn to_hsl(c: u32) -> (f32, f32, f32) {
    let [r, g, b] = [(c >> 16) & 255, (c >> 8) & 255, c & 255].map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = match max {
        m if m == r => (g - b) / d + if g < b { 6.0 } else { 0.0 },
        m if m == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    };
    (h * 60.0, s, l)
}

/// Each family's colour, tuned for tempo's dark surfaces: drums warm, bass deep violet, keys
/// teal, vocals pink; effects and textures greyed, so the music stands out.
fn family_color(f: Family) -> (f32, f32, f32) {
    match f {
        Family::Drums => (6.0, 0.92, 0.66),
        Family::Percussion => (29.0, 0.92, 0.62),
        Family::Bass => (256.0, 0.86, 0.72),
        Family::Keys => (163.0, 0.66, 0.52),
        Family::Guitar => (42.0, 0.94, 0.6),
        Family::Strings => (343.0, 0.9, 0.7),
        Family::Brass => (51.0, 0.88, 0.6),
        Family::Woodwind => (88.0, 0.62, 0.56),
        Family::Synth => (202.0, 0.92, 0.62),
        Family::Pad => (232.0, 0.86, 0.74),
        Family::Bells => (186.0, 0.72, 0.54),
        Family::Vocals => (312.0, 0.8, 0.68),
        Family::Fx => (218.0, 0.26, 0.68),
        Family::Texture => (30.0, 0.2, 0.64),
        Family::Other => (210.0, 0.14, 0.66),
    }
}

/// The `n`th shade of a colour: the first as it is, then lighter and darker by turns, its hue
/// nudged, so tracks of a family read as one and each as itself.
pub fn shade(base: (f32, f32, f32), n: usize) -> u32 {
    const STEPS: [(f32, f32); 8] = [
        (0.0, 0.0),
        (7.0, 0.09),
        (-7.0, -0.09),
        (12.0, 0.15),
        (-12.0, -0.14),
        (4.0, 0.04),
        (-4.0, -0.05),
        (16.0, 0.2),
    ];
    let (dh, dl) = STEPS[n % STEPS.len()];
    let (h, s, l) = base;
    hsl(h + dh, s, (l + dl).clamp(0.38, 0.84))
}

/// The picker's swatches: hues across, light to deep down, then greys.
pub fn swatches() -> Vec<Vec<u32>> {
    const HUES: [f32; 14] = [
        2.0, 18.0, 32.0, 44.0, 56.0, 88.0, 140.0, 165.0, 188.0, 204.0, 228.0, 256.0, 290.0, 322.0,
    ];
    let row = |s: f32, l: f32| HUES.iter().map(|h| hsl(*h, s, l)).collect::<Vec<_>>();
    vec![
        row(0.9, 0.76),
        row(0.86, 0.64),
        row(0.7, 0.5),
        [0.92, 0.8, 0.68, 0.56, 0.44, 0.34]
            .iter()
            .map(|l| hsl(220.0, 0.08, *l))
            .collect(),
    ]
}

pub fn hex(c: u32) -> String {
    format!("#{c:06x}")
}

/// A track's look on screen: its icon and colour, and what said so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub look: Look,
    pub color: u32,
}

/// What the picker is choosing for, and its search.
pub struct Picker {
    pub track: usize,
    pub at: Point<Pixels>,
    pub search: Entity<TextInput>,
    _typed: Subscription,
}

/// What's being tried, under the pointer, before it's chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trying {
    Color(u32),
    Icon(&'static str),
}

/// What the looks are worked out from: the analysis of the song's audio, by file.
#[derive(Default)]
pub struct Heard {
    pub categories: HashMap<PathBuf, Option<Category>>,
    asked: HashSet<PathBuf>,
}

impl Workspace {
    /// Works out every track's look again: after an edit, or when the analysis of its audio
    /// comes in. Files not yet heard are analysed in the background.
    pub fn refresh_looks(&mut self, cx: &mut Context<Self>) {
        let dir = self.session.dir.clone();
        let heard = &self.heard.categories;
        let category = |p: &std::path::Path| heard.get(&dir.join(p)).copied().flatten();
        let looks: Vec<Look> = self
            .session
            .project
            .tracks
            .iter()
            .map(|t| look::look(t, &category))
            .collect();
        // Each track's shade: how many of its family came before it.
        let mut seen: HashMap<Family, usize> = HashMap::new();
        self.looks = self
            .session
            .project
            .tracks
            .iter()
            .zip(looks)
            .enumerate()
            .map(|(i, (t, look))| {
                let n = seen.entry(look.family).or_default();
                let base = match look.family {
                    // Tracks nothing says the role of take turns round the palette.
                    Family::Other if look.source != Source::Chosen => {
                        let hues = [202.0, 163.0, 42.0, 256.0, 312.0];
                        (hues[i % hues.len()], 0.7, 0.64)
                    }
                    f => family_color(f),
                };
                let color = t
                    .color
                    .as_deref()
                    .and_then(look::parse_color)
                    .unwrap_or_else(|| shade(base, *n));
                *n += 1;
                Shown { look, color }
            })
            .collect();
        self.hear_new_files(cx);
    }

    /// Analyses the song's audio and samples not heard yet, then works the looks out again.
    fn hear_new_files(&mut self, cx: &mut Context<Self>) {
        use tempo_engine::project::{Content, Instrument};
        let dir = self.session.dir.clone();
        let mut new = Vec::new();
        for t in &self.session.project.tracks {
            let zones = match &t.instrument {
                Some(Instrument::Sampler(s)) => s.zones.iter().map(|z| z.file.clone()).collect(),
                _ => Vec::new(),
            };
            let audio = t.regions.iter().filter_map(|r| match &r.content {
                Content::Audio { file, .. } => Some(file.clone()),
                _ => None,
            });
            for f in zones.into_iter().chain(audio) {
                let path = dir.join(f);
                if self.heard.asked.insert(path.clone()) {
                    new.push(path);
                }
            }
        }
        // Not in tests: they say what each file is themselves.
        if new.is_empty() || cfg!(test) {
            return;
        }
        let job = cx.background_spawn(async move {
            new.into_iter()
                .map(|p| {
                    let c = tempo_engine::samples::analyze::analyze_file(&p)
                        .ok()
                        .map(|a| a.category);
                    (p, c)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let heard = job.await;
            this.update(cx, |ws, cx| {
                ws.heard.categories.extend(heard);
                ws.refresh_looks(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The look a track shows: what's being tried on it in the picker, else its own.
    pub fn shown(&self, track: usize) -> Shown {
        let mut s = self.looks.get(track).copied().unwrap_or(Shown {
            look: Look {
                icon: "audio",
                family: Family::Other,
                source: Source::Kind,
            },
            color: ACCENT,
        });
        if let Some((t, trying)) = self.trying
            && t == track
        {
            match trying {
                Trying::Color(c) => s.color = c,
                Trying::Icon(i) => s.look.icon = i,
            }
        }
        s
    }

    /// Opens the picker for a track's colour and icon, by its icon.
    pub fn open_picker(
        &mut self,
        track: usize,
        at: Point<Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx.new(|cx| TextInput::search("Search: kick, rhodes, 808, choir…", cx));
        let typed = cx.observe(&search, |_, _, cx| cx.notify());
        let focus = gpui::Focusable::focus_handle(search.read(cx), cx);
        focus.focus(window, cx);
        self.picker = Some(Picker {
            track,
            at,
            search,
            _typed: typed,
        });
        cx.notify();
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.trying = None;
        cx.notify();
    }

    fn set_look(
        &mut self,
        track: usize,
        color: Option<Option<String>>,
        icon: Option<Option<String>>,
        cx: &mut Context<Self>,
    ) {
        self.trying = None;
        self.edit(cx, |ws| {
            let t = &mut ws.session.project.tracks[track];
            if let Some(c) = color {
                t.color = c;
            }
            if let Some(i) = icon {
                t.icon = i;
            }
        });
    }

    /// Colours every track of a family in shades of one colour, as one edit.
    fn shade_family(&mut self, family: Family, color: u32, cx: &mut Context<Self>) {
        let base = to_hsl(color);
        let members: Vec<usize> = (0..self.looks.len())
            .filter(|&i| self.looks[i].look.family == family)
            .collect();
        self.trying = None;
        self.edit(cx, |ws| {
            for (n, i) in members.into_iter().enumerate() {
                ws.session.project.tracks[i].color = Some(hex(shade(base, n)));
            }
        });
    }

    /// The colour and icon picker: what the track is and why, a palette, and every icon, the
    /// ones its sound suggests first. Pointing at a swatch or icon tries it on the track.
    pub fn picker_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let p = self.picker.as_ref()?;
        let t = p.track;
        let track = self.session.project.tracks.get(t)?;
        let shown = self.shown(t);
        let own = self.looks.get(t).copied()?;
        let chosen = track.color.is_some() || track.icon.is_some();
        let family = own.look.family;
        let kin = self
            .looks
            .iter()
            .filter(|l| l.look.family == family)
            .count();
        let query = p.search.read(cx).text().trim().to_lowercase();
        let why = match own.look.source {
            Source::Chosen => "Chosen".to_string(),
            s => format!("{} · {}", family.name(), s.says()),
        };
        let tile = |id: &'static str, color: u32, size: f32| {
            div()
                .size(px(size))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(size * 0.27))
                .bg(alpha(color, 0.16))
                .border_1()
                .border_color(alpha(color, 0.32))
                .child(glyph(id, size * 0.56).text_color(rgb(color)))
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(tile(shown.look.icon, shown.color, 40.))
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(track.name.clone()),
                    )
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(why)),
            )
            .child(
                div()
                    .id("look-auto")
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(7.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .when(chosen, |d| {
                        d.bg(rgb(FIELD))
                            .text_color(rgb(TEXT))
                            .hover(|d| d.bg(rgb(RAISED)))
                    })
                    .when(!chosen, |d| d.text_color(rgb(TEXT_4)))
                    .child("Auto")
                    .debug_selector(|| "look-auto".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.set_look(t, Some(None), Some(None), cx);
                        }),
                    ),
            );
        let label = |s: &str| {
            div()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(s.to_uppercase())
        };
        let current = track.color.as_deref().and_then(look::parse_color);
        let mut colors = div().flex().flex_col().gap(px(6.));
        for (r, row) in swatches().into_iter().enumerate() {
            let mut line = div().flex().gap(px(5.));
            for (c, color) in row.into_iter().enumerate() {
                let on = current == Some(color);
                line = line.child(
                    div()
                        .id(("swatch", r * 100 + c))
                        .size(px(18.))
                        .flex_none()
                        .rounded(px(9.))
                        .bg(rgb(color))
                        .cursor_pointer()
                        .border_2()
                        .border_color(if on {
                            rgb(0xffffff).into()
                        } else {
                            alpha(color, 0.0)
                        })
                        .hover(|d| d.border_color(alpha(0xffffff, 0.7)))
                        .debug_selector(move || format!("swatch-{r}-{c}"))
                        .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                            ws.trying = over.then_some((t, Trying::Color(color)));
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                match e.modifiers.alt {
                                    // ⌥-click: the whole family in shades of it.
                                    true => ws.shade_family(family, color, cx),
                                    false => ws.set_look(t, Some(Some(hex(color))), None, cx),
                                }
                            }),
                        ),
                );
            }
            colors = colors.child(line);
        }
        let family_row = (kin > 1).then(|| {
            let color = shown.color;
            div()
                .id("shade-family")
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .text_size(px(12.))
                .text_color(rgb(TEXT_2))
                .child(div().flex().children((0..kin.min(6)).map(|n| {
                    div()
                        .size(px(12.))
                        .rounded(px(6.))
                        .ml(px(if n == 0 { 0. } else { -3. }))
                        .border_1()
                        .border_color(rgb(SURFACE))
                        .bg(rgb(shade(to_hsl(color), n)))
                })))
                .child(format!(
                    "Shade all {kin} {} tracks from this colour",
                    family.name().to_lowercase()
                ))
                .debug_selector(|| "shade-family".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.shade_family(family, color, cx);
                    }),
                )
        });
        // Icons: those its family suggests first, then every group; or what the search finds.
        let matches = |i: &&look::IconDef| {
            query.is_empty()
                || query.split_whitespace().all(|w| {
                    i.name.to_lowercase().contains(w) || i.words.contains(w) || i.id.contains(w)
                })
        };
        let icon_tile = |i: &'static look::IconDef, cx: &mut Context<Self>| {
            let on = shown.look.icon == i.id;
            let id = i.id;
            div()
                .id((
                    "icon",
                    look::ICONS.iter().position(|x| x.id == id).unwrap_or(0),
                ))
                .size(px(36.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .bg(if on {
                    alpha(shown.color, 0.18)
                } else {
                    rgb(CARD).into()
                })
                .border_1()
                .border_color(if on {
                    alpha(shown.color, 0.5)
                } else {
                    rgb(LINE_SOFT).into()
                })
                .hover(|d| d.bg(rgb(RAISED)))
                .child(glyph(id, 19.).text_color(rgb(if on { shown.color } else { TEXT_2 })))
                .debug_selector(move || format!("icon-{id}"))
                .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                    ws.trying = over.then_some((t, Trying::Icon(id)));
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.set_look(t, None, Some(Some(id.to_string())), cx);
                    }),
                )
        };
        let grid = |icons: Vec<&'static look::IconDef>, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(icons.into_iter().map(|i| icon_tile(i, cx)))
        };
        let mut sections: Vec<Div> = Vec::new();
        if query.is_empty() {
            let suggested: Vec<&'static look::IconDef> = look::ICONS
                .iter()
                .filter(|i| i.family == own.look.family && i.family != Family::Other)
                .collect();
            if !suggested.is_empty() {
                sections.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .child(label(&format!("Suggested · {}", family.name())))
                        .child(grid(suggested, cx)),
                );
            }
            let mut groups: Vec<&str> = Vec::new();
            for i in look::ICONS {
                if !groups.contains(&i.group) {
                    groups.push(i.group);
                }
            }
            for g in groups {
                let icons: Vec<&'static look::IconDef> =
                    look::ICONS.iter().filter(|i| i.group == g).collect();
                // A group all suggested already isn't shown twice.
                if icons
                    .iter()
                    .all(|i| i.family == own.look.family && i.family != Family::Other)
                {
                    continue;
                }
                sections.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .child(label(g))
                        .child(grid(icons, cx)),
                );
            }
        } else {
            let found: Vec<&'static look::IconDef> = look::ICONS.iter().filter(matches).collect();
            sections.push(match found.is_empty() {
                true => div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child(format!("No icon for \u{201c}{query}\u{201d}")),
                false => grid(found, cx),
            });
        }
        let panel = div()
            .id("look-picker")
            .w(px(364.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(14.))
            .rounded(px(14.))
            .bg(rgb(0x1a1a20))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .debug_selector(|| "look-picker".into())
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div().flex().justify_between().child(label("Colour")).child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .child("⌥-click: the whole family"),
                        ),
                    )
                    .child(colors)
                    .children(family_row),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(label("Icon"))
                    .child(
                        div()
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .rounded(px(8.))
                            .bg(rgb(FIELD))
                            .text_size(px(12.5))
                            .child(icon(Icon::Search, 13.).text_color(rgb(TEXT_3)))
                            .child(div().flex_1().child(p.search.clone())),
                    )
                    .child(
                        div()
                            .id("look-icons")
                            .max_h(px(268.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .children(sections),
                    ),
            )
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| ws.close_picker(cx)));
        let at = p.at;
        Some(deferred(anchored().position(at).snap_to_window().child(panel)).with_priority(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip_and_shades_differ() {
        for c in [0xd97757, 0x3ab7ff, 0x808080] {
            let (h, s, l) = to_hsl(c);
            let back = hsl(h, s, l);
            let near =
                |a: u32, b: u32, shift: u32| ((a >> shift) & 255).abs_diff((b >> shift) & 255) <= 1;
            assert!(
                near(c, back, 16) && near(c, back, 8) && near(c, back, 0),
                "{c:06x} {back:06x}"
            );
        }
        let base = family_color(Family::Drums);
        let shades: HashSet<u32> = (0..6).map(|n| shade(base, n)).collect();
        assert_eq!(shades.len(), 6);
        assert!(look::ICONS.iter().all(|i| shape(i.id) != shape("nothing")));
    }
}
