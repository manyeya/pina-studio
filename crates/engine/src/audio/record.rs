//! Recording audio from the default input device into a WAV file: the channels a track asks for.

use std::{fs::File, io::BufWriter, path::Path};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::song::project::Input;

/// Seconds of input buffered between the input callback and `drain`.
const BUFFER: usize = 4;

pub struct AudioRecorder {
    _stream: cpal::Stream,
    input: rtrb::Consumer<f32>,
    writer: Writer,
}

/// How many channels the default input device has; 0 without one.
pub fn input_channels() -> u16 {
    cpal::default_host()
        .default_input_device()
        .and_then(|d| d.default_input_config().ok())
        .map_or(0, |c| c.channels())
}

impl AudioRecorder {
    /// Starts recording `input` (by default the first two channels) of the default input device at
    /// `rate` into a new WAV file at `path`.
    pub fn start(path: &Path, rate: u32, input: Option<Input>) -> Result<AudioRecorder, String> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("no audio input device")?;
        let config = device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate)
            })
            .ok_or_else(|| format!("the input device can't record at {rate} Hz"))?
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
        let writer = Writer::create(path, rate, channels, input)?;
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
        })
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

/// Writes some channels of interleaved input as a mono or stereo 32-bit float WAV.
struct Writer {
    wav: hound::WavWriter<BufWriter<File>>,
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
    fn create(path: &Path, rate: u32, channels_in: usize, input: Input) -> Result<Writer, String> {
        let shown = path.display().to_string();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let first = (input.channel as usize).min(channels_in.saturating_sub(1));
        let wanted = if input.stereo { 2 } else { 1 };
        let channels_out = wanted.min(channels_in - first).max(1);
        let spec = hound::WavSpec {
            channels: channels_out as u16,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let wav = hound::WavWriter::create(path, spec).map_err(|e| format!("{shown}: {e}"))?;
        Ok(Writer {
            wav,
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
            self.wav
                .write_sample(s)
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
