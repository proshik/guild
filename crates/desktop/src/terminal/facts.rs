//! What the window knows about a terminal tab without reading its entity.
//!
//! **Reading a view while the window draws ties the window to it.** GPUI
//! records every entity read or updated during a draw, and a later
//! `notify` of any of them invalidates that window — whose root is not
//! cached, so the whole window is built, laid out and painted again. The
//! window read every terminal tab on every frame, for a title, for the
//! agent in front and for whether it had exited, and a terminal printing
//! in a tab nobody could see redrew the window at the terminal's pace:
//! measured on 2026-09-24, two background tabs printing kept a window
//! with nothing changing on it at 20–35 frames a second and 10 % of a
//! core against 3 % idle.
//!
//! So the tab publishes those facts here, beside itself, and the window
//! reads them without touching the entity. The tab tells its listener —
//! the window's root, by id — only when a fact changes what the window
//! would draw.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::EntityId;
use guild_session::HarnessKind;

/// The facts, shared between a terminal tab and whoever shows it.
#[derive(Default)]
pub struct Facts {
    /// The title the program set, as it set it.
    title: RefCell<Option<String>>,
    /// Which agent CLI the terminal runs in front — M12.3, M14.
    harness: Cell<Option<HarnessKind>>,
    /// The conversation the program in front resumes by id — M13.3, what
    /// a terminal tab hands over to the window.
    resumed: RefCell<Option<(HarnessKind, String)>>,
    /// Whether the process has ended.
    exited: Cell<bool>,
    /// Who hears of a change: the window's root, by id, so that telling
    /// it leases nothing — `notify_push_dialog`'s rule.
    listener: Cell<Option<EntityId>>,
    failure: Cell<Option<super::failure::Failure>>,
    retrying: Cell<bool>,
    launch_settings: RefCell<Option<guild_session::Settings>>,
}

impl Facts {
    pub fn failure(&self) -> Option<super::failure::Failure> {
        self.failure.get()
    }
    pub fn set_failure(&self, failure: Option<super::failure::Failure>) -> bool {
        self.failure.replace(failure) != failure
    }
    pub fn retrying(&self) -> bool {
        self.retrying.get()
    }
    pub fn set_retrying(&self, retrying: bool) {
        self.retrying.set(retrying);
    }
    pub fn set_launch_settings(&self, settings: guild_session::Settings) {
        self.launch_settings.replace(Some(settings));
    }
    pub fn launch_settings(&self) -> Option<guild_session::Settings> {
        self.launch_settings.borrow().clone()
    }

    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// The title, if the program set one — as it set it; `title::of`
    /// says what of it a tab shows.
    pub fn title(&self) -> Option<String> {
        self.title.borrow().clone()
    }

    /// Which agent CLI the terminal runs in front, if any.
    pub fn harness_in_front(&self) -> Option<HarnessKind> {
        self.harness.get()
    }

    /// The conversation the program in front resumes by id, and whose.
    pub fn resumed(&self) -> Option<(HarnessKind, String)> {
        self.resumed.borrow().clone()
    }

    /// Whether Claude Code is what the terminal runs in front.
    pub fn claude_in_front(&self) -> bool {
        self.harness.get() == Some(HarnessKind::Claude)
    }

    /// Whether the process has ended while the tab is open.
    pub fn exited(&self) -> bool {
        self.exited.get()
    }

    /// Tell this entity from now on when a fact changes.
    pub fn listen(&self, listener: EntityId) {
        self.listener.set(Some(listener));
    }

    /// Who is told, if anybody.
    pub fn listener(&self) -> Option<EntityId> {
        self.listener.get()
    }

    /// Record a title, and answer whether the name a tab would show
    /// changed. **A marker is not news**: `claude` animates the one in
    /// front of its title while it works, and `title::of` drops it — a
    /// notify per frame of that animation would be the redraw this module
    /// exists to stop.
    pub fn set_title(&self, title: String) -> bool {
        let shown = crate::title::of(&title);
        let before = self
            .title
            .replace(Some(title))
            .and_then(|before| crate::title::of(&before));
        before != shown
    }

    /// Record the agent in front, and answer whether it changed.
    pub fn set_harness(&self, harness: Option<HarnessKind>) -> bool {
        self.harness.replace(harness) != harness
    }

    /// Record what the program in front resumes, and answer whether it
    /// changed.
    pub fn set_resumed(&self, resumed: Option<(HarnessKind, String)>) -> bool {
        let changed = *self.resumed.borrow() != resumed;
        self.resumed.replace(resumed);
        changed
    }

    /// Record the exit, and answer whether it is news.
    pub fn set_exited(&self) -> bool {
        !self.exited.replace(true)
    }
}
