//! One delayed decorative tick per window, armed only by visible paint.
//!
//! A display-rate next-frame request dirties the mark's containing view and
//! ancestors. Coalesce owners and reuse recent event-driven paints, without
//! delaying input, terminal output or status notifications.
use gpui::{App, EntityId, Global, Window, WindowId};
use std::{collections::HashMap, time::Instant};

#[derive(Default)]
struct Frames(HashMap<WindowId, HashMap<EntityId, Instant>>);
impl Global for Frames {}

pub(super) fn request(window: &Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let now = cx.background_executor().now();
    let owners = cx.default_global::<Frames>().0.entry(id).or_default();
    let pending = !owners.is_empty();
    owners.insert(window.current_view(), now);
    if pending {
        return;
    }
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(crate::theme::activity::FRAME_INTERVAL)
                .await;
            let again = cx.update(|cx| {
                let now = cx.background_executor().now();
                let frames = cx.global_mut::<Frames>();
                let Some(owners) = frames.0.get_mut(&id) else {
                    return false;
                };
                let mut due = Vec::new();
                owners.retain(|owner, painted| {
                    if now.saturating_duration_since(*painted)
                        >= crate::theme::activity::FRAME_INTERVAL
                    {
                        due.push(*owner);
                        false
                    } else {
                        true
                    }
                });
                let again = !owners.is_empty();
                if !again {
                    frames.0.remove(&id);
                }
                let visible = handle.update(cx, |_, window, cx| {
                    if !window.is_visible() || cx.reduce_motion() {
                        return false;
                    }
                    for owner in due {
                        cx.notify(owner);
                    }
                    true
                });
                if !matches!(visible, Ok(true)) {
                    // Window loss must release the entry even if some owners
                    // were recently painted. No clock survives hidden work.
                    cx.global_mut::<Frames>().0.remove(&id);
                    return false;
                }
                again
            });
            if !again {
                break;
            }
        }
    })
    .detach();
}
