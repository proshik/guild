//! A link under the pointer while `cmd` is held — M5.12, task 12.
//!
//! The link's cells are underlined through the paint's mark, and a
//! `cmd`-click opens a URL or HTML page at once, or leaves a source file
//! for the window's clock:
//! section 6.31 of the design system. The rules are `terminal::links`'s.
//!
//! **Only what leads somewhere is underlined** — a URL, or a file that
//! exists where the terminal started or under the project's root — so the
//! disk is asked when the pointer with `cmd` held finds another link in the
//! grid, and not on every move over the same one.

use std::path::PathBuf;

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::Point;
use gpui::{
    Context, InteractiveElement as _, ModifiersChangedEvent, Pixels, Point as GpuiPoint,
    Styled as _,
};

use super::TerminalTab;
use crate::terminal::links::{self, Clicked, Found, Target};

/// The link under the pointer, where the pointer was, and a clicked file
/// waiting for the window.
///
/// `Default`, for the reason `find_bar::Finding` is.
#[derive(Default)]
pub(super) struct Hover {
    /// The link under the pointer while `cmd` is held, by the cells it
    /// covers, when it leads somewhere.
    link: Option<Found>,
    /// Where the pointer was last seen over the terminal — what a check of
    /// the underline after new output is aimed at.
    at: Option<GpuiPoint<Pixels>>,
    /// The cell the link was last looked for at.
    asked: Option<Point>,
    /// What the grid held there, before the disk was asked whether it
    /// leads somewhere: the same link found again is not asked again.
    found: Option<Found>,
    /// A `cmd`-click on a file, waiting for the window to open it.
    clicked: Option<Clicked>,
    /// The project's root, which the window tells the terminal in front on
    /// its clock: a relative path is looked for there when it is not where
    /// the terminal started.
    root: Option<PathBuf>,
}

impl TerminalTab {
    /// What a modifier change does to the link under the pointer, and the
    /// pointing hand while there is one.
    ///
    /// **A modifier change is heard, not taken**: no `stop_propagation`,
    /// because the window listens to them too. Only a focused terminal hears
    /// one at all (task 0) — an unfocused Output terminal learns of `cmd` on
    /// the next move, which carries the modifiers with it. **It is aimed at
    /// where the pointer is now**, asked of the window, and not at where it
    /// was last seen over the terminal: a stroke like `cmd-1` with the
    /// pointer resting on the column must not underline a link it left.
    ///
    /// **The underline is the promise and the cursor is not**: the platform
    /// sets a cursor only while the window is active and only when the hit
    /// test changes, so a hand that never appears is not a link that is
    /// not there.
    ///
    /// **While a link is underlined, every frame checks it is still there**,
    /// after the frame: output moves text under a still pointer and a
    /// program redraws in place, and nothing else would take an underline
    /// off cells that now hold other text. The grid is read; the disk is
    /// asked only when the link found is another one.
    pub(super) fn bind_links(&self, el: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        if self.hover.link.is_some() {
            let tab = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = tab.update(cx, |tab, cx| tab.recheck_link(cx));
            });
        }
        let el = el.on_modifiers_changed(cx.listener(
            |tab, event: &ModifiersChangedEvent, window, cx| {
                tab.hover_at(Some(window.mouse_position()), event.modifiers.platform, cx);
            },
        ));
        if self.hover.link.is_some() {
            el.cursor_pointer()
        } else {
            el
        }
    }

    /// Look for the link under a window point with `cmd` held or not, and
    /// repaint when what is underlined changed.
    pub(super) fn hover_at(
        &mut self,
        position: Option<GpuiPoint<Pixels>>,
        cmd: bool,
        cx: &mut Context<Self>,
    ) {
        let point = position
            .filter(|_| cmd)
            .and_then(|position| self.link_point(position));
        let Some(point) = point else {
            self.forget_link(cx);
            return;
        };
        self.hover.at = position;
        if self.hover.asked == Some(point) {
            return;
        }
        let found = self.link_in_grid(point);
        self.show_link(point, found, cx);
    }

    /// The underline checked against the grid as it is now, from where the
    /// pointer was last seen — after output, or a redraw in place.
    fn recheck_link(&mut self, cx: &mut Context<Self>) {
        if self.hover.link.is_none() {
            return;
        }
        match self.hover.at.and_then(|at| self.link_point(at)) {
            Some(point) => {
                let found = self.link_in_grid(point);
                self.show_link(point, found, cx);
            }
            None => self.forget_link(cx),
        }
    }

    /// Underline what the grid holds at a point, asking the disk only when
    /// it is not the link found there last time.
    fn show_link(&mut self, point: Point, found: Option<Found>, cx: &mut Context<Self>) {
        self.hover.asked = Some(point);
        if found == self.hover.found {
            return;
        }
        self.hover.found = found.clone();
        let link = found.filter(|found| self.leads_somewhere(&found.target));
        if link != self.hover.link {
            self.hover.link = link;
            cx.notify();
        }
    }

    /// Nothing under the hand any more: `cmd` let go, or the pointer off
    /// the grid.
    fn forget_link(&mut self, cx: &mut Context<Self>) {
        self.hover.at = None;
        self.hover.asked = None;
        self.hover.found = None;
        if self.hover.link.take().is_some() {
            cx.notify();
        }
    }

    /// A press with `cmd` on a link: a URL or unlocated HTML page goes to
    /// the browser at once; a source file is kept for the window. Answers
    /// whether the press was the link's, in which case it begins no selection.
    ///
    /// **The link is looked for in the grid again, at the press**, and not
    /// taken from the underline — [`links::pressed`] says which counts. The
    /// press is where the hand is, too, whatever the last move said: an
    /// unfocused terminal may not have heard `cmd` go down.
    pub(super) fn press_link(
        &mut self,
        position: GpuiPoint<Pixels>,
        cmd: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !cmd {
            return false;
        }
        let Some(point) = self.link_point(position) else {
            return false;
        };
        let now = self.link_in_grid(point);
        let target = links::pressed(now.clone(), self.hover.link.as_ref(), |target| {
            self.leads_somewhere(target)
        });
        self.hover.at = Some(position);
        self.show_link(point, now, cx);

        let Some(target) = target else {
            return false;
        };
        match target {
            Target::Url(url) => cx.open_url(&url),
            Target::File { path, line, column } => {
                let clicked = Clicked { path, line, column };
                let root = self.hover.root.as_deref().unwrap_or(&self.cwd);
                if let Some(url) = clicked.browser_url(&self.cwd, root) {
                    if let Err(error) = crate::system::browser::open_local_page(&url) {
                        tracing::error!(
                            "Could not open HTML page {url} in the default web browser: {error}"
                        );
                    }
                } else {
                    self.hover.clicked = Some(clicked);
                }
            }
        }
        true
    }

    /// The file a `cmd`-click left, taken once — the M5.10 witness shape:
    /// the tab records, the window's clock takes.
    pub fn take_link(&mut self) -> Option<Clicked> {
        self.hover.clicked.take()
    }

    /// The project's root a relative path in this terminal's output is
    /// looked for under, after where it started. The window tells it.
    pub fn set_link_root(&mut self, root: Option<PathBuf>) {
        if self.hover.root != root {
            self.hover.root = root;
            // What leads somewhere may have changed with it.
            self.hover.asked = None;
            self.hover.found = None;
        }
    }

    /// Whether a link is one to underline: a URL always, a file only when
    /// one is there.
    fn leads_somewhere(&self, target: &Target) -> bool {
        match target {
            Target::Url(_) => true,
            Target::File { path, .. } => {
                let root = self.hover.root.as_deref().unwrap_or(&self.cwd);
                links::resolve(path, &self.cwd, root).is_some()
            }
        }
    }

    /// The grid point under a window point, or nothing past the grid's
    /// edge — where a link on the row's last character must not follow the
    /// pointer out into the padding, or onto the panel beside it.
    pub(super) fn link_point(&self, position: GpuiPoint<Pixels>) -> Option<Point> {
        let (line, column, _) = self.cell_at(position)?;
        let term = self.emulator.term().lock().ok()?;
        let grid = term.grid();
        if line >= grid.screen_lines() || column >= grid.columns() {
            return None;
        }
        drop(term);
        Some(self.emulator.point_at(line, column))
    }

    /// The link the grid holds at a point, whether or not it leads anywhere
    /// — except a path an agent CLI wrapped onto the next row itself, which
    /// is one link only when the joined path names a file.
    fn link_in_grid(&self, point: Point) -> Option<Found> {
        let term = self.emulator.term().lock().ok()?;
        let mut found = links::at(&term, point, |found| self.resolved(found).is_some())?;
        if let Target::File { path, .. } = &found.target
            && path.is_relative()
        {
            let resolved = self.resolved(&found)?;
            if let Target::File { path, .. } = &mut found.target {
                *path = resolved;
            }
        }
        Some(found)
    }

    /// Where a file link found in the grid leads: a relative path against
    /// the directory the output was printed in, then the project's root.
    fn resolved(&self, found: &Found) -> Option<PathBuf> {
        let Target::File { path, .. } = &found.target else {
            return None;
        };
        let cwd = if path.is_relative() {
            self.emulator.output_cwd(found.start(), found.end())?
        } else {
            self.cwd.clone()
        };
        let root = self.hover.root.as_deref().unwrap_or(&cwd);
        Some(match links::resolve(path, &cwd, root)? {
            links::Resolved::Inside(p) => root.join(p),
            links::Resolved::Outside(p) => p,
        })
    }
}

/// Whether a cell is part of the link under the pointer.
pub(super) fn underlined(hover: &Hover, point: Point) -> bool {
    hover.link.as_ref().is_some_and(|link| link.covers(point))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{
        emulator::{Emulator, GridSize},
        view::Look,
    };
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    use gpui::{AppContext as _, Bounds, point, px, size};

    // The press handler and pending click are private. Exercise their real
    // routing here, including the default-browser boundary, with one owned PTY.
    #[gpui::test]
    fn html_path_presses_open_the_browser_and_preserve_source_navigation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.executor().allow_parking();
        let browser = crate::system::browser::tests::Capture::new();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let cwd = root.join("sub");
        std::fs::create_dir_all(&cwd).unwrap();
        for path in [
            root.join("index.html"),
            cwd.join("local.HTM"),
            dir.path().join("outside.html"),
            root.join("отчёт #1?.html"),
            root.join("main.rs"),
            root.join("index.html.bak"),
            root.join("страница.html"),
        ] {
            std::fs::write(path, "fixture").unwrap();
        }
        std::fs::create_dir(root.join("folder.html")).unwrap();
        let mut command = portable_pty::CommandBuilder::new("/bin/sleep");
        command.arg("30");
        let emulator = Emulator::spawn_job(command, &cwd, GridSize::new(250, 24)).unwrap();
        let tab = cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                cwd.clone(),
                None,
                cx,
            )
        });
        let absolute = root.join("index.html");
        let outside = dir.path().join("outside.html");
        let unicode = root.join("отчёт #1?.html");
        let unicode_url = gpui::http_client::Url::from_file_path(&unicode)
            .unwrap()
            .to_string();
        for (text, path) in [
            (format!("({}).", absolute.display()), absolute.clone()),
            ("`index.html`,".into(), absolute.clone()),
            ("[page](local.HTM)".into(), cwd.join("local.HTM")),
            (outside.display().to_string(), outside.clone()),
            ("страница.html!?".into(), root.join("страница.html")),
            (
                format!("\x1b]8;;{unicode_url}\x1b\\page\x1b]8;;\x1b\\"),
                unicode.clone(),
            ),
        ] {
            tab.update(cx, |tab, cx| {
                tab.set_link_root(Some(root.clone()));
                let mut parser: Processor<StdSyncHandler> = Processor::new();
                parser.advance(
                    &mut *tab.emulator.term().lock().unwrap(),
                    format!("\x1b[2J\x1b[H{text}").as_bytes(),
                );
                let bounds = Bounds::new(point(px(0.), px(0.)), size(px(2026.), px(394.)));
                tab.bounds = Some(bounds);
                tab.cell = Some(size(px(8.), px(16.)));
                // All labels start their path within the first ten cells.
                let column = if text.starts_with("[page]") { 8. } else { 2. };
                let position = crate::terminal::geometry::content(bounds).origin
                    + point(px(column * 8. + 1.), px(1.));
                assert!(
                    !tab.press_link(position, false, cx),
                    "ordinary click: {text}"
                );
                assert!(tab.press_link(position, true, cx), "Cmd-click: {text}");
            });
            assert_eq!(
                cx.opened_url(),
                None,
                "HTML must bypass the generic file-association opener: {text}"
            );
            assert_eq!(
                browser.urls().last(),
                Some(
                    &gpui::http_client::Url::from_file_path(path.canonicalize().unwrap())
                        .unwrap()
                        .to_string()
                ),
                "default-browser dispatch: {text}"
            );
            assert_eq!(
                tab.update(cx, |tab, _| tab.take_link()),
                None,
                "HTML must not queue an editor open"
            );
        }
        let last_url = cx.opened_url();
        let browser_urls = browser.urls();
        for (text, expected) in [
            (
                "index.html:12:3",
                Some(Clicked {
                    path: "index.html".into(),
                    line: Some(12),
                    column: Some(3),
                }),
            ),
            (
                "main.rs:7",
                Some(Clicked {
                    path: "main.rs".into(),
                    line: Some(7),
                    column: None,
                }),
            ),
            (
                "main.rs",
                Some(Clicked {
                    path: "main.rs".into(),
                    line: None,
                    column: None,
                }),
            ),
            (
                "index.html:9",
                Some(Clicked {
                    path: "index.html".into(),
                    line: Some(9),
                    column: None,
                }),
            ),
            (
                "index.html.bak",
                Some(Clicked {
                    path: "index.html.bak".into(),
                    line: None,
                    column: None,
                }),
            ),
            ("folder.html", None),
            ("missing.html", None),
            ("sub/", None),
        ] {
            tab.update(cx, |tab, cx| {
                let mut parser: Processor<StdSyncHandler> = Processor::new();
                parser.advance(
                    &mut *tab.emulator.term().lock().unwrap(),
                    format!("\x1b[2J\x1b[H{text}").as_bytes(),
                );
                let position = crate::terminal::geometry::content(tab.bounds.unwrap()).origin
                    + point(px(9.), px(1.));
                assert_eq!(
                    tab.press_link(position, true, cx),
                    expected.is_some(),
                    "{text}"
                );
                let expected = expected.map(|mut clicked| {
                    if clicked.path.is_relative() {
                        clicked.path = if cwd.join(&clicked.path).is_file() {
                            cwd.join(&clicked.path)
                        } else {
                            root.join(&clicked.path)
                        };
                    }
                    clicked
                });
                assert_eq!(tab.take_link(), expected, "{text}");
            });
            assert_eq!(
                cx.opened_url(),
                last_url,
                "source navigation must not open a browser"
            );
            assert_eq!(browser.urls(), browser_urls);
        }
        tab.update(cx, |tab, cx| {
            let mut parser: Processor<StdSyncHandler> = Processor::new();
            parser.advance(
                &mut *tab.emulator.term().lock().unwrap(),
                b"\x1b[2J\x1b[Hhttps://example.org/page.html",
            );
            let position = crate::terminal::geometry::content(tab.bounds.unwrap()).origin
                + point(px(9.), px(1.));
            assert!(tab.press_link(position, true, cx));
            assert_eq!(tab.take_link(), None);
            tab.cancel_job();
        });
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.org/page.html")
        );
    }
}
