//! A link clicked in a terminal, opened by the window — M5.12.
//!
//! A tab's terminal records a `cmd`-click on a file and cannot open it:
//! opening a file is the window's. The window takes it on the watch loop's
//! clock, the shape M5.10's interrupt witness has.

use gpui::{Context, Window};

use crate::Root;
use std::path::PathBuf;

use crate::root::history::Caret;
use crate::terminal::links::{self, Clicked, Resolved, Shown};

impl Root {
    /// Open the file a `cmd`-click left in the terminal in front or in the
    /// active run — M5.12, task 12.
    ///
    /// **Resolved again here**, against where the terminal started and the
    /// project's root: the disk may have changed since the pointer found
    /// the link, and a file gone since is a notice rather than a tab of
    /// nothing — and acted on by [`Root::open_link`].
    ///
    /// The same pass tells both terminals the project's root, which is how
    /// they know what to underline before anything is clicked.
    pub(crate) fn poll_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.project_root();
        let front = match self.active_body() {
            Some(crate::Body::Terminal { view, .. }) => Some(view.clone()),
            _ => None,
        };
        let run = self.active_run().and_then(|run| run.view.clone());

        // A click left in a terminal that stopped being in front before the
        // tick is dropped, not kept for the day that terminal comes back —
        // a file opening an hour after its click is nobody's gesture.
        let behind: Vec<_> = self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::Terminal { view, .. } => Some(view.clone()),
                _ => None,
            })
            .chain(
                self.runs
                    .values()
                    .flat_map(|tabs| tabs.entries())
                    .filter_map(|entry| entry.value.view.clone()),
            )
            .filter(|view| front.as_ref() != Some(view) && run.as_ref() != Some(view))
            .collect();
        for view in behind {
            view.update(cx, |terminal, _cx| {
                terminal.take_link();
            });
        }

        let mut clicked = None;
        for view in front.into_iter().chain(run) {
            let taken = view.update(cx, |terminal, _cx| {
                terminal.set_link_root(root.clone());
                terminal
                    .take_link()
                    .map(|link| (link, terminal.cwd().to_path_buf()))
            });
            clicked = clicked.or(taken);
        }
        let (Some((link, cwd)), Some(root)) = (clicked, root) else {
            return;
        };
        self.open_link(link, cwd, root, window, cx);
    }

    /// Open what a `cmd`-click on a file leads to.
    ///
    /// **Resolved and read off the window's thread**, then acted on where
    /// the window is: the path is found again — the disk may have changed
    /// since the pointer found it, and a file gone since is a notice
    /// rather than a tab of nothing — and its first bytes decide who shows
    /// it, by [`links::shown_by`]. A click whose project is no longer in
    /// front when the answer comes is nobody's gesture any more.
    ///
    /// What Guild shows opens in a tab: the project's own files as they
    /// always have, and a file elsewhere on the disk as an
    /// [`crate::Origin::Outside`] tab — read-only, and nobody's root.
    /// What it cannot show goes to the application macOS opens it with,
    /// wherever it is; a program is refused, never run.
    pub(crate) fn open_link(
        &mut self,
        link: Clicked,
        cwd: PathBuf,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let asked = link.path.clone();
        let work = cx.background_executor().spawn(async move {
            let resolved = links::resolve(&asked, &cwd, &root)?;
            let absolute = match &resolved {
                Resolved::Inside(relative) => root.canonicalize().unwrap_or(root).join(relative),
                Resolved::Outside(absolute) => absolute.clone(),
            };
            let shown = links::shown_by(&absolute);
            Some((resolved, absolute, shown))
        });
        cx.spawn_in(window, async move |this, cx| {
            let found = work.await;
            let _ = this.update_in(cx, |root, window, cx| {
                if root.active_project == Some(project) {
                    root.open_resolved(link, found, window, cx);
                }
            });
        })
        .detach();
    }

    fn open_resolved(
        &mut self,
        link: Clicked,
        found: Option<(Resolved, PathBuf, Shown)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match found {
            None | Some((_, _, Shown::Gone)) => {
                self.inform(
                    self.ui()
                        .notice_link_missing(&link.path.display().to_string()),
                );
            }
            Some((_, absolute, Shown::Refused)) => {
                self.warn(self.ui().notice_link_program(&crate::short_path(&absolute)));
            }
            Some((_, absolute, Shown::BySystem)) => {
                let shown = crate::short_path(&absolute);
                match crate::system::default_app::open_file(&absolute) {
                    Err(reason) => {
                        self.fail(self.ui().notice_link_not_handed_over(&shown, &reason));
                    }
                    // The application can still fail to open it; AppKit
                    // says so later, off this thread, and it is the same
                    // notice.
                    Ok(launched) => cx
                        .spawn(async move |this, cx| {
                            if let Ok(Err(reason)) = launched.await {
                                let _ = this.update(cx, |root, cx| {
                                    root.fail(
                                        root.ui().notice_link_not_handed_over(&shown, &reason),
                                    );
                                    cx.notify();
                                });
                            }
                        })
                        .detach(),
                }
            }
            Some((resolved, _, Shown::InGuild)) => {
                // A compiler's column is one-based and the editor counts
                // the characters before the place, so `:5` is 4 and no
                // column is the line's start.
                let column = link.column.map_or(0, |column| column.saturating_sub(1));
                match resolved {
                    Resolved::Inside(path) => {
                        if let Some(line) = link.line {
                            self.open_file_at(path, line as usize, column, window, cx);
                        } else {
                            self.open_file(&path, window, cx);
                        }
                    }
                    Resolved::Outside(absolute) => {
                        // Relative to `/`, as a tab's path is relative to
                        // its origin's root.
                        let Ok(relative) = absolute.strip_prefix(crate::Origin::OUTSIDE_ROOT)
                        else {
                            return;
                        };
                        let caret = link.line.map(|line| Caret::Characters {
                            line: line.saturating_sub(1),
                            column,
                        });
                        self.go_to_library(crate::Origin::Outside, relative, caret, window, cx);
                    }
                }
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
