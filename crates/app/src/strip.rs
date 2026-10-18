//! A channel strip, as in Logic's inspector and mixer: the instrument or input, an EQ curve,
//! effect slots (the built-in EQ and compressor, then plugins), sends, where it plays, pan,
//! level, fader and meters, mute and solo, name; for a track, an aux, or the master.
//!
//! Clicking a plugin opens its editor; clicking an empty slot, or right-clicking any, picks one.
//! Sends drag up and down for level; right-click one to remove it.

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, canvas, div,
    fill, point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    dsp::{LOW_CUT_OFF, eq_response},
    player::{MASTER_ID, Which},
    project::{AutoMode, Comp, Eq, Input, Instrument, Param, PluginRef, Saturator, value_at},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{CompField, Drag, Name, SatField, View, Workspace},
};

/// Quieter than anything audible: the fader's floor.
pub const SILENT: f32 = -144.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Track(usize),
    Aux(usize),
    Master,
}

/// The built-in processors every channel can have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Builtin {
    Eq,
    Comp,
    Sat,
}

/// Where a strip is drawn: narrow in the inspector, or in a row of the mixer.
#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    /// Side by side in the inspector: at least this many effect and send rows, so strips with
    /// fewer line up with the one beside them.
    Compact { effects: usize, sends: usize },
    /// Which of the EQ, effect and send rows show, and how tall the fader stands.
    Mixer { rows: [bool; 3], fader: f32 },
}

impl Layout {
    pub fn fader_h(self) -> f32 {
        match self {
            Layout::Compact { .. } => 136.0,
            Layout::Mixer { fader, .. } => fader,
        }
    }
}

/// Mixer strips line up: this many effect and send rows each.
pub const MIXER_INSERTS: usize = 4;
pub const MIXER_SENDS: usize = 2;

/// Fader position (0 to 1) for a gain in dB: an audio taper, 0 dB at about 0.7, +6 dB at the top.
/// A pan as a mixer prints it: L30, C, R45.
pub fn pan_label(pan: f32) -> String {
    match (pan * 100.0).round() as i32 {
        0 => "C".to_string(),
        v if v < 0 => format!("L{}", -v),
        v => format!("R{v}"),
    }
}

/// A pan typed: "L30", "r 45", "C", or -100 to 100.
pub fn parse_pan(text: &str) -> Option<f32> {
    let t = text.trim().to_uppercase().replace('−', "-");
    let (side, rest) = match t.chars().next()? {
        'L' => (-1.0, &t[1..]),
        'R' => (1.0, &t[1..]),
        'C' if t.len() == 1 => return Some(0.0),
        _ => (1.0, t.as_str()),
    };
    let v: f32 = rest.trim().parse().ok()?;
    (v.is_finite() && v.abs() <= 100.0).then(|| side * v / 100.0)
}

/// A level typed: "-6", "−6 dB", "+2", "-inf"; up to the fader's top (+6).
pub fn parse_db(text: &str) -> Option<f32> {
    let t = text.trim().to_lowercase().replace('−', "-");
    let t = t.trim_end_matches("db").trim();
    if matches!(t, "-inf" | "-∞" | "off") {
        return Some(SILENT);
    }
    let db: f32 = t.parse().ok()?;
    db.is_finite().then(|| db.clamp(SILENT, 6.0))
}

pub fn fader_pos(db: f32) -> f32 {
    10f32.powf((db - 6.0) / 40.0).clamp(0.0, 1.0)
}

pub fn fader_db(pos: f32) -> f32 {
    if pos <= 1e-4 {
        SILENT
    } else {
        (40.0 * pos.log10() + 6.0).max(SILENT)
    }
}

/// "80 Hz", "2.4 kHz".
pub fn hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{f:.0} Hz")
    }
}

// Knob scales for the built-in EQ and compressor: value (0 to 1) to setting and back.
pub fn low_cut_pos(hz: f32) -> f32 {
    if hz <= LOW_CUT_OFF {
        0.0
    } else {
        (hz / LOW_CUT_OFF).ln() / 50f32.ln()
    }
}

pub fn low_cut_hz(v: f32) -> f32 {
    if v <= 0.005 {
        0.0
    } else {
        (LOW_CUT_OFF * 50f32.powf(v)).round()
    }
}

pub fn eq_gain_pos(db: f32) -> f32 {
    (db + 12.0) / 24.0
}

pub fn eq_gain_db(v: f32) -> f32 {
    ((v * 24.0 - 12.0) * 2.0).round() / 2.0
}

fn log_pos(v: f32, lo: f32, hi: f32) -> f32 {
    (v / lo).ln() / (hi / lo).ln()
}

fn log_value(p: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(p)
}

pub fn comp_pos(field: CompField, c: &Comp) -> f32 {
    match field {
        CompField::Threshold => (c.threshold_db + 60.0) / 60.0,
        CompField::Ratio => log_pos(c.ratio, 1.0, 20.0),
        CompField::Attack => log_pos(c.attack_ms, 0.1, 100.0),
        CompField::Release => log_pos(c.release_ms, 10.0, 1000.0),
        CompField::Makeup => c.makeup_db / 24.0,
    }
    .clamp(0.0, 1.0)
}

pub fn comp_set(field: CompField, c: &mut Comp, v: f32) {
    match field {
        CompField::Threshold => c.threshold_db = (v * 60.0 - 60.0).round(),
        CompField::Ratio => c.ratio = (log_value(v, 1.0, 20.0) * 10.0).round() / 10.0,
        CompField::Attack => c.attack_ms = (log_value(v, 0.1, 100.0) * 10.0).round() / 10.0,
        CompField::Release => c.release_ms = log_value(v, 10.0, 1000.0).round(),
        CompField::Makeup => c.makeup_db = (v * 48.0).round() / 2.0,
    }
}

pub fn comp_text(field: CompField, c: &Comp) -> String {
    match field {
        CompField::Threshold => format!("{} dB", db_text(c.threshold_db)),
        CompField::Ratio => format!("{:.1}:1", c.ratio),
        CompField::Attack => format!("{} ms", trim(c.attack_ms)),
        CompField::Release => format!("{} ms", trim(c.release_ms)),
        CompField::Makeup => format!("{} dB", db_text(c.makeup_db)),
    }
}

/// A saturator knob's position, 0 to 1.
pub fn sat_pos(field: SatField, s: &Saturator) -> f32 {
    match field {
        SatField::Drive => s.drive_db / 24.0,
        SatField::Tone => (s.tone + 1.0) / 2.0,
        SatField::Mix => s.mix,
        SatField::Output => (s.output_db + 12.0) / 24.0,
    }
    .clamp(0.0, 1.0)
}

pub fn sat_set(field: SatField, s: &mut Saturator, v: f32) {
    match field {
        SatField::Drive => s.drive_db = (v * 48.0).round() / 2.0,
        SatField::Tone => s.tone = ((v * 2.0 - 1.0) * 100.0).round() / 100.0,
        SatField::Mix => s.mix = (v * 100.0).round() / 100.0,
        SatField::Output => s.output_db = (v * 48.0).round() / 2.0 - 12.0,
    }
}

pub fn sat_text(field: SatField, s: &Saturator) -> String {
    match field {
        SatField::Drive => format!("{} dB", trim(s.drive_db)),
        SatField::Tone if s.tone.abs() < 0.005 => "Neutral".into(),
        SatField::Tone => format!(
            "{} {:.0}",
            if s.tone < 0.0 { "Dark" } else { "Bright" },
            s.tone.abs() * 100.0
        ),
        SatField::Mix => format!("{:.0}%", s.mix * 100.0),
        SatField::Output => format!("{} dB", db_text(s.output_db)),
    }
}

fn trim(v: f32) -> String {
    if v >= 10.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// An EQ's gain at a frequency, in dB.
pub fn eq_curve_at(eq: &Eq, freq: f32, rate: f64) -> f32 {
    eq_response(eq, freq as f64, rate) as f32
}

/// An EQ's curve across `width` points, as gains in dB from 20 Hz to 20 kHz.
pub fn eq_curve(eq: &Eq, rate: f64, width: usize) -> Vec<f32> {
    (0..width)
        .map(|i| {
            let f = 20.0 * 1000f64.powf(i as f64 / (width - 1).max(1) as f64);
            eq_response(eq, f, rate) as f32
        })
        .collect()
}

impl Workspace {
    /// A channel's level: for a track whose volume is automated, where the automation has it.
    pub fn volume(&self, ch: Channel) -> f32 {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => self
                .automated(i, Param::Volume)
                .unwrap_or(p.tracks[i].volume_db),
            Channel::Aux(i) => p.auxes[i].volume_db,
            Channel::Master => p.master_volume_db,
        }
    }

    /// A track parameter's value at the playhead, while automation plays it.
    pub fn automated(&self, track: usize, param: Param) -> Option<f32> {
        let a = &self.session.project.tracks.get(track)?.automation;
        if a.mode == AutoMode::Off {
            return None;
        }
        value_at(a.points(param), self.playhead() as f64)
    }

    /// A channel's built-in saturator.
    pub fn sat(&self, ch: Channel) -> Option<Saturator> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].sat,
            Channel::Aux(i) => p.auxes[i].sat,
            Channel::Master => p.master_sat,
        }
    }

    pub fn sat_mut(&mut self, ch: Channel) -> &mut Option<Saturator> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => &mut p.tracks[i].sat,
            Channel::Aux(i) => &mut p.auxes[i].sat,
            Channel::Master => &mut p.master_sat,
        }
    }

    pub fn set_volume(&mut self, ch: Channel, db: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].volume_db = db,
            Channel::Aux(i) => p.auxes[i].volume_db = db,
            Channel::Master => p.master_volume_db = db,
        }
    }

    pub fn set_pan(&mut self, ch: Channel, pan: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].pan = pan,
            Channel::Aux(i) => p.auxes[i].pan = pan,
            Channel::Master => {}
        }
    }

    /// The inserts of a track or aux.
    pub fn inserts_mut(&mut self, ch: Channel) -> Option<&mut Vec<PluginRef>> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => Some(&mut p.tracks[i].inserts),
            Channel::Aux(i) => Some(&mut p.auxes[i].inserts),
            Channel::Master => Some(&mut p.master_inserts),
        }
    }

    pub fn inserts(&self, ch: Channel) -> &[PluginRef] {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => &p.tracks[i].inserts,
            Channel::Aux(i) => &p.auxes[i].inserts,
            Channel::Master => &p.master_inserts,
        }
    }

    pub fn channel_id(&self, ch: Channel) -> Option<u64> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => Some(p.tracks[i].id),
            Channel::Aux(i) => Some(p.auxes[i].id),
            Channel::Master => None,
        }
    }

    /// A channel's name and color.
    pub fn channel_look(&self, ch: Channel) -> (String, u32) {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => (p.tracks[i].name.clone(), self.color(i)),
            Channel::Aux(i) => (p.auxes[i].name.clone(), BUS),
            Channel::Master => ("Stereo Out".into(), MASTER),
        }
    }

    /// The id a channel's plugins go by in the player.
    pub fn owner_id(&self, ch: Channel) -> u64 {
        self.channel_id(ch).unwrap_or(MASTER_ID)
    }

    /// Opens a plugin's editor, saying why not if it can't.
    pub fn open_editor(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let id = self.owner_id(ch);
        if let Err(e) = self.player.open_editor(id, which) {
            self.status = Some(e);
            cx.notify();
        }
    }

    /// Opens a plugin's window once the engine has it loaded (a just-added plugin loads in the
    /// background), giving up after a few seconds.
    pub fn open_editor_when_loaded(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let id = self.owner_id(ch);
        cx.spawn(async move |this, cx| {
            for _ in 0..50 {
                let opened = this
                    .update(cx, |ws, _| ws.player.open_editor(id, which).is_ok())
                    .unwrap_or(true);
                if opened {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
        })
        .detach();
    }

    /// The plugin in a slot, if there's one.
    fn slot_plugin(&self, ch: Channel, which: Which) -> Option<&PluginRef> {
        match (ch, which) {
            (Channel::Track(t), w) => self.session.project.tracks.get(t)?.plugin(w),
            (_, Which::Insert(n)) => self.inserts(ch).get(n),
            _ => None,
        }
    }

    /// The plugins that can go in a slot: instruments for the instrument slot, effects (and the
    /// built-in ones a channel doesn't have yet) for inserts, by name.
    pub fn plugin_menu(&self, ch: Channel, which: Which, at: Point<Pixels>) -> Menu {
        let instrument = which == Which::Instrument;
        let mut items = Vec::new();
        match (ch, which) {
            (Channel::Track(track), Which::Instrument) => {
                for (name, instrument) in [
                    ("Sine Synth", Instrument::Synth),
                    ("Drum Kit", Instrument::Drums),
                ] {
                    items.push((
                        name.to_string(),
                        MenuAction::Instrument { track, instrument },
                    ));
                }
            }
            // Taking out a plugin, from a slot that has one.
            (_, Which::Insert(index)) if index < self.inserts(ch).len() => {
                let action = MenuAction::Insert {
                    channel: ch,
                    index,
                    plugin: None,
                };
                items.push(("No Plug-in".to_string(), action));
            }
            _ => {}
        }
        // The presets of the plugin in the slot, the loaded one ticked.
        let loaded = self.slot_plugin(ch, which).and_then(|r| r.preset.clone());
        if let Some(plugin) = self.player.plugin(self.owner_id(ch), which) {
            for preset in plugin.presets() {
                let mark = if loaded.as_ref() == Some(&preset) {
                    "✓ "
                } else {
                    ""
                };
                let label = format!("{mark}Preset: {}", preset.name);
                let action = MenuAction::Preset {
                    channel: ch,
                    which,
                    preset,
                };
                items.push((label, action));
            }
        }
        for p in self
            .plugins
            .iter()
            .flatten()
            .filter(|p| p.instrument == instrument)
        {
            let plugin = p.plugin_ref();
            let action = match (ch, which) {
                (Channel::Track(track), Which::Instrument) => MenuAction::Instrument {
                    track,
                    instrument: Instrument::Plugin(plugin),
                },
                (_, Which::Insert(index)) => MenuAction::Insert {
                    channel: ch,
                    index,
                    plugin: Some(plugin),
                },
                _ => continue,
            };
            items.push((
                format!("{} — {} · {}", p.class, p.vendor, p.format.name()),
                action,
            ));
        }
        if !instrument {
            let (eq, comp) = self.strip_dsp(ch);
            if eq.is_none() {
                items.push((
                    "Channel EQ (built in)".into(),
                    MenuAction::AddBuiltin {
                        channel: ch,
                        builtin: Builtin::Eq,
                    },
                ));
            }
            if comp.is_none() {
                items.push((
                    "Compressor (built in)".into(),
                    MenuAction::AddBuiltin {
                        channel: ch,
                        builtin: Builtin::Comp,
                    },
                ));
            }
            if self.sat(ch).is_none() {
                items.push((
                    "Saturator (built in)".into(),
                    MenuAction::AddBuiltin {
                        channel: ch,
                        builtin: Builtin::Sat,
                    },
                ));
            }
        }
        if self.plugins.is_none() {
            items.push(("Looking for plugins…".to_string(), MenuAction::Nothing));
        }
        // Presets and plugins run long: typing narrows them down.
        Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        }
    }

    /// Where a track can play: the master, or any aux or stack.
    pub fn output_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items = vec![(
            "Stereo Out".to_string(),
            MenuAction::Output { track, bus: None },
        )];
        items.extend(self.session.project.auxes.iter().map(|a| {
            (
                format!("Bus {} ({})", a.bus, a.name),
                MenuAction::Output {
                    track,
                    bus: Some(a.bus),
                },
            )
        }));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The buses a track can send to: every aux, and a new one.
    pub fn send_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items: Vec<_> = self
            .session
            .project
            .auxes
            .iter()
            .map(|a| {
                (
                    format!("Bus {} ({})", a.bus, a.name),
                    MenuAction::Send {
                        track,
                        bus: Some(a.bus),
                    },
                )
            })
            .collect();
        items.push(("New Aux".to_string(), MenuAction::Send { track, bus: None }));
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// What an audio track can record: each input channel alone, and in pairs.
    pub fn input_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let channels = tempo_engine::record::input_channels();
        let mut items = Vec::new();
        for stereo in [true, false] {
            let step = if stereo { 2 } else { 1 };
            for channel in (0..channels).step_by(step) {
                let input = Input { channel, stereo };
                let action = MenuAction::Input {
                    track,
                    input: Some(input),
                };
                items.push((input.name(), action));
            }
        }
        if items.is_empty() {
            items.push(("No audio input".into(), MenuAction::Nothing));
        }
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// What a send can do: switch between before and after the fader, or go.
    pub fn send_options(&self, track: usize, index: usize, at: Point<Pixels>) -> Menu {
        let pre = self.session.project.tracks[track].sends[index].pre;
        let mark = |on: bool| if on { "✓ " } else { "    " };
        Menu {
            at,
            items: vec![
                (
                    format!("{}Post-fader", mark(!pre)),
                    MenuAction::SendPre {
                        track,
                        index,
                        pre: false,
                    },
                ),
                (
                    format!("{}Pre-fader", mark(pre)),
                    MenuAction::SendPre {
                        track,
                        index,
                        pre: true,
                    },
                ),
                (
                    "Remove Send".into(),
                    MenuAction::RemoveSend { track, index },
                ),
            ],
            ..Default::default()
        }
    }

    fn has_builtin_instrument(&self, ch: Channel) -> bool {
        matches!(ch, Channel::Track(i) if matches!(self.session.project.tracks[i].instrument, Some(Instrument::Synth | Instrument::Drums | Instrument::Sampler(_))))
    }

    /// Shows a channel in the mixer, its effects in the side panel.
    pub fn show_channel(&mut self, ch: Channel, cx: &mut Context<Self>) {
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        self.view = View::Mixer;
        cx.notify();
    }
}

/// A slot: an instrument, an effect, a send; with a dot of color before it, if given.
fn slot(label: impl Into<gpui::SharedString>, used: bool, h: f32) -> Div {
    dotted(label, used, h, None)
}

fn dotted(label: impl Into<gpui::SharedString>, used: bool, h: f32, dot: Option<u32>) -> Div {
    div()
        .flex_none()
        .h(px(h))
        .px(px(6.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(5.))
        .text_size(px(10.5))
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .when(used, |d| {
            d.bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        })
        .when(!used, |d| {
            d.border_1()
                .border_dashed()
                .border_color(rgb(RAISED))
                .text_color(rgb(TEXT_4))
                .hover(|s| s.border_color(rgb(LINE_STRONG)).text_color(rgb(TEXT_3)))
        })
        .when_some(dot, |d, c| {
            d.child(div().size(px(6.)).flex_none().rounded(px(3.)).bg(rgb(c)))
        })
        .child(
            div()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .child(label.into()),
        )
}

/// A slot that says what feeds a channel but can't be changed: no hover, no pointer.
fn info(label: impl Into<gpui::SharedString>, h: f32) -> Div {
    div()
        .flex_none()
        .h(px(h))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(5.))
        .border_1()
        .border_color(rgb(LINE_SOFT))
        .text_size(px(10.5))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(label.into())
}

/// An empty row, keeping the mixer's strips lined up.
fn blank(h: f32) -> Div {
    div().flex_none().h(px(h))
}

/// A plugin slot: left-click opens the plugin (or picks one, if empty), right-click picks one.
fn plugin_slot(
    label: String,
    used: bool,
    ch: Channel,
    which: Which,
    h: f32,
    dot: Option<u32>,
    cx: &mut Context<Workspace>,
) -> Div {
    dotted(label, used, h, dot)
        .debug_selector(move || format!("slot-{ch:?}-{which:?}"))
        .when(!used, |d| {
            d.justify_center()
                .child(icon(Icon::Plus, 11.).text_color(rgb(TEXT_4)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if used && !(which == Which::Instrument && ws.has_builtin_instrument(ch)) {
                    ws.open_editor(ch, which, cx);
                } else {
                    ws.menu = Some(ws.plugin_menu(ch, which, e.position));
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = Some(ws.plugin_menu(ch, which, e.position));
                cx.notify();
            }),
        )
}

/// A built-in processor's slot: click shows it in the mixer; right-click takes it out.
fn builtin_slot(
    label: &'static str,
    ch: Channel,
    b: Builtin,
    h: f32,
    color: u32,
    cx: &mut Context<Workspace>,
) -> Div {
    dotted(label, true, h, Some(color))
        .debug_selector(move || format!("builtin-{ch:?}-{b:?}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.show_channel(ch, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = Some(Menu {
                    at: e.position,
                    items: vec![(
                        "Remove".into(),
                        MenuAction::RemoveBuiltin {
                            channel: ch,
                            builtin: b,
                        },
                    )],
                    ..Default::default()
                });
                cx.notify();
            }),
        )
}

/// A channel's EQ curve, small: the strip's thumbnail. Clicking it shows the channel's EQ.
pub fn eq_thumbnail(
    ws: &Workspace,
    ch: Channel,
    h: f32,
    color: u32,
    cx: &mut Context<Workspace>,
) -> Div {
    let eq = ws.strip_dsp(ch).0;
    let rate = ws.session.project.sample_rate as f64;
    let curve = eq.filter(|e| !e.bypass).map(|e| eq_curve(&e, rate, 48));
    div()
        .flex_none()
        .h(px(h))
        .rounded(px(6.))
        .bg(rgb(WELL))
        .overflow_hidden()
        .cursor_pointer()
        .debug_selector(move || format!("eq-{ch:?}"))
        .child(
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (w, hh) = (f32::from(b.size.width), f32::from(b.size.height));
                    let mid = hh / 2.0;
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px(mid)),
                            size(b.size.width, px(1.)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                    let Some(curve) = &curve else { return };
                    let scale = (mid - 3.0) / 18.0;
                    let y = |g: f32| {
                        b.origin.y + px((mid - g.clamp(-18.0, 18.0) * scale).clamp(1.0, hh - 1.0))
                    };
                    let x = |i: usize| b.origin.x + px(i as f32 * w / (curve.len() - 1) as f32);
                    let mut area = gpui::PathBuilder::fill();
                    area.move_to(point(b.origin.x, b.origin.y + b.size.height));
                    for (i, &g) in curve.iter().enumerate() {
                        area.line_to(point(x(i), y(g)));
                    }
                    area.line_to(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(color, 0.14));
                    }
                    let mut line = gpui::PathBuilder::stroke(px(1.5));
                    line.move_to(point(x(0), y(curve[0])));
                    for (i, &g) in curve.iter().enumerate().skip(1) {
                        line.line_to(point(x(i), y(g)));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, rgb(color));
                    }
                },
            )
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if ws.strip_dsp(ch).0.is_none() {
                    ws.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(Eq::default()));
                }
                ws.show_channel(ch, cx);
            }),
        )
}

/// A stereo meter, on the fader's scale: green, then yellow from -6 dB, red over 0 dB; a line
/// where the peak was a moment ago.
pub fn meter(level: [f32; 2], held: [f32; 2], w: f32, gap: f32) -> impl IntoElement {
    let pos = |l: f32| fader_pos(20.0 * l.max(1e-9).log10());
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let h = f32::from(b.size.height);
            let bottom = b.origin.y + b.size.height;
            for side in 0..2 {
                let x = b.origin.x + px(side as f32 * (w + gap));
                let bar = |from: f32, to: f32| {
                    Bounds::new(
                        point(x, bottom - px(to * h)),
                        size(px(w), px((to - from) * h)),
                    )
                };
                window.paint_quad(fill(bar(0.0, 1.0), rgb(0x0a0a0d)).corner_radii(px(2.)));
                let m = pos(level[side]);
                for (from, to, color) in [
                    (0.0, fader_pos(-6.0), METER_LOW),
                    (fader_pos(-6.0), fader_pos(0.0), METER_MID),
                    (fader_pos(0.0), 1.0, METER_HIGH),
                ] {
                    let top = m.min(to);
                    if top > from {
                        window.paint_quad(fill(bar(from, top), rgb(color)));
                    }
                }
                let p = pos(held[side]);
                if p > 0.02 {
                    let color = if p >= fader_pos(0.0) {
                        METER_HIGH
                    } else {
                        TEXT
                    };
                    window.paint_quad(fill(bar(p - 2.0 / h, p), rgb(color)));
                }
            }
        },
    )
    .w(px(w * 2.0 + gap))
    .h_full()
    .flex_none()
}

/// How many effect rows (its built-ins, its plugins and an empty slot) and send rows (its sends
/// and a slot for another) a strip shows in the inspector.
pub fn rows(ws: &Workspace, ch: Channel) -> (usize, usize) {
    let (eq, comp) = ws.strip_dsp(ch);
    let effects = eq.is_some() as usize
        + comp.is_some() as usize
        + ws.sat(ch).is_some() as usize
        + ws.inserts(ch).len()
        + 1;
    let sends = match ch {
        Channel::Track(i) => ws.session.project.tracks[i].sends.len() + 1,
        _ => 0,
    };
    (effects, sends)
}

pub fn strip(ws: &Workspace, ch: Channel, layout: Layout, cx: &mut Context<Workspace>) -> Div {
    let project = &ws.session.project;
    let mixer = matches!(layout, Layout::Mixer { .. });
    let rows = match layout {
        Layout::Mixer { rows, .. } => rows,
        Layout::Compact { .. } => [true; 3],
    };
    let (name, color) = ws.channel_look(ch);
    let (pan, mute, solo, id) = match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            let pan = ws.automated(i, Param::Pan).unwrap_or(t.pan);
            (Some(pan), Some(t.mute), Some(t.solo), Some(t.id))
        }
        Channel::Aux(i) => {
            let a = &project.auxes[i];
            (Some(a.pan), Some(a.mute), None, Some(a.id))
        }
        Channel::Master => (None, None, None, None),
    };
    let volume = ws.volume(ch);
    let level = ws.levels.get(&id).copied().unwrap_or_default();
    let pos = fader_pos(volume);
    let selected = if mixer { ws.channel == ch } else { false };
    let slot_h = 22.;

    // The instrument, or what feeds the strip.
    let source = match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            let dot = Some(color);
            match &t.instrument {
                None => {
                    let input = t.input.unwrap_or(Input {
                        channel: 0,
                        stereo: true,
                    });
                    dotted(input.name(), true, 24., dot)
                        .debug_selector(move || format!("input-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(ws.input_menu(i, e.position));
                                cx.notify();
                            }),
                        )
                }
                Some(Instrument::Synth) => plugin_slot(
                    "Sine Synth".into(),
                    true,
                    ch,
                    Which::Instrument,
                    24.,
                    dot,
                    cx,
                ),
                Some(Instrument::Sampler(_)) => {
                    plugin_slot("Sampler".into(), true, ch, Which::Instrument, 24., dot, cx)
                }
                Some(Instrument::Drums) => {
                    plugin_slot("Drum Kit".into(), true, ch, Which::Instrument, 24., dot, cx)
                }
                Some(Instrument::Plugin(r)) => {
                    plugin_slot(r.class.clone(), true, ch, Which::Instrument, 24., dot, cx)
                }
            }
        }
        Channel::Aux(i) => info(format!("Bus {}", project.auxes[i].bus), 24.),
        Channel::Master => info("Output 1–2", 24.),
    };

    // Effect slots: the built-ins it has, its plugins, and an empty one to add another.
    let (eq, comp) = ws.strip_dsp(ch);
    let mut effects = Vec::new();
    if eq.is_some() {
        effects.push(builtin_slot(
            "Channel EQ",
            ch,
            Builtin::Eq,
            slot_h,
            if eq.is_some_and(|e| e.bypass) {
                TEXT_4
            } else {
                color
            },
            cx,
        ));
    }
    if comp.is_some() {
        effects.push(builtin_slot(
            "Compressor",
            ch,
            Builtin::Comp,
            slot_h,
            if comp.is_some_and(|c| c.bypass) {
                TEXT_4
            } else {
                color
            },
            cx,
        ));
    }
    if let Some(sat) = ws.sat(ch) {
        let color = if sat.bypass { TEXT_4 } else { color };
        effects.push(builtin_slot(
            "Saturator",
            ch,
            Builtin::Sat,
            slot_h,
            color,
            cx,
        ));
    }
    let inserts = ws.inserts(ch);
    for (n, r) in inserts.iter().enumerate() {
        effects.push(plugin_slot(
            r.class.clone(),
            true,
            ch,
            Which::Insert(n),
            slot_h,
            Some(TEXT_4),
            cx,
        ));
    }
    {
        effects.push(plugin_slot(
            String::new(),
            false,
            ch,
            Which::Insert(inserts.len()),
            slot_h,
            None,
            cx,
        ));
    }
    let (fit_effects, fit_sends) = match layout {
        Layout::Mixer { .. } => (MIXER_INSERTS, MIXER_SENDS),
        Layout::Compact { effects, sends } => (effects, sends),
    };
    if mixer {
        effects.truncate(MIXER_INSERTS);
    }
    while effects.len() < fit_effects {
        effects.push(blank(slot_h));
    }

    // Sends, for tracks.
    let mut sends = Vec::new();
    if let Channel::Track(track) = ch {
        let list = &project.tracks[track].sends;
        for (index, s) in list.iter().enumerate() {
            let (level, pre) = (s.level_db, s.pre);
            let bus = project
                .auxes
                .iter()
                .find(|a| a.bus == s.bus)
                .map_or(format!("Bus {}", s.bus), |a| a.name.clone());
            // Before the fader: amber, its dot and its knob.
            let tone = if pre { AMBER } else { TEXT };
            sends.push(
                dotted(bus, true, slot_h, pre.then_some(AMBER))
                    .justify_between()
                    .pr(px(3.))
                    .cursor_ns_resize()
                    .child(knob(fader_pos(level), 16., tone, false))
                    .debug_selector(move || format!("send-{track}-{index}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| {
                                    ws.session.project.tracks[track].sends[index].level_db = 0.0
                                });
                            } else {
                                let y = f32::from(e.position.y);
                                ws.begin_drag(
                                    Drag::Send {
                                        track,
                                        index,
                                        y,
                                        level,
                                    },
                                    cx,
                                );
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(ws.send_options(track, index, e.position));
                            cx.notify();
                        }),
                    ),
            );
        }
        if !mixer || sends.len() < MIXER_SENDS {
            sends.push(
                slot("Send", false, slot_h)
                    .justify_center()
                    .debug_selector(move || format!("add-send-{track}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(ws.send_menu(track, e.position));
                            cx.notify();
                        }),
                    ),
            );
        }
    }
    if mixer {
        sends.truncate(MIXER_SENDS);
    }
    while sends.len() < fit_sends {
        sends.push(blank(slot_h));
    }

    // Where it plays, in the mixer.
    let output = match ch {
        Channel::Track(track) => {
            let bus = project.tracks[track].output;
            let to = bus.and_then(|b| project.auxes.iter().find(|a| a.bus == b));
            let label = to.map_or("Stereo Out".to_string(), |a| a.name.clone());
            pill(label)
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(LINE_STRONG)))
                .debug_selector(move || format!("mixer-output-{track}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(ws.output_menu(track, e.position));
                        cx.notify();
                    }),
                )
        }
        Channel::Aux(_) => info("Stereo Out", 22.).justify_center().rounded(px(11.)),
        Channel::Master => info("Out 1–2", 22.).justify_center().rounded(px(11.)),
    };

    let pan_row = div()
        .flex_none()
        .h(px(if mixer { 36. } else { 28. }))
        .flex()
        .items_center()
        .justify_between()
        .px(px(2.))
        .child(match pan {
            Some(pan) => div()
                .cursor_ns_resize()
                .child(knob(pan, if mixer { 32. } else { 26. }, TEXT, true))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.modifiers.alt {
                            ws.edit(cx, |ws| ws.set_pan(ch, 0.0));
                        } else {
                            let y = f32::from(e.position.y);
                            ws.begin_drag(
                                Drag::Pan {
                                    channel: ch,
                                    x: f32::from(e.position.x),
                                    y,
                                    pan,
                                },
                                cx,
                            );
                        }
                    }),
                )
                .into_any_element(),
            None => div().into_any_element(),
        })
        .child(
            div()
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(match pan {
                    Some(p) if p.abs() < 0.005 => "C".to_string(),
                    Some(p) => format!("{}{:.0}", if p < 0.0 { "L" } else { "R" }, p.abs() * 64.0),
                    None => String::new(),
                }),
        );

    let peak_db = 20.0 * level.held[0].max(level.held[1]).max(1e-9).log10();
    let readout = |text: String, color: u32| {
        div()
            .flex_1()
            .h(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(rgb(WELL))
            .font_family(MONO)
            .text_size(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(text)
    };
    let readouts = div()
        .flex_none()
        .flex()
        .gap(px(4.))
        .child(readout(db_text(volume), TEXT_2))
        .child(readout(
            db_text(if peak_db < -100.0 { SILENT } else { peak_db }),
            if peak_db > 0.0 {
                METER_HIGH
            } else if peak_db > -6.0 {
                METER_MID
            } else {
                TEXT_2
            },
        ));

    let fader_h = layout.fader_h();
    let (track_x, cap_w, scale_x, meter_x) = if mixer {
        (13.0, 30.0, 31.0, 54.0)
    } else {
        (38.0, 26.0, 0.0, 66.0)
    };
    let usable = fader_h - 16.0;
    let at = |p: f32| 8.0 + (1.0 - p) * usable;
    let labels = [6.0f32, 0.0, -6.0, -12.0, -24.0, -40.0].map(|db| {
        div()
            .absolute()
            .left(px(scale_x))
            .top(px(at(fader_pos(db)) - 5.0))
            .w(px(20.))
            .when(mixer, |d| d.flex().justify_center())
            .when(!mixer, |d| d.flex().justify_end())
            .font_family(MONO)
            .text_size(px(9.))
            .line_height(px(10.))
            .text_color(rgb(TEXT_3))
            .child(if db > 0.0 {
                format!("+{db:.0}")
            } else {
                format!("{:.0}", db.abs())
            })
    });
    let fader = div()
        .flex_none()
        .relative()
        .h(px(fader_h))
        .cursor_ns_resize()
        .debug_selector(move || format!("fader-{ch:?}"))
        .child(
            div()
                .absolute()
                .left(px(track_x))
                .top(px(8.))
                .w(px(4.))
                .h(px(usable))
                .rounded(px(2.))
                .bg(rgb(0x0a0a0d)),
        )
        .children(labels)
        .child(
            div()
                .absolute()
                .left(px(meter_x))
                .top(px(8.))
                .h(px(usable))
                .child(meter(
                    level.now,
                    level.held,
                    if mixer { 5. } else { 4. },
                    2.,
                )),
        )
        .child(
            div()
                .absolute()
                .left(px(track_x + 2.0 - cap_w / 2.0))
                .top(px(at(pos) - if mixer { 10. } else { 7. }))
                .w(px(cap_w))
                .h(px(if mixer { 20. } else { 14. }))
                .rounded(px(if mixer { 5. } else { 4. }))
                .bg(rgb(0x3a3a44))
                .border_1()
                .border_color(rgb(0x4e4e5a))
                .shadow_md()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2.))
                .when(mixer, |d| {
                    d.child(div().w(px(18.)).h(px(1.)).bg(rgb(TEXT_3)))
                })
                .child(
                    div()
                        .w(px(if mixer { 18. } else { 16. }))
                        .h(px(1.))
                        .bg(rgb(TEXT)),
                )
                .when(mixer, |d| {
                    d.child(div().w(px(18.)).h(px(1.)).bg(rgb(TEXT_3)))
                }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if e.modifiers.alt {
                    ws.edit(cx, |ws| ws.set_volume(ch, 0.0));
                } else {
                    let y = f32::from(e.position.y);
                    ws.begin_drag(
                        Drag::Fader {
                            channel: ch,
                            y,
                            pos,
                            height: usable,
                        },
                        cx,
                    );
                }
            }),
        );

    let buttons = div()
        .flex_none()
        .h(px(22.))
        .flex()
        .gap(px(3.))
        .when_some(mute, |d, on| {
            d.child(
                toggle("M", on, TEXT, BG, 22.)
                    .flex_1()
                    .debug_selector(move || format!("strip-mute-{ch:?}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| match ch {
                                Channel::Track(i) => ws.session.project.tracks[i].mute ^= true,
                                Channel::Aux(i) => ws.session.project.auxes[i].mute ^= true,
                                Channel::Master => {}
                            });
                        }),
                    ),
            )
        })
        .when_some(
            solo.zip(if let Channel::Track(i) = ch {
                Some(i)
            } else {
                None
            }),
            |d, (on, i)| {
                d.child(toggle("S", on, SOLO, BG, 22.).flex_1().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                    }),
                ))
            },
        )
        .when(mixer, |d| {
            d.when_some(
                if let Channel::Track(i) = ch {
                    Some(i)
                } else {
                    None
                },
                |d, i| {
                    d.child(
                        toggle("R", ws.armed() == Some(i), RECORD, 0xffffff, 22.)
                            .flex_1()
                            .debug_selector(move || format!("strip-arm-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.toggle_arm(i);
                                    cx.notify();
                                }),
                            ),
                    )
                },
            )
        });

    let plate = div()
        .flex_none()
        .h(px(if mixer { 28. } else { 22. }))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(if mixer { 7. } else { 6. }))
        .text_size(px(if mixer { 12. } else { 11. }))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .overflow_hidden()
        .bg(if selected {
            rgb(color).into()
        } else {
            alpha(color, 0.14)
        })
        .text_color(rgb(if selected { BG } else { tint(color, 0.5) }));
    // Double-click a track's or bus's name to rename it.
    let naming = match ch {
        Channel::Track(i) => Some(Name::Track(i)),
        Channel::Aux(i) => Some(Name::Aux(i)),
        Channel::Master => None,
    };
    let plate = match naming.and_then(|n| ws.name_field(n)) {
        Some(field) => plate.child(field),
        None => plate.child(name).when_some(naming, |d, what| {
            d.debug_selector(move || format!("plate-{ch:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        if e.click_count == 2 {
                            cx.stop_propagation();
                            ws.start_naming(what, window, cx);
                        }
                    }),
                )
        }),
    };

    let reduction = ws.reductions.get(&id).copied().unwrap_or(0.0);
    let header = mixer.then(|| {
        let (i, number) = match ch {
            Channel::Track(i) => (
                crate::looks::glyph(ws.shown(i).look.icon, 14.),
                format!("{}", i + 1),
            ),
            Channel::Aux(i) => (icon(Icon::Bus, 14.), format!("B{}", i + 1)),
            Channel::Master => (icon(Icon::Output, 14.), String::new()),
        };
        div()
            .flex_none()
            .h(px(28.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .text_color(rgb(color))
                    .child(i.text_color(rgb(color))),
            )
            .when(reduction > 0.3, |d| {
                d.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(METER_MID))
                        .child(format!("GR {:.0}", reduction)),
                )
            })
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(number),
            )
    });

    div()
        .debug_selector(move || format!("strip-{ch:?}"))
        .when(mixer, |d| d.w(px(90.)).flex_none())
        .when(!mixer, |d| d.flex_1().min_w(px(0.)))
        .flex()
        .flex_col()
        .gap(px(if mixer { 8. } else { 6. }))
        .p(px(if mixer { 8. } else { 7. }))
        .rounded(px(if mixer { 12. } else { 10. }))
        .border_1()
        .bg(rgb(if selected {
            0x1c1c23
        } else if mixer {
            0x141418
        } else {
            STRIP
        }))
        .border_color(rgb(if selected { 0x3a3a44 } else { 0x1e1e24 }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                ws.channel = ch;
                if let Channel::Track(i) = ch {
                    ws.track = i;
                }
                cx.notify();
            }),
        )
        .children(header)
        .when(rows[0], |d| {
            d.child(eq_thumbnail(
                ws,
                ch,
                if mixer { 44. } else { 34. },
                color,
                cx,
            ))
        })
        .child(source)
        .when(rows[1], |d| {
            d.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(effects),
            )
        })
        .when(rows[2] && (mixer || !sends.is_empty()), |d| {
            d.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(sends),
            )
        })
        .when(mixer, |d| d.child(output))
        .child(pan_row)
        .child(readouts)
        .child(fader)
        .child(buttons)
        .child(plate)
}

/// A rounded outline around a little text: where a strip plays.
fn pill(label: String) -> Div {
    div()
        .flex_none()
        .h(px(22.))
        .px(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(11.))
        .border_1()
        .border_color(rgb(RAISED))
        .text_size(px(10.5))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_taper_round_trips() {
        for db in [-60.0, -12.0, 0.0, 6.0] {
            assert!((fader_db(fader_pos(db)) - db).abs() < 1e-3);
        }
        assert!((fader_pos(0.0) - 0.708).abs() < 1e-3);
        assert_eq!(fader_db(0.0), SILENT);
    }

    #[test]
    fn knob_scales_round_trip() {
        assert_eq!(low_cut_hz(low_cut_pos(80.0)), 80.0);
        assert_eq!(low_cut_hz(0.0), 0.0);
        assert_eq!(eq_gain_db(eq_gain_pos(3.5)), 3.5);
        let mut c = Comp::default();
        for field in [
            CompField::Threshold,
            CompField::Ratio,
            CompField::Attack,
            CompField::Release,
            CompField::Makeup,
        ] {
            let before = comp_text(field, &c);
            let v = comp_pos(field, &c);
            comp_set(field, &mut c, v);
            assert_eq!(comp_text(field, &c), before, "{field:?}");
        }
    }
}
