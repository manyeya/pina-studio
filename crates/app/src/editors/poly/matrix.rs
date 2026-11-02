//! The matrix: every route the synth has, to read and change at once.

use super::*;

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
        ])
        .collect()
}

impl Workspace {
    fn dest_name(&self, t: usize, d: PolyDest) -> String {
        match d {
            PolyDest::Param(i) => POLY_PARAMS
                .get(i as usize)
                .map_or("?".into(), |p| p.name.clone()),
            PolyDest::Effect(k, i) => self.param_name(t, Param::Tool(k, i)),
        }
    }

    pub(crate) fn poly_matrix_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let mut rows = Vec::new();
        for (i, r) in p.routes.iter().enumerate() {
            rows.push(self.route_row(t, i, r, cx));
        }
        let add = button("Add a route", false)
            .debug_selector(|| "poly-add-route".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.poly_op(t, PolyOp::AddRoute, cx);
                }),
            );
        let head = |s: &'static str, w: f32| {
            div()
                .w(px(w))
                .text_size(px(10.5))
                .text_color(rgb(TEXT_4))
                .child(s)
        };
        div()
            .flex_1()
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(add)
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("Or drag a ↗ from any envelope, LFO or macro onto any knob here or on the Effects page."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .pt(px(4.))
                    .child(head("Source", 190.))
                    .child(head("Scaled by", 170.))
                    .child(head("Moves", 300.))
                    .child(head("Depth", 300.)),
            )
            .children(rows)
    }

    fn route_row(&self, t: usize, i: usize, r: &PolyRoute, cx: &mut Context<Self>) -> Div {
        let source = self.poly_menu(
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
        );
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
        let depth = r.depth;
        let w = 260.0;
        let bar = div()
            .relative()
            .w(px(w))
            .h(px(18.))
            .rounded(px(4.))
            .bg(rgb(0x191a20))
            .cursor_ew_resize()
            .debug_selector(move || format!("poly-route-depth-{i}"))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(w / 2.0 + depth.min(0.0) * w / 2.0))
                    .w(px((depth.abs() * w / 2.0).max(1.0)))
                    .bg(alpha(SHAPE_COLOR_, 0.7)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.begin_drag(
                        Drag::PolyDepth {
                            track: t,
                            route: i,
                            x: pt(e.position).x,
                            depth,
                        },
                        cx,
                    );
                }),
            );
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().w(px(190.)).child(source))
            .child(div().w(px(170.)).child(by))
            .child(div().w(px(300.)).child(dest))
            .child(bar)
            .child(
                div()
                    .w(px(44.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("{:+.0}%", depth * 100.0)),
            )
            .child(
                div()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(OFF)))
                    .debug_selector(move || format!("poly-route-remove-{i}"))
                    .child("×")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_op(t, PolyOp::RemoveRoute(i), cx);
                        }),
                    ),
            )
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
            let table = strip::knobs(u.kind());
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

const SHAPE_COLOR_: u32 = 0xa78bfa;
