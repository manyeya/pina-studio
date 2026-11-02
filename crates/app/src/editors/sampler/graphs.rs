//! The sampler's drawn controls: envelopes whose points are dragged (attack across the first
//! quarter, decay and the level held across the second, release across the last), the filter's
//! response with its cutoff and resonance a point to drag, and an LFO's wave.

use super::*;

/// Where an envelope's stages end, across a graph (0 to 1): the attack in the first quarter,
/// the decay in the second, the release in the last (each by the log of its time).
const ATTACK: (f32, f32) = (0.0, 0.25);
const DECAY: (f32, f32) = (0.25, 0.5);
const RELEASE: (f32, f32) = (0.72, 1.0);

/// An envelope's points (x, level): attack's peak, decay's end at the level held, the hold's
/// end, release's end.
fn points(a: f32, d: f32, s: f32, r: f32) -> [(f32, f32); 5] {
    let at = |zone: (f32, f32), from: f32, ms: f32| from + (zone.1 - zone.0) * ms_pos(ms);
    let pa = at(ATTACK, 0.0, a);
    let pd = at(DECAY, pa.max(DECAY.0), d);
    let pr = at(RELEASE, RELEASE.0, r);
    [(0.0, 0.0), (pa, 1.0), (pd, s), (RELEASE.0, s), (pr, 0.0)]
}

/// An envelope's times and level, from a sampler.
pub(crate) fn env_of(v: &SamplerVoice, env: EnvKind) -> (f32, f32, f32, f32) {
    match env {
        EnvKind::Amp => (v.attack_ms, v.decay_ms, v.sustain, v.release_ms),
        EnvKind::Filter => (
            v.filter_attack_ms,
            v.filter_decay_ms,
            v.filter_sustain,
            v.filter_release_ms,
        ),
        EnvKind::Mod => (
            v.mod_env.attack_ms,
            v.mod_env.decay_ms,
            v.mod_env.sustain,
            v.mod_env.release_ms,
        ),
    }
}

/// A graph's frame: dark, rounded, faint lines across.
fn graph_well() -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(8.))
        .bg(rgb(0x0b0b0e))
        .border_1()
        .border_color(rgb(LINE_SOFT))
}

/// A line across a graph at `y` (0 at the top, 1 at the bottom).
fn rule(window: &mut Window, b: Bounds<Pixels>, y: f32, c: Hsla) {
    let h = f32::from(b.size.height);
    window.paint_quad(fill(
        Bounds::new(
            point(b.origin.x, b.origin.y + px((y * h).round())),
            size(b.size.width, px(1.)),
        ),
        c,
    ));
}

impl Workspace {
    /// An envelope, drawn: drag its points (attack, decay and the level held, release).
    pub(crate) fn env_graph(
        &self,
        t: usize,
        env: EnvKind,
        color: u32,
        h: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let v = self.sampler_of(t).map(|s| s.voice).unwrap_or_default();
        let (a, d, s, r) = env_of(&v, env);
        let pts = points(a, d, s, r);
        let cell = self.sampler_ui.graphs.clone();
        let graph = canvas(
            move |b, _, _| cell.of(env).set(b),
            move |b, _, window, _| {
                let (w, hh) = (f32::from(b.size.width), f32::from(b.size.height));
                for y in [0.25, 0.5, 0.75] {
                    rule(window, b, y, rgb(LINE_SOFT).into());
                }
                let at = |(x, level): (f32, f32)| {
                    point(
                        b.origin.x + px(6.0 + x * (w - 12.0)),
                        b.origin.y + px(6.0 + (1.0 - level) * (hh - 12.0)),
                    )
                };
                let mut area = PathBuilder::fill();
                area.move_to(at((0.0, 0.0)));
                for &p in &pts {
                    area.line_to(at(p));
                }
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, 0.16));
                }
                let mut line = PathBuilder::stroke(px(2.));
                line.move_to(at(pts[0]));
                for &p in &pts[1..] {
                    line.line_to(at(p));
                }
                if let Ok(p) = line.build() {
                    window.paint_path(p, rgb(color));
                }
                // The points to drag.
                for &i in &[1usize, 2, 4] {
                    if let Some(p) = disc(at(pts[i]), 5.0) {
                        window.paint_path(p, rgb(0x0b0b0e));
                    }
                    if let Some(p) = disc(at(pts[i]), 3.5) {
                        window.paint_path(p, rgb(TEXT));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        graph_well()
            .h(px(h))
            .w_full()
            .cursor_pointer()
            .debug_selector(move || format!("sampler-env-{env:?}"))
            .child(graph)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let b = ws.sampler_ui.graphs.of(env).get();
                    let (x, y) = (
                        f32::from(e.position.x - b.origin.x),
                        f32::from(e.position.y - b.origin.y),
                    );
                    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                    // The point nearest where it's pressed.
                    let at = |(px_, level): (f32, f32)| {
                        (6.0 + px_ * (w - 12.0), 6.0 + (1.0 - level) * (h - 12.0))
                    };
                    let near = [1u8, 2, 4]
                        .into_iter()
                        .min_by(|&i, &j| {
                            let d = |k: u8| {
                                let (px_, py) = at(pts[k as usize]);
                                (px_ - x).hypot(py - y)
                            };
                            d(i).total_cmp(&d(j))
                        })
                        .unwrap_or(1);
                    let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
                    ws.begin_drag(
                        Drag::SamplerEnv {
                            track: t,
                            env,
                            point: near,
                            left,
                            top,
                            width: w,
                            height: h,
                        },
                        cx,
                    );
                    ws.drag_sampler_env(pt(e.position));
                }),
            )
    }

    /// An envelope's point dragged to `p`: its stage's time across, the level held up and down.
    pub(crate) fn drag_sampler_env(&mut self, p: Point<f32>) {
        let Some(Drag::SamplerEnv {
            track,
            env,
            point,
            left,
            top,
            width,
            height,
        }) = self.drag
        else {
            return;
        };
        let x = ((p.x - left - 6.0) / (width - 12.0)).clamp(0.0, 1.0);
        let level = (1.0 - (p.y - top - 6.0) / (height - 12.0)).clamp(0.0, 1.0);
        let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(track)
            .map(|t| &mut t.instrument)
        else {
            return;
        };
        let v = &mut s.voice;
        let (a, _, _, _) = env_of(v, env);
        let attack_end = ATTACK.1 * ms_pos(a);
        // A time from where across its stage's quarter the point is.
        let ms =
            |from: f32, zone: (f32, f32)| ms_of(((x - from) / (zone.1 - zone.0)).clamp(0.0, 1.0));
        let (ta, td, ts, tr) = match env {
            EnvKind::Amp => (
                &mut v.attack_ms,
                &mut v.decay_ms,
                &mut v.sustain,
                &mut v.release_ms,
            ),
            EnvKind::Filter => (
                &mut v.filter_attack_ms,
                &mut v.filter_decay_ms,
                &mut v.filter_sustain,
                &mut v.filter_release_ms,
            ),
            EnvKind::Mod => (
                &mut v.mod_env.attack_ms,
                &mut v.mod_env.decay_ms,
                &mut v.mod_env.sustain,
                &mut v.mod_env.release_ms,
            ),
        };
        match point {
            1 => *ta = ms(0.0, ATTACK),
            2 => {
                *td = ms(attack_end.max(DECAY.0), DECAY);
                *ts = (level * 100.0).round() / 100.0;
            }
            _ => *tr = ms(RELEASE.0, RELEASE),
        }
    }

    /// The filter's response, 20 Hz to 20 kHz: drag its point across for the cutoff, up and
    /// down for the resonance.
    pub(crate) fn filter_graph(&self, t: usize, color: u32, h: f32, cx: &mut Context<Self>) -> Div {
        let v = self.sampler_of(t).map(|s| s.voice).unwrap_or_default();
        let cell = self.sampler_ui.graphs.clone();
        let (mode, cutoff, reso) = (v.filter, v.cutoff_hz, v.resonance);
        let graph = canvas(
            move |b, _, _| cell.curve.set(b),
            move |b, _, window, _| {
                let (w, hh) = (f32::from(b.size.width), f32::from(b.size.height));
                // 18 dB over to 30 under, 0 dB a third of the way down.
                let y = |db: f32| ((18.0 - db.clamp(-30.0, 18.0)) / 48.0) * hh;
                rule(window, b, y(0.0) / hh, alpha(0xffffff, 0.1));
                for f in [100.0, 1000.0, 10_000.0] {
                    let x = (f32::log10(f / 20.0) / 3.0 * w).round();
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y),
                            size(px(1.), b.size.height),
                        ),
                        rgb(LINE_SOFT),
                    ));
                }
                let n = 120;
                let at = |i: usize| {
                    let f = 20.0 * 1000f32.powf(i as f32 / n as f32);
                    point(
                        b.origin.x + px(i as f32 / n as f32 * w),
                        b.origin.y + px(y(response(mode, cutoff, reso, f))),
                    )
                };
                let mut area = PathBuilder::fill();
                area.move_to(point(b.origin.x, b.origin.y + b.size.height));
                for i in 0..=n {
                    area.line_to(at(i));
                }
                area.line_to(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, 0.14));
                }
                let mut line = PathBuilder::stroke(px(2.));
                line.move_to(at(0));
                for i in 1..=n {
                    line.line_to(at(i));
                }
                if let Ok(p) = line.build() {
                    let c: Hsla = match mode {
                        FilterMode::Off => alpha(TEXT_3, 1.0),
                        _ => rgb(color).into(),
                    };
                    window.paint_path(p, c);
                }
                if mode != FilterMode::Off {
                    let x = (cutoff / 20.0).max(1.0).log10() / 3.0 * w;
                    let node = point(
                        b.origin.x + px(x),
                        b.origin.y + px(6.0 + (1.0 - reso) * (hh - 12.0)),
                    );
                    if let Some(p) = disc(node, 7.0) {
                        window.paint_path(p, alpha(color, 0.3));
                    }
                    if let Some(p) = disc(node, 4.0) {
                        window.paint_path(p, rgb(TEXT));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        graph_well()
            .h(px(h))
            .w_full()
            .cursor_pointer()
            .debug_selector(|| "sampler-filter-curve".into())
            .child(graph)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let b = ws.sampler_ui.graphs.curve.get();
                    ws.begin_drag(
                        Drag::SamplerFilter {
                            track: t,
                            left: f32::from(b.origin.x),
                            top: f32::from(b.origin.y),
                            width: f32::from(b.size.width),
                            height: f32::from(b.size.height),
                        },
                        cx,
                    );
                    ws.drag_sampler_filter(pt(e.position));
                }),
            )
    }

    /// The filter's point dragged: the cutoff across, the resonance up and down. Off, it comes
    /// on as a low pass.
    pub(crate) fn drag_sampler_filter(&mut self, p: Point<f32>) {
        let Some(Drag::SamplerFilter {
            track,
            left,
            top,
            width,
            height,
        }) = self.drag
        else {
            return;
        };
        let x = ((p.x - left) / width.max(1.0)).clamp(0.0, 1.0);
        let reso = (1.0 - (p.y - top - 6.0) / (height - 12.0)).clamp(0.0, 1.0);
        if let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(track)
            .map(|t| &mut t.instrument)
        {
            let v = &mut s.voice;
            if v.filter == FilterMode::Off {
                v.filter = FilterMode::LowPass;
            }
            v.cutoff_hz = (20.0 * 1000f32.powf(x)).round();
            v.resonance = (reso * 100.0).round() / 100.0;
        }
    }
}

/// How loud a filter leaves `hz` (dB), as the sampler's filters do (their analog prototypes;
/// close at what's drawn).
pub(crate) fn response(mode: FilterMode, cutoff: f32, reso: f32, hz: f32) -> f32 {
    let w = hz / cutoff.max(1.0);
    let reso = reso.clamp(0.0, 1.0);
    let k = 2.0 * (1.0 - 0.97 * reso);
    let den = ((1.0 - w * w).powi(2) + (k * w).powi(2)).sqrt();
    let gain = match mode {
        FilterMode::Off => 1.0,
        FilterMode::LowPass => 1.0 / den,
        FilterMode::HighPass => w * w / den,
        FilterMode::BandPass => w / den,
        FilterMode::Notch => (1.0 - w * w).abs() / den,
        FilterMode::Ladder => {
            // G = 1/(1 + jw); H = G^4 / (1 + K G^4), made up for its loss of loudness.
            let fb = 3.9 * reso;
            let (re, im) = (1.0 / (1.0 + w * w), -w / (1.0 + w * w));
            let (mut gr, mut gi) = (1.0f32, 0.0f32);
            for _ in 0..4 {
                (gr, gi) = (gr * re - gi * im, gr * im + gi * re);
            }
            let (dr, di) = (1.0 + fb * gr, fb * gi);
            (gr.hypot(gi) / dr.hypot(di)) * (1.0 + 0.6 * fb)
        }
        FilterMode::Comb => {
            let fb = 0.98 * reso;
            let phase = std::f32::consts::TAU * hz / cutoff.max(1.0);
            (1.0 - fb).sqrt() / (1.0 - 2.0 * fb * phase.cos() + fb * fb).sqrt()
        }
        FilterMode::Formant => {
            const VOWELS: [[f32; 3]; 5] = [
                [730.0, 1090.0, 2440.0],
                [530.0, 1840.0, 2480.0],
                [270.0, 2290.0, 3010.0],
                [570.0, 840.0, 2410.0],
                [300.0, 870.0, 2240.0],
            ];
            let at = ((cutoff / 20.0).log2() / 1000f32.log2() * 4.0).clamp(0.0, 4.0);
            let (i, t) = ((at.floor() as usize).min(3), at - at.floor().min(3.0));
            let q = 4.0 + 16.0 * reso;
            let (mut re, mut im) = (0f32, 0f32);
            for (f, g) in [1.0, 0.6, 0.35].into_iter().enumerate() {
                let (a, b) = (VOWELS[i][f], VOWELS[i + 1][f]);
                let wf = hz / (a * (b / a).powf(t));
                // (j wf / q) / (1 - wf² + j wf / q), each its loudness.
                let (nr, ni) = (0.0, wf / q);
                let (dr, di) = (1.0 - wf * wf, wf / q);
                let d2 = dr * dr + di * di;
                re += g * 2.5 * (nr * dr + ni * di) / d2;
                im += g * 2.5 * (ni * dr - nr * di) / d2;
            }
            re.hypot(im)
        }
    };
    20.0 * gain.max(1e-6).log10()
}

/// An LFO's wave, drawn over two cycles.
pub(crate) fn wave_graph(wave: LfoWave, color: u32, h: f32) -> Div {
    let drawn = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, hh) = (f32::from(b.size.width), f32::from(b.size.height));
            rule(window, b, 0.5, alpha(0xffffff, 0.08));
            let n = 160;
            let at = |i: usize| {
                let phase = i as f64 / n as f64 * 2.0;
                let y = pina_engine::dsp::sampler::wave(wave, phase, 5);
                point(
                    b.origin.x + px(i as f32 / n as f32 * w),
                    b.origin.y + px(hh / 2.0 - y * (hh / 2.0 - 5.0)),
                )
            };
            let mut line = PathBuilder::stroke(px(2.));
            line.move_to(at(0));
            for i in 1..=n {
                line.line_to(at(i));
            }
            if let Ok(p) = line.build() {
                window.paint_path(p, rgb(color));
            }
        },
    )
    .absolute()
    .size_full();
    graph_well().h(px(h)).w_full().child(drawn)
}

/// A wave's button: its shape drawn small, lit when it's the one.
pub(crate) fn wave_icon(wave: LfoWave, on: bool, color: u32) -> Div {
    let ink = if on { color } else { TEXT_3 };
    let drawn = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let n = 40;
            let at = |i: usize| {
                let phase = i as f64 / n as f64 * 1.5;
                let y = pina_engine::dsp::sampler::wave(wave, phase, 3);
                point(
                    b.origin.x + px(i as f32 / n as f32 * w),
                    b.origin.y + px(h / 2.0 - y * (h / 2.0 - 1.5)),
                )
            };
            let mut line = PathBuilder::stroke(px(1.5));
            line.move_to(at(0));
            for i in 1..=n {
                line.line_to(at(i));
            }
            if let Ok(p) = line.build() {
                window.paint_path(p, rgb(ink));
            }
        },
    )
    .size_full();
    div()
        .w(px(30.))
        .h(px(22.))
        .px(px(5.))
        .py(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .when(on, |d| d.bg(alpha(color, 0.18)))
        .when(!on, |d| d.bg(rgb(FIELD)).hover(|d| d.bg(rgb(RAISED))))
        .child(drawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_read_as_their_filters_do() {
        let lp = |hz| response(FilterMode::LowPass, 1_000.0, 0.0, hz);
        assert!(lp(50.0).abs() < 0.5 && lp(10_000.0) < -35.0);
        let ladder = |hz| response(FilterMode::Ladder, 1_000.0, 0.0, hz);
        assert!(ladder(10_000.0) < lp(10_000.0) - 20.0);
        assert!(response(FilterMode::Notch, 1_000.0, 0.0, 1_000.0) < -60.0);
        assert!(response(FilterMode::Off, 1_000.0, 0.5, 300.0).abs() < 1e-3);
        // An envelope's points stay in their lanes.
        let p = points(10.0, 300.0, 0.5, 200.0);
        assert!(p[1].0 <= ATTACK.1 && p[2].0 <= DECAY.1 && p[4].0 >= RELEASE.0);
        assert_eq!(p[2].1, 0.5);
    }
}
