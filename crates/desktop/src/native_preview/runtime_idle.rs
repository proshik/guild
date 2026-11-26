//! Marked, provider-free Root document fixtures for finite idle trials.
use crate::{Root, file_tab::FileTab};
use gpui::{App, AppContext, Entity, Window};
use std::path::Path;
pub fn documents(fixture: &Path, count: usize, window: &mut Window, cx: &mut App) -> Entity<Root> {
    assert!(fixture.is_absolute() && fixture.join(".owned-runtime-idle").is_file());
    let mut desk = guild_session::Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), fixture);
    let root = cx.new(|cx| {
        Root::from_state_with_policy(
            guild_session::Settings {
                autosave: false,
                ..Default::default()
            },
            desk,
            crate::root::isolation::Policy::Diagnostic {
                state_dir: crate::paths::state_dir(),
            },
            cx,
        )
    });
    root.update(cx, |r, cx| {
        r.active_project = Some(project);
        r.scoped_settings.identities.insert(
            project,
            crate::repository_settings::resolve_repository(fixture),
        );
        for n in 0..count {
            let path = std::path::PathBuf::from(format!("source-{n}.rs"));
            let file =
                cx.new(|cx| FileTab::open(fixture, &path, r.file_look(), r.ui(), window, cx));
            let editor = file.read(cx).editor().unwrap().clone();
            r.open_document(project, &path, &editor, cx);
            r.adopt(
                guild_session::Seat {
                    workspace: r.active_workspace,
                    project,
                },
                project,
                crate::Body::File {
                    path,
                    tab: file,
                    presentation: None,
                    origin: crate::Origin::Project,
                },
                cx,
            );
        }
        assert_eq!(r.documents.len(), count);
        assert!(r.servers.is_empty());
    });
    root
}
pub fn flush(root: &Entity<Root>, cx: &mut App) {
    root.update(cx, |r, cx| r.flush_documents(cx));
}
pub fn observations() -> [usize; 2] {
    crate::root::lsp::document_trace::take()
}
