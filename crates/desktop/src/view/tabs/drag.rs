//! One held tab owns its frame pump and a cheap, non-painting lifecycle clock.
use super::TabDrag;
use crate::{Root, TabId, view::drag::EdgeMotion};
use gpui::{Bounds, Context, IntoElement, Pixels, ScrollHandle, Styled, Window, point, px};
use guild_session::{AreaId, Seat};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

type Area = Option<AreaId>;
#[derive(Clone)]
struct Geometry {
    bar: Bounds<Pixels>,
    strip: Bounds<Pixels>,
    scroll: ScrollHandle,
}
#[derive(Default)]
pub(crate) struct State {
    pub active: Option<Gesture>,
    bounds: Rc<RefCell<HashMap<Area, Geometry>>>,
}
pub(crate) struct Gesture {
    token: uuid::Uuid,
    tab: TabId,
    seat: Seat,
    source_area: Area,
    motion: EdgeMotion,
    _clock: gpui::Task<()>,
}
impl State {
    pub fn retain(&self, areas: &[Area]) {
        self.bounds
            .borrow_mut()
            .retain(|area, _| areas.contains(area));
    }
    pub fn measure(&self, area: Area, scroll: ScrollHandle) -> impl IntoElement {
        let geometry = self.bounds.clone();
        gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let viewport = Bounds::new(point(px(0.), px(0.)), window.viewport_size());
                let bar = bounds
                    .intersect(&window.content_mask().bounds)
                    .intersect(&viewport);
                geometry.borrow_mut().insert(
                    area,
                    Geometry {
                        bar,
                        strip: scroll.bounds().intersect(&bar),
                        scroll: scroll.clone(),
                    },
                );
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}
impl Root {
    pub(crate) fn begin_tab_drag(
        &mut self,
        drag: &TabDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(seat) = self.active_seat() else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|t| t.id == drag.id && t.seat == seat) else {
            return;
        };
        let source_area = Some(tab.area);
        let token = drag.token;
        let weak = cx.weak_entity();
        let clock = window.spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let alive = cx
                    .update(|window, cx| {
                        weak.update(cx, |root, cx| {
                            if root
                                .tab_drag
                                .active
                                .as_ref()
                                .is_none_or(|g| g.token != token)
                            {
                                return false;
                            }
                            root.wake_tab_drag(window, cx);
                            root.tab_drag.active.is_some()
                        })
                    })
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        });
        self.tab_drag.active = Some(Gesture {
            token,
            tab: drag.id,
            seat,
            source_area,
            motion: EdgeMotion::new(cx.background_executor().now()),
            _clock: clock,
        });
        // on_drag runs before GPUI installs the payload; validate on the frame.
        self.schedule_tab_frame(window, cx);
        cx.notify();
    }
    pub(crate) fn clear_tab_drag(&mut self, cx: &mut Context<Self>) {
        if self.tab_drag.active.take().is_some() {
            cx.notify();
        }
    }
    fn tab_drag_valid(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(g) = &self.tab_drag.active else {
            return false;
        };
        let areas = self.stage.areas(g.seat);
        if !cx.has_active_drag()
            || (!cfg!(test) && !crate::system::primary_button_down())
            || self.active_seat() != Some(g.seat)
            || !self.tabs.iter().any(|t| t.id == g.tab && t.seat == g.seat)
            || !g
                .source_area
                .is_some_and(|a| areas.is_some_and(|areas| areas.contains(a)))
            || self.workspace_swipe_offset != 0.
        {
            self.clear_tab_drag(cx);
            cx.stop_active_drag(window);
            return false;
        }
        true
    }
    fn tab_edge(&self, window: &Window) -> Option<(ScrollHandle, Pixels)> {
        let seat = self.tab_drag.active.as_ref()?.seat;
        let areas = self.stage.areas(seat)?;
        let at = window.mouse_position();
        let row = px(crate::theme::space::TREE_ROW.0 * f32::from(window.rem_size()));
        self.tab_drag.bounds.borrow().iter().find_map(|(area, g)| {
            if !area.is_some_and(|a| areas.contains(a))
                || !g.bar.contains(&at)
                || g.strip.size.width <= px(0.)
            {
                return None;
            }
            // Include the fixed adjoining chrome, but it never becomes a drop target.
            let sign = if at.x < g.strip.left() + row {
                1.
            } else if at.x > g.strip.right() - row {
                -1.
            } else {
                return None;
            };
            let x = g.scroll.offset().x;
            if (sign > 0. && x >= px(0.)) || (sign < 0. && x <= -g.scroll.max_offset().x) {
                return None;
            }
            Some((g.scroll.clone(), row * sign))
        })
    }
    pub(crate) fn wake_tab_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tab_drag_valid(window, cx) {
            return;
        }
        if self.tab_drag.active.as_ref().unwrap().motion.pending {
            return;
        }
        self.tab_drag
            .active
            .as_mut()
            .unwrap()
            .motion
            .pause(cx.background_executor().now());
        if self.tab_edge(window).is_some() {
            self.schedule_tab_frame(window, cx);
        }
    }
    fn schedule_tab_frame(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(g) = &mut self.tab_drag.active else {
            return;
        };
        if g.motion.pending {
            return;
        }
        g.motion.pending = true;
        let token = g.token;
        let weak = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |root, cx| {
                if root
                    .tab_drag
                    .active
                    .as_ref()
                    .is_none_or(|g| g.token != token)
                {
                    return;
                }
                root.tab_drag.active.as_mut().unwrap().motion.pending = false;
                if !root.tab_drag_valid(window, cx) {
                    return;
                }
                let now = cx.background_executor().now();
                let Some((scroll, row)) = root.tab_edge(window) else {
                    root.tab_drag.active.as_mut().unwrap().motion.pause(now);
                    return;
                };
                let distance = root
                    .tab_drag
                    .active
                    .as_mut()
                    .unwrap()
                    .motion
                    .distance(now, row);
                let mut offset = scroll.offset();
                offset.x = (offset.x + distance).clamp(-scroll.max_offset().x, px(0.));
                if offset != scroll.offset() {
                    scroll.set_offset(offset);
                    cx.notify();
                }
                if root.tab_edge(window).is_some() {
                    root.schedule_tab_frame(window, cx);
                }
            });
        });
    }
}
