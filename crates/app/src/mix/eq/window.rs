//! The EQ's window: opened and closed, the channel it shows, what it's heard as, its frame.

use super::*;

impl Workspace {
    /// A channel's EQ in its window (one added, if it had none).
    pub fn open_eq(&mut self, ch: Channel, cx: &mut Context<Self>) {
        if self.strip_dsp(ch).0.is_none() {
            self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(Eq::default()));
        }
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        match &mut self.eq_view {
            Some(v) if v.ch == ch => v.focused = true,
            Some(v) => {
                v.ch = ch;
                v.picked.clear();
                (v.hover, v.solo, v.other, v.on_b) = (None, None, None, false);
                v.focused = true;
            }
            None => {
                let w = (self.viewport_w - 80.0).clamp(MIN_W, 1_000.0);
                let h = (self.viewport_h - 120.0).clamp(MIN_H, 600.0);
                let at = (
                    ((self.viewport_w - w) / 2.0).max(0.0),
                    ((self.viewport_h - h) / 2.0).max(CONTROL_BAR_H),
                );
                self.eq_view = Some(EqView::new(ch, at, (w, h)));
            }
        }
        cx.notify();
    }

    pub fn close_eq(&mut self, cx: &mut Context<Self>) {
        self.eq_view = None;
        self.player.watch_eq(None);
        self.player.watch_lanes(false);
        self.player.solo_band(None);
        cx.notify();
    }

    /// The EQ the window shows.
    pub(crate) fn shown_eq(&self) -> Option<(Channel, Eq)> {
        let ch = self.eq_view.as_ref()?.ch;
        Some((ch, self.strip_dsp(ch).0?))
    }

    /// The shown EQ as it's heard now: its bands where their automation has them.
    pub(crate) fn eq_as_heard(&self, ch: Channel, mut eq: Eq) -> Eq {
        let Channel::Track(t) = ch else { return eq };
        for (i, b) in eq.bands.iter_mut().enumerate() {
            for field in [EqField::Freq, EqField::Gain, EqField::Q] {
                if let Some(v) = self.automated(t, Param::Eq(i as u8, field)) {
                    match field {
                        EqField::Freq => b.freq = v,
                        EqField::Gain => b.gain_db = v,
                        EqField::Q => b.q = v,
                    }
                }
            }
        }
        eq
    }

    /// The bands with `ids` moved by hand: what that does to their automation (recorded in
    /// Touch, Latch or Write; taken over from it in Read), as any track control.
    pub(crate) fn eq_moved(&mut self, ids: &[u16], fields: &[EqField]) {
        let Some((Channel::Track(t), eq)) = self.shown_eq() else {
            return;
        };
        let mut moves = Vec::new();
        for id in ids {
            let Some(i) = eq.bands.find(*id) else {
                continue;
            };
            let b = eq.bands[i];
            for &field in fields {
                if field == EqField::Gain && !b.shape.has_gain() {
                    continue;
                }
                let v = match field {
                    EqField::Freq => b.freq,
                    EqField::Gain => b.gain_db,
                    EqField::Q => b.q,
                };
                moves.push((Param::Eq(i as u8, field), v));
            }
        }
        for (param, v) in moves {
            self.moved(t, param, v);
        }
    }

    /// Let go of the EQ's bands: their automation carries on (or stops recording).
    pub fn eq_let_go(&mut self) {
        let Some(Channel::Track(t)) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        let Some(id) = self.session.project.tracks.get(t).map(|tr| tr.id) else {
            return;
        };
        let held: Vec<Param> = self
            .taken
            .keys()
            .chain(self.touches.keys())
            .filter(|(track, p)| *track == id && matches!(p, Param::Eq(..)))
            .map(|&(_, p)| p)
            .collect();
        for p in held {
            self.let_go(t, p);
        }
    }

    pub(crate) fn eq_band(&self, id: u16) -> Option<(usize, EqBand)> {
        let (_, eq) = self.shown_eq()?;
        let i = eq.bands.find(id)?;
        Some((i, eq.bands[i]))
    }

    pub(crate) fn eq_geo(&self) -> Option<Geo> {
        let rate = self.session.project.sample_rate as f64;
        Some(self.eq_view.as_ref()?.geo(rate))
    }

    /// The shown EQ, to change (no edit of its own: inside one, or a drag's).
    pub(crate) fn eq_mut(&mut self) -> Option<&mut Eq> {
        let ch = self.eq_view.as_ref()?.ch;
        self.strip_dsp_mut(ch).0.as_mut()
    }

    /// Changes the bands picked, as one edit.
    pub(crate) fn edit_picked(&mut self, cx: &mut Context<Self>, f: impl Fn(&mut EqBand)) {
        let picked = self.eq_view.as_ref().map(|v| v.picked.clone());
        let Some(picked) = picked.filter(|p| !p.is_empty()) else {
            return;
        };
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut() {
                for id in &picked {
                    if let Some(i) = eq.bands.find(*id) {
                        f(&mut eq.bands[i]);
                    }
                }
            }
        });
    }

    /// Each frame the window's open: tells the engine which EQ to tap and which band to solo,
    /// and moves the analyzer and meter on with what it's heard.
    pub fn eq_frame(&mut self) {
        self.list_frame();
        let rate = self.session.project.sample_rate as f64;
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let p = &self.session.project;
        let id = match view.ch {
            Channel::Track(i) => p.tracks.get(i).map(|t| Some(t.id)),
            Channel::Aux(i) => p.auxes.get(i).map(|a| Some(a.id)),
            Channel::Master => Some(None),
        };
        self.player.watch_eq(id);
        self.player.solo_band(view.solo.map(|s| (s.id, s.level)));
        let geo = view.geo(rate);
        let n = (geo.w / 3.0) as usize;
        let fresh_columns = view.columns.len() != n
            || view
                .columns
                .first()
                .is_none_or(|&f| (f - geo.lo).abs() > 1e-6)
            || view
                .columns
                .last()
                .is_none_or(|&f| (f - geo.hi).abs() > 1e-3);
        let floor = -(view.analysis.range as f32);
        if fresh_columns {
            view.columns = (0..n)
                .map(|i| geo.freq(i as f32 / (n - 1) as f32 * geo.w))
                .collect();
            view.pre = vec![floor; n];
            view.post = vec![floor; n];
        }
        self.player.watch_ext(view.ext);
        let tap = self.player.eq_tap();
        let [pre, left, right, ext] = &mut *view.buf;
        let written = tap.read(pre, left, right);
        let ext_written = tap.read_ext(ext);
        let heard = written != view.written;
        view.written = written;
        view.hearing = heard && pre[TAP_LEN - 1024..].iter().any(|x| x.abs() > 1e-4);
        // The meter: each side's peak over the last block or so, falling away.
        for (side, buf) in [&*left, &*right].into_iter().enumerate() {
            let peak = buf[TAP_LEN - 1024..]
                .iter()
                .fold(0f32, |m, x| m.max(x.abs()));
            let db = if heard {
                20.0 * peak.max(1e-6).log10()
            } else {
                -120.0
            };
            view.meter[side] = db.max(view.meter[side] - 1.5);
            view.peak = view.peak.max(view.meter[side]);
        }
        // EQ Match learns while it listens: this track going in, and the reference.
        if let Some(m) = view.matching.as_mut()
            && heard
        {
            m.learn(
                &pre[..],
                (ext_written > 0 && view.ext.is_some()).then_some(&ext[..]),
                rate,
            );
        }
        // Spectrum Grab: the pointer resting on the spectrum a moment grabs it.
        let on_spectrum = view.pointer.is_some_and(|(x, y)| {
            let i = ((x / geo.w) * (n.max(2) - 1) as f32).round() as usize;
            let floor = -(view.analysis.range as f32);
            view.post.get(i).is_some_and(|&db| {
                db > floor + 3.0 && ((-db / view.analysis.range as f32 * geo.h) - y).abs() < 14.0
            })
        });
        let can_grab = view.analysis.grab
            && view.analysis.post
            && view.hover.is_none()
            && view.grabbing.is_none()
            && !view.sketching
            && view.matching.is_none();
        match (can_grab && on_spectrum, view.grab_since) {
            (true, None) => view.grab_since = Some(std::time::Instant::now()),
            (true, Some(at)) if at.elapsed().as_secs_f32() > 1.2 => {
                view.grabbing = Some(view.post.clone());
                view.grab_since = None;
            }
            (false, _) => view.grab_since = None,
            _ => {}
        }
        if view.grabbing.is_some() && view.pointer.is_none() {
            view.grabbing = None;
        }
        if !(view.analysis.pre || view.analysis.post) {
            return;
        }
        let len = view.analysis.len();
        let tilt = view.analysis.tilt;
        let fall = view.analysis.fall();
        let freeze = view.analysis.freeze;
        let post: Vec<f32> = left[TAP_LEN - len..]
            .iter()
            .zip(&right[TAP_LEN - len..])
            .map(|(a, b)| 0.5 * (a + b))
            .collect();
        let fresh = |s: &[f32], on: bool| -> Vec<f32> {
            match heard && on {
                true => spectrum_db(s, rate, &view.columns),
                false => vec![floor - 30.0; n],
            }
        };
        let now = (
            fresh(&pre[TAP_LEN - len..], view.analysis.pre),
            fresh(&post, view.analysis.post),
        );
        // The track compared with.
        let ext_now = match (view.ext.is_some(), ext_written != 0) {
            (true, true) => spectrum_db(&ext[TAP_LEN - len..], rate, &view.columns),
            _ => vec![floor - 30.0; n],
        };
        if view.ext_spec.len() != n {
            view.ext_spec = vec![floor; n];
        }
        for (smooth, now) in [
            (&mut view.pre, now.0),
            (&mut view.post, now.1),
            (&mut view.ext_spec, ext_now),
        ] {
            for (i, (s, v)) in smooth.iter_mut().zip(now).enumerate() {
                let v = (v + tilt * (view.columns[i] / 1_000.0).log2() as f32).max(floor);
                // Up quickly, down slowly; frozen, it only rises.
                *s = match (freeze, v > *s) {
                    (_, true) => *s + (v - *s) * 0.6,
                    (true, false) => *s,
                    (false, false) => (*s - fall).max(v),
                };
            }
        }
    }

    pub(crate) fn eq_menu(
        &mut self,
        at: gpui::Point<Pixels>,
        items: Vec<(String, MenuAction)>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The window moved by its title, or resized by its corner.
    pub fn drag_eq_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        resize: bool,
        p: gpui::Point<f32>,
    ) {
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        let (dx, dy) = (p.x - from.x, p.y - from.y);
        v.restore = None;
        if resize {
            v.size = (
                (start.0 + dx).clamp(MIN_W, vw.max(MIN_W)),
                (start.1 + dy).clamp(MIN_H, vh.max(MIN_H)),
            );
        } else {
            v.at = (
                (start.0 + dx).clamp(-v.size.0 + 80.0, vw - 80.0),
                (start.1 + dy).clamp(0.0, vh - HEAD_H),
            );
        }
    }

    /// The EQ window, if it's open.
    pub fn eq_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let Some((ch, eq)) = self.shown_eq() else {
            // Its channel or EQ is gone.
            if self.eq_view.is_some() {
                self.eq_view = None;
                self.player.watch_eq(None);
                self.player.solo_band(None);
            }
            return None;
        };
        let eq = self.eq_as_heard(ch, eq);
        let view = self.eq_view.as_ref()?;
        let (at, size) = (view.at, view.size);
        let head = self.eq_head(ch, &eq, cx);
        let display = self.eq_display_area(&eq, cx);
        let foot = self.eq_foot(&eq, cx);
        let popup = self.eq_popup(&eq, cx);
        let frame = div()
            .id("eq-window")
            .occlude()
            .relative()
            .w(px(size.0))
            .h(px(size.1))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x101115))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(head)
            .child(display)
            .child(foot)
            .children(popup)
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                if let Some(v) = ws.eq_view.as_mut()
                    && v.focused
                    && ws.drag.is_none()
                {
                    v.focused = false;
                    cx.notify();
                }
            }));
        Some(
            deferred(
                anchored()
                    .position(point(px(at.0), px(at.1)))
                    .snap_to_window()
                    .child(frame),
            )
            .with_priority(1),
        )
    }
}
