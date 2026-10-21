//! The mouse on the EQ: bands made, picked, dragged, cycled, soloed, and their menus.

use super::*;

impl Workspace {
    /// A band made where the plot was clicked (or pulled): picked; ⌥, dynamic.
    pub(crate) fn make_band(
        &mut self,
        mut band: EqBand,
        alt: bool,
        cx: &mut Context<Self>,
    ) -> Option<u16> {
        if alt && band.shape.has_gain() {
            band.dynamic = Some(Dynamic::default());
        }
        let mut id = None;
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut()
                && let Some(i) = eq.bands.push(band)
            {
                id = Some(eq.bands[i].id);
            }
        });
        match id {
            Some(id) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = vec![id];
                }
            }
            None => {
                self.session.cancel();
                self.status = Some(format!("An EQ has at most {MAX_BANDS} bands."));
            }
        }
        cx.notify();
        id
    }

    /// The bands picked as they are now: where a drag of them starts from.
    pub(crate) fn hold_picked(&mut self) {
        let Some((_, eq)) = self.shown_eq() else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut() {
            v.start = v
                .picked
                .iter()
                .filter_map(|&id| eq.bands.find(id).map(|i| eq.bands[i]))
                .collect();
        }
    }

    /// A band's dot pressed: picked (⌘ adds it, ⇧ a run to it), and dragging the bands picked.
    pub(crate) fn press_dot(&mut self, id: u16, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let m = e.modifiers;
        if e.click_count == 2 && !m.alt {
            // Double-click: type its frequency (the field opens once the click's done).
            self.pending_name = Some(Name::Eq(id, EqKnob::Freq));
            cx.notify();
            return;
        }
        if m.alt && m.platform {
            return self.cycle_shape(id, cx);
        }
        if m.alt && m.shift {
            return self.cycle_slope(id, cx);
        }
        let order = self.eq_order();
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        view.focused = true;
        let picked = view.picked.contains(&id);
        let mut deselect = false;
        if m.platform {
            match picked {
                true => deselect = true,
                false => view.picked.push(id),
            }
        } else if m.shift && !picked {
            // A run: from the band picked last to this one, as they lie across the display.
            let from = view.lead().and_then(|l| order.iter().position(|&o| o == l));
            let to = order.iter().position(|&o| o == id);
            match (from, to) {
                (Some(a), Some(b)) => {
                    let (a, b) = (a.min(b), a.max(b));
                    for &o in &order[a..=b] {
                        if !view.picked.contains(&o) {
                            view.picked.push(o);
                        }
                    }
                }
                _ => view.picked = vec![id],
            }
        } else if !picked {
            view.picked = vec![id];
        }
        // The one pressed leads.
        if let Some(i) = view.picked.iter().position(|&p| p == id) {
            let lead = view.picked.remove(i);
            view.picked.push(lead);
        }
        self.hold_picked();
        self.begin_drag(
            Drag::EqNode {
                id,
                from: pt(e.position),
                q: m.platform && !deselect,
                alt: m.alt,
                axis: 0,
                moved: false,
                deselect,
            },
            cx,
        );
    }

    /// The bands' ids as they lie across the display, low to high.
    pub(crate) fn eq_order(&self) -> Vec<u16> {
        let Some((_, eq)) = self.shown_eq() else {
            return Vec::new();
        };
        let mut bands: Vec<(f32, u16)> = eq.bands.iter().map(|b| (b.freq, b.id)).collect();
        bands.sort_by(|a, b| a.0.total_cmp(&b.0));
        bands.into_iter().map(|(_, id)| id).collect()
    }

    pub(crate) fn cycle_shape(&mut self, id: u16, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let i = Shape::ALL.iter().position(|&s| s == b.shape).unwrap_or(0);
        let next = Shape::ALL[(i + 1) % Shape::ALL.len()];
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = vec![id];
        }
        self.eq_op(EqOp::Shape(next), cx);
    }

    pub(crate) fn cycle_slope(&mut self, id: u16, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let slopes = b.shape.slopes();
        if slopes.is_empty() {
            return;
        }
        // The next steeper; from the steepest, the gentlest.
        let next = slopes
            .iter()
            .copied()
            .find(|&s| s > b.slope)
            .unwrap_or(slopes[0]);
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = vec![id];
        }
        self.eq_op(EqOp::Slope(next), cx);
    }

    /// The picked bands dragged by one of them: across moves their frequencies (each by the
    /// same ratio), up and down their gains (each in proportion), or with ⌘ their widths (a
    /// cut's, notch's or band pass's always). ⌥ keeps to the way it first moves.
    #[allow(clippy::too_many_arguments)]
    pub fn drag_eq_node(
        &mut self,
        id: u16,
        from: gpui::Point<f32>,
        q_only: bool,
        alt: bool,
        axis: u8,
        p: gpui::Point<f32>,
    ) -> u8 {
        let Some(geo) = self.eq_geo() else {
            return axis;
        };
        let piano = self.eq_view.as_ref().is_some_and(|v| v.piano);
        let start = self
            .eq_view
            .as_ref()
            .map(|v| v.start.clone())
            .unwrap_or_default();
        let Some(lead) = start.iter().find(|b| b.id == id).copied() else {
            return axis;
        };
        let by = if self.fine { 0.1 } else { 1.0 };
        let (mut dx, mut dy) = ((p.x - from.x) * by, (p.y - from.y) * by);
        // ⌥: one way only, the way it first went.
        let axis = match (alt, axis) {
            (true, 0) if dx.abs().max(dy.abs()) > 3.0 => {
                if dx.abs() > dy.abs() {
                    1
                } else {
                    2
                }
            }
            (_, a) => a,
        };
        match (alt, axis) {
            (true, 1) => dy = 0.0,
            (true, 2) => dx = 0.0,
            (true, _) => (dx, dy) = (0.0, 0.0),
            _ => {}
        }
        let ratio = 2f32.powf(dx / geo.w * geo.octaves());
        let mut lead_freq = (lead.freq * ratio).clamp(10.0, 30_000.0);
        if piano {
            lead_freq = note_hz(note_of(lead_freq).0);
        }
        let ratio = lead_freq / lead.freq;
        let fine = self.fine;
        let Some(eq) = self.eq_mut() else { return axis };
        let scale = eq.scale.max(0.01);
        let per_px = geo.range / (geo.h / 2.0 - PAD) / scale;
        let lead_gain = lead.gain_db - dy * per_px;
        let q_by = 2f32.powf(-dy / 60.0);
        let mut widest = 0f32;
        for s in &start {
            let Some(i) = eq.bands.find(s.id) else {
                continue;
            };
            let b = &mut eq.bands[i];
            b.freq = (s.freq * ratio).clamp(10.0, 30_000.0);
            if !fine && !piano {
                b.freq = b.freq.round();
            }
            if q_only || !b.shape.has_gain() {
                b.q = (s.q * q_by).clamp(0.025, 40.0);
            } else if dy != 0.0 {
                // The rest go in proportion to the one dragged; from flat, by as much.
                let g = match lead.gain_db.abs() > 0.5 {
                    true => s.gain_db * lead_gain / lead.gain_db,
                    false => s.gain_db + (lead_gain - lead.gain_db),
                };
                b.gain_db = ((g * 10.0).round() / 10.0).clamp(-30.0, 30.0);
                widest = widest.max(b.gain_db.abs() * scale);
            }
        }
        // Dragged past the display's range: it grows to show it.
        if let Some(v) = self.eq_view.as_mut()
            && widest > v.range as f32
        {
            v.range = RANGES
                .into_iter()
                .find(|&r| r as f32 >= widest)
                .unwrap_or(30);
        }
        let ids: Vec<u16> = start.iter().map(|b| b.id).collect();
        self.eq_moved(&ids, &[EqField::Freq, EqField::Gain, EqField::Q]);
        axis
    }

    /// The plot pressed where there's no dot: a click makes a band (once none's picked; else it
    /// lets them go), a pull of the curve makes one and drags it, a drag elsewhere picks the
    /// bands in a box.
    pub(crate) fn press_plot(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        view.focused = true;
        view.popup = None;
        let o = view.display.get().origin;
        let (x, y) = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
        let m = e.modifiers;
        let sketching = view.sketching;
        // Spectrum Grab: a peak pressed, a bell to pull.
        if view.grabbing.is_some() && self.grab_peak(x, e, cx) {
            return;
        }
        if let Some(v) = self.eq_view.as_mut() {
            v.grabbing = None;
        }
        if e.click_count == 2 || m.platform {
            let band = band_at(&geo, x, y, false);
            self.make_band(band, m.alt, cx);
            return;
        }
        let curve = self.eq_curve_at(geo.freq(x));
        let on_curve = (geo.y(curve) - y).abs() < 8.0;
        // EQ Sketch, or on an empty EQ a drag off the line: a curve drawn.
        let empty = self.shown_eq().is_some_and(|(_, e)| e.bands.is_empty());
        if sketching || (empty && !on_curve) {
            if let Some(v) = self.eq_view.as_mut() {
                v.stroke = vec![(x, y)];
            }
            self.begin_drag(Drag::EqSketch, cx);
            return;
        }
        self.begin_drag(
            Drag::EqPress {
                from: pt(e.position),
                on_curve,
                alt: m.alt,
                moved: false,
            },
            cx,
        );
    }

    /// What the shown EQ does at a frequency, as the overall curve has it.
    pub(crate) fn eq_curve_at(&self, freq: f64) -> f32 {
        let rate = self.session.project.sample_rate as f64;
        self.shown_eq().map_or(0.0, |(_, eq)| {
            pina_engine::dsp::eq::eq_response(&eq, freq, rate) as f32
        })
    }

    /// A press on the plot dragged: off the curve, a box picking bands; on it, a band made
    /// there (a shelf at either end) and pulled.
    pub fn drag_eq_press(
        &mut self,
        from: gpui::Point<f32>,
        on_curve: bool,
        alt: bool,
        p: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let o = pt(view.display.get().origin);
        let (a, b) = ((from.x - o.x, from.y - o.y), (p.x - o.x, p.y - o.y));
        if (p.x - from.x).hypot(p.y - from.y) < 4.0 {
            return;
        }
        if !on_curve {
            view.marquee = Some((a, b));
            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
            let Some((_, eq)) = self.shown_eq() else {
                return;
            };
            let scale = eq.scale;
            let inside: Vec<u16> = eq
                .bands
                .iter()
                .filter(|band| {
                    let (x, y) = (geo.x(band.freq as f64), geo.y(dot_db(band, scale)));
                    (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
                })
                .map(|band| band.id)
                .collect();
            if let Some(v) = self.eq_view.as_mut() {
                v.picked = inside;
            }
            cx.notify();
            return;
        }
        // Pulled off the curve: a band made where it was pressed, and dragged from there.
        let mut band = band_at(&geo, a.0, a.1, true);
        band.gain_db = 0.0;
        let Some(id) = self.make_band(band, alt, cx) else {
            self.drag = None;
            return;
        };
        self.hold_picked();
        self.drag = Some(Drag::EqNode {
            id,
            from,
            q: false,
            alt: false,
            axis: 0,
            moved: true,
            deselect: false,
        });
        self.drag_eq_node(id, from, false, false, 0, p);
        self.changed(cx);
    }

    /// The plot let go of: a click (no drag) makes a band, or lets the picked ones go.
    pub fn release_eq_press(
        &mut self,
        from: gpui::Point<f32>,
        moved: bool,
        alt: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        if view.marquee.take().is_some() || moved {
            cx.notify();
            return;
        }
        if !view.picked.is_empty() {
            view.picked.clear();
            cx.notify();
            return;
        }
        let o = pt(view.display.get().origin);
        let band = band_at(&geo, from.x - o.x, from.y - o.y, false);
        self.make_band(band, alt, cx);
    }

    /// A dot let go of without moving: ⌥ switches it off, ⌘ on one picked lets it go, on the
    /// piano it snaps to its note.
    pub fn release_eq_node(&mut self, id: u16, alt: bool, deselect: bool, cx: &mut Context<Self>) {
        if alt {
            if let Some(v) = self.eq_view.as_mut() {
                v.picked = vec![id];
            }
            self.eq_op(EqOp::Off, cx);
            return;
        }
        if deselect {
            if let Some(v) = self.eq_view.as_mut() {
                v.picked.retain(|&p| p != id);
            }
            cx.notify();
            return;
        }
        if self.eq_view.as_ref().is_some_and(|v| v.piano) {
            self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut()
                    && let Some(i) = eq.bands.find(id)
                {
                    let b = &mut eq.bands[i];
                    b.freq = note_hz(note_of(b.freq).0);
                }
            });
        }
    }

    /// The wheel over a band: its width (a cut's slope, a step at a time); with ⌥ its dynamic
    /// range, with ⌘ its gain, with both one traded for the other. ⇧ fine. If it's picked, the
    /// others picked go with it.
    pub(crate) fn wheel_band(&mut self, id: u16, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        // With ⇧ a mouse's wheel may come sideways.
        let d = e.delta.pixel_delta(px(16.));
        let dy = f32::from(if d.y != px(0.) { d.y } else { d.x });
        if dy == 0.0 {
            return;
        }
        let m = e.modifiers;
        let fine = if m.shift { 0.25 } else { 1.0 };
        let ids: Vec<u16> = match self.eq_view.as_ref() {
            Some(v) if v.picked.contains(&id) => v.picked.clone(),
            _ => vec![id],
        };
        let steps = (dy / 16.0) * fine;
        self.edit(cx, |ws| {
            let Some(eq) = ws.eq_mut() else { return };
            for id in &ids {
                let Some(i) = eq.bands.find(*id) else {
                    continue;
                };
                let b = &mut eq.bands[i];
                match (m.alt, m.platform) {
                    (true, true) => {
                        if let Some(d) = b.dynamic.as_mut() {
                            d.range_db = (d.range_db - steps * 0.5).clamp(-30.0, 30.0);
                            b.gain_db = (b.gain_db + steps * 0.5).clamp(-30.0, 30.0);
                        }
                    }
                    (true, false) if b.shape.has_gain() => {
                        let d = b.dynamic.get_or_insert_default();
                        d.range_db = (d.range_db + steps * 0.5).clamp(-30.0, 30.0);
                    }
                    (false, true) if b.shape.has_gain() => {
                        b.gain_db = (b.gain_db + steps * 0.5).clamp(-30.0, 30.0);
                    }
                    (false, false) if matches!(b.shape, Shape::LowCut | Shape::HighCut) => {
                        step_slope(b, steps.signum() as i32, m.shift);
                    }
                    _ => b.q = (b.q * 2f32.powf(steps / 8.0)).clamp(0.025, 40.0),
                }
            }
        });
        // An unchanged band (a cut at its steepest) leaves nothing to undo.
        if self.session.unchanged() {
            self.session.cancel();
        }
    }

    /// Solo held on a band: what it works on, heard; dragged sideways it moves the band, up
    /// and down the level it's heard at.
    pub(crate) fn press_solo(&mut self, id: u16, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut() {
            v.solo = Some(Solo { id, level: 0.0 });
            v.picked = vec![id];
        }
        self.hold_picked();
        self.begin_drag(
            Drag::EqSolo {
                id,
                from: pt(e.position),
                freq: b.freq,
            },
            cx,
        );
    }

    pub fn drag_eq_solo(
        &mut self,
        id: u16,
        from: gpui::Point<f32>,
        freq: f32,
        p: gpui::Point<f32>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let level = ((from.y - p.y) / 6.0).clamp(-24.0, 12.0);
        if let Some(v) = self.eq_view.as_mut() {
            v.solo = Some(Solo { id, level });
        }
        let f = (freq * 2f32.powf((p.x - from.x) / geo.w * geo.octaves())).clamp(10.0, 30_000.0);
        if let Some(eq) = self.eq_mut()
            && let Some(i) = eq.bands.find(id)
        {
            eq.bands[i].freq = f.round();
        }
    }

    /// The frequency scale dragged: up zooms in round where it was pressed, down out;
    /// sideways pans.
    pub fn drag_eq_scale(&mut self, from: gpui::Point<f32>, span: (f64, f64), p: gpui::Point<f32>) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let o = pt(view.display.get().origin);
        let at = ((from.x - o.x) / geo.w).clamp(0.0, 1.0) as f64;
        let octaves = (span.1 / span.0).log2();
        let zoomed =
            (octaves * 2f64.powf((p.y - from.y) as f64 / 80.0)).clamp(1.0, (HIGH / LOW).log2());
        let anchor = span.0 * 2f64.powf(octaves * at);
        let pan = -((p.x - from.x) / geo.w) as f64 * zoomed;
        let lo =
            (anchor / 2f64.powf(zoomed * at) * 2f64.powf(pan)).clamp(LOW, HIGH / 2f64.powf(zoomed));
        view.span = (lo, lo * 2f64.powf(zoomed));
    }

    /// A band's menu: its shape, slope and part of the sound, and the rest.
    pub(crate) fn band_menu(&mut self, id: u16, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut()
            && !v.picked.contains(&id)
        {
            v.picked = vec![id];
        }
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![("Shape".to_string(), MenuAction::Nothing)];
        items.extend(Shape::ALL.iter().map(|&s| {
            (
                format!("{}{}", tick(s == b.shape), s.name()),
                MenuAction::Eq(EqOp::Shape(s)),
            )
        }));
        if !b.shape.slopes().is_empty() {
            items.push((String::new(), MenuAction::Nothing));
            items.push(("Slope".into(), MenuAction::Nothing));
            items.extend(b.shape.slopes().iter().map(|&s| {
                (
                    format!("{}{}", tick(s == b.slope), slope_text(s)),
                    MenuAction::Eq(EqOp::Slope(s)),
                )
            }));
        }
        items.push((String::new(), MenuAction::Nothing));
        items.push(("Works on".into(), MenuAction::Nothing));
        items.extend(Placement::ALL.iter().map(|&p| {
            (
                format!("{}{}", tick(p == b.placement), p.name()),
                MenuAction::Eq(EqOp::Placement(p)),
            )
        }));
        items.push(("Split".into(), MenuAction::Eq(EqOp::Split)));
        items.push((String::new(), MenuAction::Nothing));
        if b.shape.has_gain() {
            items.push((
                if b.dynamic.is_some() {
                    "Make Still"
                } else {
                    "Make Dynamic"
                }
                .into(),
                MenuAction::Eq(EqOp::Dynamic),
            ));
        }
        items.push((
            if b.off { "Switch On" } else { "Switch Off" }.into(),
            MenuAction::Eq(EqOp::Off),
        ));
        items.push(("Copy".into(), MenuAction::Eq(EqOp::Copy)));
        items.push(("Delete".into(), MenuAction::Eq(EqOp::Delete)));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The display's own menu: paste, copy, pick or clear its bands.
    pub(crate) fn plot_menu(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let n = self.eq_clipboard.len();
        let mut items = vec![];
        if n > 0 {
            items.push((
                format!("Paste {n} band{}", if n == 1 { "" } else { "s" }),
                MenuAction::Eq(EqOp::Paste),
            ));
        }
        items.extend([
            ("Copy All Bands".to_string(), MenuAction::Eq(EqOp::CopyAll)),
            ("Select All".to_string(), MenuAction::Eq(EqOp::SelectAll)),
            (String::new(), MenuAction::Nothing),
            ("Clear All Bands".to_string(), MenuAction::Eq(EqOp::Clear)),
        ]);
        self.eq_menu(at, items, cx);
    }
}
