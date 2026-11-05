//! What comes out: its wave as it plays, its level, and how its voices play.

use super::*;

/// What came out, from where a wave next rises through zero: `n` samples, or nothing yet.
pub(crate) fn steady(scope: &[f32], n: usize) -> &[f32] {
    if scope.len() < n * 2 {
        return &[];
    }
    let from = scope.len() - n * 2;
    let rise = (from + 1..scope.len() - n)
        .find(|&i| scope[i - 1] < 0.0 && scope[i] >= 0.0)
        .unwrap_or(from);
    &scope[rise..rise + n]
}

impl Workspace {
    pub(crate) fn poly_output(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let scope = self
            .poly_view
            .as_ref()
            .map(|v| steady(&v.scope, 1200).to_vec())
            .unwrap_or_default();
        let id = self.session.project.tracks.get(t).map(|t| t.id);
        let level = self.levels.get(&id).map_or(0.0, |l| l.now[0].max(l.now[1]));
        let wave = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let mid = b.origin.y + b.size.height / 2.0;
                window.paint_quad(gpui::fill(
                    Bounds::new(point(b.origin.x, mid), gpui::size(b.size.width, px(1.))),
                    alpha(TEXT, 0.06),
                ));
                if scope.is_empty() {
                    return;
                }
                let n = scope.len();
                let points = scope
                    .iter()
                    .enumerate()
                    .step_by(2)
                    .map(|(i, s)| (i as f32 / (n - 1) as f32, 0.5 - s.clamp(-1.1, 1.1) * 0.46));
                polyline(window, b, points, 1.5, rgb(TEXT).into());
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let meter = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                window.paint_quad(gpui::fill(b, rgb(FIELD)).corner_radii(px(2.)));
                let db = 20.0 * level.max(1e-5).log10();
                let v = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
                let h = f32::from(b.size.height) * v;
                window.paint_quad(
                    gpui::fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + b.size.height - px(h)),
                            gpui::size(b.size.width, px(h)),
                        ),
                        rgb(if db > -1.0 { METER_HIGH } else { TEXT }),
                    )
                    .corner_radii(px(2.)),
                );
            },
        )
        .w(px(8.))
        .h(px(58.));
        let mode = match (p.voicing.mono, p.voicing.legato) {
            (false, _) => 0u8,
            (true, false) => 1,
            (true, true) => 2,
        };
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .child(title("Output")),
            )
            .child(
                picture()
                    .flex_none()
                    .h(px(96.))
                    .mx(px(10.))
                    .debug_selector(|| "poly-scope".into())
                    .child(wave),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .px(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(self.poly_knob(t, POLY_MASTER, "Master", 46.0, cx))
                            .child(meter),
                    )
                    .child(self.poly_choices(
                        t,
                        "poly-voicing",
                        mode,
                        &[("Poly", 0u8), ("Mono", 1), ("Legato", 2)],
                        |p, m| {
                            p.voicing.mono = m > 0;
                            p.voicing.legato = m == 2;
                        },
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_center()
                            .gap(px(6.))
                            .child(self.poly_knob(t, POLY_GLIDE, "Glide", 30.0, cx))
                            .child(self.poly_knob(t, POLY_DRIFT, "Drift", 30.0, cx))
                            .child(self.poly_knob(t, POLY_VELOCITY, "Velocity", 30.0, cx))
                            .child(self.poly_amount(t, PolyAmount::Voices, p, cx))
                            .child(self.poly_amount(t, PolyAmount::Bend, p, cx)),
                    )
                    .child(self.poly_switch(
                        t,
                        "Glide only when held",
                        "poly-glide-held",
                        p.voicing.glide_held,
                        |p| p.voicing.glide_held ^= true,
                        cx,
                    )),
            )
    }
}
