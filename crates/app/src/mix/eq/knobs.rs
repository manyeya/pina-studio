//! The band panel's knobs: dragged, set, typed and reset.

use super::*;

/// A knob (or value) in the EQ window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqKnob {
    Freq,
    Gain,
    Q,
    Range,
    Threshold,
    Scale,
    Output,
    /// A dynamic band's attack and release (ms), and the two frequencies it listens between.
    Attack,
    Release,
    Low,
    High,
    /// The output's balance.
    Pan,
    /// A spectral band's density: wide stretches moving together (0) or each frequency alone (1).
    Density,
    /// A band's slope, dB an octave (fractions too).
    Slope,
}

impl EqKnob {
    /// Where a value sits on the knob, 0 to 1.
    pub(crate) fn pos(self, v: f32) -> f32 {
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => {
                ((v / 10.0).ln() / 3000f32.ln()).clamp(0.0, 1.0)
            }
            EqKnob::Attack | EqKnob::Release => ((v / 0.1).ln() / 10_000f32.ln()).clamp(0.0, 1.0),
            EqKnob::Pan => (v + 1.0) / 2.0,
            EqKnob::Density => v.clamp(0.0, 1.0),
            EqKnob::Slope => (v / 96.0).clamp(0.0, 1.0),
            EqKnob::Q => ((v / 0.025).ln() / 1600f32.ln()).clamp(0.0, 1.0),
            EqKnob::Gain | EqKnob::Range => (v + 30.0) / 60.0,
            EqKnob::Threshold => (v + 80.0) / 80.0,
            EqKnob::Scale => v / 2.0,
            EqKnob::Output => (v + 36.0) / 72.0,
        }
    }

    /// The value at a place on the knob.
    pub(crate) fn value(self, pos: f32) -> f32 {
        let p = pos.clamp(0.0, 1.0);
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => 10.0 * 3000f32.powf(p),
            EqKnob::Attack | EqKnob::Release => ((0.1 * 10_000f32.powf(p)) * 10.0).round() / 10.0,
            EqKnob::Pan => ((p * 2.0 - 1.0) * 100.0).round() / 100.0,
            EqKnob::Density => (p * 100.0).round() / 100.0,
            EqKnob::Slope => (p * 960.0).round() / 10.0,
            EqKnob::Q => 0.025 * 1600f32.powf(p),
            EqKnob::Gain | EqKnob::Range => ((p * 60.0 - 30.0) * 100.0).round() / 100.0,
            EqKnob::Threshold => (p * 80.0 - 80.0).round(),
            EqKnob::Scale => (p * 200.0).round() / 100.0,
            EqKnob::Output => ((p * 72.0 - 36.0) * 100.0).round() / 100.0,
        }
    }

    pub(crate) fn bipolar(self) -> bool {
        matches!(
            self,
            EqKnob::Gain | EqKnob::Range | EqKnob::Output | EqKnob::Scale | EqKnob::Pan
        )
    }

    pub fn text(self, v: f32) -> String {
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => hz_text(v),
            EqKnob::Attack | EqKnob::Release => match v < 10.0 {
                true => format!("{v:.1} ms"),
                false => format!("{v:.0} ms"),
            },
            EqKnob::Pan => match (v * 100.0).round() as i32 {
                0 => "C".into(),
                p if p < 0 => format!("L {}", -p),
                p => format!("R {p}"),
            },
            EqKnob::Q => q_text(v),
            EqKnob::Gain | EqKnob::Range | EqKnob::Output => format!("{} dB", db_text(v)),
            EqKnob::Threshold => format!("{} dB", db_text(v).trim_end_matches(".0")),
            EqKnob::Scale | EqKnob::Density => format!("{:.0}%", v * 100.0),
            EqKnob::Slope => slope_text(v),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            EqKnob::Freq => "FREQ",
            EqKnob::Gain => "GAIN",
            EqKnob::Q => "Q",
            EqKnob::Range => "RANGE",
            EqKnob::Threshold => "THRESHOLD",
            EqKnob::Scale => "GAIN SCALE",
            EqKnob::Output => "OUTPUT",
            EqKnob::Attack => "ATTACK",
            EqKnob::Release => "RELEASE",
            EqKnob::Low => "LOW",
            EqKnob::High => "HIGH",
            EqKnob::Pan => "PAN",
            EqKnob::Density => "DENSITY",
            EqKnob::Slope => "SLOPE",
        }
    }

    /// A value typed for it, as it reads: frequencies as Hz, kHz ("2k") or notes ("A4",
    /// "C#2+13"); gains as dB or a ratio ("2x"); any as a share of the knob ("50%").
    pub fn parse(self, text: &str) -> Option<f32> {
        let t = text.trim().to_lowercase().replace(['−', '–'], "-");
        if let Some(share) = t
            .strip_suffix('%')
            .and_then(|n| n.trim().parse::<f32>().ok())
        {
            return Some(match self {
                EqKnob::Scale => share / 100.0,
                _ => self.value(share / 100.0),
            });
        }
        let number = |s: &str| s.trim().parse::<f32>().ok();
        let v = match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => parse_freq(&t)?,
            EqKnob::Attack | EqKnob::Release => number(t.trim_end_matches("ms"))?,
            EqKnob::Pan => match t.chars().next()? {
                'c' => 0.0,
                'l' => -number(&t[1..])? / 100.0,
                'r' => number(&t[1..])? / 100.0,
                _ => number(&t)?,
            },
            EqKnob::Gain | EqKnob::Range | EqKnob::Output | EqKnob::Threshold => {
                let t = t.trim_end_matches("db").trim();
                match t.strip_suffix('x') {
                    Some(r) => 20.0 * number(r)?.max(1e-6).log10(),
                    None => number(t.trim_start_matches('+'))?,
                }
            }
            EqKnob::Q => number(t.trim_start_matches('q'))?,
            EqKnob::Scale => number(&t)?,
            // As a share: "0.3", or "30".
            EqKnob::Density => number(&t).map(|n| if n > 1.0 { n / 100.0 } else { n })?,
            EqKnob::Slope => match t.trim() {
                "brickwall" | "bw" => Shape::BRICKWALL,
                t => number(t.trim_end_matches("db/oct").trim_end_matches("db"))?,
            },
        };
        v.is_finite().then_some(v)
    }
}

impl Workspace {
    /// A knob dragged: up turns it up. On a band picked with others, the others go with it.
    pub fn drag_eq_knob(&mut self, id: u16, knob: EqKnob, from: f32, start: f32, y: f32) {
        let fine = if self.fine { 0.1 } else { 1.0 };
        let pos = knob.pos(start) + (from - y) / 200.0 * fine;
        let v = knob.value(pos);
        let others: Vec<EqBand> = self
            .eq_view
            .as_ref()
            .filter(|v| v.picked.contains(&id))
            .map(|v| v.start.clone())
            .unwrap_or_default();
        let lead = others.iter().find(|b| b.id == id).copied();
        self.set_eq_knob(id, knob, v);
        // The others picked: their frequencies and widths by the same ratio, gains by as much.
        if let (Some(lead), Some(eq)) = (lead, self.eq_mut()) {
            for s in others.iter().filter(|s| s.id != id) {
                let Some(i) = eq.bands.find(s.id) else {
                    continue;
                };
                let b = &mut eq.bands[i];
                match knob {
                    EqKnob::Freq => b.freq = (s.freq * v / lead.freq).clamp(10.0, 30_000.0).round(),
                    EqKnob::Q => b.q = (s.q * v / lead.q).clamp(0.025, 40.0),
                    EqKnob::Gain => b.gain_db = (s.gain_db + v - lead.gain_db).clamp(-30.0, 30.0),
                    _ => {}
                }
            }
        }
        let field = match knob {
            EqKnob::Freq => Some(EqField::Freq),
            EqKnob::Gain => Some(EqField::Gain),
            EqKnob::Q => Some(EqField::Q),
            _ => None,
        };
        if let Some(field) = field {
            let mut ids: Vec<u16> = others.iter().map(|b| b.id).collect();
            if !ids.contains(&id) {
                ids.push(id);
            }
            self.eq_moved(&ids, &[field]);
        }
    }

    pub fn set_eq_knob(&mut self, id: u16, knob: EqKnob, v: f32) {
        let Some(eq) = self.eq_mut() else { return };
        match knob {
            EqKnob::Scale => eq.scale = v.clamp(0.0, 2.0),
            EqKnob::Output => eq.output_db = v.clamp(-36.0, 36.0),
            EqKnob::Pan => eq.pan = v.clamp(-1.0, 1.0),
            _ => {
                let Some(i) = eq.bands.find(id) else { return };
                let b = &mut eq.bands[i];
                match knob {
                    EqKnob::Freq => b.freq = v.clamp(10.0, 30_000.0),
                    EqKnob::Gain => b.gain_db = v.clamp(-30.0, 30.0),
                    EqKnob::Q => b.q = v.clamp(0.025, 40.0),
                    EqKnob::Range => {
                        b.dynamic.get_or_insert_default().range_db = v.clamp(-30.0, 30.0)
                    }
                    EqKnob::Threshold => {
                        b.dynamic.get_or_insert_default().threshold_db = Some(v.clamp(-80.0, 0.0))
                    }
                    EqKnob::Attack => {
                        b.dynamic.get_or_insert_default().attack_ms = Some(v.clamp(0.1, 1_000.0))
                    }
                    EqKnob::Release => {
                        b.dynamic.get_or_insert_default().release_ms = Some(v.clamp(1.0, 5_000.0))
                    }
                    EqKnob::Density => {
                        b.dynamic.get_or_insert_default().density = v.clamp(0.0, 1.0)
                    }
                    EqKnob::Slope => b.slope = b.shape.fit_slope(v),
                    EqKnob::Low | EqKnob::High => {
                        let d = b.dynamic.get_or_insert_default();
                        let mut t = d.trigger.unwrap_or([b.freq / 2.0, b.freq * 2.0]);
                        t[(knob == EqKnob::High) as usize] = v.clamp(10.0, 30_000.0);
                        d.trigger = Some(t);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn eq_knob_value(&self, id: u16, knob: EqKnob) -> f32 {
        let Some((_, eq)) = self.shown_eq() else {
            return 0.0;
        };
        let b = eq.bands.find(id).map(|i| eq.bands[i]).unwrap_or_default();
        let d = b.dynamic.unwrap_or_default();
        let (attack, release) = auto_times(b.freq);
        let trigger = d.trigger.unwrap_or([b.freq / 2.0, b.freq * 2.0]);
        match knob {
            EqKnob::Freq => b.freq,
            EqKnob::Gain => b.gain_db,
            EqKnob::Q => b.q,
            EqKnob::Range => d.range_db,
            EqKnob::Threshold => d.threshold_db.unwrap_or(-30.0),
            EqKnob::Scale => eq.scale,
            EqKnob::Output => eq.output_db,
            EqKnob::Attack => d.attack_ms.unwrap_or(attack as f32),
            EqKnob::Release => d.release_ms.unwrap_or(release as f32),
            EqKnob::Low => trigger[0],
            EqKnob::High => trigger[1],
            EqKnob::Pan => eq.pan,
            EqKnob::Density => d.density,
            EqKnob::Slope => b.slope,
        }
    }

    /// What a knob goes back to on ⌘-click.
    pub(crate) fn eq_knob_reset(&mut self, id: u16, knob: EqKnob, cx: &mut Context<Self>) {
        let shape = self.eq_band(id).map_or(Shape::Bell, |(_, b)| b.shape);
        let d = Dynamic::default();
        // A dynamic band's threshold, attack and release go back to its own.
        if matches!(knob, EqKnob::Threshold | EqKnob::Attack | EqKnob::Release) {
            return self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut()
                    && let Some(i) = eq.bands.find(id)
                    && let Some(d) = eq.bands[i].dynamic.as_mut()
                {
                    match knob {
                        EqKnob::Threshold => d.threshold_db = None,
                        EqKnob::Attack => d.attack_ms = None,
                        _ => d.release_ms = None,
                    }
                }
            });
        }
        let v = match knob {
            EqKnob::Freq => 1_000.0,
            EqKnob::Gain | EqKnob::Output | EqKnob::Pan => 0.0,
            EqKnob::Q => shape.default_q(),
            EqKnob::Range => d.range_db,
            EqKnob::Density => d.density,
            EqKnob::Slope => 12.0,
            EqKnob::Scale => 1.0,
            EqKnob::Low => 100.0,
            EqKnob::High => 10_000.0,
            _ => return,
        };
        self.edit(cx, |ws| ws.set_eq_knob(id, knob, v));
    }

    /// A knob pressed: dragging it (⌘-click resets it, a double-click types a value).
    pub(crate) fn press_knob(
        &mut self,
        id: u16,
        knob: EqKnob,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if e.modifiers.platform {
            return self.eq_knob_reset(id, knob, cx);
        }
        if e.click_count == 2 && knob != EqKnob::Slope {
            self.pending_name = Some(Name::Eq(id, knob));
            cx.notify();
            return;
        }
        self.hold_picked();
        let start = self.eq_knob_value(id, knob);
        self.begin_drag(
            Drag::EqKnob {
                id,
                knob,
                from: f32::from(e.position.y),
                x: f32::from(e.position.x),
                start,
            },
            cx,
        );
    }

    /// A band's slopes to pick from.
    pub fn eq_slope_menu(
        &mut self,
        id: u16,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let items = b
            .shape
            .slopes()
            .iter()
            .map(|&s| {
                let tick = if s == b.slope { "✓ " } else { "" };
                (
                    format!("{tick}{}", slope_text(s)),
                    MenuAction::Eq(EqOp::Slope(s)),
                )
            })
            .collect();
        self.eq_menu(at, items, cx);
    }

    /// The wheel over a band's slope: a step through its slopes, ⇧ a dB an octave.
    pub(crate) fn wheel_slope(&mut self, id: u16, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        // With ⇧ a mouse's wheel may come sideways.
        let d = e.delta.pixel_delta(px(16.));
        let dy = f32::from(if d.y != px(0.) { d.y } else { d.x });
        if dy == 0.0 {
            return;
        }
        let fine = e.modifiers.shift;
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut()
                && let Some(i) = eq.bands.find(id)
            {
                step_slope(&mut eq.bands[i], dy.signum() as i32, fine);
            }
        });
        if self.session.unchanged() {
            self.session.cancel();
        }
    }

    /// The text a typed value starts as.
    pub fn eq_value_text(&self, id: u16, knob: EqKnob) -> String {
        let v = self.eq_knob_value(id, knob);
        match knob {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => match v < 1_000.0 {
                true => format!("{v:.0}"),
                false => format!("{:.2}k", v / 1_000.0),
            },
            EqKnob::Pan => EqKnob::Pan.text(v),
            EqKnob::Scale => format!("{:.0}%", v * 100.0),
            EqKnob::Q => q_text(v),
            _ => format!("{v:.1}"),
        }
    }

    /// A typed value set, if it reads as one.
    pub fn eq_value_typed(&mut self, id: u16, knob: EqKnob, text: &str, cx: &mut Context<Self>) {
        match knob.parse(text) {
            Some(v) => self.edit(cx, |ws| ws.set_eq_knob(id, knob, v)),
            None => {
                self.status = Some(format!(
                    "{text:?} isn't a {}: try {}",
                    knob.label().to_lowercase(),
                    match knob {
                        EqKnob::Freq => "100, 2k, A4 or C#2+13",
                        EqKnob::Gain => "-3, +4.5 or 2x",
                        _ => "a number",
                    }
                ));
                cx.notify();
            }
        }
    }
}
