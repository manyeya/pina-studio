//! Putting a device on a track: picked, dropped, or dragged onto one.

use super::*;

impl Workspace {
    /// Puts a device on a track, or a new track for it: an instrument plays it (in place of
    /// what it had), an effect goes after the track's others. With a preset, that loads too; it
    /// all undoes as one.
    pub fn use_device(
        &mut self,
        d: usize,
        target: Target,
        preset: Option<Preset>,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self.catalog.devices.get(d).cloned() else {
            return;
        };
        let tracks = self.session.project.tracks.len();
        let t = match target {
            Target::Track(t) if t < tracks => t,
            Target::Track(_) | Target::NewTrack
                if !device.instrument() && tracks > 0 && target != Target::NewTrack =>
            {
                self.track.min(tracks - 1)
            }
            _ => {
                // A new track: an instrument's, or for an effect an audio track.
                let instrument = device
                    .instrument()
                    .then(|| instrument_of(&device))
                    .flatten();
                let plays = instrument.is_some();
                // Named for what it plays: the preset, else the instrument.
                let name = preset
                    .as_ref()
                    .filter(|_| plays)
                    .map_or(device.name.clone(), |p| p.name.clone());
                self.new_track_named(instrument, Some(name), cx);
                self.remember(&device.key);
                // The new track goes after the selected one, and is selected now.
                let t = self.track;
                if plays && let Some(p) = preset.clone() {
                    self.choose(
                        MenuAction::Preset {
                            channel: Channel::Track(t),
                            which: Which::Instrument,
                            preset: p,
                        },
                        cx,
                    );
                }
                if plays {
                    self.status = Some(format!(
                        "{} is ready: play it on your keyboard (⌘K for the computer's), or double-click its lane to write notes.",
                        self.session.project.tracks[t].name
                    ));
                    // A plugin shows its window, as it does in any DAW.
                    if device.own.is_none() {
                        self.open_editor_when_loaded(Channel::Track(t), Which::Instrument, cx);
                    }
                    return;
                }
                // An effect: on the new audio track, as on any other.
                t
            }
        };
        self.track = t;
        let channel = Channel::Track(t);
        let action = match (device.own, device.instrument()) {
            (Some(Own::Eq), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Eq,
            },
            (Some(Own::Comp), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Comp,
            },
            (Some(Own::Sat), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Sat,
            },
            (_, true) => {
                let track = &self.session.project.tracks[t];
                if track.instrument.is_none() && !track.regions.is_empty() {
                    self.status = Some(format!(
                        "{} is an audio track with audio on it: drop {} under the tracks for a track of its own.",
                        track.name, device.name
                    ));
                    cx.notify();
                    return;
                }
                MenuAction::Instrument {
                    track: t,
                    instrument: instrument_of(&device).unwrap_or(Instrument::Synth),
                }
            }
            (_, false) => MenuAction::Insert {
                channel,
                index: self.session.project.tracks[t].inserts.len(),
                plugin: device.formats.first().map(|p| p.plugin_ref()),
            },
        };
        let which = match &action {
            MenuAction::Insert { index, .. } => Which::Insert(*index),
            _ => Which::Instrument,
        };
        let plugin = device.own.is_none();
        self.choose(action, cx);
        if plugin {
            self.open_editor_when_loaded(channel, which, cx);
        }
        if let Some(p) = preset {
            self.choose(
                MenuAction::Preset {
                    channel,
                    which,
                    preset: p,
                },
                cx,
            );
        }
        self.remember(&device.key);
    }

    pub(crate) fn remember(&mut self, key: &str) {
        let c = &mut self.catalog;
        c.recent.retain(|k| k != key);
        c.recent.insert(0, key.to_string());
        c.recent.truncate(24);
        c.save();
    }

    pub(crate) fn toggle_favourite(&mut self, key: &str, cx: &mut Context<Self>) {
        let c = &mut self.catalog;
        match c.favourites.iter().position(|k| k == key) {
            Some(i) => drop(c.favourites.remove(i)),
            None => c.favourites.push(key.to_string()),
        }
        c.save();
        cx.notify();
    }

    /// Enter: the picked row goes in (⇧: on a new track).
    pub fn use_picked(&mut self, new: bool, cx: &mut Context<Self>) {
        let rows: Vec<Row> = self.catalog_rows(cx);
        let usable: Vec<&Row> = rows
            .iter()
            .filter(|r| !matches!(r, Row::Heading(_)))
            .collect();
        let Some(row) = self
            .catalog
            .picked
            .and_then(|i| usable.get(i).copied())
            .cloned()
        else {
            return;
        };
        let target = if new {
            Target::NewTrack
        } else {
            Target::Track(self.track)
        };
        match row {
            Row::Device(d) => self.use_device(d, target, None, cx),
            Row::Preset(d, p) => self.use_device(d, target, Some(p), cx),
            Row::Sample(i) => self.select_sample(i, true, cx),
            Row::Song(s) => self.ask_to_open(s.dir, cx),
            Row::Heading(_) => {}
        }
    }

    /// ↑ ↓ in the Browser: the row picked moves.
    pub fn move_picked(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self
            .catalog_rows(cx)
            .iter()
            .filter(|r| !matches!(r, Row::Heading(_)))
            .count();
        if n == 0 {
            return;
        }
        let at = self.catalog.picked.map_or(-1, |p| p as isize) + by;
        self.catalog.picked = Some(at.clamp(0, n as isize - 1) as usize);
        cx.notify();
    }

    /// Lets go of a device over the tracks: on the track under it, or under them a new one.
    pub fn drop_device(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        let Some(Drag::Device {
            device,
            at,
            moved: true,
            ..
        }) = drag
        else {
            return;
        };
        if let Some(target) = self.device_target(at) {
            self.use_device(device, target, None, cx);
        }
    }

    /// The track a device dragged to `p` goes on: the row under it, headers or lanes; under the
    /// tracks, a new one.
    pub fn device_target(&self, p: gpui::Point<f32>) -> Option<Target> {
        let b = self.lanes_bounds.get();
        let (left, top) = (f32::from(b.origin.x) - self.header_w, f32::from(b.origin.y));
        let (w, h) = (
            f32::from(b.size.width) + self.header_w,
            f32::from(b.size.height),
        );
        if p.x < left || p.y < top || p.x > left + w || p.y > top + h {
            return None;
        }
        let row = self.track_at_y(p.y - top).unwrap_or(usize::MAX);
        Some(match row < self.session.project.tracks.len() {
            true => Target::Track(row),
            false => Target::NewTrack,
        })
    }

    /// The device being dragged, under the pointer, saying where it'll go.
    pub fn device_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Device {
            device,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let d = self.catalog.devices.get(device)?;
        let hint = match self.device_target(at) {
            Some(Target::Track(t)) => {
                let name = &self.session.project.tracks[t].name;
                match d.instrument() {
                    true => format!("Plays {name}"),
                    false => format!("On {name}"),
                }
            }
            Some(Target::NewTrack) => "A new track".into(),
            None => "Drop it on a track".into(),
        };
        let color = d.cat.color();
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1f1f26))
                .border_1()
                .border_color(alpha(color, 0.5))
                .shadow_lg()
                .text_size(px(12.))
                .child(d.cat.glyph(15.).text_color(rgb(color)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(d.name.clone()),
                )
                .child(div().text_color(rgb(TEXT_3)).child(hint)),
        )
    }
}
