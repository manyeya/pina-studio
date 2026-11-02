//! A limiter set by listening: what reaches it measured, a style picked for it, its release put
//! to the tempo, and its gain found with the limiter itself, so the loudness it gives is the
//! loudness aimed at.

use super::{Limiting, latency};
use crate::{
    music::analysis::loudness,
    song::project::{LimitStyle, Limiter},
};

/// Loudnesses to aim at, LUFS, by what the song's for.
pub const TARGETS: [(&str, f32); 5] = [
    ("Streaming", -14.0),
    ("Quieter streaming", -16.0),
    ("Broadcast", -23.0),
    ("Club", -9.0),
    ("Loud", -7.0),
];

/// What listening set, and the loudness it comes out at.
#[derive(Clone, Copy, Debug)]
pub struct Learned {
    pub limit: Limiter,
    pub lufs: f32,
}

/// The style for a sound pushed `push` dB, its peaks `plr` dB over its loudness, `bass` of its
/// power under 150 Hz.
fn style(push: f32, plr: f32, bass: f32) -> LimitStyle {
    match () {
        _ if push <= 2.0 => LimitStyle::Transparent,
        _ if bass > 0.5 && push > 4.0 => LimitStyle::Multiband,
        _ if plr > 18.0 && push > 6.0 => LimitStyle::Punchy,
        _ if push > 11.0 => LimitStyle::Aggressive,
        _ if push > 7.0 => LimitStyle::Modern,
        _ if plr > 15.0 => LimitStyle::Dynamic,
        _ => LimitStyle::Allround,
    }
}

/// How much of a sound's power is under 150 Hz: two one-pole low passes, both sides together.
fn bass_share(render: &[Vec<f32>; 2], rate: f64) -> f32 {
    let a = (-std::f64::consts::TAU * 150.0 / rate).exp();
    let (mut lo1, mut lo2, mut low, mut all) = (0.0, 0.0, 0.0, 0.0);
    for (l, r) in render[0].iter().zip(&render[1]) {
        let x = (*l as f64 + *r as f64) / 2.0;
        lo1 = x + (lo1 - x) * a;
        lo2 = lo1 + (lo2 - lo1) * a;
        low += lo2 * lo2;
        all += x * x;
    }
    (low / all.max(1e-12)) as f32
}

/// The loudness `render` comes out of the limiter at, set `lim`.
fn limited(render: &[Vec<f32>; 2], rate: f64, lim: &Limiter) -> f32 {
    let mut limiting = Limiting::new(rate);
    let (mut l, mut r) = (render[0].clone(), render[1].clone());
    for at in (0..l.len()).step_by(512) {
        let end = (at + 512).min(l.len());
        limiting.process(lim, &mut l[at..end], &mut r[at..end], false);
    }
    let late = (latency(lim, rate) as usize).min(l.len());
    loudness(&l[late..], &r[late..], rate)
}

/// A limiter set for `render` (what reaches it), from `current` (its ceiling, lookahead and
/// listening aids kept), to come out at `target` LUFS. None when it's silent.
pub fn learn(
    render: &[Vec<f32>; 2],
    rate: f64,
    bpm: f64,
    current: &Limiter,
    target: f32,
) -> Option<Learned> {
    let now = loudness(&render[0], &render[1], rate);
    if now <= -69.0 {
        return None;
    }
    let peak = render
        .iter()
        .flatten()
        .fold(0f32, |m, x| m.max(x.abs()))
        .max(1e-9);
    let plr = 20.0 * peak.log10() - now;
    let push = target - now;
    // A sixteenth at the tempo: it lets go in time with the music.
    let sixteenth = (60_000.0 / bpm.max(30.0) / 4.0) as f32;
    let mut lim = Limiter {
        style: style(push, plr, bass_share(render, rate)),
        release_ms: sixteenth.clamp(40.0, 400.0).round(),
        auto_release: true,
        bypass: false,
        target_lufs: Some(target),
        ..*current
    };
    // Up from where it'd be unlimited: a limiter gives back less than it's pushed, so each
    // step adds what's still missing, kept between the gains known too quiet and too loud.
    let (mut lo, mut hi) = (0f32, 30f32);
    let mut gain = push.clamp(lo, hi);
    let mut lufs = now;
    for _ in 0..12 {
        lim.gain_db = (gain * 10.0).round() / 10.0;
        lufs = limited(render, rate, &lim);
        let off = target - lufs;
        if off.abs() < 0.1 {
            break;
        }
        match off > 0.0 {
            true => lo = lim.gain_db,
            false => hi = lim.gain_db,
        }
        if hi - lo < 0.1 {
            break;
        }
        let next = lim.gain_db + off * 1.3;
        gain = match next > lo && next < hi {
            true => next,
            false => (lo + hi) / 2.0,
        };
    }
    Some(Learned { limit: lim, lufs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_finds_the_gain_for_the_loudness() {
        let quiet: Vec<f32> = super::super::tests::music(8.0)
            .iter()
            .map(|x| x * 0.25)
            .collect();
        let render = [quiet.clone(), quiet];
        let rate = 48_000.0;
        for target in [-14.0, -9.0] {
            let got = learn(&render, rate, 120.0, &Limiter::default(), target).unwrap();
            assert!((got.lufs - target).abs() < 0.3, "{target}: {got:?}");
            assert_eq!(got.limit.target_lufs, Some(target));
            assert!(got.limit.gain_db > 0.0);
            assert_eq!(got.limit.release_ms, 125.0);
        }
        assert!(
            learn(
                &[vec![0.0; 48_000], vec![0.0; 48_000]],
                rate,
                120.0,
                &Limiter::default(),
                -14.0
            )
            .is_none()
        );
    }

    #[test]
    fn styles_follow_the_sound() {
        assert_eq!(style(1.0, 12.0, 0.2), LimitStyle::Transparent);
        assert_eq!(style(6.0, 12.0, 0.6), LimitStyle::Multiband);
        assert_eq!(style(8.0, 20.0, 0.2), LimitStyle::Punchy);
        assert_eq!(style(12.0, 12.0, 0.2), LimitStyle::Aggressive);
        assert_eq!(style(8.0, 12.0, 0.2), LimitStyle::Modern);
        assert_eq!(style(4.0, 16.0, 0.2), LimitStyle::Dynamic);
        assert_eq!(style(4.0, 12.0, 0.2), LimitStyle::Allround);
    }
}
