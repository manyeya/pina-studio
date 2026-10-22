//! A channel's settings, read and set: volume, pan, inserts, saturator, and which channel a strip shows.

use super::*;

impl Workspace {
    /// A channel's level: for a track whose volume is automated, where the automation has it.
    pub fn volume(&self, ch: Channel) -> f32 {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => self
                .automated(i, Param::Volume)
                .unwrap_or(p.tracks[i].volume_db),
            Channel::Aux(i) => p.auxes[i].volume_db,
            Channel::Master => p.master_volume_db,
        }
    }

    /// A channel's built-in saturator.
    pub fn sat(&self, ch: Channel) -> Option<Saturator> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].sat,
            Channel::Aux(i) => p.auxes[i].sat,
            Channel::Master => p.master_sat,
        }
    }

    pub fn sat_mut(&mut self, ch: Channel) -> &mut Option<Saturator> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => &mut p.tracks[i].sat,
            Channel::Aux(i) => &mut p.auxes[i].sat,
            Channel::Master => &mut p.master_sat,
        }
    }

    pub fn set_volume(&mut self, ch: Channel, db: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].volume_db = db,
            Channel::Aux(i) => p.auxes[i].volume_db = db,
            Channel::Master => p.master_volume_db = db,
        }
    }

    pub fn set_pan(&mut self, ch: Channel, pan: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].pan = pan,
            Channel::Aux(i) => p.auxes[i].pan = pan,
            Channel::Master => {}
        }
    }

    /// The inserts of a track or aux.
    pub fn inserts_mut(&mut self, ch: Channel) -> Option<&mut Vec<PluginRef>> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => Some(&mut p.tracks[i].inserts),
            Channel::Aux(i) => Some(&mut p.auxes[i].inserts),
            Channel::Master => Some(&mut p.master_inserts),
        }
    }

    pub fn inserts(&self, ch: Channel) -> &[PluginRef] {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => &p.tracks[i].inserts,
            Channel::Aux(i) => &p.auxes[i].inserts,
            Channel::Master => &p.master_inserts,
        }
    }

    pub fn channel_id(&self, ch: Channel) -> Option<u64> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => Some(p.tracks[i].id),
            Channel::Aux(i) => Some(p.auxes[i].id),
            Channel::Master => None,
        }
    }

    /// A channel's name and color.
    pub fn channel_look(&self, ch: Channel) -> (String, u32) {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => (p.tracks[i].name.clone(), self.color(i)),
            Channel::Aux(i) => (p.auxes[i].name.clone(), BUS),
            Channel::Master => ("Stereo Out".into(), MASTER),
        }
    }

    /// The id a channel's plugins go by in the player.
    pub fn owner_id(&self, ch: Channel) -> u64 {
        self.channel_id(ch).unwrap_or(MASTER_ID)
    }

    pub(crate) fn has_builtin_instrument(&self, ch: Channel) -> bool {
        matches!(ch, Channel::Track(i) if matches!(self.session.project.tracks[i].instrument, Some(Instrument::Synth | Instrument::Drums | Instrument::Sampler(_))))
    }

    /// Shows a channel in the mixer, its effects in the side panel.
    pub fn show_channel(&mut self, ch: Channel, cx: &mut Context<Self>) {
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        self.view = View::Mixer;
        cx.notify();
    }
}
