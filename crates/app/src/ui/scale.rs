//! A window scaled as a whole, as a plugin's is: its corner dragged, everything in it (sizes, gaps,
//! type, knobs, pictures) grows or shrinks together, and nothing moves about inside it.
//!
//! The windows that scale build their elements with this `px`, which multiplies by the scale of
//! the window being built (1 everywhere else). `Scaled` keeps that scale while what's in it is
//! laid out, for views (a text field) that build themselves then. A picture drawn in the
//! window's own units (not its box's) is a `scaled_canvas`, which keeps the scale while it draws.

use std::{cell::Cell, panic::Location};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

thread_local! {
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// `n` of the window's own units, as pixels at the scale it's being built at.
pub fn px(n: f32) -> Pixels {
    gpui::px(n * scale())
}

/// The scale of the window being built (1 outside one).
pub fn scale() -> f32 {
    SCALE.with(Cell::get)
}

/// `f` with what it builds at scale `s`.
pub fn with_scale<R>(s: f32, f: impl FnOnce() -> R) -> R {
    let was = SCALE.with(|c| c.replace(s));
    let r = f();
    SCALE.with(|c| c.set(was));
    r
}

/// A picture drawn in the window's own units: what it draws keeps the window's scale.
pub fn scaled_canvas<T: 'static>(
    prepaint: impl 'static + FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> T,
    paint: impl 'static + FnOnce(Bounds<Pixels>, T, &mut Window, &mut App),
) -> gpui::Canvas<T> {
    let s = scale();
    gpui::canvas(
        move |b, window, cx| with_scale(s, || prepaint(b, window, cx)),
        move |b, t, window, cx| with_scale(s, || paint(b, t, window, cx)),
    )
}

/// What's in it, laid out at `scale`: the views in it (a text field) build themselves then.
pub struct Scaled {
    scale: f32,
    child: AnyElement,
}

pub fn scaled(scale: f32, child: impl IntoElement) -> Scaled {
    Scaled {
        scale,
        child: child.into_any_element(),
    }
}

impl IntoElement for Scaled {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Scaled {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let child = &mut self.child;
        (
            with_scale(self.scale, || child.request_layout(window, cx)),
            (),
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_the_scale_being_built_at_and_only_there() {
        assert_eq!(px(10.), gpui::px(10.));
        let inside = with_scale(1.5, || (px(10.), with_scale(2.0, || px(10.)), px(4.)));
        assert_eq!(inside, (gpui::px(15.), gpui::px(20.), gpui::px(6.)));
        assert_eq!(px(10.), gpui::px(10.));
    }
}
