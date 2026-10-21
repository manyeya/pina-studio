//! The step sequencer's control bar.

use super::*;

impl Workspace {
    /// The step sequencer's own controls in the editor's bar: length, rate, swing, to MIDI.
    pub fn steps_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let at = self.pattern()?;
        let Content::Pattern {
            swing,
            step,
            ref rows,
        } = self.session.project.region(at).content
        else {
            return None;
        };
        let steps = rows.iter().map(|r| r.cells.len()).max().unwrap_or(16);
        let length = [8usize, 16, 32, 64].map(|n| {
            seg(n.to_string(), steps == n, 22.)
                .px(px(7.))
                .font_family(MONO)
                .text_size(px(10.5))
                .debug_selector(move || format!("steps-{n}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.set_pattern_steps(at, n, cx)),
                )
        });
        let rates = [("1/8", PPQ / 2), ("1/16", PPQ / 4), ("1/32", PPQ / 8)].map(|(label, r)| {
            seg(label, step == r, 22.)
                .px(px(7.))
                .font_family(MONO)
                .text_size(px(10.5))
                .debug_selector(move || format!("rate-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.set_pattern_rate(at, r, cx)),
                )
        });
        let swing_button = button(format!("Swing {swing:.0}%"), swing > 50.0)
            .debug_selector(|| "pattern-swing".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let items = SWINGS
                        .iter()
                        .map(|&s| {
                            let name = match s {
                                50.0 => "50%  straight".to_string(),
                                66.0 => "66%  triplet feel".to_string(),
                                s => format!("{s:.0}%"),
                            };
                            (name, MenuAction::PatternSwing { at, swing: s })
                        })
                        .collect();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w(px(0.))
                .overflow_hidden()
                .child(tray().p(px(2.)).rounded(px(7.)).children(length))
                .child(tray().p(px(2.)).rounded(px(7.)).children(rates))
                .child(swing_button)
                .child(
                    button("To MIDI", false)
                        .debug_selector(|| "pattern-to-midi".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.pattern_to_midi(at, cx)),
                        ),
                ),
        )
    }
}
