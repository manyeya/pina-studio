//! Which panels show: the editor pane's tabs, the mixer.

use super::*;

/// What the pane under the tracks shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bottom {
    PianoRoll,
    Steps,
    Sampler,
    Modulation,
    Score,
    Takes,
    Loops,
}

/// The arrangement, or the mixer or the sample browser's page in its place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Arrange,
    Mixer,
    Samples,
    /// Every song, to open, start or manage.
    Songs,
    /// The genres' playbooks, to read and change.
    Playbooks,
    /// Songs taken apart.
    Study,
    /// What the window shows (⌘,).
    Settings,
    /// Every plugin, as pictures: to find, map to keys, and keep healthy.
    Plugins,
}

impl Workspace {
    pub fn toggle_bottom(&mut self, pane: Bottom, cx: &mut Context<Self>) {
        self.view = View::Arrange;
        self.bottom = if self.bottom == Some(pane) {
            None
        } else {
            Some(pane)
        };
        if self.bottom == Some(Bottom::PianoRoll) {
            self.focus_roll();
        }
        cx.notify();
    }

    pub fn show_bottom(&mut self, pane: Bottom, cx: &mut Context<Self>) {
        self.view = View::Arrange;
        self.bottom = Some(pane);
        // The Sampler wants room for its waveform, keys and knobs.
        if pane == Bottom::Sampler {
            self.editor_h = self.editor_h.max(500.0);
        }
        if pane == Bottom::PianoRoll {
            self.focus_roll();
        }
        cx.notify();
    }

    pub fn toggle_mixer(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Mixer => View::Arrange,
            _ => View::Mixer,
        };
        cx.notify();
    }
}
