//! Owned resize gestures outlive the hitbox where they began.
use gpui::{Context, DispatchPhase, MouseMoveEvent, MouseUpEvent, Window, prelude::*};

/// Mount on every frame, including before the press. The callbacks return
/// whether this owner consumed the gesture; ordinary input keeps its routing.
pub(crate) fn capture<T: 'static>(
    cx: &Context<T>,
    motion: fn(&mut T, &MouseMoveEvent, &mut Window, &mut Context<T>) -> bool,
    release: fn(&mut T, &MouseUpEvent, &mut Window, &mut Context<T>) -> bool,
) -> gpui::AnyElement {
    let owner = cx.entity().downgrade();
    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let moved = owner.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, app| {
                if phase == DispatchPhase::Capture
                    && moved
                        .update(app, |owner, cx| motion(owner, event, window, cx))
                        .unwrap_or(false)
                {
                    app.stop_propagation();
                }
            });
            let released = owner.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, app| {
                if phase == DispatchPhase::Capture
                    && released
                        .update(app, |owner, cx| release(owner, event, window, cx))
                        .unwrap_or(false)
                {
                    app.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}
