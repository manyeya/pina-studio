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
}

/// The buffer sizes worth offering, those a device can't do left out.
const BUFFERS: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

/// `name`, if it's still plugged in, else the system's output.
fn device_for(name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let named = name.and_then(|n| {
        let is_it = |d: &cpal::Device| d.description().is_ok_and(|d| d.name() == n);
        host.output_devices().ok()?.find(is_it)
    });
    named.or_else(|| host.default_output_device())
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
    let device = device_for(setup.device.as_deref()).ok_or("no audio output device")?;
    let can = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .any(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.channels() >= 2
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

/// The metronome: a short blip on every beat from `heard`, higher on the bar, added to `out`.
/// `click` is the blip still sounding: samples left and its pitch.
pub(crate) fn click(
    click: &mut (u32, f32),
    heard: u64,
    tempo: &TempoMap,
    (beat, per_bar): (f64, u64),
    rate: f64,
    [l, r]: [&mut [f32]; 2],
) {
    const LENGTH: f64 = 0.03;
    let length = (LENGTH * rate) as u32;
    // `beat` is a beat in quarter notes; the tempo map says where each one falls.
    let mut next = (tempo.beats(heard) / beat - 1e-9).ceil();
    let mut at = tempo.sample(next * beat);
    for i in 0..l.len() {
        let t = heard + i as u64;
        if t as f64 >= at - 0.5 {
            let bar = (next as u64).is_multiple_of(per_bar);
            *click = (length, if bar { 1_760.0 } else { 1_320.0 });
            next += 1.0;
            at = tempo.sample(next * beat);
        }
        if click.0 > 0 {
            let age = (length - click.0) as f32 / rate as f32;
            let s = (std::f32::consts::TAU * click.1 * age).sin() * (-age * 140.0).exp() * 0.3;
            l[i] += s;
            r[i] += s;
            click.0 -= 1;
        }
    }
}

/// The engine, playing. It outlives the stream so the sound can move to another device, or take
/// another buffer, without the song stopping being loaded.
pub struct Output {
    engine: Arc<Mutex<Engine>>,
    stream: Option<cpal::Stream>,
    setup: AudioSetup,
}

impl Output {
    /// Moves to `setup`; failing that, back to what it was, and says why.
    pub fn change(&mut self, setup: &AudioSetup) -> Result<(), String> {
        self.stream = None;
        match open(&self.engine, setup) {
            Ok(stream) => {
                self.stream = Some(stream);
                self.setup = setup.clone();
                Ok(())
            }
            Err(e) => {
                self.stream = open(&self.engine, &self.setup).ok();
                Err(e)
            }
        }
    }
}

impl Engine {
    /// Plays the engine through `setup`'s output, set to the engine's rate (see `output_rate`).
    pub fn start_output(mut self, setup: &AudioSetup) -> Result<Output, String> {
        self.crew = Crew::new(helpers_wanted(), MAX_BLOCK as f64 / self.rate);
        let engine = Arc::new(Mutex::new(self));
        let stream = Some(open(&engine, setup)?);
        let setup = setup.clone();
        Ok(Output {
            engine,
            stream,
            setup,
        })
    }
}

fn open(engine: &Arc<Mutex<Engine>>, setup: &AudioSetup) -> Result<cpal::Stream, String> {
    let device = device_for(setup.device.as_deref()).ok_or("no audio output device")?;
    let rate = engine.lock().map_err(|e| e.to_string())?.rate;
    let wanted = rate as u32;
    let mut config = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .find(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.channels() >= 2
                && (c.min_sample_rate()..=c.max_sample_rate()).contains(&wanted)
        })
        .ok_or_else(|| format!("the output device can't run at {wanted} Hz"))?
        .with_sample_rate(wanted)
        .config();
    if let Some(buffer) = setup.buffer {
        config.buffer_size = cpal::BufferSize::Fixed(buffer);
    }
    let ch = config.channels as usize;
    let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
    let engine = engine.clone();
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], _| {
                // Only a change of device holds this, and then the stream's already gone.
                let Ok(mut engine) = engine.try_lock() else {
                    out.fill(0.0);
                    return;
                };
                let began = Instant::now();
                engine.note_buffer(out.len() / ch);
                for chunk in out.chunks_mut(MAX_BLOCK * ch) {
                    let frames = chunk.len() / ch;
                    engine.process([&mut l, &mut r], frames);
                    for (f, frame) in chunk.chunks_mut(ch).enumerate() {
                        frame[0] = l[f];
                        frame[1] = r[f];
                        frame[2..].fill(0.0);
                    }
                }
                engine.note_load(began, out.len() / ch);
            },
            |e| eprintln!("audio: {e}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
