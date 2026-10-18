//! The Sampler editor, in the pane under the tracks: the selected track's built-in sampler, as
//! Logic's Sampler and Quick Sampler, Ableton's Simpler and FL Studio's Slicex show theirs. Its
//! zones down the left (drop samples there, or anywhere on it); the picked zone's sample as a
//! waveform, its start, end and loop dragged in place and its slices marked; the keys, each zone
//! over the notes it plays (click a key to hear it); and how it plays: the zone's notes, tuning,
//! level, pan and velocities; the instrument's pitch and glide, amp envelope, filter (with its
//! envelope) and LFO, as knobs. A loop can be sliced across the keys at its hits or into equal
//! parts. Every change is an edit (undone like any) that the playing sampler follows at once.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, PathPromptOptions, Point,
    Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    commands::{self, Command},
    engine::read_audio,
    peaks::Peaks,
    project::{FilterMode, Instrument, LoopMode, Sampler, Zone},
    sampler::{cuts, onsets, slices},
};

use crate::{
    menu::{Menu, MenuAction},
    pianoroll::name_in,
    theme::*,
    workspace::{Drag, Target, Workspace, pt},
};

/// A sample's waveform and hits, worked out once.
pub struct Wave {
    peaks: Peaks,
    len: usize,
    seconds: f32,
    onsets: Vec<(f32, f32)>,
}

#[derive(Default)]
pub struct SamplerUi {
    /// The zone picked, by index.
    pub zone: usize,
    /// Waveforms by file (relative to the song), None while one's being read.
    waves: HashMap<PathBuf, Option<Arc<Wave>>>,
}

/// A sampler setting a knob turns: the instrument's, or a zone's (by index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Attack,
    Decay,
    Sustain,
    Release,
    Cutoff,
    Resonance,
    FilterEnv,
    FilterAttack,
    FilterDecay,
    FilterSustain,
    FilterRelease,
    KeyTrack,
    Velocity,
    Glide,
    LfoRate,
    Vibrato,
    LfoFilter,
    Tremolo,
    Transpose,
    Volume,
    Note(usize),
    High(usize),
    Root(usize),
    Tune(usize),
    Gain(usize),
    Pan(usize),
    VelLow(usize),
    VelHigh(usize),
}

/// How a zone's sample is cut into slices across the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slicing {
    /// At its strongest hits, at most this many.
    Hits(usize),
    /// Into this many equal parts.
    Equal(usize),
}

/// Which mark on the waveform is dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Start,
    End,
    LoopStart,
    LoopEnd,
}

/// Milliseconds on a knob: 0 at the bottom, then 1 ms to 10 s, evenly by ratio.
fn ms_of(v: f32) -> f32 {
    if v < 0.01 {
        0.0
    } else {
        10f32.powf(4.0 * (v - 0.01) / 0.99)
    }
}

fn ms_pos(ms: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        0.01 + 0.99 * (ms.max(1.0).log10() / 4.0).clamp(0.0, 1.0)
    }
}

fn ms_text(ms: f32) -> String {
    match ms {
        m if m <= 0.0 => "0".into(),
        m if m < 10.0 => format!("{m:.1} ms"),
        m if m < 1000.0 => format!("{m:.0} ms"),
        m => format!("{:.2} s", m / 1000.0),
    }
}

/// Where a setting sits on its knob, 0 to 1.
pub fn field_pos(s: &Sampler, f: Field) -> f32 {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    match f {
        Field::Attack => ms_pos(v.attack_ms),
        Field::Decay => ms_pos(v.decay_ms),
        Field::Sustain => v.sustain,
        Field::Release => ms_pos(v.release_ms),
        Field::Cutoff => (v.cutoff_hz / 20.0).max(1.0).log10() / 3.0,
        Field::Resonance => v.resonance,
        Field::FilterEnv => (v.filter_env + 6.0) / 12.0,
        Field::FilterAttack => ms_pos(v.filter_attack_ms),
        Field::FilterDecay => ms_pos(v.filter_decay_ms),
        Field::FilterSustain => v.filter_sustain,
        Field::FilterRelease => ms_pos(v.filter_release_ms),
        Field::KeyTrack => v.key_track,
        Field::Velocity => v.velocity,
        Field::Glide => ms_pos(v.glide_ms),
        Field::LfoRate => (v.lfo_hz / 0.05).max(1.0).log10() / 400f32.log10(),
        Field::Vibrato => v.vibrato_cents / 100.0,
        Field::LfoFilter => v.lfo_filter / 4.0,
        Field::Tremolo => v.tremolo,
        Field::Transpose => (v.transpose + 24.0) / 48.0,
        Field::Volume => (v.gain_db + 24.0) / 36.0,
        Field::Note(i) => z(i).note as f32 / 127.0,
        Field::High(i) => z(i).high.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Root(i) => z(i).root.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Tune(i) => (z(i).tune_cents + 100.0) / 200.0,
        Field::Gain(i) => (z(i).gain_db + 24.0) / 36.0,
        Field::Pan(i) => (z(i).pan + 1.0) / 2.0,
        Field::VelLow(i) => z(i).velocities.unwrap_or((1, 127)).0 as f32 / 127.0,
        Field::VelHigh(i) => z(i).velocities.unwrap_or((1, 127)).1 as f32 / 127.0,
    }
    .clamp(0.0, 1.0)
}

/// Turns a setting to `v` (0 to 1) on its knob.
pub fn field_set(s: &mut Sampler, f: Field, v: f32) {
    let v = v.clamp(0.0, 1.0);
    let note = |v: f32| (v * 127.0).round() as u8;
    let voice = &mut s.voice;
    match f {
        Field::Attack => voice.attack_ms = ms_of(v),
        Field::Decay => voice.decay_ms = ms_of(v),
        Field::Sustain => voice.sustain = v,
        Field::Release => voice.release_ms = ms_of(v),
        Field::Cutoff => voice.cutoff_hz = 20.0 * 1000f32.powf(v),
        Field::Resonance => voice.resonance = v,
        Field::FilterEnv => voice.filter_env = ((v * 12.0 - 6.0) * 10.0).round() / 10.0,
        Field::FilterAttack => voice.filter_attack_ms = ms_of(v),
        Field::FilterDecay => voice.filter_decay_ms = ms_of(v),
        Field::FilterSustain => voice.filter_sustain = v,
        Field::FilterRelease => voice.filter_release_ms = ms_of(v),
        Field::KeyTrack => voice.key_track = v,
        Field::Velocity => voice.velocity = v,
        Field::Glide => voice.glide_ms = ms_of(v).min(4000.0),
        Field::LfoRate => voice.lfo_hz = 0.05 * 400f32.powf(v),
        Field::Vibrato => voice.vibrato_cents = (v * 100.0).round(),
        Field::LfoFilter => voice.lfo_filter = v * 4.0,
        Field::Tremolo => voice.tremolo = v,
        Field::Transpose => voice.transpose = (v * 48.0 - 24.0).round(),
        Field::Volume => voice.gain_db = ((v * 36.0 - 24.0) * 2.0).round() / 2.0,
        Field::Note(i)
        | Field::High(i)
        | Field::Root(i)
        | Field::Tune(i)
        | Field::Gain(i)
        | Field::Pan(i)
        | Field::VelLow(i)
        | Field::VelHigh(i) => {
            let Some(z) = s.zones.get_mut(i) else { return };
            match f {
                Field::Note(_) => {
                    let n = note(v);
                    // A range keeps its width; a kit's sound just moves.
                    if let Some(h) = z.high {
                        let width = h.saturating_sub(z.note);
                        z.high = Some(n.saturating_add(width).min(127));
                        z.root = z
                            .root
                            .map(|r| (r as i32 + n as i32 - z.note as i32).clamp(0, 127) as u8);
                    }
                    z.note = n;
                }
                Field::High(_) => {
                    let h = note(v).max(z.note);
                    z.high = (h > z.note).then_some(h);
                }
                Field::Root(_) => z.root = Some(note(v)),
                Field::Tune(_) => z.tune_cents = (v * 200.0 - 100.0).round(),
                Field::Gain(_) => z.gain_db = ((v * 36.0 - 24.0) * 2.0).round() / 2.0,
                Field::Pan(_) => z.pan = ((v * 2.0 - 1.0) * 50.0).round() / 50.0,
                Field::VelLow(_) => {
                    let (_, hi) = z.velocities.unwrap_or((1, 127));
                    let lo = (v * 127.0).round().clamp(1.0, hi as f32) as u8;
                    z.velocities = Some((lo, hi)).filter(|&r| r != (1, 127));
                }
                _ => {
                    let (lo, _) = z.velocities.unwrap_or((1, 127));
                    let hi = (v * 127.0).round().clamp(lo as f32, 127.0) as u8;
                    z.velocities = Some((lo, hi)).filter(|&r| r != (1, 127));
                }
            }
        }
    }
}

/// A setting as it reads under its knob.
fn field_text(s: &Sampler, f: Field, key: Option<tempo_engine::music::Key>) -> String {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    match f {
        Field::Attack => ms_text(v.attack_ms),
        Field::Decay => ms_text(v.decay_ms),
        Field::Sustain | Field::FilterSustain => format!(
            "{:.0}%",
            if f == Field::Sustain {
                v.sustain
            } else {
                v.filter_sustain
            } * 100.0
        ),
        Field::Release => ms_text(v.release_ms),
        Field::Cutoff => match v.cutoff_hz {
            c if c >= 1000.0 => format!("{:.1} kHz", c / 1000.0),
            c => format!("{c:.0} Hz"),
        },
        Field::Resonance => format!("{:.0}%", v.resonance * 100.0),
        Field::FilterEnv => format!("{:+.1} oct", v.filter_env),
        Field::FilterAttack => ms_text(v.filter_attack_ms),
        Field::FilterDecay => ms_text(v.filter_decay_ms),
        Field::FilterRelease => ms_text(v.filter_release_ms),
        Field::KeyTrack => format!("{:.0}%", v.key_track * 100.0),
        Field::Velocity => format!("{:.0}%", v.velocity * 100.0),
        Field::Glide => ms_text(v.glide_ms),
        Field::LfoRate => format!("{:.2} Hz", v.lfo_hz),
        Field::Vibrato => format!("{:.0} ct", v.vibrato_cents),
        Field::LfoFilter => format!("{:.1} oct", v.lfo_filter),
        Field::Tremolo => format!("{:.0}%", v.tremolo * 100.0),
        Field::Transpose => format!("{:+.0} st", v.transpose),
        Field::Volume | Field::Gain(_) => {
            let db = if f == Field::Volume {
                v.gain_db
            } else {
                z(match f {
                    Field::Gain(i) => i,
                    _ => 0,
                })
                .gain_db
            };
            format!("{db:+.1} dB").replace('-', "−")
        }
        Field::Note(i) => name_in(key, z(i).note),
        Field::High(i) => name_in(key, z(i).high.unwrap_or(z(i).note)),
        Field::Root(i) => name_in(key, z(i).root.unwrap_or(z(i).note)),
        Field::Tune(i) => format!("{:+.0} ct", z(i).tune_cents),
        Field::Pan(i) => match z(i).pan {
            p if p.abs() < 0.01 => "C".into(),
            p if p < 0.0 => format!("L{:.0}", -p * 100.0),
            p => format!("R{:.0}", p * 100.0),
        },
        Field::VelLow(i) => z(i).velocities.unwrap_or((1, 127)).0.to_string(),
        Field::VelHigh(i) => z(i).velocities.unwrap_or((1, 127)).1.to_string(),
    }
}

/// A zone's colour, by its place.
fn zone_color(i: usize) -> u32 {
    [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, 0xfb923c][i % 8]
}

fn zone_name(z: &Zone) -> String {
    match z.name.is_empty() {
        true => z
            .file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        false => z.name.clone(),
    }
}

impl Workspace {
    /// The selected track, if it plays the built-in sampler.
    pub fn sampler_track(&self) -> Option<usize> {
        let t = self.session.project.tracks.get(self.track)?;
        matches!(t.instrument, Some(Instrument::Sampler(_))).then_some(self.track)
    }

    fn sampler_of(&self, t: usize) -> Option<&Sampler> {
        match &self.session.project.tracks.get(t)?.instrument {
            Some(Instrument::Sampler(s)) => Some(s),
            _ => None,
        }
    }

    /// An edit to a sampler's settings (its samples the same): undone like any edit, and
    /// followed by the playing sampler without a reload.
    fn edit_sampler(&mut self, t: usize, cx: &mut Context<Self>, f: impl FnOnce(&mut Sampler)) {
        self.edit(cx, |ws| {
            if let Some(Instrument::Sampler(s)) = &mut ws.session.project.tracks[t].instrument {
                f(s);
            }
        });
    }

    /// A sampler setting's knob position (for `Target::Sampler`).
    pub fn sampler_pos(&self, t: usize, f: Field) -> f32 {
        self.sampler_of(t).map_or(0.0, |s| field_pos(s, f))
    }

    pub fn sampler_set(&mut self, t: usize, f: Field, v: f32) {
        if let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(t)
            .map(|t| &mut t.instrument)
        {
            field_set(s, f, v);
        }
    }

    /// A sample's waveform, read in the background the first time it's asked for.
    fn wave(&mut self, file: &Path, cx: &mut Context<Self>) -> Option<Arc<Wave>> {
        if let Some(w) = self.sampler_ui.waves.get(file) {
            return w.clone();
        }
        self.sampler_ui.waves.insert(file.to_path_buf(), None);
        let (key, full) = (file.to_path_buf(), self.session.dir.join(file));
        let job = cx.background_spawn(async move {
            let (sides, rate) = read_audio(&full).ok()?;
            let n = sides.first()?.len();
            let mono: Vec<f32> = (0..n)
                .map(|i| sides.iter().map(|s| s[i]).sum::<f32>() / sides.len() as f32)
                .collect();
            Some(Wave {
                peaks: Peaks::from_samples(&mono),
                len: n,
                seconds: n as f32 / rate as f32,
                onsets: onsets(&mono, rate as f32),
            })
        });
        cx.spawn(async move |this, cx| {
            let wave = job.await;
            this.update(cx, |ws, cx| {
                ws.sampler_ui.waves.insert(key, wave.map(Arc::new));
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    /// Samples added to the selected sampler: a sound on each free note from C1 (a kit), or one
    /// played across the keys (with none yet, and a single file).
    pub fn add_samples(&mut self, files: &[PathBuf], cx: &mut Context<Self>) {
        let Some(t) = self.sampler_track() else {
            return;
        };
        let Some(mut sampler) = self.sampler_of(t).cloned() else {
            return;
        };
        let across = sampler.zones.is_empty() && files.len() == 1;
        for f in files {
            let name = f
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let zone = match across {
                true => Zone {
                    file: f.clone(),
                    name,
                    note: 0,
                    high: Some(127),
                    root: Some(60),
                    gate: true,
                    ..Zone::default()
                },
                false => {
                    let taken = |n: u8| sampler.zones.iter().any(|z| z.covers(n));
                    let Some(note) = (36..=127).find(|&n| !taken(n)) else {
                        self.status = Some("There's no free note left on this sampler.".into());
                        break;
                    };
                    Zone {
                        file: f.clone(),
                        name,
                        note,
                        ..Zone::default()
                    }
                }
            };
            sampler.zones.push(zone);
        }
        let count = sampler.zones.len();
        let dir = self.session.dir.clone();
        self.session.checkpoint();
        match commands::apply(
            &mut self.session.project,
            &dir,
            Command::SetInstrument {
                track: t,
                instrument: Instrument::Sampler(sampler),
            },
        ) {
            Ok(_) => {
                self.sampler_ui.zone = count.saturating_sub(1);
                self.changed(cx);
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    fn pick_samples(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to Sampler".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| ws.add_samples(&paths, cx)).ok();
        })
        .detach();
    }

    /// A new track with an empty sampler, selected, its editor open.
    fn new_sampler_track(&mut self, cx: &mut Context<Self>) {
        let n = self.session.project.tracks.len();
        let dir = self.session.dir.clone();
        self.session.checkpoint();
        let add = Command::AddTrack {
            name: format!("Sampler {}", n + 1),
            instrument: Some(Instrument::Sampler(Sampler::default())),
            index: None,
        };
        match commands::apply(&mut self.session.project, &dir, add) {
            Ok(_) => {
                self.select_track(self.session.project.tracks.len() - 1);
                self.sampler_ui.zone = 0;
                self.changed(cx);
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// The picked zone's sample cut into slices across the keys, in place of the zones that
    /// play it.
    pub fn slice_zone(&mut self, how: Slicing, cx: &mut Context<Self>) {
        let Some(t) = self.sampler_track() else {
            return;
        };
        let Some(s) = self.sampler_of(t) else { return };
        let Some(z) = s.zones.get(self.sampler_ui.zone).cloned() else {
            return;
        };
        let Some(Some(wave)) = self.sampler_ui.waves.get(&z.file).cloned() else {
            self.status = Some("Still reading that sample: try again in a moment.".into());
            cx.notify();
            return;
        };
        let cut: Vec<f32> = match how {
            Slicing::Hits(most) => cuts(&wave.onsets, most),
            Slicing::Equal(n) => (0..n).map(|k| k as f32 / n as f32).collect(),
        };
        // From C1; or, slicing slices again, from where they were.
        let same: Vec<&Zone> = s.zones.iter().filter(|o| o.file == z.file).collect();
        let first = match same.iter().all(|o| o.high.is_none()) {
            true => same.iter().map(|o| o.note).min().unwrap_or(36),
            false => 36,
        };
        let file = z.file.clone();
        let count = cut.len();
        self.edit_sampler(t, cx, |s| {
            let at = s
                .zones
                .iter()
                .position(|o| o.file == file)
                .unwrap_or(s.zones.len());
            s.zones.retain(|o| o.file != file);
            let mut new = slices(&file, &cut, first);
            // Out of the way of the other zones: sounds already on those notes move up.
            for o in s.zones.iter_mut() {
                if o.note >= first && o.note < first + new.len() as u8 && o.high.is_none() {
                    o.note = o.note.saturating_add(new.len() as u8).min(127);
                }
            }
            let at = at.min(s.zones.len());
            for (k, zone) in new.drain(..).enumerate() {
                s.zones.insert(at + k, zone);
            }
        });
        self.status = Some(format!(
            "Sliced into {count} across the keys from {}",
            name_in(None, first)
        ));
        cx.notify();
    }

    /// A mark on the waveform dragged: the zone's start, end or loop points.
    pub(crate) fn drag_mark(&mut self, p: Point<f32>) {
        let Some(Drag::Mark {
            track,
            zone,
            mark,
            left,
            width,
        }) = self.drag
        else {
            return;
        };
        let at = ((p.x - left) / width.max(1.0)).clamp(0.0, 1.0);
        if let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(track)
            .map(|t| &mut t.instrument)
            && let Some(z) = s.zones.get_mut(zone)
        {
            let end = z.end.unwrap_or(1.0);
            match mark {
                Mark::Start => z.start = at.min(end - 0.001),
                Mark::End => z.end = Some(at.max(z.start + 0.001)).filter(|&e| e < 0.9995),
                Mark::LoopStart => {
                    z.loop_start = Some(at.clamp(z.start, z.loop_end.unwrap_or(end) - 0.001))
                }
                Mark::LoopEnd => {
                    z.loop_end = Some(at.clamp(z.loop_start.unwrap_or(z.start) + 0.001, end))
                }
            }
        }
    }

    /// The sampler pane's own controls in the editor's bar.
    pub fn sampler_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.sampler_track()?;
        let s = self.sampler_of(t)?.clone();
        let v = s.voice;
        let toggle = |label: &'static str, on: bool, sel: &'static str| {
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .bg(if on {
                    alpha(ACCENT, 0.16)
                } else {
                    rgb(0x23232a).into()
                })
                .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || sel.into())
                .child(label)
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(toggle("Mono", v.mono, "sampler-mono").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| s.voice.mono ^= true)
                    }),
                ))
                .child(toggle("Legato", v.legato, "sampler-legato").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| {
                            s.voice.legato ^= true;
                            s.voice.mono |= s.voice.legato;
                        })
                    }),
                ))
                .child(toggle("Slice…", false, "sampler-slice").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = vec![
                            (
                                "At its hits (up to 8)".to_string(),
                                MenuAction::Slice(Slicing::Hits(8)),
                            ),
                            (
                                "At its hits (up to 16)".to_string(),
                                MenuAction::Slice(Slicing::Hits(16)),
                            ),
                            (
                                "At its hits (up to 32)".to_string(),
                                MenuAction::Slice(Slicing::Hits(32)),
                            ),
                            (
                                "Into 4 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(4)),
                            ),
                            (
                                "Into 8 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(8)),
                            ),
                            (
                                "Into 16 equal parts".to_string(),
                                MenuAction::Slice(Slicing::Equal(16)),
                            ),
                        ];
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                ))
                .child(
                    button("+ Sample", false)
                        .debug_selector(|| "sampler-add".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.pick_samples(cx)),
                        ),
                ),
        )
    }

    /// A knob for a sampler setting: turned by dragging up and down.
    fn sampler_knob(
        &self,
        t: usize,
        f: Field,
        label: &'static str,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = self.sampler_of(t).cloned().unwrap_or_default();
        let value = field_pos(&s, f);
        let bipolar = matches!(
            f,
            Field::FilterEnv | Field::Transpose | Field::Tune(_) | Field::Pan(_)
        );
        let shown = if bipolar { value * 2.0 - 1.0 } else { value };
        let text = field_text(&s, f, self.session.project.song_key());
        let target = Target::Sampler { track: t, field: f };
        div()
            .w(px(50.))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .cursor_ns_resize()
            .debug_selector(move || format!("sampler-{f:?}"))
            .child(div().size(px(28.)).child(knob(shown, 28., color, bipolar)))
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(text),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let value = ws.target(target);
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
    }

    pub fn sampler_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(t) = self.sampler_track() else {
            return self.sampler_none(cx).into_any_element();
        };
        let s = self.sampler_of(t).cloned().unwrap_or_default();
        if s.zones.is_empty() {
            return self.sampler_empty(cx).into_any_element();
        }
        let picked = self.sampler_ui.zone.min(s.zones.len() - 1);
        self.sampler_ui.zone = picked;
        let z = s.zones[picked].clone();
        let wave = self.wave(&z.file, cx);
        let key = self.session.project.song_key();
        let color = zone_color(picked);
        let track_id = self.session.project.tracks[t].id;

        // The zones, down the left.
        let rows = s.zones.iter().enumerate().map(|(i, zone)| {
            let on = i == picked;
            let range = match zone.high {
                Some(h) => format!("{}–{}", name_in(key, zone.note), name_in(key, h)),
                None => name_in(key, zone.note),
            };
            let vel = zone
                .velocities
                .map(|(a, b)| format!(" · v{a}–{b}"))
                .unwrap_or_default();
            div()
                .id(("zone", i))
                .debug_selector(move || format!("zone-{i}"))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(7.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(RAISED)))
                .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                .child(
                    div()
                        .size(px(8.))
                        .flex_none()
                        .rounded(px(2.))
                        .bg(rgb(zone_color(i))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(zone_name(zone)),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .child(format!("{range}{vel}")),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.sampler_ui.zone = i;
                        let pitch = ws.sampler_of(t).and_then(|s| s.zones.get(i)).map(|z| {
                            z.root
                                .unwrap_or(z.note)
                                .clamp(z.note, z.high.unwrap_or(z.note))
                        });
                        if let Some(p) = pitch {
                            ws.player.preview(track_id, p, 100);
                            ws.begin_drag(
                                Drag::Key {
                                    track: track_id,
                                    pitch: p,
                                },
                                cx,
                            );
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, _, _, cx| {
                        ws.sampler_ui.zone = 0;
                        ws.edit_sampler(t, cx, |s| {
                            if i < s.zones.len() {
                                s.zones.remove(i);
                            }
                        });
                        ws.status = Some("Zone removed (⌘Z puts it back)".into());
                    }),
                )
        });
        let left = div()
            .w(px(HEADER_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .px(px(12.))
                    .pt(px(10.))
                    .pb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_3))
                            .child(format!("ZONES · {}", s.zones.len())),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_4))
                            .child("right-click removes"),
                    ),
            )
            .child(
                div()
                    .id("zones")
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                div()
                    .m(px(8.))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .border_1()
                    .border_dashed()
                    .border_color(if self.file_drag.is_some() {
                        rgb(ACCENT).into()
                    } else {
                        alpha(0xffffff, 0.14)
                    })
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .cursor_pointer()
                    .hover(|d| d.bg(alpha(0xffffff, 0.04)))
                    .child("Drop samples, or click")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.pick_samples(cx)),
                    ),
            );

        // The waveform, with the zone's marks and the other slices of its sample.
        let siblings: Vec<(usize, f32, u8)> = s
            .zones
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != picked && o.file == z.file)
            .map(|(i, o)| (i, o.start, o.note))
            .collect();
        let (start, end) = (z.start, z.end.unwrap_or(1.0));
        let looping = z.loop_mode != LoopMode::Off;
        let (ls, le) = (z.loop_start.unwrap_or(start), z.loop_end.unwrap_or(end));
        let wave_for_paint = wave.clone();
        let reverse = z.reverse;
        let drawn = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x), px(y)), size(px(ww), px(hh))),
                        c,
                    ));
                };
                quad(window, x0, y0 + h / 2.0, w, 1.0, alpha(0xffffff, 0.06));
                let Some(wave) = &wave_for_paint else { return };
                let mid = y0 + h / 2.0;
                let cols = w.max(1.0) as usize;
                for c in 0..cols {
                    let (a, bb) = (
                        c * wave.len / cols,
                        ((c + 1) * wave.len / cols).max(c * wave.len / cols + 1),
                    );
                    let (lo, hi) = wave.peaks.range(a, bb);
                    let frac = c as f32 / cols as f32;
                    let inside = frac >= start && frac <= end;
                    let ink = if inside {
                        alpha(color, 0.85)
                    } else {
                        alpha(0xffffff, 0.16)
                    };
                    let (top, bottom) =
                        (mid - hi.max(0.0) * h * 0.46, mid - lo.min(0.0) * h * 0.46);
                    quad(
                        window,
                        x0 + c as f32,
                        top,
                        1.0,
                        (bottom - top).max(1.0),
                        ink,
                    );
                }
                // Outside what plays, shaded; the loop lit.
                quad(window, x0, y0, w * start, h, alpha(0x000000, 0.35));
                quad(
                    window,
                    x0 + w * end,
                    y0,
                    w * (1.0 - end),
                    h,
                    alpha(0x000000, 0.35),
                );
                if looping {
                    quad(
                        window,
                        x0 + w * ls,
                        y0,
                        w * (le - ls),
                        h,
                        alpha(color, 0.08),
                    );
                    quad(
                        window,
                        x0 + w * ls,
                        y0,
                        w * (le - ls),
                        3.0,
                        rgb(color).into(),
                    );
                }
                let _ = reverse;
            },
        )
        .absolute()
        .size_full();
        let mark = |ws: &Self,
                    at: f32,
                    which: Mark,
                    label: &'static str,
                    tone: u32,
                    cx: &mut Context<Self>| {
            let _ = ws;
            div()
                .absolute()
                .left(gpui::relative(at))
                .top_0()
                .bottom_0()
                .w(px(9.))
                .ml(px(-4.))
                .cursor_ew_resize()
                .debug_selector(move || format!("mark-{which:?}"))
                .child(
                    div()
                        .absolute()
                        .left(px(4.))
                        .top_0()
                        .bottom_0()
                        .w(px(1.5))
                        .bg(rgb(tone)),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(4.))
                        .top(px(if matches!(which, Mark::LoopStart | Mark::LoopEnd) {
                            4.
                        } else {
                            0.
                        }))
                        .px(px(4.))
                        .rounded_r(px(3.))
                        .bg(rgb(tone))
                        .text_size(px(9.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(BG))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let b = ws.sampler_wave_bounds.get();
                        let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
                        let _ = e;
                        ws.begin_drag(
                            Drag::Mark {
                                track: t,
                                zone: picked,
                                mark: which,
                                left,
                                width,
                            },
                            cx,
                        );
                    }),
                )
        };
        let wave_bounds = self.sampler_wave_bounds.clone();
        let header = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().size(px(9.)).rounded(px(2.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(zone_name(&z)),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(match &wave {
                        Some(w) => format!(
                            "{:.2} s · plays {:.0}–{:.0}%",
                            w.seconds,
                            start * 100.0,
                            end * 100.0
                        ),
                        None => "reading…".into(),
                    }),
            )
            .child(div().flex_1())
            .child(self.zone_switches(t, picked, &z, cx));
        let wave_view = div()
            .flex_1()
            .min_h(px(90.))
            .relative()
            .rounded(px(10.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .debug_selector(|| "sampler-wave".into())
            .child(
                canvas(move |b, _, _| wave_bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(drawn)
            .children(siblings.iter().map(|&(i, at, note)| {
                div()
                    .absolute()
                    .left(gpui::relative(at))
                    .top_0()
                    .bottom_0()
                    .w(px(1.))
                    .bg(alpha(zone_color(i), 0.7))
                    .child(
                        div()
                            .absolute()
                            .left(px(3.))
                            .bottom(px(3.))
                            .font_family(MONO)
                            .text_size(px(9.))
                            .whitespace_nowrap()
                            .text_color(rgb(zone_color(i)))
                            .child(name_in(None, note)),
                    )
            }))
            .child(mark(self, start, Mark::Start, "S", MINT, cx))
            .child(mark(self, end, Mark::End, "E", CORAL, cx))
            .when(looping, |d| {
                d.child(mark(self, ls, Mark::LoopStart, "L", AMBER, cx))
                    .child(mark(self, le, Mark::LoopEnd, "L", AMBER, cx))
            });

        // The keys, each zone over the notes it plays; click one to hear it.
        let keymap = self.sampler_keys(t, &s, picked, cx);

        // How it plays.
        let card = |title: &'static str, side: Option<Div>| {
            div()
                .flex_none()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(10.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_3))
                                .child(title),
                        )
                        .child(div().flex_1())
                        .children(side),
                )
        };
        let row = || div().flex().items_start().gap(px(2.));
        let zone_card = card("ZONE", None).child(
            row()
                .child(self.sampler_knob(
                    t,
                    Field::Note(picked),
                    if z.high.is_some() { "Low" } else { "Note" },
                    color,
                    cx,
                ))
                .when(z.high.is_some(), |d| {
                    d.child(self.sampler_knob(t, Field::High(picked), "High", color, cx))
                        .child(self.sampler_knob(t, Field::Root(picked), "Root", color, cx))
                })
                .child(self.sampler_knob(t, Field::Tune(picked), "Fine", color, cx))
                .child(self.sampler_knob(t, Field::Gain(picked), "Level", color, cx))
                .child(self.sampler_knob(t, Field::Pan(picked), "Pan", color, cx))
                .child(self.sampler_knob(t, Field::VelLow(picked), "Vel lo", color, cx))
                .child(self.sampler_knob(t, Field::VelHigh(picked), "Vel hi", color, cx)),
        );
        let v = s.voice;
        let amp_card = card(
            "AMP",
            Some(envelope(v.attack_ms, v.decay_ms, v.sustain, v.release_ms, MINT).w(px(110.))),
        )
        .child(
            row()
                .child(self.sampler_knob(t, Field::Attack, "Attack", MINT, cx))
                .child(self.sampler_knob(t, Field::Decay, "Decay", MINT, cx))
                .child(self.sampler_knob(t, Field::Sustain, "Sustain", MINT, cx))
                .child(self.sampler_knob(t, Field::Release, "Release", MINT, cx))
                .child(self.sampler_knob(t, Field::Velocity, "Velocity", MINT, cx)),
        );
        let modes = [
            (FilterMode::Off, "Off"),
            (FilterMode::LowPass, "LP"),
            (FilterMode::HighPass, "HP"),
            (FilterMode::BandPass, "BP"),
        ]
        .map(|(m, label)| {
            seg(label, v.filter == m, 17.)
                .px(px(7.))
                .text_size(px(10.))
                .debug_selector(move || format!("filter-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.edit_sampler(t, cx, |s| s.voice.filter = m)),
                )
        });
        let filter_card = card(
            "FILTER",
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(tray().p(px(1.)).rounded(px(6.)).children(modes))
                    .child(
                        envelope(
                            v.filter_attack_ms,
                            v.filter_decay_ms,
                            v.filter_sustain,
                            v.filter_release_ms,
                            SKY,
                        )
                        .w(px(110.)),
                    ),
            ),
        )
        .child(
            row()
                .child(self.sampler_knob(t, Field::Cutoff, "Cutoff", SKY, cx))
                .child(self.sampler_knob(t, Field::Resonance, "Reso", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterEnv, "Env", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterAttack, "F att", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterDecay, "F dec", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterSustain, "F sus", SKY, cx))
                .child(self.sampler_knob(t, Field::FilterRelease, "F rel", SKY, cx))
                .child(self.sampler_knob(t, Field::KeyTrack, "Key trk", SKY, cx)),
        );
        let pitch_card = card("PITCH & LFO", None).child(
            row()
                .child(self.sampler_knob(t, Field::Transpose, "Transpose", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Glide, "Glide", VIOLET, cx))
                .child(self.sampler_knob(t, Field::LfoRate, "LFO rate", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Vibrato, "Vibrato", VIOLET, cx))
                .child(self.sampler_knob(t, Field::LfoFilter, "Wah", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Tremolo, "Tremolo", VIOLET, cx))
                .child(self.sampler_knob(t, Field::Volume, "Volume", VIOLET, cx)),
        );
        let params = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(zone_card)
            .child(amp_card)
            .child(filter_card)
            .child(pitch_card);

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(left)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(header)
                    .child(wave_view)
                    .child(keymap)
                    .child(params),
            )
            .into_any_element()
    }

    /// The zone's switches: how it ends, loops and which way it plays.
    fn zone_switches(&mut self, t: usize, i: usize, z: &Zone, cx: &mut Context<Self>) -> Div {
        let looping = z.loop_mode;
        let loops = [
            (LoopMode::Off, "No loop"),
            (LoopMode::Forward, "Loop"),
            (LoopMode::PingPong, "Ping-pong"),
        ]
        .map(|(m, label)| {
            seg(label, looping == m, 22.)
                .px(px(8.))
                .text_size(px(11.))
                .debug_selector(move || format!("loop-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_sampler(t, cx, |s| {
                            if let Some(z) = s.zones.get_mut(i) {
                                z.loop_mode = m;
                            }
                        })
                    }),
                )
        });
        let switch =
            |label: &'static str, on: bool| seg(label, on, 22.).px(px(8.)).text_size(px(11.));
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .child(
                        switch("One-shot", !z.gate)
                            .debug_selector(|| "zone-oneshot".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit_sampler(t, cx, |s| {
                                        if let Some(z) = s.zones.get_mut(i) {
                                            z.gate = false
                                        }
                                    })
                                }),
                            ),
                    )
                    .child(
                        switch("Gate", z.gate)
                            .debug_selector(|| "zone-gate".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit_sampler(t, cx, |s| {
                                        if let Some(z) = s.zones.get_mut(i) {
                                            z.gate = true
                                        }
                                    })
                                }),
                            ),
                    ),
            )
            .child(tray().p(px(2.)).rounded(px(7.)).children(loops))
            .child(
                tray().p(px(2.)).rounded(px(7.)).child(
                    switch("Reverse", z.reverse)
                        .debug_selector(|| "zone-reverse".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.edit_sampler(t, cx, |s| {
                                    if let Some(z) = s.zones.get_mut(i) {
                                        z.reverse ^= true
                                    }
                                })
                            }),
                        ),
                ),
            )
            .child(
                tray().p(px(2.)).rounded(px(7.)).child(
                    switch(
                        match z.choke {
                            Some(g) => ["Choke 1", "Choke 2", "Choke 3", "Choke 4"]
                                [(g as usize).saturating_sub(1).min(3)],
                            None => "No choke",
                        },
                        z.choke.is_some(),
                    )
                    .debug_selector(|| "zone-choke".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_sampler(t, cx, |s| {
                                if let Some(z) = s.zones.get_mut(i) {
                                    z.choke = match z.choke {
                                        None => Some(1),
                                        Some(g) if g < 4 => Some(g + 1),
                                        _ => None,
                                    };
                                }
                            })
                        }),
                    ),
                ),
            )
    }

    /// Four octaves and more of keys, the zones over the notes they play.
    fn sampler_keys(
        &mut self,
        t: usize,
        s: &Sampler,
        picked: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let (lo, hi) = {
            let used_lo = s.zones.iter().map(|z| z.note).min().unwrap_or(36);
            let used_hi = s
                .zones
                .iter()
                .map(|z| z.high.unwrap_or(z.note))
                .max()
                .unwrap_or(84);
            let lo = (used_lo.min(36) / 12) * 12;
            let hi = ((used_hi.max(lo + 47) / 12) * 12 + 11).min(127);
            (lo, hi)
        };
        let track = self.session.project.tracks[t].id;
        let keys = (lo..=hi).map(|p| {
            let black = matches!(p % 12, 1 | 3 | 6 | 8 | 10);
            let zone = s.zones.iter().enumerate().find(|(_, z)| z.covers(p));
            let c = zone.map(|(i, _)| zone_color(i));
            let mine = zone.is_some_and(|(i, _)| i == picked);
            let root = s
                .zones
                .get(picked)
                .is_some_and(|z| z.high.is_some() && z.root.unwrap_or(z.note) == p);
            div()
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .h(px(8.))
                        .rounded(px(2.))
                        .when_some(c, |d, c| d.bg(alpha(c, if mine { 0.95 } else { 0.4 }))),
                )
                .child(
                    div()
                        .when(black, |d| d.h(px(20.)))
                        .when(!black, |d| d.flex_1())
                        .rounded_b(px(3.))
                        .bg(rgb(if black { 0x1c1c22 } else { 0xd9d9df }))
                        .when(root, |d| d.border_b_2().border_color(rgb(AMBER)))
                        .cursor_pointer()
                        .when(p % 12 == 0 && !black, |d| {
                            d.child(
                                div()
                                    .pt(px(10.))
                                    .flex()
                                    .justify_center()
                                    .text_size(px(8.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x6a6a74))
                                    .child(name_in(None, p)),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if let Some(i) = ws
                                    .sampler_of(t)
                                    .and_then(|s| s.zones.iter().position(|z| z.covers(p)))
                                {
                                    ws.sampler_ui.zone = i;
                                }
                                ws.player.preview(track, p, 100);
                                ws.begin_drag(Drag::Key { track, pitch: p }, cx);
                            }),
                        ),
                )
        });
        div()
            .h(px(44.))
            .flex_none()
            .flex()
            .gap(px(1.))
            .debug_selector(|| "sampler-keys".into())
            .children(keys)
    }

    /// The selected track isn't a sampler.
    fn sampler_none(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .bg(rgb(LANE))
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("The Sampler plays samples on the keys"))
            .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child("A kit, one sound across the keys, a loop sliced at its hits, layers by velocity, round robins."))
            .child(button("New Sampler Track", true).debug_selector(|| "sampler-new".into()).on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, cx| ws.new_sampler_track(cx))))
    }

    /// A sampler with nothing in it yet.
    fn sampler_empty(&mut self, cx: &mut Context<Self>) -> Div {
        let lit = self.file_drag.is_some();
        div().flex_1().flex().items_center().justify_center().bg(rgb(LANE)).child(
            div()
                .w(px(520.))
                .h(px(170.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .rounded(px(14.))
                .border_1()
                .border_dashed()
                .border_color(if lit { rgb(ACCENT).into() } else { alpha(0xffffff, 0.16) })
                .bg(if lit { alpha(ACCENT, 0.06) } else { alpha(0xffffff, 0.02) })
                .cursor_pointer()
                .hover(|d| d.bg(alpha(0xffffff, 0.05)))
                .debug_selector(|| "sampler-drop".into())
                .child(icon(Icon::Wave, 24.).text_color(rgb(TEXT_2)))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Drop samples here"))
                .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child("One plays across the keys; several make a kit from C1. Or click to choose."))
                .on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, cx| ws.pick_samples(cx))),
        )
    }
}

/// An envelope's shape, drawn: rising, falling to its level, holding, letting go.
fn envelope(attack: f32, decay: f32, sustain: f32, release: f32, color: u32) -> Div {
    let sustain = sustain.clamp(0.0, 1.0);
    // Each stage's width by the log of its time, so short and long both show.
    let w = |ms: f32| 0.15 + (ms.max(0.0) + 1.0).log10();
    let (a, d, r) = (w(attack), w(decay), w(release));
    let hold = 1.2;
    let total = a + d + hold + r;
    div().h(px(20.)).w_full().child(
        canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
                let x = |t: f32| x0 + t / total * bw;
                let y = |level: f32| y0 + bh - 2.0 - level * (bh - 4.0);
                let pts = [
                    (x(0.0), y(0.0)),
                    (x(a), y(1.0)),
                    (x(a + d), y(sustain)),
                    (x(a + d + hold), y(sustain)),
                    (x(total), y(0.0)),
                ];
                let mut area = gpui::PathBuilder::fill();
                area.move_to(point(px(pts[0].0), px(y(0.0))));
                for (px_, py) in pts {
                    area.line_to(point(px(px_), px(py)));
                }
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, 0.15));
                }
                let mut line = gpui::PathBuilder::stroke(px(1.5));
                line.move_to(point(px(pts[0].0), px(pts[0].1)));
                for (px_, py) in &pts[1..] {
                    line.line_to(point(px(*px_), px(*py)));
                }
                if let Ok(p) = line.build() {
                    window.paint_path(p, rgb(color));
                }
            },
        )
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_turn_settings_and_back() {
        let mut s = Sampler {
            zones: vec![Zone {
                note: 36,
                ..Zone::default()
            }],
            ..Sampler::default()
        };
        for f in [
            Field::Attack,
            Field::Release,
            Field::Cutoff,
            Field::Resonance,
            Field::LfoRate,
            Field::Volume,
            Field::Gain(0),
            Field::Pan(0),
            Field::Tune(0),
        ] {
            field_set(&mut s, f, 0.7);
            assert!(
                (field_pos(&s, f) - 0.7).abs() < 0.02,
                "{f:?} {}",
                field_pos(&s, f)
            );
        }
        field_set(&mut s, Field::Cutoff, 1.0);
        assert!((s.voice.cutoff_hz - 20_000.0).abs() < 1.0);
        field_set(&mut s, Field::Attack, 0.0);
        assert_eq!(s.voice.attack_ms, 0.0);
        // A range's low note moves it whole.
        let mut s = Sampler {
            zones: vec![Zone {
                note: 24,
                high: Some(48),
                root: Some(36),
                ..Zone::default()
            }],
            ..Sampler::default()
        };
        field_set(&mut s, Field::Note(0), 36.0 / 127.0);
        assert_eq!(
            (s.zones[0].note, s.zones[0].high, s.zones[0].root),
            (36, Some(60), Some(48))
        );
        // Velocities kept in order, and none set when it's all of them.
        field_set(&mut s, Field::VelLow(0), 64.0 / 127.0);
        assert_eq!(s.zones[0].velocities, Some((64, 127)));
        field_set(&mut s, Field::VelLow(0), 0.0);
        assert_eq!(s.zones[0].velocities, None);
    }
}
