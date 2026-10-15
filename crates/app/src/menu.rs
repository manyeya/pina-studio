//! A pop-up list of choices, like the plugin and bus menus on Logic's channel strips.

use gpui::{Context, MouseButton, Pixels, Point, anchored, deferred, div, prelude::*, px, rgb};
use tempo_engine::project::{AuxSend, Instrument, PluginRef};

use crate::{strip::Channel, theme::*, workspace::Workspace};

pub struct Menu {
    pub at: Point<Pixels>,
    pub items: Vec<(String, MenuAction)>,
}

#[derive(Clone)]
pub enum MenuAction {
    Nothing,
    Instrument {
        track: usize,
        instrument: Instrument,
    },
    /// Puts a plugin in an insert slot (an `index` past the end adds one), or takes it out.
    Insert {
        channel: Channel,
        index: usize,
        plugin: Option<PluginRef>,
    },
    /// Sends a track to a bus, or to a new aux.
    Send {
        track: usize,
        bus: Option<u32>,
    },
    /// Plays a track into a bus, or the master.
    Output {
        track: usize,
        bus: Option<u32>,
    },
}

impl Workspace {
    pub fn choose(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            MenuAction::Nothing => {}
            MenuAction::Instrument { track, instrument } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].instrument = Some(instrument);
            }),
            MenuAction::Insert {
                channel,
                index,
                plugin,
            } => self.edit(cx, |ws| {
                let Some(list) = ws.inserts_mut(channel) else {
                    return;
                };
                match plugin {
                    Some(p) if index < list.len() => list[index] = p,
                    Some(p) => list.push(p),
                    None if index < list.len() => drop(list.remove(index)),
                    None => {}
                }
            }),
            MenuAction::Send { track, bus } => self.edit(cx, |ws| {
                let project = &mut ws.session.project;
                let bus = bus.unwrap_or_else(|| project.add_aux());
                project.tracks[track]
                    .sends
                    .push(AuxSend { bus, level_db: 0.0 });
            }),
            MenuAction::Output { track, bus } => self.edit(cx, |ws| {
                ws.session.project.tracks[track].output = bus;
            }),
        }
        cx.notify();
    }

    pub fn menu_view(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.menu.as_ref()?;
        let items: Vec<_> = menu
            .items
            .iter()
            .enumerate()
            .map(|(i, (label, action))| {
                let action = action.clone();
                div()
                    .px_2()
                    .py_1()
                    .rounded(px(3.))
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .hover(|s| s.bg(rgb(0x3d6fd6)))
                    .debug_selector(move || format!("menu-{i}"))
                    .child(label.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.choose(action.clone(), cx);
                        }),
                    )
            })
            .collect();
        let list = div()
            .id("menu")
            .min_w(px(200.))
            .max_h(px(420.))
            .overflow_y_scroll()
            .p_1()
            .rounded(px(6.))
            .bg(rgb(0x2c2c30))
            .border_1()
            .border_color(rgb(0x4a4a50))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .children(items)
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                ws.menu = None;
                cx.notify();
            }));
        Some(deferred(anchored().position(menu.at).snap_to_window().child(list)).with_priority(1))
    }
}
