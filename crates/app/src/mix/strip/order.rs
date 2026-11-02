//! A channel's effects put in another order: dragged up and down its strip (a click still opens
//! the one clicked), or moved up and down from their menus. The EQ, compressor, saturator,
//! limiter and plugins run in whatever order they're in.

use pina_engine::song::project::{ChainMut, Fx, chain};

use super::*;

/// Where each effect's row was drawn this frame: its channel, whether it's a card in the mixer's
/// panel, its row, and its bounds.
pub type EffectRows = std::rc::Rc<std::cell::RefCell<Vec<(Channel, bool, usize, Bounds<Pixels>)>>>;

impl Workspace {
    pub(crate) fn chain_mut(&mut self, ch: Channel) -> ChainMut<'_> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].chain_mut(),
            Channel::Aux(i) => p.auxes[i].chain_mut(),
            Channel::Master => p.master_chain_mut(),
        }
    }

    /// The effects a channel has, in the order they run: each one's place in its whole chain
    /// (where the built-ins it hasn't count too), and what it is.
    pub(crate) fn shown_effects(&self, ch: Channel) -> Vec<(usize, Fx)> {
        let p = &self.session.project;
        let order = match ch {
            Channel::Track(i) => &p.tracks[i].chain,
            Channel::Aux(i) => &p.auxes[i].chain,
            Channel::Master => &p.master_chain,
        };
        let (eq, comp) = self.strip_dsp(ch);
        let has = |fx: Fx| match fx {
            Fx::Eq => eq.is_some(),
            Fx::Comp => comp.is_some(),
            Fx::Sat => self.sat(ch).is_some(),
            Fx::Limit => self.limiter(ch).is_some(),
            Fx::Plugin | Fx::More => true,
            // Never in a chain: stacked ones are `More`s.
            _ => false,
        };
        chain(order, self.inserts(ch).len(), self.more(ch).len())
            .into_iter()
            .enumerate()
            .filter(|&(_, fx)| has(fx))
            .collect()
    }

    /// Which of a channel's stacked built-ins the `row`th effect its strip shows is (a `More`):
    /// its place among them.
    pub(crate) fn stacked_at(shown: &[(usize, Fx)], row: usize) -> usize {
        shown[..row].iter().filter(|(_, f)| *f == Fx::More).count()
    }

    /// One of a channel's stacked built-ins: what it is, and its name, numbered after the
    /// channel's own ("Compressor 2").
    pub(crate) fn stacked(&self, ch: Channel, k: usize) -> Option<(Builtin, String)> {
        let more = self.more(ch);
        let kind = more.get(k)?.kind();
        let b = Builtin::of(kind)?;
        // Numbered after the channel's own ("Compressor 2"); a reverb's first is just "Reverb".
        let n = more[..k].iter().filter(|u| u.kind() == kind).count() + 1 + b.own() as usize;
        Some((
            b,
            match n {
                1 => b.name().to_string(),
                n => format!("{} {n}", b.name()),
            },
        ))
    }

    /// Moves the effect at place `from` of a channel's chain to `to`, as one edit.
    pub fn move_effect(&mut self, ch: Channel, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to {
            return;
        }
        let mut problem = None;
        self.edit(cx, |ws| problem = ws.chain_mut(ch).move_fx(from, to).err());
        if problem.is_some() {
            self.status = problem;
        }
    }

    /// The `row`th effect a strip shows (or the mixer panel's card for it, `card`) pressed:
    /// dragged, it moves; let go where it was, it opens.
    pub(crate) fn press_effect(
        &mut self,
        ch: Channel,
        row: usize,
        card: bool,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let at = crate::workspace::pt(e.position);
        self.begin_drag(
            Drag::Effect {
                ch,
                row,
                to: row,
                card,
                from: (at.x, at.y),
                at: (at.x, at.y),
                moved: false,
            },
            cx,
        );
    }

    /// An effect dragged to `p`: it would take the place of the row nearest there, in the
    /// strip (or panel) it's in, as this frame drew them.
    pub(crate) fn drag_effect(&mut self, p: gpui::Point<f32>) {
        let Some(Drag::Effect { ch, card, from, .. }) = self.drag else {
            return;
        };
        let nearest = self
            .effect_rows
            .borrow()
            .iter()
            .filter(|r| r.0 == ch && r.1 == card)
            .min_by(|a, b| {
                let d = |b: &Bounds<Pixels>| (f32::from(b.center().y) - p.y).abs();
                d(&a.3).total_cmp(&d(&b.3))
            })
            .map(|r| r.2);
        if let Some(Drag::Effect { to, at, moved, .. }) = &mut self.drag {
            if let Some(n) = nearest {
                *to = n;
            }
            *at = (p.x, p.y);
            *moved |= (p.y - from.1).abs() > 4.0 || (p.x - from.0).abs() > 4.0;
        }
    }

    /// The effect being dragged, lifted, under the pointer.
    pub fn effect_ghost(&self) -> Option<Div> {
        let Some(Drag::Effect {
            ch,
            row,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let shown = self.shown_effects(ch);
        let (_, fx) = *shown.get(row)?;
        let name = match fx {
            Fx::Eq => "Channel EQ".to_string(),
            Fx::Comp => "Compressor".to_string(),
            Fx::Sat => "Saturator".to_string(),
            Fx::Limit => "Limiter".to_string(),
            Fx::More => self.stacked(ch, Self::stacked_at(&shown, row))?.1,
            Fx::Plugin => {
                let n = self.shown_effects(ch)[..row]
                    .iter()
                    .filter(|(_, f)| *f == Fx::Plugin)
                    .count();
                self.inserts(ch).get(n)?.class.clone()
            }
            _ => return None,
        };
        let color = self.channel_look(ch).1;
        Some(
            div()
                .absolute()
                .left(px(at.0 - 20.))
                .top(px(at.1 - 13.))
                .h(px(26.))
                .min_w(px(120.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(px(7.))
                .bg(rgb(RAISED))
                .border_1()
                .border_color(rgb(ACCENT))
                .shadow_lg()
                .opacity(0.95)
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT))
                .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(color)))
                .child(name),
        )
    }

    /// A handle to drag an effect's card by (in the mixer's panel), to another place in the
    /// chain.
    pub(crate) fn effect_grip(&self, ch: Channel, row: usize, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(14.))
            .h(px(22.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor(gpui::CursorStyle::OpenHand)
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(move || format!("card-grip-{ch:?}-{row}"))
            .child(
                div()
                    .w(px(7.))
                    .flex()
                    .flex_wrap()
                    .gap(px(2.))
                    .children((0..6).map(|_| div().size(px(2.)).rounded(px(1.)).bg(rgb(TEXT_3)))),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.press_effect(ch, row, true, e, cx);
                }),
            )
    }

    /// Where a row of effects was drawn this frame (in a strip, or the panel's cards), for
    /// dragging to.
    pub(crate) fn effect_place(&self, ch: Channel, card: bool, row: usize) -> impl IntoElement {
        let rows = self.effect_rows.clone();
        canvas(
            move |b, _, _| rows.borrow_mut().push((ch, card, row, b)),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    pub(crate) fn release_effect(&mut self, cx: &mut Context<Self>) {
        let Some(Drag::Effect {
            ch, row, to, moved, ..
        }) = self.drag.take()
        else {
            return;
        };
        let shown = self.shown_effects(ch);
        let Some(&(from, fx)) = shown.get(row) else {
            return;
        };
        if moved {
            if let Some(&(at, _)) = shown.get(to) {
                self.move_effect(ch, from, at, cx);
            }
            return;
        }
        match fx {
            Fx::Eq => self.open_eq(ch, cx),
            Fx::Comp => self.open_fx(ch, Builtin::Comp, cx),
            Fx::Sat => self.open_fx(ch, Builtin::Sat, cx),
            Fx::Limit => self.open_fx(ch, Builtin::Limit, cx),
            Fx::More => {
                let k = Self::stacked_at(&shown, row);
                if let Some((b, _)) = self.stacked(ch, k) {
                    self.open_fx_at(ch, b, Some(k), cx);
                }
            }
            Fx::Plugin => {
                let n = shown[..row]
                    .iter()
                    .filter(|(_, f)| *f == Fx::Plugin)
                    .count();
                self.open_editor(ch, Which::Insert(n), cx);
            }
            _ => {}
        }
    }

    /// Move up and Move down, for the `row`th effect a strip shows.
    pub(crate) fn move_items(&self, ch: Channel, row: usize) -> Vec<(String, MenuAction)> {
        let shown = self.shown_effects(ch);
        let Some(&(from, _)) = shown.get(row) else {
            return Vec::new();
        };
        let to = |r: usize| MenuAction::MoveEffect {
            channel: ch,
            from,
            to: shown[r].0,
        };
        let mut items = Vec::new();
        if row > 0 {
            items.push(("Move up".to_string(), to(row - 1)));
        }
        if row + 1 < shown.len() {
            items.push(("Move down".to_string(), to(row + 1)));
        }
        items
    }

    /// Where an effect being dragged in this strip (or the panel's cards, `card`) would go: (the
    /// row it's from, the row it would land on), once it's moved.
    pub(crate) fn effect_drop(&self, ch: Channel, card: bool) -> Option<(usize, usize)> {
        match self.drag {
            Some(Drag::Effect {
                ch: c,
                card: k,
                row,
                to,
                moved: true,
                ..
            }) if c == ch && k == card => Some((row, to)),
            _ => None,
        }
    }
}
