//! Recording audio from an input device into a WAV file: the channels a track asks for, at a rate
//! the device can run at, as 16-bit, 24-bit or 32-bit float samples. And the inputs' levels, live,
//! for Settings' meters.

use std::{
    fs::File,
    io::BufWriter,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::song::project::Input;

/// Seconds of input buffered between the input callback and `drain`.
const BUFFER: usize = 4;

pub struct AudioRecorder {
    _stream: cpal::Stream,
    input: rtrb::Consumer<f32>,
    writer: Writer,
    rate: u32,
}

/// How a recording's samples are kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Depth {
    Int16,
    Int24,
    #[default]
    Float32,
}

/// `name`, if it's plugged in, else what the system records from.
fn input_for(name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let named = name.and_then(|n| {
        let is_it = |d: &cpal::Device| d.description().is_ok_and(|d| d.name() == n);
        host.input_devices().ok()?.find(is_it)
    });
    named.or_else(|| host.default_input_device())
}

/// How many channels an input device has (None: the system's); 0 without one.
pub fn input_channels_of(name: Option<&str>) -> u16 {
    input_for(name)
        .and_then(|d| d.default_input_config().ok())
        .map_or(0, |c| c.channels())
}

/// How many channels the default input device has; 0 without one.
pub fn input_channels() -> u16 {
    input_channels_of(None)
}

/// The input `name` resolves to now (its own, else the system's), by name.
pub fn input_now(name: Option<&str>) -> Option<String> {
    Some(input_for(name)?.description().ok()?.name().to_string())
}

/// The input devices there are, as `output_info` has the outputs.
pub fn input_info() -> Vec<crate::audio::engine::DeviceInfo> {
    let host = cpal::default_host();
    host.input_devices().map_or(Vec::new(), |devices| {
        devices
            .filter_map(|d| {
                let name = d.description().ok()?.name().to_string();
                let configs: Vec<_> = d.supported_input_configs().ok()?.collect();
                Some(crate::audio::engine::DeviceInfo::of(
                    name,
                    &configs,
                    d.default_input_config().ok().map(|c| c.sample_rate()),
                ))
            })
            .collect()
    })
}

/// An input's levels as they come in: each channel's peak since last read.
pub struct InputMeter {
    _stream: cpal::Stream,
    peaks: Arc<Vec<AtomicU32>>,
}

impl InputMeter {
    /// Listening to `name` (None: the system's input).
    pub fn start(name: Option<&str>) -> Result<InputMeter, String> {
        let device = input_for(name).ok_or("no audio input device")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = config.channels() as usize;
        let peaks: Arc<Vec<AtomicU32>> =
            Arc::new((0..channels).map(|_| AtomicU32::new(0)).collect());
        let heard = peaks.clone();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config.config(),
                move |data: &[f32], _| {
                    for frame in data.chunks(channels) {
                        for (p, s) in heard.iter().zip(frame) {
                            p.fetch_max(s.abs().to_bits(), Ordering::Relaxed);
                        }
                    }
                },
                |e| eprintln!("audio input: {e}"),
                None,
            ),
            other => return Err(format!("the input gives {other} samples")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(InputMeter {
            _stream: stream,
            peaks,
        })
    }

    /// Each channel's peak (0 to 1) since the last read.
    pub fn read(&self) -> Vec<f32> {
        self.peaks
            .iter()
            .map(|p| f32::from_bits(p.swap(0, Ordering::Relaxed)))
            .collect()
    }
}

impl AudioRecorder {
    /// Starts recording `input` (by default the first two channels) of the default input device at
    /// `rate` into a new WAV file at `path`.
    pub fn start(path: &Path, rate: u32, input: Option<Input>) -> Result<AudioRecorder, String> {
        AudioRecorder::start_from(path, rate, input, None, Depth::default())
    }

    /// The same from `device` (None: the system's input), its samples kept as `depth`: at `rate`
    /// if it can run at it, else at its own (the file says which; the song converts it).
    pub fn start_from(
        path: &Path,
        rate: u32,
        input: Option<Input>,
        device: Option<&str>,
        depth: Depth,
    ) -> Result<AudioRecorder, String> {
        let device = input_for(device).ok_or("no audio input device")?;
        let configs: Vec<_> = device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32)
            .collect();
        let own = device
            .default_input_config()
            .map(|c| c.sample_rate())
            .unwrap_or(rate);
        let rate = match configs
            .iter()
            .any(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate))
        {
            true => rate,
            false => own,
        };
        let config = configs
            .into_iter()
            .find(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate))
            .ok_or_else(|| "the input device can't record at any rate Pina can read".to_string())?
            .with_sample_rate(rate)
            .config();
        let channels = config.channels as usize;
        let input = input.unwrap_or(Input {
            channel: 0,
            stereo: true,
        });
        if input.channel as usize >= channels {
            return Err(format!(
                "the input device has {channels} channels: there's no {}",
                input.name()
            ));
        }
        let writer = Writer::create(path, rate, channels, input, depth)?;
        let (mut tx, input) = rtrb::RingBuffer::new(rate as usize * channels * BUFFER);
        let stream = device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    // Whole buffers or nothing, so channels stay in step.
                    if tx.slots() >= data.len() {
                        for &s in data {
                            let _ = tx.push(s);
                        }
                    }
                },
                |e| eprintln!("audio input: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(AudioRecorder {
            _stream: stream,
            input,
            writer,
            rate,
        })
    }

    /// The rate it records at: the file's.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Writes what has come in since the last call. Call often: the buffer holds a few seconds.
    pub fn drain(&mut self) -> Result<(), String> {
        while let Ok(s) = self.input.pop() {
            self.writer.write(s)?;
        }
        Ok(())
    }

    /// Frames recorded so far.
    pub fn frames(&self) -> u64 {
        self.writer.frames
    }

    /// Stops and closes the file; returns its length in frames.
    pub fn finish(mut self) -> Result<u64, String> {
        self.drain()?;
        self.writer.finish()
    }
}

/// Writes some channels of interleaved input as a mono or stereo WAV, `depth` deep.
struct Writer {
    wav: hound::WavWriter<BufWriter<File>>,
    depth: Depth,
    path: String,
    channels_in: usize,
    /// The channels kept: from `first`, `channels_out` of them.
    first: usize,
    channels_out: usize,
    /// Which channel of the current frame comes next.
    channel: usize,
    frames: u64,
}

impl Writer {
    fn create(
        path: &Path,
        rate: u32,
        channels_in: usize,
        input: Input,
        depth: Depth,
    ) -> Result<Writer, String> {
        let shown = path.display().to_string();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let first = (input.channel as usize).min(channels_in.saturating_sub(1));
        let wanted = if input.stereo { 2 } else { 1 };
        let channels_out = wanted.min(channels_in - first).max(1);
        let (bits_per_sample, sample_format) = match depth {
            Depth::Int16 => (16, hound::SampleFormat::Int),
            Depth::Int24 => (24, hound::SampleFormat::Int),
            Depth::Float32 => (32, hound::SampleFormat::Float),
        };
        let spec = hound::WavSpec {
            channels: channels_out as u16,
            sample_rate: rate,
            bits_per_sample,
            sample_format,
        };
        let wav = hound::WavWriter::create(path, spec).map_err(|e| format!("{shown}: {e}"))?;
        Ok(Writer {
            wav,
            depth,
            path: shown,
            channels_in,
            first,
            channels_out,
            channel: 0,
            frames: 0,
        })
    }

    fn write(&mut self, s: f32) -> Result<(), String> {
        if (self.first..self.first + self.channels_out).contains(&self.channel) {
            // Float keeps what goes over full scale; whole numbers stop there.
            let full = s.clamp(-1.0, 1.0);
            match self.depth {
                Depth::Int16 => self.wav.write_sample((full * 32_767.0).round() as i16),
                Depth::Int24 => self.wav.write_sample((full * 8_388_607.0).round() as i32),
                Depth::Float32 => self.wav.write_sample(s),
            }
            .map_err(|e| format!("{}: {e}", self.path))?;
        }
        self.channel += 1;
        if self.channel == self.channels_in {
            self.channel = 0;
            self.frames += 1;
        }
        Ok(())
    }

    fn finish(self) -> Result<u64, String> {
        let path = self.path;
        self.wav.finalize().map_err(|e| format!("{path}: {e}"))?;
        Ok(self.frames)
    }
}

#[cfg(test)]
mod tests;
