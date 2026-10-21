//! The window's listeners for drags in progress and files dropped in from the Finder.

use super::*;

impl Workspace {
    /// Catches pointer moves and releases anywhere in the window while dragging.
    pub(crate) fn drag_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| {
                            ws.fine = e.modifiers.shift;
                            ws.drag_to(pt(e.position), cx)
                        })
                        .ok();
                    }
                });
                let t = this.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| ws.end_drag(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// While files from the Finder are over the window: letting go drops them where the pointer
    /// is, wherever that is.
    pub(crate) fn file_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| {
                            if let Some((_, paths)) = ws.file_drag.take() {
                                ws.drop_files(&paths, pt(e.position), cx);
                            }
                        })
                        .ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}
