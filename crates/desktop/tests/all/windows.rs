//! Windows beside Guild's own — M8.3.6, decision 1.

use guild_desktop::windows::{Registry, Satellite, quits};

/// **A satellite kind is opened once.** `cmd-shift-k` with Push Commits up
/// brings that window forward; a second file click reuses the diff window.
/// A handle whose window has gone is forgotten when it is found gone.
#[test]
fn a_satellite_kind_is_opened_once() {
    let mut open: Registry<u32> = Registry::default();
    assert_eq!(open.handle(Satellite::Push), None);

    open.opened(Satellite::Push, 7);
    open.opened(Satellite::Diff, 9);
    assert_eq!(open.handle(Satellite::Push), Some(7));
    assert_eq!(open.handle(Satellite::Diff), Some(9));

    open.gone(Satellite::Push);
    assert_eq!(open.handle(Satellite::Push), None);
    assert_eq!(
        open.handle(Satellite::Diff),
        Some(9),
        "the other is untouched"
    );

    assert_eq!(open.all(), vec![9]);
}

/// **The application quits when the main window closes, and not before.**
/// It used to quit when no window was left, which a Push window still open
/// would have turned into an application with no main window alive.
#[test]
fn the_application_quits_when_the_main_window_closes_and_not_before() {
    assert!(!quits(1, &[1, 2]), "a satellite closed");
    assert!(quits(1, &[2]), "the main window closed, a satellite is up");
    assert!(quits(1, &[]), "the last window closed");
}

/// **A window beside the main one opens over it** — 2026-09-24. With two
/// monitors Push Commits opened on the primary one while the main window
/// stood on the other: it was centred on the primary display. It is
/// centred on the main window now, on the main window's own display.
#[test]
fn a_satellite_opens_centred_over_the_main_window() {
    use gpui::{Bounds, point, px, size};
    let main = Bounds {
        origin: point(px(100.0), px(50.0)),
        size: size(px(1200.0), px(800.0)),
    };
    let opened = guild_desktop::windows::over(main, size(px(600.0), px(400.0)));
    assert_eq!(opened.origin, point(px(400.0), px(250.0)));
    assert_eq!(opened.size, size(px(600.0), px(400.0)));
}
