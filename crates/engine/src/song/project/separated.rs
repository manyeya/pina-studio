//! An audio region's stems (as `crate::separate` makes them) on tracks of their own: each plays
//! as the region did, in a stack after its track; the region muted.

use std::path::PathBuf;

use super::*;

impl Project {
    /// The audio region at `at`'s stems, each a name and its file (standing in for the region's
    /// file sample for sample): an audio track a stem, after the region's track, each with the
    /// region as it is (its start, offset, length, loops, fades, gain, timing and warps) on its
    /// stem; together in a stack named after the region's track; the region muted. Where each
    /// stem's track is now.
    pub fn stems_from_audio(&mut self, at: At, stems: &[(String, PathBuf)]) -> Vec<usize> {
        let r = self.region(at).clone();
        if !matches!(r.content, Content::Audio { .. }) || stems.is_empty() {
            return Vec::new();
        }
        let name = self.tracks[at.0].name.clone();
        let first = at.0 + 1;
        for (k, (stem, file)) in stems.iter().enumerate() {
            let mut region = Region {
                name: stem.clone(),
                mute: false,
                link: None,
                ..r.clone()
            };
            if let Content::Audio { file: f, .. } = &mut region.content {
                *f = file.clone();
            }
            let track = Track {
                name: stem.clone(),
                regions: vec![region],
                ..Default::default()
            };
            self.add_track(first + k, track);
        }
        let tracks: Vec<usize> = (first..first + stems.len()).collect();
        let (_, moved) = self.add_stack(&tracks);
        if let Some(stack) = self.auxes.last_mut() {
            stack.name = format!("{name} Stems");
        }
        self.tracks[at.0].regions[at.1].mute = true;
        tracks.iter().map(|&t| moved[t]).collect()
    }
}
