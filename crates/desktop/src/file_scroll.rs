//! File-local gesture filtering; the component still owns scrolling and its side effects.
use gpui::{prelude::*, *};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct Gesture {
    started: bool,
    axis: Option<Axis>,
}
impl Gesture {
    fn filter(&mut self, event: &ScrollWheelEvent) -> ScrollDelta {
        let ScrollDelta::Pixels(mut delta) = event.delta else {
            // macOS supplies Lines for an ordinary wheel, including Shift-wheel.
            *self = Self::default();
            return event.delta;
        };
        if event.touch_phase == TouchPhase::Started {
            self.started = true;
            self.axis = None;
        }
        if self.started && self.axis.is_none() && (delta.x != px(0.) || delta.y != px(0.)) {
            self.axis = Some(if delta.x.abs() > delta.y.abs() {
                Axis::Horizontal
            } else {
                Axis::Vertical
            });
        }
        match self.axis {
            Some(Axis::Horizontal) => delta.y = px(0.),
            Some(Axis::Vertical) => delta.x = px(0.),
            None => {}
        }
        // gpui-pre-macos maps native Ended to Ended, then momentum to Moved.
        // Ended therefore retains the lock; explicit cancellation still unwinds it.
        if event.touch_phase == TouchPhase::Cancelled {
            *self = Self::default();
        }
        ScrollDelta::Pixels(delta)
    }
}

#[derive(Clone, Default)]
pub(crate) struct Scroll {
    gesture: Rc<RefCell<Gesture>>,
    replaying: Rc<Cell<bool>>,
}
impl Scroll {
    pub(crate) fn wrap(&self, body: impl IntoElement) -> Surface {
        Surface {
            body: body.into_any_element(),
            scroll: self.clone(),
        }
    }
    pub(crate) fn reset(&self) {
        *self.gesture.borrow_mut() = Gesture::default();
    }
}

pub(crate) struct Surface {
    body: AnyElement,
    scroll: Scroll,
}
impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Surface {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.body.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        // Insert behind the child: Find, completion, hover and other occluding
        // surfaces retain their own input, even when they overlap this rectangle.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.body.prepaint(window, cx);
        hitbox
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let hitbox = hitbox.clone();
        let scroll = self.scroll.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || scroll.replaying.get() { return; }
            if !hitbox.should_handle_scroll(window) {
                // A new gesture over a neighboring surface cannot belong to us.
                scroll.reset();
                return;
            }
            let delta = scroll.gesture.borrow_mut().filter(event);
            if !matches!((delta, event.delta), (ScrollDelta::Pixels(a), ScrollDelta::Pixels(b)) if a != b) { return; }
            let mut filtered = event.clone();
            filtered.delta = delta;
            cx.stop_propagation();
            let scroll = scroll.clone();
            // During dispatch GPUI takes its listener vector out of the window.
            // Defer to the end of this effect cycle, not the next frame, so the
            // original handlers receive exactly one event and retain clamping,
            // popover dismissal, edit-menu phases and nested Markdown routing.
            window.defer(cx, move |window, cx| {
                scroll.replaying.set(true);
                window.dispatch_event(PlatformInput::ScrollWheel(filtered), cx);
                scroll.replaying.set(false);
            });
        });
        self.body.paint(window, cx);
    }
}
