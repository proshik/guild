//! Marked isolated production Root and read-only diff readiness for local trials.
use crate::{Root, diff_tab::DiffTab};
use gpui::{App, AppContext, Entity, Window};
use std::path::Path;

pub fn matches(tab: &Entity<DiffTab>, path: &Path, expected: &str, cx: &App) -> bool {
    tab.read(cx)
        .documents
        .get(path)
        .is_some_and(|d| d.sources[1].text.as_ref() == expected)
}

pub fn root(fixture: &Path, window: &mut Window, cx: &mut App) -> anyhow::Result<Entity<Root>> {
    anyhow::ensure!(
        fixture.is_absolute()
            && fixture.starts_with(crate::paths::state_dir())
            && fixture.join(".owned-runtime-diff").is_file(),
        "marked isolated fixture required"
    );
    let mut desk = guild_session::Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), fixture);
    let root = cx.new(|cx| {
        Root::from_state_with_policy(
            guild_session::Settings {
                autosave: false,
                panels: Some(Default::default()),
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
        r.open_file_from(crate::Origin::Project, Path::new("fixture.txt"), window, cx);
        r.open_diff(Some("fixture.txt".into()), guild_session::Base::Head, window, cx);
        let file = r.tabs.iter().flat_map(crate::Tab::bodies).find_map(|b| match b { crate::Body::File {tab, ..} => Some(tab.clone()), _ => None }).unwrap();
        let diff = r.tabs.iter().flat_map(crate::Tab::bodies).find_map(|b| match b { crate::Body::Diff {tab, ..} => Some(tab.clone()), _ => None }).unwrap();
        let weak_file = file.downgrade();
        cx.observe(&diff, move |_, diff, cx| {
            let d = diff.read(cx);
            let source = d.documents.get(Path::new("fixture.txt"));
            let fresh = weak_file.upgrade().and_then(|file| file.read(cx).editor().cloned()).is_some_and(|editor| source.is_some_and(|source| editor.read(cx).text() == source.sources[1].text.as_ref()));
            println!("{}", serde_json::json!({"diff_state":format!("{:?}",d.files()[0].state),"fresh":fresh,"counts":d.files()[0].counts,"mode":format!("{:?}",d.mode())}));
        }).detach();
        cx.subscribe(&file, |_, file, _: &crate::file_tab::BufferChanged, cx| {
            let f=file.read(cx);
            println!("{}",serde_json::json!({"buffer_changed":true,"dirty":f.is_dirty(),"bytes":f.editor().map(|e|e.read(cx).text().len())}));
        }).detach();
        cx.observe(&file, |_, file, cx| {
            let f=file.read(cx);
            println!("{}",serde_json::json!({"file_dirty":f.is_dirty(),"saved":f.last_save().is_some()}));
        }).detach();
        cx.notify();
    });
    Ok(root)
}
