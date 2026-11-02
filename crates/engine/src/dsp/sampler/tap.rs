//! Where a sampler's voices are playing, for the editor to draw: written by the audio thread
//! each block, read by the window each frame, without locking.

use std::sync::atomic::{AtomicU64, Ordering};

/// How many playheads it shows at once.
pub const SLOTS: usize = 48;
const EMPTY: u64 = u64::MAX;

/// A playhead: the zone it reads, where in its sample (0 to 1), how loud (0 to 1), and whether
/// it's a grain rather than a voice's own head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing {
    pub zone: usize,
    pub at: f32,
    pub level: f32,
    pub grain: bool,
}

pub struct SamplerTap {
    slots: [AtomicU64; SLOTS],
}

impl Default for SamplerTap {
    fn default() -> Self {
        SamplerTap {
            slots: std::array::from_fn(|_| AtomicU64::new(EMPTY)),
        }
    }
}

impl SamplerTap {
    /// What's playing now.
    pub fn read(&self) -> Vec<Playing> {
        self.slots
            .iter()
            .map(|s| s.load(Ordering::Relaxed))
            .take_while(|&v| v != EMPTY)
            .map(|v| Playing {
                zone: (v >> 48) as usize & 0x7fff,
                grain: v >> 63 == 1,
                level: ((v >> 32) & 0xffff) as f32 / 65_535.0,
                at: f32::from_bits(v as u32),
            })
            .collect()
    }

    /// Slot `i` to a playhead, or (None) the end of what's playing.
    pub(crate) fn write(&self, i: usize, p: Option<Playing>) {
        let Some(slot) = self.slots.get(i) else {
            return;
        };
        let v = match p {
            None => EMPTY,
            Some(p) => {
                ((p.grain as u64) << 63)
                    | ((p.zone.min(0x7fff) as u64) << 48)
                    | (((p.level.clamp(0.0, 1.0) * 65_535.0) as u64) << 32)
                    | p.at.to_bits() as u64
            }
        };
        slot.store(v, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playheads_go_through_and_stop_at_the_first_empty_slot() {
        let tap = SamplerTap::default();
        let p = Playing {
            zone: 3,
            at: 0.25,
            level: 0.5,
            grain: true,
        };
        tap.write(0, Some(p));
        tap.write(1, None);
        tap.write(2, Some(p));
        let read = tap.read();
        assert_eq!(read.len(), 1);
        assert_eq!((read[0].zone, read[0].at, read[0].grain), (3, 0.25, true));
        assert!((read[0].level - 0.5).abs() < 1e-3);
    }
}
