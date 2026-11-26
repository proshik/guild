//! Owned schema fixtures through the production Root, job registry, adapter and card.
use crate::{Body, Root};
use gpui::{App, AppContext, Entity, Window};
use guild_session::{
    Desk, Runs, Seat, SessionRecord,
    terminal_job::{LaunchSpec, RevmuxArchive, TerminalCommand},
};
use std::{path::Path, time::Duration};

pub fn root(fixture: &Path, window: &mut Window, cx: &mut App) -> anyhow::Result<Entity<Root>> {
    anyhow::ensure!(
        fixture.is_absolute() && fixture.join(".owned-review-fixture").is_file(),
        "Use a marked task-owned fixture"
    );
    let mut desk = Desk::default();
    let project = desk.open_in(desk.active_workspace(), fixture);
    desk.look_at(Some(project), None);
    let root = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
    root.update(cx,|root,cx|{
        let seat=Seat{workspace:root.desk.active_workspace(),project};
        let owner=root.desk.add_session(SessionRecord::new("Review schema fixtures".into(),seat,fixture.into(),Runs::Shell));
        let view=super::resource_probe::terminal("quiet",cx);let facts=view.read(cx).facts().clone();
        let tab=root.adopt(seat,project,Body::Terminal{session:owner,view,facts},cx);root.active_tab=Some(tab);
        if !fixture.join(".started").exists() {
            std::fs::write(fixture.join(".started"),"Owned production UI fixture\n").unwrap();
            for (name,title) in [("findings","Terminal ownership and keyboard focus"),("clean","Scrolled diagnostics and Cyrillic source"),("failed","Dependency resolution — launch error"),("generic","Local terminal · no saved output")] {
                let spec=LaunchSpec{version:1,request_id:format!("schema-{name}"),program:if name=="failed"{"/nonexistent/guild-review-fixture".into()}else{"/usr/bin/python3".into()},args:vec![fixture.join("writer.py").to_string_lossy().into(),fixture.join(name).to_string_lossy().into(),name.into()],cwd:fixture.into(),target:guild_session::overlay::Target::Session(owner),env:Default::default(),title:title.into(),geometry:None,foreground:false,revmux_archive:if name=="generic"{None}else{Some(Box::new(RevmuxArchive{directory:fixture.join(name),program:"/usr/local/bin/revust".into()}))}};
                let answer=root.command_terminal(&TerminalCommand::Start{spec},Some(&owner.to_string()),cx);
                println!("fixture_start {}",serde_json::to_string(&answer).unwrap());
            }
        }
        root.toggle_terminal_list(window,cx);
        if let Some(list)=root.terminal_list.as_mut(){list.scope=guild_session::terminal_job::Scope::All;}
        cx.spawn(async move |root,cx|{
            loop {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if root.update(cx,|root,cx|{
                    root.terminal_jobs.poll(cx);
                    if root.review_activity.poll(||root.terminal_jobs.receipt_snapshot()) {
                        for (id,s) in &root.review_activity.snapshots {
                            println!("activity {}",serde_json::json!({"job":id,"phase":s.phase,"counts":s.counts(),"final":s.final_result,"clean":s.clean,"stale":s.stale,"legacy":s.legacy}));
                        }
                        cx.notify();
                    }
                }).is_err(){break;}
            }
        }).detach();
        cx.notify();
    });
    Ok(root)
}
