//! The modulators' values as they play, read each frame.

use super::*;

impl Workspace {
    /// Each frame: the modulators' values, and a trace of each to draw.
    pub fn read_mods(&mut self) {
        let ids: Vec<(u64, usize)> = self
            .session
            .project
            .tracks
            .iter()
            .filter(|t| !t.modulators.is_empty())
            .map(|t| (t.id, t.modulators.len()))
            .collect();
        for (id, n) in ids {
            let values = self.player.modulators(id);
            self.mod_values.insert(id, values);
            for (k, &v) in values.iter().enumerate().take(n) {
                let trace = self.mod_trace.entry((id, k)).or_default();
                trace.push_back(v);
                while trace.len() > TRACE {
                    trace.pop_front();
                }
            }
        }
    }

    /// Where a modulator in time with the song is round its cycle: the playhead's, while it
    /// plays; the clock's, when stopped.
    pub(crate) fn mod_phase(&self, rate: ModRate) -> f64 {
        let tempo = self.session.project.tempo;
        match (rate, self.player.playing()) {
            (ModRate::Beats(b), true) => {
                let beats = self.playhead() as f64 / pina_engine::song::project::PPQ as f64;
                beats / b.max(1e-3) as f64
            }
            _ => self.mod_clock.elapsed().as_secs_f64() * rate.hz(tempo),
        }
    }
}
