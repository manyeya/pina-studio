//! The picked pad's effects, after its Drum Sampler: its channel's chain in the order it runs,
//! each opening its window (drag to reorder, right-click to move or take out), and an empty
//! slot to add another.

use pina_engine::{audio::player::Which, song::project::Fx};

use super::*;
use crate::mix::strip::{Builtin, Channel, builtin_slot, dropping, insert_slot, plugin_slot};

const SLOT_H: f32 = 22.0;

impl Workspace {
    pub(crate) fn effects_view(&mut self, t: usize, cx: &mut Context<Self>) -> Div {
        let ch = Channel::Track(t);
        let color = TEAL;
        let (eq, comp) = self.strip_dsp(ch);
        let inserts = self.inserts(ch).to_vec();
        let drop = self.effect_drop(ch, false);
        let mut slots = Vec::new();
        let (mut n, mut m) = (0, 0);
        for (row, (_, fx)) in self.shown_effects(ch).into_iter().enumerate() {
            let off = |bypass: bool| if bypass { TEXT_4 } else { color };
            let slot = match fx {
                Fx::More => {
                    m += 1;
                    let k = m - 1;
                    let Some((b, name)) = self.stacked(ch, k) else {
                        continue;
                    };
                    let bypassed = self.more(ch).get(k).is_some_and(|u| u.bypassed());
                    builtin_slot(name, ch, b, Some(k), row, SLOT_H, off(bypassed), cx)
                }
                Fx::Eq => builtin_slot(
                    "Channel EQ",
                    ch,
                    Builtin::Eq,
                    None,
                    row,
                    SLOT_H,
                    off(eq.is_some_and(|e| e.bypass)),
                    cx,
                ),
                Fx::Comp => builtin_slot(
                    "Compressor",
                    ch,
                    Builtin::Comp,
                    None,
                    row,
                    SLOT_H,
                    off(comp.is_some_and(|c| c.bypass)),
                    cx,
                ),
                Fx::Sat => builtin_slot(
                    "Saturator",
                    ch,
                    Builtin::Sat,
                    None,
                    row,
                    SLOT_H,
                    off(self.sat(ch).is_some_and(|s| s.bypass)),
                    cx,
                ),
                Fx::Limit => builtin_slot(
                    "Limiter",
                    ch,
                    Builtin::Limit,
                    None,
                    row,
                    SLOT_H,
                    off(self.limiter(ch).is_some_and(|l| l.bypass)),
                    cx,
                ),
                Fx::Plugin => {
                    n += 1;
                    insert_slot(inserts[n - 1].class.clone(), ch, n - 1, row, SLOT_H, cx)
                }
                _ => continue,
            };
            let slot = slot.relative().child(self.effect_place(ch, false, row));
            slots.push(div().w(px(170.)).child(dropping(slot, row, drop)));
        }
        let count = slots.len();
        slots.push(div().w(px(170.)).child(plugin_slot(
            String::new(),
            false,
            ch,
            Which::Insert(inserts.len()),
            SLOT_H,
            None,
            cx,
        )));
        // Its effects across in the order they run, wrapping to the width.
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(8.))
            .child(div().flex().flex_wrap().gap(px(4.)).children(slots))
            .when(count == 0, |d| {
                d.child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .child("Its own effects go here: click + to add one."),
                )
            })
    }
}
