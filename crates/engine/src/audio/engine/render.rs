//! Rendering a song offline, and bouncing it to files.

use super::*;

impl Engine {
    /// Plays `frames` from the start into memory, straight through.
    pub fn render(&mut self, frames: u64) -> [Vec<f32>; 2] {
        self.render_from(0, frames)
    }

    /// Starts a render at sample `start`: the song plays from there, once through, cycle or not.
    pub fn start_render(&mut self, start: u64) {
        self.arrangement.cycle = None;
        self.locate(start);
        self.play();
    }

    /// The next `frames` of a render begun with `start_render`.
    pub fn render_on(&mut self, frames: u64) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        for at in (0..frames as usize).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames as usize - at);
            let [l, r] = &mut out;
            self.process([&mut l[at..], &mut r[at..]], n);
        }
        out
    }

    /// `frames` of the song from sample `start`, as a bounce plays them (notes begun before it
    /// aside).
    pub fn render_from(&mut self, start: u64, frames: u64) -> [Vec<f32>; 2] {
        let mut out = [vec![0f32; frames as usize], vec![0f32; frames as usize]];
        // A bounce plays the song through once, cycle or not.
        self.arrangement.cycle = None;
        self.locate(start);
        self.play();
        for at in (0..frames as usize).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames as usize - at);
            let [l, r] = &mut out;
            self.process([&mut l[at..], &mut r[at..]], n);
        }
        self.stop();
        out
    }

    /// Plays `frames` from the start into a stereo 32-bit float WAV.
    pub fn bounce(&mut self, path: &Path, frames: u64) -> Result<(), String> {
        let err = |e: hound::Error| format!("{}: {e}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut wav = hound::WavWriter::create(path, spec).map_err(err)?;
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        // A bounce plays the song through once, cycle or not.
        self.arrangement.cycle = None;
        self.locate(0);
        self.play();
        let mut left = frames;
        while left > 0 {
            let n = left.min(MAX_BLOCK as u64) as usize;
            self.process([&mut l, &mut r], n);
            for i in 0..n {
                wav.write_sample(l[i]).map_err(err)?;
                wav.write_sample(r[i]).map_err(err)?;
            }
            left -= n as u64;
        }
        self.stop();
        wav.finalize().map_err(err)
    }
}
