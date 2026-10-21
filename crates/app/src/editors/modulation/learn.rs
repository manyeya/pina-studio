//! Learn: a modulator mapped to the next control turned in a plugin's own window.

use super::*;

/// A modulator learning its target: the track's plugin parameters as they were when it began
/// (those already moved by a route or automation left out, as they move on their own), the
/// first to change being what it maps to.
pub struct Learn {
    pub track: usize,
    pub index: usize,
    base: Vec<(Which, u32, f64)>,
}

impl Workspace {
    /// Modulator `k` of track `t` learning its target (again: stops): the next parameter turned
    /// in one of the track's plugins' own windows. Opens the instrument's window if none is.
    pub fn learn(&mut self, t: usize, k: usize, cx: &mut Context<Self>) {
        if self
            .mod_learn
            .as_ref()
            .is_some_and(|l| (l.track, l.index) == (t, k))
        {
            self.mod_learn = None;
            self.status = Some("Stopped learning.".into());
            cx.notify();
            return;
        }
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        let id = track.id;
        // What moves on its own (a route, automation) can't be told from a turn.
        let moved = |which: Which, param: u32| {
            track.modulators.iter().any(|m| {
                m.routes
                    .iter()
                    .any(|r| r.target == ModTarget::Plugin(which, param))
            }) || track
                .automation
                .plugins
                .iter()
                .any(|a| a.plugin == which && a.param == param)
        };
        let slots = self.plugin_slots(t);
        let base: Vec<(Which, u32, f64)> = slots
            .iter()
            .flat_map(|&which| {
                self.player
                    .params(id, which)
                    .into_iter()
                    .filter(move |p| !moved(which, p.id))
                    .map(move |p| (which, p.id, p.value))
            })
            .collect();
        if base.is_empty() {
            self.status = Some(
                "No plugin on this track to learn from: give it a plugin instrument or effect."
                    .into(),
            );
            cx.notify();
            return;
        }
        // Its window, to turn something in (not in tests: no windows on the screen).
        if let Some(&first) = slots.first().filter(|_| !cfg!(test)) {
            let _ = self.player.open_editor(id, first);
        }
        self.mod_learn = Some(Learn {
            track: t,
            index: k,
            base,
        });
        self.status = Some(format!(
            "{}: turn a control in the plugin's window to map it there (Learn again stops).",
            self.mod_name(t, k)
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                match this.update(cx, |ws, cx| ws.learn_tick(cx)) {
                    Ok(false) => {}
                    _ => break,
                }
            }
        })
        .detach();
    }

    /// Whether learning's done: the first parameter turned since it began, mapped.
    pub(crate) fn learn_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(l) = &self.mod_learn else {
            return true;
        };
        let (t, k) = (l.track, l.index);
        let Some(id) = self
            .session
            .project
            .tracks
            .get(t)
            .filter(|tr| k < tr.modulators.len())
            .map(|tr| tr.id)
        else {
            self.mod_learn = None;
            return true;
        };
        let turned = l
            .base
            .iter()
            .find(|&&(which, param, was)| {
                (self.player.param_value(id, which, param) - was).abs() > TURNED
            })
            .map(|&(which, param, _)| ModTarget::Plugin(which, param));
        let Some(mt) = turned else {
            return false;
        };
        self.mod_learn = None;
        self.map_modulator(t, k, mt, cx);
        cx.notify();
        true
    }
}
