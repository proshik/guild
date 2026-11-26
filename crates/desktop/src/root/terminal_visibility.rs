//! Explicit retained-terminal visibility, matching the body's and docks' hosts.
use crate::{Body, Root};
use gpui::{Context, Window};
use guild_session::{
    Tool,
    overlay::{Content, Target},
};
use std::collections::HashSet;
impl Root {
    pub(super) fn sync_terminal_visibility(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings.adaptive_terminal_refresh {
            return;
        }
        let visible = self.visible_terminal_ids();
        for view in self.terminals() {
            let shown = visible.contains(&view.entity_id());
            view.update(cx, |tab, cx| {
                tab.set_surface_visible(shown, window, cx);
                tab.set_refresh_enabled(true, window, cx);
            });
        }
    }
    fn visible_terminal_ids(&self) -> HashSet<gpui::EntityId> {
        let mut visible = HashSet::new();
        let fronts = if self
            .work_room()
            .is_none_or(|r| r.width <= 0.0 || r.height <= 0.0)
            || self.area_frames().is_empty()
        {
            self.area_front(self.focused_area()).into_iter().collect()
        } else {
            self.fronts_on_screen()
        };
        for tab in self.tabs.iter().filter(|tab| fronts.contains(&tab.id)) {
            for (pane, body) in tab.panes.bodies() {
                if let Body::Terminal { view, .. } = body {
                    visible.insert(view.entity_id());
                }
                for target in
                    std::iter::once(Target::Pane(pane)).chain(body.session().map(Target::Session))
                {
                    for entry in self.overlays.at(&target) {
                        if entry.visible
                            && let Content::Terminal { job } = &entry.content
                            && let Some(view) = self.terminal_jobs.terminal(job)
                        {
                            visible.insert(view.entity_id());
                        }
                    }
                }
            }
        }
        if self.tool_is_up(Tool::Output)
            && let Some(view) = self.active_run().and_then(|r| r.view.as_ref())
        {
            visible.insert(view.entity_id());
        }
        if self.tool_is_up(Tool::Terminal)
            && let Some(view) = self.active_panel_terminal()
        {
            visible.insert(view.entity_id());
        }
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::view::{Look, open_job};
    use gpui::{AppContext, TestAppContext};
    #[gpui::test]
    fn retained_tabs_hide_and_reveal_without_new_output(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let state = tempfile::tempdir().unwrap();
        let mut desk = guild_session::Desk::default();
        let project = desk.open_in(desk.active_workspace(), state.path());
        desk.look_at(Some(project), None);
        let root = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        root.update(cx, |root, cx| {
            let seat = guild_session::Seat {
                workspace: root.desk.active_workspace(),
                project,
            };
            let mut views = Vec::new();
            let mut tabs = Vec::new();
            for index in 0..2 {
                let mut command = portable_pty::CommandBuilder::new("/bin/sleep");
                command.arg("30");
                let view = open_job(
                    command,
                    std::path::Path::new("/"),
                    Look::of(&Default::default(), Default::default()),
                    cx,
                )
                .unwrap();
                let session = root.desk.add_session(guild_session::SessionRecord::new(
                    format!("fixture-{index}"),
                    seat,
                    state.path().into(),
                    guild_session::Runs::Shell,
                ));
                let facts = view.read(cx).facts().clone();
                tabs.push(root.adopt(
                    seat,
                    project,
                    Body::Terminal {
                        session,
                        view: view.clone(),
                        facts,
                    },
                    cx,
                ));
                views.push(view);
            }
            root.active_tab = Some(tabs[1]);
            assert_eq!(
                root.visible_terminal_ids(),
                HashSet::from([views[1].entity_id()])
            );
            assert_eq!(
                root.terminals().len(),
                2,
                "hidden terminal must remain alive"
            );
        });
    }
}
