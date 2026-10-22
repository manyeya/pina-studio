//! The output device: its rate, starting it, and the metronome's click.

use super::*;

/// The rate to play at through the default output: `wanted` if the device can run at it, else
/// the device's own (the live engine runs at that, converting audio as it loads it).
pub fn output_rate(wanted: u32) -> Result<u32, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no audio output device")?;
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
    (beat, per_bar): (f64, u64),
    rate: f64,
    [l, r]: [&mut [f32]; 2],
) {
    const LENGTH: f64 = 0.03;
    let length = (LENGTH * rate) as u32;
    let mut next = (heard as f64 / beat).ceil();
    for i in 0..l.len() {
        let t = heard + i as u64;
        if t as f64 >= next * beat - 0.5 {
            let bar = (next as u64).is_multiple_of(per_bar);
            *click = (length, if bar { 1_760.0 } else { 1_320.0 });
            next += 1.0;
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

impl Engine {
    /// Plays the engine through the default output device, set to the engine's rate (see
    /// `output_rate`).
    pub fn start_output(mut self) -> Result<cpal::Stream, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let rate = self.rate as u32;
        let config = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.channels() >= 2
                    && (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate)
            })
            .ok_or_else(|| format!("the output device can't run at {rate} Hz"))?
            .with_sample_rate(rate)
            .config();
        let ch = config.channels as usize;
        let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| {
                    self.note_buffer(out.len() / ch);
                    for chunk in out.chunks_mut(MAX_BLOCK * ch) {
                        let frames = chunk.len() / ch;
                        self.process([&mut l, &mut r], frames);
                        for (f, frame) in chunk.chunks_mut(ch).enumerate() {
                            frame[0] = l[f];
                            frame[1] = r[f];
                            frame[2..].fill(0.0);
                        }
                    }
                },
                |e| eprintln!("audio: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    }
}
