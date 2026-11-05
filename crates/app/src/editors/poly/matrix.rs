//! The matrix: every route the synth has, to read and change at once, narrowed to a kind of source.

use super::*;
use modulation::{depth_bar, remove};

pub(crate) fn source_name(s: PolySource) -> String {
    match s {
        PolySource::Env(i) => format!("Envelope {}", i + 1),
        PolySource::Lfo(i) => format!("LFO {}", i + 1),
        PolySource::Macro(i) => format!("Macro {}", i + 1),
        PolySource::Velocity => "Velocity".into(),
        PolySource::Note => "Note".into(),
        PolySource::Wheel => "Mod Wheel".into(),
        PolySource::Pressure => "Pressure".into(),
        PolySource::Bend => "Pitch Bend".into(),
        PolySource::Random => "Random".into(),
        PolySource::Slide => "Slide".into(),
    }
}

fn sources() -> Vec<PolySource> {
    (0..ENVS as u8)
        .map(PolySource::Env)
        .chain((0..LFOS as u8).map(PolySource::Lfo))
        .chain((0..MACROS as u8).map(PolySource::Macro))
        .chain([
            PolySource::Velocity,
            PolySource::Note,
            PolySource::Wheel,
            PolySource::Pressure,
            PolySource::Bend,
            PolySource::Random,
            PolySource::Slide,
        ])
        .collect()
}

impl Workspace {
    pub(crate) fn dest_name(&self, t: usize, d: PolyDest) -> String {
        match d {
            PolyDest::Param(i) => POLY_PARAMS
                .get(i as usize)
                .map_or("?".into(), |p| p.name.clone()),
            PolyDest::Effect(k, i) => self.param_name(t, Param::Tool(k, i)),
        }
    }

    pub(crate) fn poly_matrix_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let kind = self.poly_view.as_ref().map_or(RouteKind::All, |v| v.routes);
        let rows: Vec<Div> = p
            .routes
            .iter()
            .enumerate()
            .filter(|(_, r)| kind.has(r.source))
            .map(|(i, r)| self.route_row(t, i, r, cx))
            .collect();
        let none = rows.is_empty();
        let add = button("Add a route", false)
            .debug_selector(|| "poly-add-route".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.poly_op(t, PolyOp::AddRoute, cx);
                }),
            );
        let kinds = tray().p(px(2.)).rounded(px(7.)).children(
            [
                ("All", RouteKind::All),
                ("Envelopes", RouteKind::Env),
                ("LFOs", RouteKind::Lfo),
                ("Macros", RouteKind::Macro),
                ("Playing", RouteKind::Playing),
            ]
            .map(|(name, k)| {
                seg(name, k == kind, 24.)
                    .debug_selector(move || format!("poly-routes-{}", name.to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(v) = ws.poly_view.as_mut() {
                                v.routes = k;
                            }
                            cx.notify();
                        }),
                    )
            }),
        );
        let head = |s: &'static str, w: f32| {
            div()
                .w(px(w))
                .flex_none()
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(s)
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .px(px(20.))
            .pt(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(title("Modulation"))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .child(format!("{} of {MAX_POLY_ROUTES}", p.routes.len())),
                    )
                    .child(div().flex_1())
                    .child(kinds)
                    .child(add),
            )
            .child(label(
                "Or drag any source onto any knob, here or on the Effects page.",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .pt(px(6.))
                    .pb(px(4.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(head("Source", 190.))
                    .child(head("Scaled by", 170.))
                    .child(div().flex_1().child(head("Moves", 100.)))
                    .child(head("Depth", 330.)),
            )
            .child(
                div()
                    .id("poly-routes")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(none, |d| {
                        d.child(label("No routes of this kind.").py(px(10.)))
                    }),
            )
    }

    fn route_row(&self, t: usize, i: usize, r: &PolyRoute, cx: &mut Context<Self>) -> Div {
        let source = self
            .poly_menu(
                format!("poly-route-source-{i}"),
                source_name(r.source),
                move |_| {
                    sources()
                        .into_iter()
                        .map(|s| {
                            (
                                source_name(s),
                                MenuAction::Poly(t, PolyOp::RouteSource(i, s)),
                            )
                        })
                        .collect()
                },
                true,
                cx,
            )
            .text_color(rgb(source_color(r.source)));
        let by = self.poly_menu(
            format!("poly-route-by-{i}"),
            r.by.map_or("Nothing".to_string(), source_name),
            move |_| {
                std::iter::once(("Nothing".to_string(), None))
                    .chain(sources().into_iter().map(|s| (source_name(s), Some(s))))
                    .map(|(n, s)| (n, MenuAction::Poly(t, PolyOp::RouteBy(i, s))))
                    .collect()
            },
            true,
            cx,
        );
        let dest = self.poly_menu(
            format!("poly-route-dest-{i}"),
            self.dest_name(t, r.dest),
            move |ws| ws.poly_dests(t, i),
            true,
            cx,
        );
        div()
            .h(px(38.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(div().w(px(190.)).flex_none().child(source))
            .child(div().w(px(170.)).flex_none().child(by))
            .child(div().flex_1().min_w(px(0.)).child(dest))
            .child(depth_bar(r.depth, 260.0, source_color(r.source), i, t, cx))
            .child(
                div()
                    .w(px(44.))
                    .flex_none()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("{:+.0}%", r.depth * 100.0)),
            )
            .child(remove(i, t, "poly-route-remove", cx))
    }

    /// Everything a route can move: the synth's parameters, then the track's effects' knobs.
    fn poly_dests(&self, t: usize, route: usize) -> Vec<(String, MenuAction)> {
        let mut items = Vec::new();
        let mut group = "";
        for (i, q) in POLY_PARAMS.iter().enumerate() {
            if q.group != group {
                group = q.group;
                items.push((group.to_string(), MenuAction::Nothing));
            }
            let d = PolyDest::Param(i as u16);
            items.push((
                q.name.clone(),
                MenuAction::Poly(t, PolyOp::RouteDest(route, d)),
            ));
        }
        let track = &self.session.project.tracks[t];
        let own = [
            (OWN_COMP, track.comp.map(Unit::Comp)),
            (OWN_SAT, track.sat.map(Unit::Sat)),
        ];
        let stacked = track
            .more
            .iter()
            .enumerate()
            .map(|(k, u)| (k as u8, Some(*u)));
        for (place, u) in own.into_iter().chain(stacked) {
            let Some(u) = u else { continue };
            let table = crate::mix::strip::knobs(u.kind());
            for (i, _) in table.iter().enumerate().filter(|(i, _)| knob_shows(&u, *i)) {
                let d = PolyDest::Effect(place, i as u8);
                items.push((
                    self.dest_name(t, d),
                    MenuAction::Poly(t, PolyOp::RouteDest(route, d)),
                ));
            }
        }
        items
    }
}
