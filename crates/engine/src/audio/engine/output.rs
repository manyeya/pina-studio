//! The output device: its rate, starting it, and the metronome's click.

use serde::{Deserialize, Serialize};

use super::*;
use crate::song::project::TempoMap;

/// Where the song is heard from: a device by name, and samples to a buffer. `None` leaves each to
/// the system.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSetup {
    pub device: Option<String>,
    pub buffer: Option<u32>,
    /// What recording hears: a device by name (None: the system's input).
    pub input: Option<String>,
    /// The rate the engine runs at (None: the song's, if the output can).
    pub rate: Option<u32>,
    /// Which pair of the output's channels the song plays out of: 0 is 1 and 2.
    pub out_pair: u16,
    /// How many cores make sound (None: as many as suit this computer).
    pub cores: Option<usize>,
}

/// An audio device: its name, its channels, the usual rates it can run at, its own rate, and
/// whether it's Bluetooth.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DeviceInfo {
    pub name: String,
    pub channels: u16,
    pub rates: Vec<u32>,
    pub own_rate: Option<u32>,
    pub bluetooth: bool,
}

/// The usual sample rates, low to high.
pub const RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

impl DeviceInfo {
    pub(crate) fn of(
        name: String,
        configs: &[cpal::SupportedStreamConfigRange],
        own_rate: Option<u32>,
    ) -> DeviceInfo {
        let f32s = || {
            configs
                .iter()
                .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
        };
        DeviceInfo {
            bluetooth: is_bluetooth(&name),
            name,
            channels: f32s().map(|c| c.channels()).max().unwrap_or(0),
            rates: RATES
                .into_iter()
                .filter(|r| f32s().any(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(r)))
                .collect(),
            own_rate,
        }
    }
}

/// The outputs there are, each with what it can do.
pub fn output_info() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    host.output_devices().map_or(Vec::new(), |devices| {
        devices
            .filter_map(|d| {
                let name = d.description().ok()?.name().to_string();
                let configs: Vec<_> = d.supported_output_configs().ok()?.collect();
                Some(DeviceInfo::of(
                    name,
                    &configs,
                    d.default_output_config().ok().map(|c| c.sample_rate()),
                ))
            })
            .collect()
    })
}

/// The buffer sizes worth offering, those a device can't do left out.
const BUFFERS: [u32; 7] = [32, 64, 128, 256, 512, 1024, 2048];

/// `name`, if it's still plugged in, else the system's output.
fn device_for(name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let named = name.and_then(|n| {
        let is_it = |d: &cpal::Device| d.description().is_ok_and(|d| d.name() == n);
        host.output_devices().ok()?.find(is_it)
    });
    named.or_else(|| host.default_output_device())
}

/// The output `setup` plays through right now: its own, if it's plugged in, else whatever the
/// system plays through. None with no output at all.
pub fn output_now(setup: &AudioSetup) -> Option<String> {
    Some(
        device_for(setup.device.as_deref())?
            .description()
            .ok()?
            .name()
            .to_string(),
    )
}

/// The outputs there are, by name.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map_or(Vec::new(), |devices| {
        let names = devices.filter_map(|d| Some(d.description().ok()?.name().to_string()));
        names.collect()
    })
}

/// The buffer sizes `name` can run at.
pub fn buffer_sizes(name: Option<&str>) -> Vec<u32> {
    let range = device_for(name).and_then(|d| {
        let c = d.supported_output_configs().ok()?.next()?;
        match *c.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => Some((min, max)),
            cpal::SupportedBufferSize::Unknown => None,
        }
    });
    let (min, max) = range.unwrap_or((0, u32::MAX));
    BUFFERS
        .into_iter()
        .filter(|b| (min..=max).contains(b))
        .collect()
}

/// The rate to play at through `setup`'s output: `wanted` if the device can run at it, else the
/// device's own (the live engine runs at that, converting audio as it loads it).
pub fn output_rate(wanted: u32, setup: &AudioSetup) -> Result<u32, String> {
    let wanted = setup.rate.unwrap_or(wanted);
    let device = device_for(setup.device.as_deref()).ok_or("no audio output device")?;
    let can = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .any(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && (c.min_sample_rate()..=c.max_sample_rate()).contains(&wanted)
        });
    match can {
        true => Ok(wanted),
        false => device
            .default_output_config()
            .map(|c| c.sample_rate())
            .map_err(|e| e.to_string()),
    }
}

/// How the metronome sounds: its level (dB from as it comes), its sound (a click, wood, a beep, a
/// tick), how many times a beat it clicks (1, 2 or 4), and the bar's first louder and higher.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickStyle {
    pub level_db: f32,
    pub sound: u8,
    pub every: u8,
    pub accent: bool,
}

impl Default for ClickStyle {
    fn default() -> Self {
        ClickStyle {
            level_db: 0.0,
            sound: 0,
            every: 1,
            accent: true,
        }
    }
}

/// A click sounding: samples left of how many, its pitch and loudness, and noise's seed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClickVoice {
    left: u32,
    len: u32,
    pitch: f32,
    amp: f32,
    sound: u8,
    seed: u32,
}

impl ClickVoice {
    /// One struck: on the bar (`bar`), a beat, or between.
    fn strike(style: ClickStyle, rate: f64, bar: bool, beat: bool) -> ClickVoice {
        let accent = bar && style.accent;
        let (secs, high, low) = match style.sound {
            1 => (0.035, 2_400.0, 1_900.0),
            2 => (0.09, 1_000.0, 800.0),
            3 => (0.015, 0.0, 0.0),
            _ => (0.03, 1_760.0, 1_320.0),
        };
        let len = (secs * rate) as u32;
        ClickVoice {
            left: len,
            len,
            pitch: if accent { high } else { low },
            amp: 10f32.powf(style.level_db / 20.0)
                * match (accent, beat) {
                    (true, _) => 1.0,
                    (false, true) => 0.75,
                    _ => 0.45,
                },
            sound: style.sound,
            seed: 0x9e37_79b9,
        }
    }

    /// Its next sample.
    fn next(&mut self, rate: f32) -> f32 {
        if self.left == 0 {
            return 0.0;
        }
        let age = (self.len - self.left) as f32 / rate;
        self.left -= 1;
        let tau = std::f32::consts::TAU;
        0.3 * self.amp
            * match self.sound {
                1 => {
                    ((tau * self.pitch * age).sin() + 0.5 * (tau * self.pitch * 2.76 * age).sin())
                        * (-age * 260.0).exp()
                        * 0.7
                }
                2 => {
                    (tau * self.pitch * age).sin()
                        * if age < 0.05 {
                            1.0
                        } else {
                            (-(age - 0.05) * 80.0).exp()
                        }
                }
                3 => {
                    self.seed ^= self.seed << 13;
                    self.seed ^= self.seed >> 17;
                    self.seed ^= self.seed << 5;
                    (self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * (-age * 600.0).exp()
                }
                _ => (tau * self.pitch * age).sin() * (-age * 140.0).exp(),
            }
    }
}

/// The metronome: a short sound on every beat (or every half or quarter of one) from `heard`,
/// louder and higher on the bar, added to `out`. `click` is the one still sounding.
pub(crate) fn click(
    click: &mut ClickVoice,
    heard: u64,
    tempo: &TempoMap,
    (beat, per_bar): (f64, u64),
    rate: f64,
    [l, r]: [&mut [f32]; 2],
    style: ClickStyle,
) {
    let every = style.every.clamp(1, 4) as u64;
    let step = beat / every as f64;
    // `beat` is a beat in quarter notes; the tempo map says where each one falls.
    let mut next = (tempo.beats(heard) / step - 1e-9).ceil();
    let mut at = tempo.sample(next * step);
    for i in 0..l.len() {
        let t = heard + i as u64;
        if t as f64 >= at - 0.5 {
            let k = next as u64;
            *click = ClickVoice::strike(
                style,
                rate,
                k.is_multiple_of(per_bar * every),
                k.is_multiple_of(every),
            );
            next += 1.0;
            at = tempo.sample(next * step);
        }
        let s = click.next(rate as f32);
        l[i] += s;
        r[i] += s;
    }
}

/// Counting in: the click for `at` samples into it, every `step` samples, the bar's first every
/// `bar` steps.
pub(crate) fn count_click(
    click: &mut ClickVoice,
    at: u64,
    (step, bar, every): (u64, u64, u64),
    rate: f64,
    [l, r]: [&mut [f32]; 2],
    style: ClickStyle,
) {
    for i in 0..l.len() {
        let t = at + i as u64;
        if t.is_multiple_of(step.max(1)) {
            let k = t / step.max(1);
            *click =
                ClickVoice::strike(style, rate, k.is_multiple_of(bar), k.is_multiple_of(every));
        }
        let s = click.next(rate as f32);
        l[i] += s;
        r[i] += s;
    }
}

/// The engine, playing. It outlives the stream so the sound can move to another device, or take
/// another buffer, without the song stopping being loaded.
pub struct Output {
    engine: Arc<Mutex<Engine>>,
    stream: Option<cpal::Stream>,
    setup: AudioSetup,
    /// The device the stream plays through, by name.
    device: String,
}

impl Output {
    /// Moves to `setup`; failing that, back to what it was, and says why.
    pub fn change(&mut self, setup: &AudioSetup) -> Result<(), String> {
        self.stream = None;
        match open(&self.engine, setup) {
            Ok((stream, device)) => {
                (self.stream, self.device) = (Some(stream), device);
                self.setup = setup.clone();
                Ok(())
            }
            Err(e) => {
                if let Ok((stream, device)) = open(&self.engine, &self.setup) {
                    (self.stream, self.device) = (Some(stream), device);
                }
                Err(e)
            }
        }
    }

    /// The device it plays through, by name.
    pub fn device(&self) -> &str {
        &self.device
    }
}

impl Engine {
    /// Plays the engine through `setup`'s output, set to the engine's rate (see `output_rate`).
    pub fn start_output(mut self, setup: &AudioSetup) -> Result<Output, String> {
        // Cores asked for: this thread and that many less one to help it.
        let helpers = setup
            .cores
            .map_or_else(helpers_wanted, |c| c.saturating_sub(1).min(15));
        self.crew = Crew::new(helpers, MAX_BLOCK as f64 / self.rate);
        let engine = Arc::new(Mutex::new(self));
        let (stream, device) = open(&engine, setup)?;
        Ok(Output {
            engine,
            stream: Some(stream),
            setup: setup.clone(),
            device,
        })
    }
}

/// A stream playing the engine through `setup`'s output, and that device's name.
fn open(engine: &Arc<Mutex<Engine>>, setup: &AudioSetup) -> Result<(cpal::Stream, String), String> {
    let device = device_for(setup.device.as_deref()).ok_or("no audio output device")?;
    let name = device
        .description()
        .map_or_else(|_| String::new(), |d| d.name().to_string());
    let rate = engine.lock().map_err(|e| e.to_string())?.rate;
    let wanted = rate as u32;
    let range = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        // Stereo if it can; one channel (a headset on a call) has both sides mixed into it.
        .filter(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && (c.min_sample_rate()..=c.max_sample_rate()).contains(&wanted)
        })
        .max_by_key(|c| c.channels().min(2))
        .ok_or_else(|| format!("{name} can't run at {wanted} Hz"))?;
    // A buffer this device can't do (one picked for another): the nearest it can.
    let buffer = setup.buffer.map(|b| match *range.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => b.clamp(min, max),
        cpal::SupportedBufferSize::Unknown => b,
    });
    let mut config = range.with_sample_rate(wanted).config();
    if let Some(buffer) = buffer {
        config.buffer_size = cpal::BufferSize::Fixed(buffer);
    }
    let ch = config.channels as usize;
    // The pair the song plays out of: past the device's channels, its first.
    let base = match (setup.out_pair as usize * 2 + 1) < ch {
        true => setup.out_pair as usize * 2,
        false => 0,
    };
    let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
    let engine = engine.clone();
    // What cpal's timestamps leave out: the stream's own latency, after the device's.
    let after = stream_latency(&name, true).as_nanos() as u64;
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                // Only a change of device holds this, and then the stream's already gone.
                let Ok(mut engine) = engine.try_lock() else {
                    out.fill(0.0);
                    return;
                };
                let began = Instant::now();
                engine.note_buffer(out.len() / ch);
                // When what's made now is heard: after the buffer, the device's own delay and
                // its stream's, from now.
                let stamp = info.timestamp();
                let delay = stamp.playback.duration_since(stamp.callback);
                engine.note_heard(clock() + delay.as_nanos() as u64 + after);
                for chunk in out.chunks_mut(MAX_BLOCK * ch) {
                    let frames = chunk.len() / ch;
                    engine.process([&mut l, &mut r], frames);
                    for (f, frame) in chunk.chunks_mut(ch).enumerate() {
                        frame.fill(0.0);
                        match ch {
                            1 => frame[0] = (l[f] + r[f]) * 0.5,
                            _ => (frame[base], frame[base + 1]) = (l[f], r[f]),
                        }
                    }
                }
                engine.note_load(began, out.len() / ch);
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name))
}
