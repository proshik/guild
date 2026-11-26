//! Task-owned native frame trial, using production Root and navigator rendering.
use crate::{Body, Root};
use gpui::{App, AppContext, Entity, Window};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord, status::Signal};
use std::path::Path;

pub fn root(fixture: &Path, _window: &mut Window, cx: &mut App) -> anyhow::Result<Entity<Root>> {
    anyhow::ensure!(
        fixture.is_absolute()
            && fixture.starts_with(crate::paths::state_dir())
            && fixture.join(".owned-frame-fixture").is_file(),
        "Use the marked task-owned frame fixture"
    );
    let mut desk = Desk::default();
    let project = desk.open_in(desk.active_workspace(), fixture);
    desk.look_at(Some(project), None);
    let root = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
    root.update(cx, |root, cx| {
        root.settings.column_folded = false;
        root.settings.column_width = 232.;
        root.settings.ui_font_size = 13.;
        root.settings.language = guild_session::Language::English;
        root.settings.panels = Some(Default::default());
        root.branch = Some("frame-navigation-sessions".into());
        root.branch_read = Some(std::time::Instant::now());
        let seat = Seat {
            workspace: root.desk.active_workspace(),
            project,
        };
        let mut first = None;
        for (title, runs, signal) in [
            (
                "Frame navigation and sessions",
                Runs::Codex(SessionMode::Terminal),
                Some(Signal::PromptSubmitted),
            ),
            (
                "Review the workspace layout",
                Runs::Claude(SessionMode::Terminal),
                Some(Signal::PermissionAsked),
            ),
            (
                "Verify tool placement",
                Runs::OpenCode(SessionMode::Terminal),
                Some(Signal::TurnCompleted),
            ),
            (
                "Session without observed activity",
                Runs::Codex(SessionMode::Terminal),
                None,
            ),
        ] {
            let session =
                root.desk
                    .add_session(SessionRecord::new(title.into(), seat, fixture.into(), runs));
            let tab = root.adopt(
                seat,
                project,
                Body::Broken {
                    session,
                    reason: "Owned frame preview; no harness process launched.".into(),
                },
                cx,
            );
            first.get_or_insert(tab);
            if let Some(signal) = signal {
                root.board.apply(session, signal);
            }
        }
        root.active_tab = first;
        root.focusing_front = true;
        cx.notify();
    });
    Ok(root)
}
