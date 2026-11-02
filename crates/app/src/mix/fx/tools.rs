//! The tools' windows, one shape for all: what it does along the top, a display you can drag,
//! and its knobs from its table. The filter draws its curve and how far it moves, its cutoff and
//! resonance on one point, its LFO beside it. Motion draws its LFOs, rate and depth on one point,
//! and a vectorscope of what it plays. The shaper draws a hit as it comes and as it leaves, its
//! front and tail to drag, what's going through beside it. The gate draws what it hears and the
//! threshold across it, its curve beside it. The de-esser draws the sound's spectrum, the band it
//! hears and how far it's taking that down. The stereo tool draws each band's width to drag, its
//! splits, and a vectorscope with how alike the sides are. The multiband draws its bands over the
//! spectrum, a threshold to drag on each, and what each is taking off. The pitch shifter draws its
//! interval on a ruler; the tape stop its speed falling and rising; pitch correction what's sung
//! and what comes out over the notes, with a keyboard for which it may land on.

use pina_engine::{
    dsp::{Clock, Running, tools::Shaping},
    music::Key,
    song::project::{
        DeEsser, FilterFx, FilterMode, Motion, MotionStyle, PitchShift, Shaper, Stereo, TapeStop,
        Tune, Wave,
    },
};

use super::comp::side_row;
use super::space::{
    DISPLAY_H, FREQ_W, HIGH, LOW, MIDDLE, SpaceHandle, TIME_W, chip_tag, freq_grid, handle,
    spectrum_fill,
};
use super::*;
use crate::{
    editors::sampler::response,
    mix::strip::{KnobScale, knobs},
    workspace::Knob,
};

const LFO_RATE: KnobScale = KnobScale::Log(0.02, 20.0);
/// In time with the song: a cycle in beats, as notes so any time signature reads right.
const SYNC: [(&str, f32); 7] = [
    ("Free", 0.0),
    ("1/8", 0.5),
    ("1/4", 1.0),
    ("1/2", 2.0),
    ("1/1", 4.0),
    ("2/1", 8.0),
    ("4/1", 16.0),
];
/// How high an LFO's crest goes up its picture at full depth, of the height.
const CREST: f32 = 0.46;
/// How long a tape takes to stop or start, beats.
const TAPE_TIME: KnobScale = KnobScale::Log(0.0625, 4.0);
/// How far a hit runs across the shaper's picture, seconds, and the rate it's worked out at.
const HIT_S: f32 = 0.7;
const HIT_RATE: f32 = 4000.0;

impl Workspace {
    /// One tool's window: its choices along the top, its display, its knobs in rows.
    pub(super) fn tool_window(
        &self,
        ch: Channel,
        u: Unit,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(k) = self.fx_stacked() else {
            return Vec::new();
        };
        let kind = u.kind();
        let on = !u.bypassed();
        let (middle, right, foot) = self.tool_choices(ch, &u, cx);
        let b = Builtin::Tool(kind);
        let head = self.fx_head(b.name(), ch, b, on, middle, right, cx);
        let display = div()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .flex()
            .gap(px(8.))
            .child(self.tool_left(&u, cx))
            .child(self.tool_right(&u, cx));
        let table = knobs(kind);
        // The multiband shows the band in hand: four of its knobs and its four for them all.
        let band = self.fx_view.as_ref().map_or(0, |v| v.band);
        let shown: Vec<(usize, &'static str)> = match u {
            Unit::Multiband(_) => [
                (band, "Threshold"),
                (3 + band, "Ratio"),
                (6 + band, "Lift"),
                (9 + band, "Gain"),
                (12, "Attack"),
                (13, "Release"),
                (14, "Mix"),
                (15, "Output"),
            ]
            .to_vec(),
            _ => table.iter().enumerate().map(|(i, p)| (i, p.name)).collect(),
        };
        let dial = |ws: &Self, at: usize, size: f32, cx: &mut Context<Self>| {
            let (i, name) = shown[at];
            let p = &table[i];
            ws.fx_dial(
                Target::Stacked(ch, k, Knob::Param(i)),
                name,
                (p.say)(&u),
                size,
                if on { MIDDLE } else { TEXT_4 },
                p.bipolar,
                cx,
            )
        };
        let n = shown.len();
        let (split, size) = match n {
            0..=6 => (n, 58.0),
            _ => (n.div_ceil(2), 46.0),
        };
        let row = |ws: &Self, from: usize, to: usize, cx: &mut Context<Self>| {
            div().flex().justify_between().children(
                (from..to)
                    .map(|i| dial(ws, i, size, cx))
                    .collect::<Vec<_>>(),
            )
        };
        let knobs = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(row(self, 0, split, cx))
            .when(split < n, |d| d.child(row(self, split, n, cx)));
        let controls = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(22.))
            .px(px(26.))
            .child(knobs)
            .child(div().w(px(1.)).h(px(140.)).bg(rgb(LINE)))
            .child(foot.w(px(306.)).flex_none().flex().flex_col().gap(px(10.)));
        vec![
            head.into_any_element(),
            display.into_any_element(),
            controls.into_any_element(),
        ]
    }

    /// A tool's choices: beside its name, at the title's end, and beside its knobs.
    fn tool_choices(&self, ch: Channel, u: &Unit, cx: &mut Context<Self>) -> (Div, Div, Div) {
        let line = |text: &'static str| {
            div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_4))
                .child(text)
        };
        match *u {
            Unit::Filter(f) => {
                const MODES: [FilterMode; 7] = [
                    FilterMode::LowPass,
                    FilterMode::HighPass,
                    FilterMode::BandPass,
                    FilterMode::Notch,
                    FilterMode::Ladder,
                    FilterMode::Comb,
                    FilterMode::Formant,
                ];
                let mode = choice_tray(
                    "filter-mode",
                    &MODES.map(|m| (m.name(), m)),
                    f.mode,
                    26.,
                    cx,
                    move |ws, m| {
                        if let Some(f) = ws.fx_unit_mut::<FilterFx>(ch) {
                            f.mode = m;
                        }
                    },
                );
                let wave = choice_tray(
                    "filter-wave",
                    &[
                        ("Sine", Wave::Sine),
                        ("Tri", Wave::Triangle),
                        ("Saw", Wave::SawUp),
                        ("Square", Wave::Square),
                        ("Rand", Wave::Random),
                    ],
                    f.lfo_wave,
                    22.,
                    cx,
                    move |ws, w| {
                        if let Some(f) = ws.fx_unit_mut::<FilterFx>(ch) {
                            f.lfo_wave = w;
                        }
                    },
                );
                let sync = choice_tray("filter-sync", &SYNC, f.lfo_beats, 22., cx, move |ws, b| {
                    if let Some(f) = ws.fx_unit_mut::<FilterFx>(ch) {
                        f.lfo_beats = b;
                    }
                });
                (
                    mode,
                    div(),
                    div()
                        .child(wave)
                        .child(sync)
                        .child(line("Drag: cutoff across, resonance up; LFO rate, depth")),
                )
            }
            Unit::Motion(m) => {
                let style = choice_tray(
                    "motion-style",
                    &MotionStyle::ALL.map(|s| (s.name(), s)),
                    m.style,
                    26.,
                    cx,
                    move |ws, s| {
                        if let Some(m) = ws.fx_unit_mut::<Motion>(ch)
                            && m.style != s
                        {
                            let (mix, out) = (m.mix, m.output_db);
                            let fresh = s.motion();
                            // A tremolo or vibrato is all wet; the others keep what they had.
                            let keep = !matches!(s, MotionStyle::Tremolo | MotionStyle::Vibrato);
                            *m = Motion {
                                mix: if keep { mix } else { fresh.mix },
                                output_db: out,
                                ..fresh
                            };
                        }
                    },
                );
                let sync = choice_tray("motion-sync", &SYNC, m.beats, 22., cx, move |ws, b| {
                    if let Some(m) = ws.fx_unit_mut::<Motion>(ch) {
                        m.beats = b;
                    }
                });
                (
                    style,
                    div(),
                    div()
                        .child(sync)
                        .child(line("Drag the point: rate across, depth up")),
                )
            }
            Unit::Shaper(s) => {
                let clip = choice_tray(
                    "shaper-clip",
                    &[("Off", false), ("Soft clip", true)],
                    s.clip,
                    22.,
                    cx,
                    move |ws, c| {
                        if let Some(s) = ws.fx_unit_mut::<Shaper>(ch) {
                            s.clip = c;
                        }
                    },
                );
                (
                    div(),
                    div(),
                    div()
                        .child(side_row("Over full scale", clip))
                        .child(line("Drag the front and the tail up or down; the picture is a real hit through it")),
                )
            }
            Unit::Gate(_) => (
                div(),
                div(),
                div().child(line("Drag the threshold line up or down")),
            ),
            Unit::DeEsser(d) => {
                let how = choice_tray(
                    "de-esser-mode",
                    &[("Band", false), ("Whole sound", true)],
                    d.wide,
                    22.,
                    cx,
                    move |ws, w| {
                        if let Some(d) = ws.fx_unit_mut::<DeEsser>(ch) {
                            d.wide = w;
                        }
                    },
                );
                let listen = choice_tray(
                    "de-esser-listen",
                    &[("Listen", true)],
                    d.listen,
                    26.,
                    cx,
                    move |ws, _| {
                        if let Some(d) = ws.fx_unit_mut::<DeEsser>(ch) {
                            d.listen ^= true;
                        }
                    },
                );
                (
                    div(),
                    listen,
                    div()
                        .child(side_row("Turns down", how))
                        .child(line("Drag the point: frequency across, threshold up")),
                )
            }
            Unit::Stereo(s) => {
                let flip = |name: &'static str,
                            sel: &'static str,
                            on: bool,
                            set: fn(&mut Stereo),
                            cx: &mut Context<Self>| {
                    side_row(
                        name,
                        choice_tray(
                            sel,
                            &[("Off", false), ("On", true)],
                            on,
                            22.,
                            cx,
                            move |ws, v| {
                                if let Some(s) = ws.fx_unit_mut::<Stereo>(ch)
                                    && v != on
                                {
                                    set(s);
                                }
                            },
                        ),
                    )
                };
                (
                    div(),
                    div(),
                    div()
                        .child(flip(
                            "Swap sides",
                            "stereo-swap",
                            s.swap,
                            |s| s.swap ^= true,
                            cx,
                        ))
                        .child(flip(
                            "Flip left",
                            "stereo-flip-left",
                            s.invert_left,
                            |s| s.invert_left ^= true,
                            cx,
                        ))
                        .child(flip(
                            "Flip right",
                            "stereo-flip-right",
                            s.invert_right,
                            |s| s.invert_right ^= true,
                            cx,
                        )),
                )
            }
            Unit::Multiband(_) => {
                let band = self.fx_view.as_ref().map_or(0, |v| v.band);
                let tabs = choice_tray(
                    "multiband-band",
                    &[("Low", 0), ("Middle", 1), ("High", 2)],
                    band,
                    26.,
                    cx,
                    |ws, b| {
                        if let Some(v) = ws.fx_view.as_mut() {
                            v.band = b;
                        }
                    },
                );
                (
                    tabs,
                    div(),
                    div().child(line(
                        "Drag each band's bar for its threshold, the points under them for where \
                         it splits. Lift brings the quiet parts up.",
                    )),
                )
            }
            Unit::Pitch(p) => {
                let leap = choice_tray(
                    "pitch-leap",
                    &[
                        ("-12", -12.0),
                        ("-7", -7.0),
                        ("-5", -5.0),
                        ("0", 0.0),
                        ("+5", 5.0),
                        ("+7", 7.0),
                        ("+12", 12.0),
                    ],
                    p.semitones,
                    26.,
                    cx,
                    move |ws, st| {
                        if let Some(p) = ws.fx_unit_mut::<PitchShift>(ch) {
                            p.semitones = st;
                        }
                    },
                );
                (
                    leap,
                    div(),
                    div().child(line(
                        "Drag along the ruler. Feedback plays it back into itself, moved again \
                         each round.",
                    )),
                )
            }
            Unit::TapeStop(t) => {
                let stop = choice_tray(
                    "tape-stop",
                    &[("Running", false), ("Stopped", true)],
                    t.stop > 0.5,
                    26.,
                    cx,
                    move |ws, stopped| {
                        if let Some(t) = ws.fx_unit_mut::<TapeStop>(ch) {
                            t.stop = stopped as u8 as f32;
                        }
                    },
                );
                (
                    stop,
                    div(),
                    div().child(line(
                        "Throw it with automation on Stop. Drag: how long it takes each way, \
                         and how it falls.",
                    )),
                )
            }
            Unit::Tune(t) => {
                let song = self.song_notes();
                let now = match (t.notes == [true; 12], Some(t.notes) == song) {
                    (true, _) => 0,
                    (_, true) => 1,
                    _ => 2,
                };
                let notes = choice_tray(
                    "tune-notes",
                    &[("Every note", 0), ("The song's key", 1)],
                    now,
                    26.,
                    cx,
                    move |ws, pick| {
                        let notes = match pick {
                            0 => Some([true; 12]),
                            _ => ws.song_notes(),
                        };
                        if let (Some(notes), Some(t)) = (notes, ws.fx_unit_mut::<Tune>(ch)) {
                            t.notes = notes;
                        }
                    },
                );
                (
                    notes,
                    div(),
                    div().child(line(
                        "Click the keys it may land on. Speed at nothing snaps, the robot; \
                         40 ms and up stays human.",
                    )),
                )
            }
            _ => (div(), div(), div()),
        }
    }

    /// The notes of the song's key, if it has one.
    pub(crate) fn song_notes(&self) -> Option<[bool; 12]> {
        let key = Key::parse(self.session.project.key.as_deref()?)?;
        Some(std::array::from_fn(|n| key.contains(n as u8)))
    }

    /// A point on a tool's display dragged: what it sets, set. `a` and `b` are where it is on
    /// the left and right pictures, 0 to 1.
    pub(super) fn drag_tool(
        &mut self,
        ch: Channel,
        i: u8,
        (ax, ay): (f32, f32),
        (bx, by): (f32, f32),
    ) {
        let Some(k) = self.fx_stacked() else { return };
        let Some(u) = self.chain_mut(ch).more.get_mut(k) else {
            return;
        };
        match (u, i) {
            (Unit::Filter(f), 0) => {
                f.cutoff_hz = x_freq(ax).round();
                f.resonance = ((1.0 - ay) * 100.0).round() / 100.0;
            }
            (Unit::Filter(f), _) => {
                f.lfo_hz = LFO_RATE.value(bx);
                f.lfo_depth = ((0.5 - by) / CREST * 60.0).round().clamp(0.0, 60.0) / 10.0;
            }
            (Unit::Motion(m), _) => {
                m.rate_hz = LFO_RATE.value(ax);
                m.depth = ((0.5 - ay) / CREST * 100.0).round().clamp(0.0, 100.0) / 100.0;
            }
            (Unit::Shaper(s), 0) => s.attack = ((1.0 - 2.0 * ay) * 100.0).round() / 100.0,
            (Unit::Shaper(s), _) => s.sustain = ((1.0 - 2.0 * ay) * 100.0).round() / 100.0,
            (Unit::Gate(g), _) => g.threshold_db = (ay * LEVEL_FLOOR).round(),
            (Unit::DeEsser(d), _) => {
                d.freq_hz = x_freq(ax).round().clamp(1000.0, 16_000.0);
                d.threshold_db = (ay * LEVEL_FLOOR).round().max(-70.0);
            }
            (Unit::Stereo(s), 0) => s.crossovers[0] = x_freq(ax).round().min(s.crossovers[1]),
            (Unit::Stereo(s), 1) => s.crossovers[1] = x_freq(ax).round().max(s.crossovers[0]),
            (Unit::Multiband(m), 0) => {
                m.crossovers[0] = x_freq(ax).round().min(m.crossovers[1]);
            }
            (Unit::Multiband(m), 1) => {
                m.crossovers[1] = x_freq(ax).round().max(m.crossovers[0]);
            }
            (Unit::Multiband(m), n) => {
                m.threshold_db[(n as usize - 2).min(2)] = (ay * LEVEL_FLOOR).round().max(-60.0);
            }
            (Unit::Pitch(p), _) => p.semitones = (ax * 48.0 - 24.0).round(),
            (Unit::TapeStop(t), 0) => t.stop_beats = TAPE_TIME.value(1.0 - ax * 2.0),
            (Unit::TapeStop(t), 1) => t.start_beats = TAPE_TIME.value(ax * 2.0 - 1.0),
            (Unit::TapeStop(t), _) => t.curve = (ay * 100.0).round() / 100.0,
            (Unit::Stereo(s), 5) => {
                s.mono_hz = if ax < 0.02 {
                    0.0
                } else {
                    x_freq(ax).round().min(500.0)
                };
            }
            (Unit::Stereo(s), n) => {
                s.width[(n as usize - 2).min(2)] = ((1.0 - ay) * 200.0).round() / 100.0;
            }
            _ => {}
        }
    }

    /// A tool's main display.
    fn tool_left(&self, u: &Unit, cx: &mut Context<Self>) -> Div {
        let v = self.fx_view.as_ref().expect("open");
        let plot = v.plot.clone();
        let heard = (v.spectrum.clone(), v.columns.clone());
        let levels: Vec<[f32; 4]> = v.history.iter().copied().collect();
        let picture = well().h_full().w(px(TIME_W)).flex_none();
        match *u {
            Unit::Filter(f) => {
                let curve = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        freq_grid(window, b);
                        spectrum_fill(window, b, &heard, 1.0);
                        let reach = f.lfo_depth + f.env_depth.abs();
                        let line = |window: &mut gpui::Window, hz: f32, c: gpui::Hsla, w: f32| {
                            let (bw, h) = (f32::from(b.size.width), f32::from(b.size.height));
                            let mut p = PathBuilder::stroke(px(w));
                            for i in 0..160 {
                                let x = i as f32 / 159.0;
                                let db = response(f.mode, hz, f.resonance, x_freq(x));
                                // 0 dB a third of the way down, 60 under it at the foot.
                                let y = (h * 0.33 - db * h * 0.67 / 60.0).clamp(0.0, h);
                                let at = point(b.origin.x + px(x * bw), b.origin.y + px(y));
                                match i {
                                    0 => p.move_to(at),
                                    _ => p.line_to(at),
                                }
                            }
                            if let Ok(p) = p.build() {
                                window.paint_path(p, c);
                            }
                        };
                        // Where its LFO and the sound's level can take it, faint.
                        if reach > 0.0 {
                            line(
                                window,
                                f.cutoff_hz * 2f32.powf(reach),
                                alpha(HIGH, 0.45),
                                1.0,
                            );
                            line(
                                window,
                                f.cutoff_hz / 2f32.powf(reach),
                                alpha(HIGH, 0.45),
                                1.0,
                            );
                        }
                        line(window, f.cutoff_hz, alpha(MIDDLE, 0.95), 2.0);
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "filter-display".into())
                    .child(curve)
                    .child(handle(
                        freq_x(f.cutoff_hz) * TIME_W,
                        (1.0 - f.resonance) * DISPLAY_H,
                        MIDDLE,
                        "filter-handle",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
            }
            Unit::Motion(m) => {
                let curve = lfo_picture(plot, Wave::Sine, m.depth, m.spread);
                picture
                    .debug_selector(|| "motion-display".into())
                    .child(curve)
                    .child(chip_tag(
                        8.0,
                        8.0,
                        format!(
                            "{} · {:.2} s a cycle",
                            m.style.name(),
                            1.0 / m.rate_hz.max(0.01)
                        ),
                        MIDDLE,
                    ))
                    .child(handle(
                        LFO_RATE.pos(m.rate_hz) * TIME_W,
                        (0.5 - m.depth * CREST) * DISPLAY_H,
                        MIDDLE,
                        "motion-handle",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
            }
            Unit::Shaper(s) => {
                let hit = canvas(
                    move |_, _, _| shaper_hit(s),
                    move |b, [dry, shaped], window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let at = |i: usize, n: usize, level: f32, side: f32| {
                            point(
                                b.origin.x + px(i as f32 / (n - 1) as f32 * w),
                                b.origin.y + px(h * 0.5 * (1.0 - side * level.min(1.0))),
                            )
                        };
                        let n = shaped.len();
                        let mut area = PathBuilder::fill();
                        area.move_to(at(0, n, shaped[0], 1.0));
                        (1..n).for_each(|i| area.line_to(at(i, n, shaped[i], 1.0)));
                        (0..n)
                            .rev()
                            .for_each(|i| area.line_to(at(i, n, shaped[i], -1.0)));
                        area.close();
                        if let Ok(p) = area.build() {
                            window.paint_path(p, alpha(MIDDLE, 0.32));
                        }
                        // Each side's edge, the shaped one over the one that came in.
                        for (x, c, width) in [
                            (&dry, alpha(0xffffff, 0.45), 1.0),
                            (&shaped, alpha(MIDDLE, 0.95), 1.6),
                        ] {
                            for side in [1.0, -1.0] {
                                let mut p = PathBuilder::stroke(px(width));
                                p.move_to(at(0, n, x[0], side));
                                (1..n).for_each(|i| p.line_to(at(i, n, x[i], side)));
                                if let Ok(p) = p.build() {
                                    window.paint_path(p, c);
                                }
                            }
                        }
                        // The two points' tracks; the middle of one is no change.
                        for (x, c) in [(0.08, HIGH), (0.6, LOW)] {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x + px(x * w), b.origin.y),
                                    size(px(1.), px(h)),
                                ),
                                alpha(c, 0.3),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "shaper-display".into())
                    .child(hit)
                    .child(chip_tag(TIME_W - 190.0, 8.0, "As it comes".into(), TEXT_2))
                    .child(chip_tag(TIME_W - 100.0, 8.0, "As it leaves".into(), MIDDLE))
                    .child(handle(
                        TIME_W * 0.08,
                        (1.0 - s.attack) * 0.5 * DISPLAY_H,
                        HIGH,
                        "shaper-front",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
                    .child(handle(
                        TIME_W * 0.6,
                        (1.0 - s.sustain) * 0.5 * DISPLAY_H,
                        LOW,
                        "shaper-tail",
                        SpaceHandle::Tool(1),
                        0.0,
                        cx,
                    ))
            }
            Unit::Gate(g) => {
                let threshold = g.threshold_db;
                let history = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        levels_picture(window, b, &levels);
                        let h = f32::from(b.size.height);
                        let y = (threshold / LEVEL_FLOOR).clamp(0.0, 1.0) * h;
                        dashed(window, b, y, f32::from(b.size.width), alpha(HIGH, 0.9));
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "gate-display".into())
                    .child(history)
                    .child(handle(
                        TIME_W - 24.0,
                        (g.threshold_db / LEVEL_FLOOR).clamp(0.0, 1.0) * DISPLAY_H,
                        HIGH,
                        "gate-threshold",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
                    .child(chip_tag(
                        8.0,
                        8.0,
                        format!("Opens at {:.0} dB", g.threshold_db),
                        HIGH,
                    ))
            }
            Unit::DeEsser(d) => {
                let spectrum = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        freq_grid(window, b);
                        let half = 2f32.powf(d.width / 2.0);
                        let (lo, hi) = (freq_x(d.freq_hz / half) * w, freq_x(d.freq_hz * half) * w);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(b.origin.x + px(lo), b.origin.y),
                                size(px((hi - lo).max(1.0)), px(h)),
                            ),
                            alpha(HIGH, 0.12),
                        ));
                        spectrum_fill(window, b, &heard, 2.0);
                        let y = (d.threshold_db / LEVEL_FLOOR).clamp(0.0, 1.0) * h;
                        dashed(window, b, y, w, alpha(HIGH, 0.8));
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "de-esser-display".into())
                    .child(spectrum)
                    .child(handle(
                        freq_x(d.freq_hz) * TIME_W,
                        (d.threshold_db / LEVEL_FLOOR).clamp(0.0, 1.0) * DISPLAY_H,
                        HIGH,
                        "de-esser-handle",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
            }
            Unit::Stereo(s) => {
                let bands = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        freq_grid(window, b);
                        let cuts = [0.0, freq_x(s.crossovers[0]), freq_x(s.crossovers[1]), 1.0];
                        for k in 0..3 {
                            let tall = s.width[k] / 2.0 * h;
                            let (x0, x1) = (cuts[k] * w, cuts[k + 1] * w);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x + px(x0 + 1.0), b.origin.y + px(h - tall)),
                                    size(px((x1 - x0 - 2.0).max(1.0)), px(tall)),
                                ),
                                alpha([LOW, MIDDLE, HIGH][k], 0.3),
                            ));
                        }
                        spectrum_fill(window, b, &heard, 1.0);
                        // The middle of the picture is as it came.
                        dashed(window, b, h / 2.0, w, alpha(0xffffff, 0.25));
                        if s.mono_hz > 0.0 {
                            let x = freq_x(s.mono_hz) * w;
                            window.paint_quad(fill(
                                Bounds::new(b.origin, size(px(x), px(h))),
                                alpha(0x000000, 0.5),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full();
                let mid = |k: usize| {
                    let cuts = [0.0, freq_x(s.crossovers[0]), freq_x(s.crossovers[1]), 1.0];
                    (cuts[k] + cuts[k + 1]) / 2.0 * TIME_W
                };
                let width = |k: usize, sel, cx: &mut Context<Self>| {
                    handle(
                        mid(k),
                        (1.0 - s.width[k] / 2.0) * DISPLAY_H,
                        [LOW, MIDDLE, HIGH][k],
                        sel,
                        SpaceHandle::Tool(k as u8 + 2),
                        0.0,
                        cx,
                    )
                };
                let split = |k: usize, sel, cx: &mut Context<Self>| {
                    handle(
                        freq_x(s.crossovers[k]) * TIME_W,
                        DISPLAY_H - 18.0,
                        0xffffff,
                        sel,
                        SpaceHandle::Tool(k as u8),
                        0.0,
                        cx,
                    )
                };
                let under = match s.mono_hz {
                    v if v < 1.0 => "Mono under: off".to_string(),
                    v => format!("Mono under {v:.0} Hz"),
                };
                picture
                    .debug_selector(|| "stereo-display".into())
                    .child(bands)
                    .child(chip_tag(8.0, 8.0, under, TEXT_2))
                    .child(handle(
                        freq_x(s.mono_hz) * TIME_W,
                        32.0,
                        0xffffff,
                        "stereo-mono-under",
                        SpaceHandle::Tool(5),
                        0.0,
                        cx,
                    ))
                    .child(split(0, "stereo-split-low", cx))
                    .child(split(1, "stereo-split-high", cx))
                    .child(width(0, "stereo-width-low", cx))
                    .child(width(1, "stereo-width-middle", cx))
                    .child(width(2, "stereo-width-high", cx))
            }
            Unit::Multiband(m) => {
                let (selected, took) = (v.band, v.reading);
                let bands = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        freq_grid(window, b);
                        let cuts = [0.0, freq_x(m.crossovers[0]), freq_x(m.crossovers[1]), 1.0];
                        for k in 0..3 {
                            let (x0, x1) = (cuts[k] * w, cuts[k + 1] * w);
                            let y = (m.threshold_db[k] / LEVEL_FLOOR).clamp(0.0, 1.0) * h;
                            let c = [LOW, MIDDLE, HIGH][k];
                            let lit = if k == selected { 0.2 } else { 0.08 };
                            let at = |y: f32, tall: f32, a: f32, window: &mut gpui::Window| {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(b.origin.x + px(x0 + 1.0), b.origin.y + px(y)),
                                        size(px((x1 - x0 - 2.0).max(1.0)), px(tall)),
                                    ),
                                    alpha(c, a),
                                ));
                            };
                            at(0.0, h, lit, window);
                            // What it's taking off, hanging under its threshold.
                            at(y, (took[k] / -LEVEL_FLOOR * h).max(0.0), 0.6, window);
                            at(y, 2.0, 0.95, window);
                        }
                        spectrum_fill(window, b, &heard, 2.0);
                    },
                )
                .absolute()
                .size_full();
                let cuts = [0.0, freq_x(m.crossovers[0]), freq_x(m.crossovers[1]), 1.0];
                let mid = |k: usize| (cuts[k] + cuts[k + 1]) / 2.0 * TIME_W;
                let bar = |k: usize, sel, cx: &mut Context<Self>| {
                    handle(
                        mid(k),
                        (m.threshold_db[k] / LEVEL_FLOOR).clamp(0.0, 1.0) * DISPLAY_H,
                        [LOW, MIDDLE, HIGH][k],
                        sel,
                        SpaceHandle::Tool(k as u8 + 2),
                        0.0,
                        cx,
                    )
                };
                let split = |k: usize, sel, cx: &mut Context<Self>| {
                    handle(
                        freq_x(m.crossovers[k]) * TIME_W,
                        DISPLAY_H - 18.0,
                        0xffffff,
                        sel,
                        SpaceHandle::Tool(k as u8),
                        0.0,
                        cx,
                    )
                };
                picture
                    .debug_selector(|| "multiband-display".into())
                    .child(bands)
                    .child(split(0, "multiband-split-low", cx))
                    .child(split(1, "multiband-split-high", cx))
                    .child(bar(0, "multiband-low", cx))
                    .child(bar(1, "multiband-middle", cx))
                    .child(bar(2, "multiband-high", cx))
            }
            Unit::Pitch(p) => {
                let ruler = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let x = |st: f32| (st + 24.0) / 48.0 * w;
                        for st in -24..=24 {
                            let tall = match st % 12 {
                                0 => 64.0,
                                5 | 7 | -5 | -7 => 40.0,
                                _ => 22.0,
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        b.origin.x + px(x(st as f32)),
                                        b.origin.y + px(h / 2.0 - tall / 2.0),
                                    ),
                                    size(px(1.), px(tall)),
                                ),
                                alpha(0xffffff, if st % 12 == 0 { 0.5 } else { 0.18 }),
                            ));
                        }
                        // From where it came to where it's going.
                        let (from, to) = (x(0.0), x(p.semitones));
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    b.origin.x + px(from.min(to)),
                                    b.origin.y + px(h / 2.0 - 2.0),
                                ),
                                size(px((to - from).abs().max(1.0)), px(4.)),
                            ),
                            alpha(MIDDLE, 0.8),
                        ));
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "pitch-display".into())
                    .child(ruler)
                    .child(chip_tag(8.0, 8.0, interval(p.semitones), MIDDLE))
                    .child(chip_tag(
                        TIME_W / 2.0 - 6.0,
                        DISPLAY_H - 26.0,
                        "0".into(),
                        TEXT_3,
                    ))
                    .child(chip_tag(
                        TIME_W * 0.75 - 9.0,
                        DISPLAY_H - 26.0,
                        "+12".into(),
                        TEXT_3,
                    ))
                    .child(chip_tag(
                        TIME_W * 0.25 - 9.0,
                        DISPLAY_H - 26.0,
                        "-12".into(),
                        TEXT_3,
                    ))
                    .child(handle(
                        (p.semitones + 24.0) / 48.0 * TIME_W,
                        DISPLAY_H / 2.0,
                        MIDDLE,
                        "pitch-handle",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
            }
            Unit::TapeStop(t) => {
                let speed = v.reading[0];
                let fall = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let bend = 1.0 + 2.0 * t.curve;
                        let inset = h * 0.12;
                        let y = |s: f32| inset + (1.0 - s.powf(bend)) * (h - 2.0 * inset);
                        let stop = (1.0 - TAPE_TIME.pos(t.stop_beats)) * 0.5;
                        let start = 0.5 + TAPE_TIME.pos(t.start_beats) * 0.5;
                        // Full speed, down over its time, stopped, back up over its time.
                        let at = |x: f32| match x {
                            x if x < stop => 1.0,
                            x if x < 0.5 => 1.0 - (x - stop) / (0.5 - stop).max(1e-3),
                            x if x < start => (x - 0.5) / (start - 0.5).max(1e-3),
                            _ => 1.0,
                        };
                        let mut area = PathBuilder::fill();
                        area.move_to(point(b.origin.x, b.origin.y + px(h)));
                        let mut edge = PathBuilder::stroke(px(1.8));
                        for i in 0..=240 {
                            let x = i as f32 / 240.0;
                            let p = point(b.origin.x + px(x * w), b.origin.y + px(y(at(x))));
                            area.line_to(p);
                            match i {
                                0 => edge.move_to(p),
                                _ => edge.line_to(p),
                            }
                        }
                        area.line_to(point(b.origin.x + px(w), b.origin.y + px(h)));
                        area.close();
                        if let Ok(p) = area.build() {
                            window.paint_path(p, alpha(MIDDLE, 0.16));
                        }
                        if let Ok(p) = edge.build() {
                            window.paint_path(p, alpha(MIDDLE, 0.95));
                        }
                    },
                )
                .absolute()
                .size_full();
                let stop = (1.0 - TAPE_TIME.pos(t.stop_beats)) * 0.5;
                let start = 0.5 + TAPE_TIME.pos(t.start_beats) * 0.5;
                let top = DISPLAY_H * 0.12;
                picture
                    .debug_selector(|| "tape-stop-display".into())
                    .child(fall)
                    .child(chip_tag(
                        8.0,
                        8.0,
                        format!("Stops over {:.2} beats", t.stop_beats),
                        HIGH,
                    ))
                    .child(chip_tag(
                        TIME_W - 170.0,
                        8.0,
                        format!("Starts over {:.2} beats", t.start_beats),
                        LOW,
                    ))
                    .child(chip_tag(
                        TIME_W / 2.0 - 28.0,
                        DISPLAY_H - 26.0,
                        format!("{:.0}% now", speed * 100.0),
                        MIDDLE,
                    ))
                    .child(handle(
                        stop * TIME_W,
                        top,
                        HIGH,
                        "tape-stop-handle",
                        SpaceHandle::Tool(0),
                        0.0,
                        cx,
                    ))
                    .child(handle(
                        start * TIME_W,
                        top,
                        LOW,
                        "tape-start-handle",
                        SpaceHandle::Tool(1),
                        0.0,
                        cx,
                    ))
                    .child(handle(
                        (stop + 0.5) / 2.0 * TIME_W,
                        t.curve * DISPLAY_H,
                        MIDDLE,
                        "tape-curve-handle",
                        SpaceHandle::Tool(2),
                        0.0,
                        cx,
                    ))
            }
            Unit::Tune(t) => {
                let notes = t.notes;
                let lanes = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let row = h / TUNE_NOTES.len() as f32;
                        let y = |midi: f32| (TUNE_NOTES.end as f32 - midi) * row;
                        for note in TUNE_NOTES {
                            let shade = match (note % 12 == 0, notes[note as usize % 12]) {
                                (true, _) => alpha(0xffffff, 0.12),
                                (_, true) => alpha(0xffffff, 0.045),
                                _ => alpha(0x000000, 0.0),
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x, b.origin.y + px(y(note as f32 + 1.0) + 0.5)),
                                    size(px(w), px((row - 1.0).max(1.0))),
                                ),
                                shade,
                            ));
                        }
                        let step = w / HISTORY as f32;
                        let start = w - levels.len() as f32 * step;
                        for (k, c, width) in [
                            (0, alpha(0xffffff, 0.5), 1.2),
                            (1, alpha(MIDDLE, 0.95), 1.8),
                        ] {
                            let mut p = PathBuilder::stroke(px(width));
                            let mut open = false;
                            let mut any = false;
                            for (i, l) in levels.iter().enumerate() {
                                // Silence and breath break the line.
                                if l[k] < 1.0 {
                                    open = false;
                                    continue;
                                }
                                let at = point(
                                    b.origin.x + px(start + i as f32 * step),
                                    b.origin.y + px(y(l[k] + 0.5).clamp(0.0, h)),
                                );
                                match open {
                                    true => p.line_to(at),
                                    false => p.move_to(at),
                                }
                                (open, any) = (true, true);
                            }
                            if any && let Ok(p) = p.build() {
                                window.paint_path(p, c);
                            }
                        }
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "tune-display".into())
                    .child(lanes)
                    .child(chip_tag(8.0, 8.0, "Sung".into(), TEXT_2))
                    .child(chip_tag(52.0, 8.0, "Corrected".into(), MIDDLE))
                    .children([60u8, 72].map(|c| {
                        let row = DISPLAY_H / TUNE_NOTES.len() as f32;
                        let y = (TUNE_NOTES.end - c) as f32 * row - 16.0;
                        chip_tag(TIME_W - 28.0, y, format!("C{}", c / 12 - 1), TEXT_3)
                    }))
            }
            _ => picture,
        }
    }

    /// A tool's second display.
    fn tool_right(&self, u: &Unit, cx: &mut Context<Self>) -> Div {
        let v = self.fx_view.as_ref().expect("open");
        let plot = v.plot2.clone();
        let levels: Vec<[f32; 4]> = v.history.iter().copied().collect();
        let scope = v.scope.clone();
        let picture = well().h_full().w(px(FREQ_W)).flex_none();
        match *u {
            Unit::Filter(f) => picture
                .debug_selector(|| "filter-lfo".into())
                .child(lfo_picture(
                    plot,
                    f.lfo_wave,
                    f.lfo_depth / 6.0,
                    f.lfo_stereo,
                ))
                .child(chip_tag(
                    8.0,
                    8.0,
                    format!("LFO {:+.1} oct", f.lfo_depth),
                    HIGH,
                ))
                .child(handle(
                    LFO_RATE.pos(f.lfo_hz) * FREQ_W,
                    (0.5 - f.lfo_depth / 6.0 * CREST) * DISPLAY_H,
                    HIGH,
                    "filter-lfo-handle",
                    SpaceHandle::Tool(1),
                    0.0,
                    cx,
                )),
            Unit::Motion(_) | Unit::Stereo(_) => picture
                .debug_selector(|| "vectorscope".into())
                .child(vectorscope(plot, scope.clone()))
                .child(chip_tag(
                    8.0,
                    8.0,
                    format!("Sides alike {:+.2}", correlation(&scope)),
                    TEXT_2,
                ))
                .child(chip_tag(FREQ_W * 0.5 - 92.0, 44.0, "L".into(), TEXT_3))
                .child(chip_tag(FREQ_W * 0.5 + 84.0, 44.0, "R".into(), TEXT_3)),
            Unit::Gate(g) => {
                let curve = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        // In across, out up: -80 to 0 dB each way.
                        let out = |x: f32| {
                            let under = (g.threshold_db - x).max(0.0);
                            x - (under * (g.ratio - 1.0).max(0.0)).min(g.range_db)
                        };
                        let at = |x: f32, y: f32| {
                            point(
                                b.origin.x + px((x / LEVEL_FLOOR + 1.0).clamp(0.0, 1.0) * w),
                                b.origin.y + px((y / LEVEL_FLOOR).clamp(0.0, 1.0) * h),
                            )
                        };
                        let mut even = PathBuilder::stroke(px(1.));
                        even.move_to(at(LEVEL_FLOOR, LEVEL_FLOOR));
                        even.line_to(at(0.0, 0.0));
                        if let Ok(p) = even.build() {
                            window.paint_path(p, alpha(0xffffff, 0.14));
                        }
                        let mut p = PathBuilder::stroke(px(1.8));
                        for i in 0..120 {
                            let x = (1.0 - i as f32 / 119.0) * LEVEL_FLOOR;
                            match i {
                                0 => p.move_to(at(x, out(x))),
                                _ => p.line_to(at(x, out(x))),
                            }
                        }
                        if let Ok(p) = p.build() {
                            window.paint_path(p, alpha(MIDDLE, 0.95));
                        }
                        // Where it is now.
                        if let Some(l) = levels.last() {
                            let c = at(l[0], out(l[0]));
                            window.paint_quad(
                                fill(
                                    Bounds::new(c - point(px(4.), px(4.)), size(px(8.), px(8.))),
                                    rgb(HIGH),
                                )
                                .corner_radii(px(4.)),
                            );
                        }
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "gate-curve".into())
                    .child(curve)
                    .child(chip_tag(8.0, 8.0, "In across, out up".into(), TEXT_3))
            }
            Unit::Multiband(_) => {
                let took = v.reading;
                let meters = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let wide = w / 3.0;
                        for k in 0..3 {
                            let tall = (took[k] / 24.0).min(1.0) * (h - 64.0);
                            let x = k as f32 * wide + wide * 0.3;
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x + px(x), b.origin.y + px(34.)),
                                    size(px(wide * 0.4), px(h - 64.0)),
                                ),
                                alpha(0xffffff, 0.04),
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x + px(x), b.origin.y + px(34.)),
                                    size(px(wide * 0.4), px(tall)),
                                ),
                                alpha([LOW, MIDDLE, HIGH][k], 0.85),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full();
                let wide = FREQ_W / 3.0;
                let mut d = picture
                    .debug_selector(|| "multiband-taken".into())
                    .child(meters);
                for (k, name) in ["Low", "Middle", "High"].into_iter().enumerate() {
                    let x = k as f32 * wide + wide * 0.18;
                    let c = [LOW, MIDDLE, HIGH][k];
                    d = d
                        .child(chip_tag(x, 8.0, format!("{:.1} dB", -took[k] + 0.0), c))
                        .child(chip_tag(x, DISPLAY_H - 24.0, name.into(), TEXT_3));
                }
                d
            }
            Unit::Tune(t) => {
                // A keyboard: naturals along the foot, sharps above, each a note it may land on.
                const NATURAL: [usize; 7] = [0, 2, 4, 5, 7, 9, 11];
                const SHARP: [(usize, f32); 5] =
                    [(1, 1.0), (3, 2.0), (6, 4.0), (8, 5.0), (10, 6.0)];
                const NAMES: [&str; 12] = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                let ch = self.fx_view.as_ref().map_or(Channel::Master, |v| v.ch);
                let wide = (FREQ_W - 32.0) / 7.0;
                let key = |note: usize, x: f32, top: f32, tall: f32, cx: &mut Context<Self>| {
                    let on = t.notes[note];
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(top))
                        .w(px(wide - 3.0))
                        .h(px(tall))
                        .flex()
                        .items_end()
                        .justify_center()
                        .pb(px(6.))
                        .rounded(px(4.))
                        .text_size(px(10.5))
                        .text_color(rgb(if on { 0x0b0b0e } else { TEXT_3 }))
                        .bg(rgb(if on { MIDDLE } else { LINE }))
                        .cursor_pointer()
                        .debug_selector(move || format!("tune-note-{note}"))
                        .child(NAMES[note])
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit(cx, |ws| {
                                    if let Some(t) = ws.fx_unit_mut::<Tune>(ch) {
                                        t.notes[note] ^= true;
                                    }
                                });
                            }),
                        )
                };
                let mut d = picture.debug_selector(|| "tune-keys".into());
                for (i, note) in NATURAL.into_iter().enumerate() {
                    d = d.child(key(note, 16.0 + i as f32 * wide, 112.0, 98.0, cx));
                }
                for (note, at) in SHARP {
                    d = d.child(key(note, 16.0 + (at - 0.5) * wide, 20.0, 84.0, cx));
                }
                d
            }
            Unit::TapeStop(t) => {
                let speed = v.reading[0];
                let dial = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        window.paint_quad(fill(
                            Bounds::new(
                                point(b.origin.x + px(24.), b.origin.y + px(h * 0.5 - 7.0)),
                                size(px(w - 48.0), px(14.)),
                            ),
                            alpha(0xffffff, 0.05),
                        ));
                        window.paint_quad(fill(
                            Bounds::new(
                                point(b.origin.x + px(24.), b.origin.y + px(h * 0.5 - 7.0)),
                                size(px((w - 48.0) * speed), px(14.)),
                            ),
                            alpha(if t.stop > 0.5 { HIGH } else { MIDDLE }, 0.85),
                        ));
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "tape-stop-speed".into())
                    .child(dial)
                    .child(chip_tag(
                        24.0,
                        74.0,
                        format!("Speed {:.0}%", speed * 100.0),
                        TEXT_2,
                    ))
            }
            Unit::DeEsser(_) => {
                let now = levels.last().map_or(0.0, |l| l[2]);
                let taken = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let step = w / HISTORY as f32;
                        let from = w - levels.len() as f32 * step;
                        for (i, l) in levels.iter().enumerate() {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(b.origin.x + px(from + i as f32 * step), b.origin.y),
                                    size(px(step.max(1.0)), px((l[2] / 24.0).min(1.0) * h)),
                                ),
                                alpha(HIGH, 0.7),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full();
                picture
                    .debug_selector(|| "de-esser-taken".into())
                    .child(taken)
                    .child(chip_tag(
                        8.0,
                        DISPLAY_H - 26.0,
                        format!("Taking off {now:.1} dB"),
                        HIGH,
                    ))
            }
            _ => picture.debug_selector(|| "tool-levels".into()).child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        plot.set(b);
                        levels_picture(window, b, &levels);
                    },
                )
                .absolute()
                .size_full(),
            ),
        }
    }
}

/// The notes pitch correction's picture shows.
const TUNE_NOTES: std::ops::Range<u8> = 48..84;

/// An interval as it's said.
fn interval(st: f32) -> String {
    const NAMES: [&str; 12] = [
        "An octave",
        "A minor second",
        "A major second",
        "A minor third",
        "A major third",
        "A fourth",
        "A tritone",
        "A fifth",
        "A minor sixth",
        "A major sixth",
        "A minor seventh",
        "A major seventh",
    ];
    let n = st.round() as i32;
    let way = if n > 0 { "up" } else { "down" };
    match n.abs() {
        0 => "As it came".into(),
        24 => format!("Two octaves {way}"),
        k => format!("{} {way}", NAMES[(k % 12) as usize]),
    }
}

/// The floor of the level pictures, dB.
const LEVEL_FLOOR: f32 = -80.0;

/// A hit, and what the shaper makes of it: its very code run on the hit's outline, so every knob
/// shows as it sounds.
fn shaper_hit(s: Shaper) -> [Vec<f32>; 2] {
    let n = (HIT_S * HIT_RATE) as usize;
    let hit = |i: usize| {
        let t = i as f32 / HIT_RATE;
        0.42 * (t / 0.002).min(1.0) * (-t / 0.16).exp()
    };
    let dry: Vec<f32> = (0..n).map(hit).collect();
    let (mut l, mut r) = (dry.clone(), dry.clone());
    let clock = Clock {
        bpm: 120.0,
        beats: 0.0,
    };
    Shaping::new(HIT_RATE as f64).run(&Unit::Shaper(s), clock, &mut l, &mut r);
    [dry, l]
}

/// What went in (grey) and came out (green), scrolling by, newest at the right.
fn levels_picture(window: &mut gpui::Window, b: Bounds<Pixels>, levels: &[[f32; 4]]) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let y = |db: f32| (db / LEVEL_FLOOR).clamp(0.0, 1.0) * h;
    for db in [-20.0, -40.0, -60.0] {
        window.paint_quad(fill(
            Bounds::new(
                point(b.origin.x, b.origin.y + px(y(db))),
                size(px(w), px(1.)),
            ),
            rgb(LINE_SOFT),
        ));
    }
    if levels.len() < 2 {
        return;
    }
    let step = w / HISTORY as f32;
    let start = w - levels.len() as f32 * step;
    let at = |i: usize, db: f32| {
        point(
            b.origin.x + px(start + i as f32 * step),
            b.origin.y + px(y(db)),
        )
    };
    let foot = |x: f32| point(b.origin.x + px(x), b.origin.y + px(h));
    for (k, c, a) in [(0, 0xffffff, 0.1), (1, MIDDLE, 0.3)] {
        let mut area = PathBuilder::fill();
        area.move_to(foot(start));
        levels
            .iter()
            .enumerate()
            .for_each(|(i, l)| area.line_to(at(i, l[k])));
        area.line_to(foot(w));
        area.close();
        if let Ok(p) = area.build() {
            window.paint_path(p, alpha(c, a));
        }
    }
    let mut out = PathBuilder::stroke(px(1.2));
    out.move_to(at(0, levels[0][1]));
    levels
        .iter()
        .enumerate()
        .skip(1)
        .for_each(|(i, l)| out.line_to(at(i, l[1])));
    if let Ok(p) = out.build() {
        window.paint_path(p, alpha(MIDDLE, 0.9));
    }
}

/// Two cycles of an LFO, each side's line, as tall as its depth.
fn lfo_picture(
    plot: Rc<Cell<Bounds<Pixels>>>,
    wave: Wave,
    depth: f32,
    spread: f32,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let at = |x: f32, shift: f32| {
                let p = 2.0 * x + shift;
                let v = match wave {
                    Wave::Triangle => 1.0 - 4.0 * ((p + 0.25).rem_euclid(1.0) - 0.5).abs(),
                    Wave::SawUp | Wave::SawDown => 2.0 * p.rem_euclid(1.0) - 1.0,
                    Wave::Square => {
                        if p.rem_euclid(1.0) < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    _ => (p * std::f32::consts::TAU).sin(),
                };
                point(
                    b.origin.x + px(x * w),
                    b.origin.y + px(h * (0.5 - v * depth * CREST)),
                )
            };
            window.paint_quad(fill(
                Bounds::new(
                    point(b.origin.x, b.origin.y + px(h / 2.0)),
                    size(px(w), px(1.)),
                ),
                rgb(LINE_SOFT),
            ));
            for (shift, c) in [(0.0, MIDDLE), (spread * 0.5, LOW)] {
                let mut p = PathBuilder::stroke(px(1.6));
                p.move_to(at(0.0, shift));
                for i in 1..200 {
                    p.line_to(at(i as f32 / 199.0, shift));
                }
                if let Ok(p) = p.build() {
                    window.paint_path(p, alpha(c, 0.9));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

/// What's playing as a cloud: up the middle in mono, across as the sides differ.
fn vectorscope(plot: Rc<Cell<Bounds<Pixels>>>, scope: [Vec<f32>; 2]) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let (cx, cy, r) = (w / 2.0, h / 2.0, h / 2.0 - 8.0);
            // Mono runs up the middle, each side alone up its diagonal; the diamond is full scale.
            let corner =
                |x: f32, y: f32| point(b.origin.x + px(cx + x * r), b.origin.y + px(cy + y * r));
            let mut lines = PathBuilder::stroke(px(1.));
            lines.move_to(corner(0.0, -1.0));
            for (x, y) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                lines.line_to(corner(x, y));
            }
            lines.move_to(corner(-0.5, -0.5));
            lines.line_to(corner(0.5, 0.5));
            lines.move_to(corner(0.5, -0.5));
            lines.line_to(corner(-0.5, 0.5));
            if let Ok(p) = lines.build() {
                window.paint_path(p, alpha(0xffffff, 0.14));
            }
            for (l, r_) in scope[0].iter().zip(&scope[1]).step_by(2) {
                let (x, y) = ((l - r_) * 0.7, (l + r_) * 0.7);
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            b.origin.x + px(cx + x.clamp(-1.0, 1.0) * r),
                            b.origin.y + px(cy - y.clamp(-1.0, 1.0) * r),
                        ),
                        size(px(1.6), px(1.6)),
                    ),
                    alpha(MIDDLE, 0.55),
                ));
            }
        },
    )
    .absolute()
    .size_full()
}

/// How alike the sides are: 1 the same, 0 unrelated, -1 opposite.
fn correlation(scope: &[Vec<f32>; 2]) -> f32 {
    let (mut lr, mut ll, mut rr) = (0f32, 0f32, 0f32);
    for (l, r) in scope[0].iter().zip(&scope[1]) {
        (lr, ll, rr) = (lr + l * r, ll + l * l, rr + r * r);
    }
    match ll * rr > 1e-12 {
        true => lr / (ll * rr).sqrt(),
        false => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapers_picture_shows_what_it_does() {
        let at = |x: &[f32], t: f32| x[(t * HIT_RATE) as usize];
        let [dry, punched] = shaper_hit(Shaper {
            attack: 1.0,
            ..Shaper::default()
        });
        assert!(at(&punched, 0.01) > at(&dry, 0.01) * 1.5);
        let [dry, cut] = shaper_hit(Shaper {
            sustain: -1.0,
            ..Shaper::default()
        });
        assert!(at(&cut, 0.4) < at(&dry, 0.4) * 0.7);
        let [dry, same] = shaper_hit(Shaper::default());
        assert!(dry.iter().zip(&same).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn sides_alike_reads_mono_as_one_and_flipped_as_minus_one() {
        let x: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
        let flipped = x.iter().map(|v| -v).collect();
        assert!((correlation(&[x.clone(), x.clone()]) - 1.0).abs() < 1e-3);
        assert!((correlation(&[x, flipped]) + 1.0).abs() < 1e-3);
        assert_eq!(correlation(&[vec![0.0; 8], vec![0.0; 8]]), 0.0);
    }
}
