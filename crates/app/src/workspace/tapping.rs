//! The playhead in time with what's heard. Each output says how late it is, and the playhead
//! follows that; Bluetooth says less than it is, so how much later it really is, is kept for
//! each output (a headset on a call apart from it off one), and learned by tapping along to the
//! click: where the song is heard to be at each tap, against the beat tapped to.

use super::*;

/// Taps kept to measure by; the first few, while the hand finds the beat, are left out.
const TAPS: usize = 8;
const SETTLING: usize = 2;
/// Taps further apart than this (s, typically from the middle) aren't one lateness.
const SPREAD: f64 = 0.035;

/// Tapping along: how far each tap was from its beat (s), and the click as it was before.
pub struct Tapping {
    pub offs: Vec<f64>,
    metronome_was: bool,
}

/// The middle of `x`, sorted in place.
fn median(x: &mut [f64]) -> f64 {
    x.sort_by(f64::total_cmp);
    match x.len() {
        0 => 0.0,
        n if n % 2 == 1 => x[n / 2],
        n => (x[n / 2 - 1] + x[n / 2]) / 2.0,
    }
}

/// How late taps `offs` (s from their beats) say the sound is, if they agree.
pub(crate) fn lateness(offs: &[f64]) -> Option<f64> {
    let mut x = offs.to_vec();
    let mid = median(&mut x);
    let mut apart: Vec<f64> = offs.iter().map(|o| (o - mid).abs()).collect();
    (median(&mut apart) <= SPREAD).then_some(mid)
}

impl Workspace {
    /// The output as its lateness is kept: a headset on a call (its own low rate) apart.
    pub(crate) fn heard_key(&self) -> Option<String> {
        let name = self.player.output_device()?;
        Some(match self.player.rate() < 32_000.0 {
            true => format!("{name} · on a call"),
            false => name,
        })
    }

    /// How much later than it says the output's heard (ms).
    pub fn heard_later_ms(&self) -> f32 {
        self.heard_key()
            .and_then(|k| self.settings.heard_later.get(&k).copied())
            .unwrap_or(0.0)
    }

    /// The playhead put as late as this output's kept lateness: when the engine starts or the
    /// output changes.
    pub fn apply_heard_later(&self) {
        self.player.set_heard_later(self.heard_later_ms());
    }

    pub(crate) fn set_heard_later(&mut self, ms: f32, cx: &mut Context<Self>) {
        let Some(key) = self.heard_key() else { return };
        self.settings
            .heard_later
            .insert(key, (ms * 10.0).round() / 10.0);
        self.settings.save();
        self.apply_heard_later();
        cx.notify();
    }

    /// Tap along: the song plays with the click (wherever it's set to sound); each tap (Space
    /// or the pad) on a beat. Again, it stops.
    pub fn start_tapping(&mut self, cx: &mut Context<Self>) {
        if self.tapping.is_some() {
            return self.stop_tapping(cx);
        }
        let was = self.player.metronome();
        self.player.set_metronome(true);
        self.player.set_click_gate(true);
        self.tapping = Some(Tapping {
            offs: Vec::new(),
            metronome_was: was,
        });
        if !self.player.playing() {
            self.play_stop(cx);
        }
        cx.notify();
    }

    pub fn stop_tapping(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.tapping.take() {
            self.player.set_metronome(t.metronome_was);
            self.gate_click();
            if self.player.playing() {
                self.play_stop(cx);
            }
        }
        cx.notify();
    }

    /// A tap: where the song's heard now, against the beat it's for. Ahead of it, the playhead's
    /// early by as much. Enough taps that agree set how late the output is.
    pub fn tap(&mut self, cx: &mut Context<Self>) {
        if !self.player.playing() {
            return;
        }
        let p = &self.session.project;
        let tick = p.ticks(self.player.position(), self.player.rate()) as f64;
        let beat = p.beat().max(1) as f64;
        // Taken as late, not early, up to three quarters of a beat: Bluetooth is often over
        // half of one at a quick tempo.
        let nearest = (tick / beat + 0.25).floor() * beat;
        let off = p.seconds_at(tick) - p.seconds_at(nearest);
        let Some(t) = &mut self.tapping else { return };
        t.offs.push(off);
        if t.offs.len() < TAPS + SETTLING {
            return cx.notify();
        }
        match lateness(&t.offs[SETTLING..]) {
            Some(late) => {
                let ms = self.heard_later_ms() + (late * 1000.0) as f32;
                self.set_heard_later(ms, cx);
                self.status = Some(format!(
                    "The playhead's now {ms:.0} ms later than {} says, in time with what you hear",
                    self.player.output_device().unwrap_or_default()
                ));
                self.stop_tapping(cx);
            }
            None => {
                t.offs.clear();
                self.status = Some(
                    "The taps didn't agree: tap again, on each click, as steadily as you can"
                        .into(),
                );
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_that_agree_say_how_late_and_scattered_ones_dont() {
        // Heard 180 ms after the playhead, the taps a little either side.
        let taps = [0.17, 0.19, 0.18, 0.185, 0.175, 0.18, 0.2, 0.16];
        assert!((lateness(&taps).unwrap() - 0.18).abs() < 0.005);
        // One stray tap doesn't move it.
        let one_off = [0.18, 0.18, 0.19, -0.2, 0.17, 0.18, 0.18, 0.18];
        assert!((lateness(&one_off).unwrap() - 0.18).abs() < 0.005);
        // All over the place: none.
        let wild = [0.0, 0.2, -0.2, 0.1, -0.15, 0.25, -0.05, 0.12];
        assert_eq!(lateness(&wild), None);
    }
}
