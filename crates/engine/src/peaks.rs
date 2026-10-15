//! Waveform overview: the min and max of each block of samples, so the UI can draw a region at
//! any zoom without touching the audio.

pub const BLOCK: usize = 64;

pub struct Peaks {
    min: Vec<f32>,
    max: Vec<f32>,
}

impl Peaks {
    pub fn from_samples(samples: &[f32]) -> Self {
        let (min, max) = samples
            .chunks(BLOCK)
            .map(|b| b.iter().fold((0f32, 0f32), |(lo, hi), &s| (lo.min(s), hi.max(s))))
            .unzip();
        Peaks { min, max }
    }

    pub fn len_samples(&self) -> usize {
        self.min.len() * BLOCK
    }

    /// The min and max of samples `from..to`, at block resolution.
    // ponytail: scans every block in range, O(blocks per pixel); add coarser levels when
    // zoomed-out long sessions get slow.
    pub fn range(&self, from: usize, to: usize) -> (f32, f32) {
        let a = (from / BLOCK).min(self.min.len());
        let b = to.div_ceil(BLOCK).clamp(a, self.min.len());
        let lo = self.min[a..b].iter().copied().fold(0f32, f32::min);
        let hi = self.max[a..b].iter().copied().fold(0f32, f32::max);
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_spans_blocks() {
        let mut s = vec![0f32; BLOCK * 4];
        s[10] = 0.5;
        s[BLOCK * 3 + 1] = -0.25;
        let p = Peaks::from_samples(&s);
        assert_eq!(p.range(0, BLOCK), (0.0, 0.5));
        assert_eq!(p.range(BLOCK, BLOCK * 3), (0.0, 0.0));
        assert_eq!(p.range(0, BLOCK * 4), (-0.25, 0.5));
        assert_eq!(p.range(BLOCK * 10, BLOCK * 11), (0.0, 0.0));
    }
}
