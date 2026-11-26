//! Deterministic local harness failure fixture using the production pane/launcher.
use crate::{Body, Root};
use gpui::{App, AppContext, Entity, Window};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};
use std::path::Path;

pub fn root(fixture: &Path, _window: &mut Window, cx: &mut App) -> anyhow::Result<Entity<Root>> {
    anyhow::ensure!(
        fixture.is_absolute() && fixture.join(".owned-harness-fixture").is_file(),
        "Use a marked task-owned fixture root"
    );
    let program = fixture.join("harness-fixture");
    anyhow::ensure!(
        program.is_file(),
        "Compile the local fixture executable first"
    );
    let kind = match std::env::var("GUILD_FAILURE_KIND").as_deref() {
        Ok("claude") => guild_session::HarnessKind::Claude,
        Ok("opencode") => guild_session::HarnessKind::OpenCode,
        _ => guild_session::HarnessKind::Codex,
    };
    let mut desk = Desk::default();
    let project = desk.open_in(desk.active_workspace(), fixture);
    desk.look_at(Some(project), None);
    let root = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
    root.update(cx, |root, cx| {
        root.attach_test_hooks(&fixture.join("hooks"));
        root.settings.launch_prefixes.set(kind, program.to_string_lossy().into_owned());
        root.settings.language = if std::env::var("GUILD_FAILURE_RU").as_deref() == Ok("1") { guild_session::Language::Russian } else { guild_session::Language::English };
        let seat = Seat { workspace: root.desk.active_workspace(), project };
        let runs = match kind {
            guild_session::HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            guild_session::HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
            guild_session::HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
        };
        let session = root.desk.add_session(SessionRecord::new("Harness failure fixture".into(), seat, fixture.into(), runs));
        let body = Body::open(session, runs, fixture, Some(guild_session::Conversation::New(session.to_string())), &root.settings, root.theme,
            None, root.hook_launch(), Some("fixture initial assignment; NEVER REPLAY".into()), None, &root.start_size, cx);
        let tab = root.adopt(seat, project, body, cx);
        root.active_tab = Some(tab);
        root.focusing_front = true;
        println!("{}", serde_json::json!({"pid":std::process::id(),"session":session.to_string(),"harness":kind.program(),"fixture":fixture,"state":crate::paths::state_dir()}));
        cx.spawn(async move |root, cx| {
            for _ in 0..180 {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                let _ = root.update(cx, |root, _| {
                    let status = root.tabs.iter().flat_map(crate::Tab::bodies).find_map(|body| match body {
                        Body::Terminal { session: id, facts, view } if *id == session => Some(serde_json::json!({"runtime":format!("{:?}",view.entity_id()),"exited":facts.exited(),"failure":facts.failure().map(|f|format!("{:?}",f)),"retrying":facts.retrying()})),
                        _ => None,
                    });
                    println!("{}", serde_json::json!({"fixture_status":status}));
                });
            }
        }).detach();
        root.save();
        cx.notify();
    });
    Ok(root)
}
