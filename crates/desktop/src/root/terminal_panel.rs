//! Project-owned dock terminals, independent of Run and Terminal Jobs.
use crate::{
    Root,
    terminal::view::{Look, TerminalTab},
};
use gpui::{Context, Entity, Window};
use guild_session::{ProjectId, Tool};

impl Root {
    pub(crate) fn panel_terminal_find_takes_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.tool_is_up(Tool::Terminal)
            && self
                .active_panel_terminal()
                .is_some_and(|view| view.read(cx).find_takes_focus(window))
    }

    pub(crate) fn active_panel_terminal(&self) -> Option<Entity<TerminalTab>> {
        self.active_project
            .and_then(|p| self.panel_terminals.get(&p))
            .and_then(|tabs| tabs.active())
            .map(|entry| entry.value.clone())
    }

    pub(crate) fn ensure_panel_terminals(&mut self, cx: &mut Context<Self>) {
        if !self.integrations.external() {
            return;
        }
        if self.panel_terminal_front != self.active_project {
            self.panel_terminal_front = self.active_project;
            let selected = self
                .active_project
                .and_then(|p| self.panel_terminals.get(&p))
                .and_then(|tabs| {
                    tabs.active()
                        .and_then(|active| tabs.entries().iter().position(|e| e.id == active.id))
                })
                .unwrap_or(0);
            self.panel_terminal_scroll.scroll_to_item(selected + 1);
        }
        if let Some(project) = self.active_project
            && !self.panel_terminals.contains_key(&project)
        {
            // Remember even failure/empty so rendering never retries a process launch.
            self.panel_terminals.insert(project, Default::default());
            self.new_panel_terminal(cx);
        }
    }

    pub(crate) fn new_panel_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let path = self
            .active_panel_terminal()
            .and_then(|view| view.read(cx).live_shell_cwd())
            .filter(|p| p.is_dir())
            .unwrap_or(path);
        let look = Look::of(&self.settings, self.theme);
        let mut command = crate::terminal::shell::panel_command();
        crate::terminal::shell::align_theme(&mut command, self.theme.kind);
        if let Err(error) =
            crate::network::apply_pty(&mut command, guild_session::network::Scope::Terminal)
        {
            self.fail(error.to_string());
            return;
        }
        let material = match crate::terminal::shell_integration::prepare(&mut command) {
            Ok(material) => material,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        match crate::terminal::view::open_command(command, &path, look, None, cx) {
            Ok(view) => {
                view.update(cx, |tab, _| tab.hold_shell_material(material));
                self.add_panel_terminal(project, view, cx);
            }
            Err(error) => self.fail(error.to_string()),
        }
        cx.notify();
    }

    pub(crate) fn add_panel_terminal(
        &mut self,
        project: ProjectId,
        view: Entity<TerminalTab>,
        cx: &mut Context<Self>,
    ) {
        Self::hear_terminal_input(&view, cx);
        let tabs = self.panel_terminals.entry(project).or_default();
        tabs.add(view);
        self.panel_terminal_scroll
            .scroll_to_item(tabs.entries().len());
        self.activate_dock(self.docks().dock_of(Tool::Terminal));
    }

    pub(crate) fn new_panel_terminal_by_hand(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_panel_terminal(cx);
        // A failed launch must return the keys to the shell already visible.
        if self.active_panel_terminal().is_some() {
            self.activate_dock(self.docks().dock_of(Tool::Terminal));
        }
    }

    pub(crate) fn select_panel_terminal(
        &mut self,
        project: ProjectId,
        id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_project != Some(project) {
            return;
        }
        if let Some(tabs) = self.panel_terminals.get_mut(&project) {
            tabs.select(id);
            if let Some(index) = tabs.entries().iter().position(|entry| entry.id == id) {
                self.panel_terminal_scroll.scroll_to_item(index + 1);
            }
        }
        self.activate_dock(self.docks().dock_of(Tool::Terminal));
        cx.notify();
    }

    pub(crate) fn close_panel_terminal(
        &mut self,
        project: ProjectId,
        id: u64,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self
            .panel_terminals
            .get_mut(&project)
            .and_then(|tabs| tabs.remove(id))
        {
            entry.value.update(cx, |terminal, _| terminal.let_go());
        }
        if self.active_project == Some(project) {
            self.activate_dock(self.docks().dock_of(Tool::Terminal));
        }
        cx.notify();
    }

    pub(crate) fn forget_panel_terminals(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if let Some(tabs) = self.panel_terminals.remove(&project) {
            for entry in tabs.entries() {
                entry.value.update(cx, |terminal, _| terminal.let_go());
            }
        }
    }
}
