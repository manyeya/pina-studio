//! The toolbar over the track headers: the tools, snap, and adding tracks.

use super::*;

impl Workspace {
    /// Over the track headers: the tools, the snap, and adding a track.
    pub(crate) fn toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = |i: Icon, t: Tool, on: bool, cx: &mut Context<Self>| {
            icon_seg(i, on, 24.)
                .debug_selector(move || format!("tool-{t:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.tool = t;
                        cx.notify();
                    }),
                )
        };
        let snap = match self.snap {
            Snap::Smart => match self.grid() {
                g if g == PPQ / 4 => "1/16",
                g if g == PPQ => "Beat",
                _ => "Bar",
            },
            s => s.label(),
        };
        div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(10.))
            .pr(px(8.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_r_1()
            .border_color(rgb(LINE))
            .child({
                // The four always to hand, then the one other in hand (or the rest's menu): Text,
                // Glue, Solo, Mute, Zoom, Fade, Flex: the rest of the Tool menu (T).
                let mains: Vec<_> = Tool::ALL
                    .into_iter()
                    .filter(|t| t.main())
                    .map(|t| tool(t.icon(), t, self.tool == t, cx))
                    .collect();
                let other = (!self.tool.main()).then_some(self.tool);
                let more = div()
                    .h(px(24.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(other.is_some(), |d| d.bg(rgb(RAISED)))
                    .when(other.is_none(), |d| d.hover(|d| d.bg(rgb(OFF))))
                    .debug_selector(|| "tool-more".into())
                    .child(
                        icon(other.map_or(Icon::More, |t| t.icon()), 14.)
                            .text_color(rgb(if other.is_some() { TEXT } else { TEXT_3 })),
                    )
                    .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.tool_menu(e.position, cx);
                        }),
                    );
                tray().p(px(2.)).rounded(px(8.)).children(mains).child(more)
            })
            .child(
                div()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(OFF)))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_2))
                    .child(icon(Icon::Magnet, 13.).text_color(rgb(TEXT_3)))
                    .child(snap)
                    .debug_selector(|| "snap".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let items = Snap::ALL
                                .iter()
                                .map(|&s| {
                                    let label = if s == Snap::Smart {
                                        "Smart (follows zoom)".to_string()
                                    } else {
                                        s.label().to_string()
                                    };
                                    (label, MenuAction::Snap(s))
                                })
                                .collect();
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(OFF)))
                    .text_color(rgb(TEXT_2))
                    .child(icon(Icon::Plus, 14.).text_color(rgb(TEXT_2)))
                    .debug_selector(|| "new-track".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, window, cx| {
                            // The New Track palette, opening down from the button.
                            let at = e.position + gpui::point(px(-14.), px(18.));
                            ws.open_adder(at, window, cx);
                        }),
                    ),
            )
    }
}
