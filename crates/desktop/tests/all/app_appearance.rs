//! Guild's Light/Dark mode handed to AppKit: what `NSApp.appearance` is
//! set to, and a return to System that reads the system only after the
//! override is gone.
//!
//! AppKit itself is not here — the test platform has none. What these
//! prove is the mapping and the order of the two calls, against a stand-in
//! that answers the way `NSApp.effectiveAppearance` does: the forced
//! appearance while one is set, the system's otherwise.

use std::cell::Cell;

use gpui::WindowAppearance;
use guild_desktop::theme::{self, Mode};

#[test]
fn each_mode_names_the_appearance_appkit_is_given() {
    assert_eq!(theme::forced_appearance(Mode::System), None);
    assert_eq!(
        theme::forced_appearance(Mode::Light),
        Some(WindowAppearance::Light)
    );
    assert_eq!(
        theme::forced_appearance(Mode::Dark),
        Some(WindowAppearance::Dark)
    );
}

/// `NSApp`, as far as the appearance goes.
struct App {
    system: WindowAppearance,
    forced: Cell<Option<WindowAppearance>>,
}

impl App {
    fn new(system: WindowAppearance) -> Self {
        Self {
            system,
            forced: Cell::new(None),
        }
    }

    fn effective(&self) -> WindowAppearance {
        self.forced.get().unwrap_or(self.system)
    }

    fn follow(&self, mode: Mode) -> Option<bool> {
        theme::follow_mode(mode, |forced| self.forced.set(forced), || self.effective())
    }
}

/// Light forced under a dark macOS, then System: the answer is the
/// system's dark, not the Light Guild forced a moment before — which is
/// what reading before clearing would have answered.
#[test]
fn back_to_system_reads_the_system_after_the_override_is_cleared() {
    let app = App::new(WindowAppearance::Dark);
    assert_eq!(
        app.follow(Mode::Light),
        None,
        "an explicit mode asks nothing"
    );
    assert_eq!(app.effective(), WindowAppearance::Light);
    assert_eq!(app.follow(Mode::System), Some(true));
    assert_eq!(app.forced.get(), None, "System leaves AppKit to the system");

    let app = App::new(WindowAppearance::Light);
    assert_eq!(app.follow(Mode::Dark), None);
    assert_eq!(app.effective(), WindowAppearance::Dark);
    assert_eq!(app.follow(Mode::System), Some(false));
}

/// Light to Dark and back forces each in turn; neither asks the system.
#[test]
fn an_explicit_mode_replaces_the_one_before() {
    let app = App::new(WindowAppearance::Dark);
    for (mode, effective) in [
        (Mode::Dark, WindowAppearance::Dark),
        (Mode::Light, WindowAppearance::Light),
        (Mode::Dark, WindowAppearance::Dark),
    ] {
        assert_eq!(app.follow(mode), None);
        assert_eq!(app.effective(), effective);
    }
}
