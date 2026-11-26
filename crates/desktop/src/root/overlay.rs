//! Generic presentation control. Its producer owns cancellation and persistence.
use crate::Root;
use gpui::{Context, Window};
use guild_session::{
    SessionId,
    command::Answer,
    overlay::{Content, Target},
};

pub(crate) struct Focus {
    pub(crate) terminal_geometry_set: bool,
    pub(crate) handle: gpui::FocusHandle,
    pub(crate) previous: Option<gpui::FocusHandle>,
    pub(crate) presentation: crate::observer::Presentation,
    pub(crate) viewport: std::rc::Rc<std::cell::Cell<[f32; 2]>>,
}

impl Root {
    /// Commands arrive without a Window; restore hidden presentation focus on paint.
    pub(crate) fn settle_overlay_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live: Vec<_> = self.tabs.iter().flat_map(|t| t.sessions()).collect();
        self.overlays.prune_sessions(&live);
        self.overlay_focus.borrow_mut().retain(|id, focus| {
            let entry = self.overlays.get(id);
            if !entry.is_some_and(|entry| entry.visible)
                && focus.handle.contains_focused(window, cx)
            {
                if let Some(previous) = focus.previous.take() {
                    previous.focus(window, cx);
                } else {
                    self.focusing_front = true;
                }
            }
            entry.is_some()
        });
    }

    pub(crate) fn command_overlay(
        &mut self,
        operation: &str,
        id: &str,
        title: &str,
        text: &str,
        caller: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Answer {
        let result = (|| {
            let caller = caller.ok_or("Run overlay commands inside a Guild session")?;
            let session: SessionId = caller.parse().map_err(|_| "Invalid calling session")?;
            let record = self
                .desk
                .session(session)
                .ok_or("Calling session is not in this window")?;
            if !record.open {
                return Err("Calling session is closed".to_owned());
            }
            match operation {
                "open" => self.overlays.open(
                    id,
                    caller,
                    Target::Session(session),
                    title,
                    Content::Text {
                        text: text.to_owned(),
                    },
                )?,
                "show" => self.overlays.show(id, caller)?,
                "hide" => self.overlays.hide(id, caller)?,
                "close" => self.overlays.close(id, caller)?,
                "list" => {}
                _ => return Err("Unknown overlay operation".into()),
            }
            Ok(self.overlays.list(caller))
        })();
        cx.notify();
        match result {
            Ok(overlays) => Answer::Overlays { overlays },
            Err(because) => Answer::Refused { because },
        }
    }
    pub(crate) fn dismiss_overlay(
        &mut self,
        id: &str,
        owner: &str,
        close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.overlays.get(id)
            && let Content::Terminal { job } = entry.content
        {
            let _ = self.terminal_jobs.present(
                &job,
                Some(entry.target.clone()),
                if close {
                    guild_session::terminal_job::Presentation::Closed
                } else {
                    guild_session::terminal_job::Presentation::Hidden
                },
            );
        }
        if close {
            let _ = self.overlays.close(id, owner);
        } else {
            let _ = self.overlays.hide(id, owner);
        }
        let mut handles = self.overlay_focus.borrow_mut();
        if let Some(focus) = handles.get_mut(id) {
            focus.presentation.gesture = None;
        }
        if let Some(focus) = handles.get_mut(id)
            && focus.handle.contains_focused(window, cx)
        {
            if let Some(previous) = focus.previous.take() {
                previous.focus(window, cx);
            } else {
                self.focusing_front = true;
            }
        }
        if close {
            handles.remove(id);
        }
        cx.notify();
    }
}

impl Focus {
    pub(crate) fn new(
        handle: gpui::FocusHandle,
        width: f32,
        height: f32,
        large_width: f32,
        large_height: f32,
        minimum: [[f32; 2]; 2],
    ) -> Self {
        Self {
            terminal_geometry_set: false,
            handle,
            previous: None,
            presentation: crate::observer::Presentation::new(
                crate::observer::Geometry {
                    x: f32::MAX,
                    y: 0.,
                    width,
                    height,
                },
                crate::observer::Geometry {
                    x: f32::MAX,
                    y: 0.,
                    width: large_width,
                    height: large_height,
                },
                minimum,
            ),
            viewport: Default::default(),
        }
    }
}
