//! The menu bar, and the items that need the window to answer them.

use gpui::{Context, Window};

use crate::Root;
use crate::logging;
use crate::menu;
use crate::short_path;
use std::path::Path;

impl Root {
    /// Open a directory from the recent list.
    ///
    /// The position is resolved against the list as it is now: the menu
    /// was built earlier, and the list may have moved under it.
    pub(crate) fn open_recent(&mut self, index: usize, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.open_recent_without_history(index, cx);
        self.history.suspended -= 1;
    }

    fn open_recent_without_history(&mut self, index: usize, cx: &mut Context<Root>) {
        let Some(path) = self.desk.recent().nth(index).map(Path::to_path_buf) else {
            return;
        };

        // An already open project is selected rather than opened twice —
        // `add_project` returns the existing one for the same directory.
        let project = self.desk.add_project(&path);
        self.adopt_project(project);
        // M12.7 final fix — see `select_project`.
        self.close_menu_for_switch();
        self.active_project = Some(project);
        self.unfold_group_of(project);

        // A project that is already open keeps its tabs; a fresh one has
        // none, and the empty body says so. Nothing is started: a session
        // begins when the person asks for one.
        self.come_back();

        self.save();
        self.rebuild_menu(cx);
        cx.notify();
    }

    /// What this is and which version of it.
    ///
    /// The log's path is in it: someone asked to send their log should not
    /// have to hunt for where it lives.
    pub(crate) fn about(&self, window: &mut Window, cx: &mut Context<Root>) {
        let ui = self.ui();
        // The version and, for a bundle, which commit it is — every build
        // is 0.1.0, and the commit is what tells two of them apart. M5.13.
        let version = match self.update.build() {
            Some(build) => format!("{} · {}", env!("CARGO_PKG_VERSION"), build.label()),
            None => env!("CARGO_PKG_VERSION").to_owned(),
        };
        let detail = ui.about(&version, &short_path(&logging::file()));

        // The answer is of no interest — there is one button — but the
        // receiver has to outlive the prompt, or it closes at once.
        let answer = window.prompt(gpui::PromptLevel::Info, "Guild", Some(&detail), &["OK"], cx);
        cx.spawn(async move |_root, _cx| {
            let _ = answer.await;
        })
        .detach();
    }

    /// Reveal the log in Finder.
    ///
    /// Revealed in Finder rather than opened in an editor: a log is read
    /// as a whole and usually sent on rather than edited.
    pub(crate) fn show_log(&self, cx: &mut Context<Root>) {
        cx.reveal_path(&logging::file());
    }

    /// Rebuild the menu bar.
    ///
    /// The system draws it and does not re-read anything on its own, so
    /// every change it shows — the language and the recent list — has to
    /// come through here.
    pub(crate) fn rebuild_menu(&self, cx: &mut Context<Root>) {
        menu::install(self.ui(), &self.desk.recent().collect::<Vec<_>>(), cx);
    }
}
