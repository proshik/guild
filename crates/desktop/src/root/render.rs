//! The window itself: what is drawn, in what order, and what is bound.
//!
//! One method, and it is the map of the whole window — the title bar, the
//! project row, the tab strip, the body, and the overlays over them. It has
//! a file of its own because that is what it is: the place to look first
//! when asking where something on screen comes from.

use gpui::prelude::*;
use gpui::{Context, Window, div, rgb};

use crate::{Root, scale, theme};

/// How many frames a strip whose place has not settled is waited for
/// before its front is brought into view — M12.8's second window check,
/// O5. Three is what a restart takes; the rest is margin.
const STRIP_WAITS: u8 = 6;

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // What has the keys before this render hands them anywhere.
        let focused_before = window.focused(cx);
        // A column-owned disclosure must not leave an orphan menu when the
        // column folds, including programmatic folds and hover-peek.
        if self.settings.column_folded
            && matches!(
                self.context_menu.as_ref().map(|m| &m.target),
                Some(crate::view::menu::Target::Agents { .. })
            )
        {
            self.close_context_menu(window, cx);
        }
        self.ensure_open_in_apps();
        self.acknowledge_update_after_frame(window, cx);
        #[cfg(feature = "native-preview")]
        crate::native_preview::adaptive_refresh_probe::ROOT_RENDERS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if self.update_focus_listeners.is_empty() {
            let focus = &self.update_branch_focus;
            self.update_focus_listeners
                .push(cx.on_focus(focus, window, |_, _, cx| cx.notify()));
            self.update_focus_listeners
                .push(cx.on_blur(focus, window, |_, _, cx| cx.notify()));
        }
        let theme = self.theme;
        // One number for the whole interface, set before the tree is
        // built. A rem is only a rem against the window's size, so half a
        // tree reading a different one is a window that scales in pieces.
        //
        // `set_rem_size` rather than `with_rem_size`: the closure form
        // asserts it is called while painting, and this is neither paint
        // nor prepaint — it builds the tree.
        window.set_rem_size(scale::rem_size(self.settings.ui_font_size));
        for (project, clean) in &self.updating_branches {
            clean.store(
                self.dirty_file_tabs(Some(*project), cx).is_empty(),
                std::sync::atomic::Ordering::Release,
            );
        }
        self.prepare_settings_scopes(cx);
        let panels = self.panels();
        // Whatever an open dock shows is made ready — the tree read, the
        // message box made, the log asked — before its panel draws it.
        for dock in guild_session::Dock::ALL {
            let open = match dock {
                guild_session::Dock::Left => panels.left.open,
                guild_session::Dock::Right => panels.right.open,
                guild_session::Dock::Bottom => panels.bottom.open,
            };
            if !open {
                continue;
            }
            match self.tool_shown(*dock) {
                Some(guild_session::Tool::Git) if !self.integrations.external() => {}
                Some(guild_session::Tool::Explorer) => {
                    self.ensure_active_tree(cx);
                    self.follow_active_file_in_tree();
                }
                Some(guild_session::Tool::Commit) => self.ensure_message_box(window, cx),
                Some(guild_session::Tool::Git) => {
                    self.ensure_log(cx);
                    self.ensure_console_view(window, cx);
                }
                // What the project declares it can run, asked once and
                // off this thread — the list offers it.
                Some(guild_session::Tool::Output) => {
                    if let Some(project) = self.active_project {
                        self.ensure_declared(project, cx);
                    }
                }
                Some(guild_session::Tool::Terminal) => self.ensure_panel_terminals(cx),
                Some(guild_session::Tool::Search) => self.ensure_search(window, cx),
                // **Nothing to make ready.** Problems has no thread, no
                // field to type into and no first page to ask for: its
                // rows are read out of the diagnostics map at each
                // render, and the map is filled by the servers whether
                // the panel is open or not.
                Some(guild_session::Tool::Problems) => {}
                // Nor Find: a tab is made by `alt-f7`, and what it draws
                // is already in hand.
                Some(guild_session::Tool::Find) => {}
                // Retired by M8.3.6; no dock shows it.
                Some(guild_session::Tool::Push) | None => {}
            }
        }
        #[cfg(feature = "adaptive-terminal-refresh")]
        self.sync_terminal_visibility(window, cx);
        self.settle_speed_search();
        self.enforce_frame_minimum(window, cx);
        self.viewport = window.viewport_size();
        self.scale_factor = window.scale_factor();
        self.fit_frame_panels();
        self.fullscreen = window.is_fullscreen();
        // Leave AppKit's fullscreen title-bar reveal under native control.
        self.refresh_branch();
        self.refresh_group_reads(false);
        // A group that went because its last member left hands its folder
        // over; its root is closed after this frame — GLD-055 decision 18.
        let released = self.desk.take_released_group_folders();
        if !released.is_empty() {
            cx.defer_in(window, move |root, _window, cx| {
                root.close_released_group_folders(released, cx);
            });
        }
        self.remember_window(window);
        self.main_display = window
            .display(cx)
            .map(|display| (display.id(), window.bounds()));
        self.main_window = Some(window.window_handle());
        self.hear_tab_shortcuts(window, cx);
        self.hear_tab_launcher(window, cx);

        // With a panel over the tab, or with no tab at all, nothing below
        // takes focus — and a shortcut is dispatched along the path from
        // whatever is focused. Without this the path is empty and every
        // shortcut silently does nothing.
        //
        // **Unless the focus is in a dock** — M12.3. This was written while
        // the panels were empty, and with no tab open it took the focus
        // back from the tree, the Log or a panel pressed on every frame:
        // their keys, and a stretch of the panel, went to the window.
        // A dock a person just opened takes the focus, and so does the
        // sidebar brought back. M12.3.
        self.carry_activation(window, cx);
        // The window's own Settings page goes when a project is in front,
        // and a shortcut is recorded only while Settings is asking for one.
        // M5.11's decisions 15 and 14, in that order: a page that went this
        // frame is not asking.
        self.settle_settings_page();
        self.settle_settings_scroll();
        self.settle_customizer(cx);
        self.settle_action_bar_menu(window, cx);
        self.settle_recording(window, cx);
        if std::mem::take(&mut self.activating_sidebar) && !self.settings.column_folded {
            window.focus(&self.sidebar_focus, cx);
        }
        // **With nothing focused, the body in front takes the keys here**,
        // before any body is drawn — M12.8's second window check, 12.2. A
        // terminal and a file take the focus on render when nothing has
        // it, which was the right body while one was drawn at a time; a
        // divided tab draws every pane, and the first drawn won.
        // A closed pane's body that still holds the keys hands them on —
        // a buffer another workspace shares outlives its pane, and a
        // dropped body's handle is kept alive by `closed_focus` for this
        // one question. Anything else that holds them keeps them.
        // The window's own handle — a closed Settings pane's — is asked
        // exactly: everything in the window is inside it.
        let closed = std::mem::take(&mut self.closed_focus).iter().any(|handle| {
            if *handle == self.focus {
                handle.is_focused(window)
            } else {
                handle.contains_focused(window, cx)
            }
        });
        self.settle_terminal_jobs(window, cx);
        self.settle_overlay_focus(window, cx);
        self.settle_notes_focus(window, cx);
        let focusing =
            std::mem::take(&mut self.focusing_front) || closed || window.focused(cx).is_none();
        if focusing
            && self.worktree_removal.is_none()
            && !self.clone_project.as_ref().is_some_and(|open| open.visible)
        {
            match self.active_body().map(|body| body.focus_handle(cx)) {
                Some(Some(handle)) => window.focus(&handle, cx),
                // A body with nothing to type into — Settings — takes the
                // window's own handle, as `refocus` gives it: the keys
                // must not stay in the pane beside it.
                Some(None) => window.focus(&self.focus, cx),
                None => {}
            }
        }
        // A hanging list whose button is not drawn any more is put away
        // rather than drawn where the button used to be — and before the
        // focus rule below, which would otherwise spare its focus.
        if self.picker.is_some() && !self.picker_anchored() {
            self.close_picker(crate::root::picker::Focus::Back, window, cx);
        }
        let held_below = self.worktree_removal.is_some() || self
            .dock_focus
            .iter()
            .chain([&self.sidebar_focus])
            .any(|dock| dock.contains_focused(window, cx))
            // **And unless a popup's own card holds it** — M8.9's decision
            // 2, which took over M5.11's font-list clause here. A popup
            // takes a focus handle of its own, and with no tab open or the
            // session list up this took it back on the frame after, so
            // nothing typed reached the popup: the branch popup had carried
            // that latent since M12.3. It is also M5.11's task 16's clause
            // for the font list, which lost every key to the root on the
            // window's own Settings page; a list whose button is gone was
            // closed just above, so its focus is not spared.
            || self.panel_terminal_find_takes_focus(window, cx)
            || crate::text_field::focused(window, cx)
            || gpui_component::WindowExt::has_focused_input(window, cx)
            || self.action_bar_holds_focus(window, cx)
            || self.update_branch_focus.is_focused(window)
            || self.update_settings.holds_focus(window, cx)
            || self.update.presentation.holds_focus(window, cx)
            || self.picker_holds_focus(window, cx)
            // Anything focused on the window's own Settings page — its
            // buttons, the theme customizer, its fields — keeps the focus
            // the keyboard or a click gave it (code review r2,
            // arch+quality-1).
            || self.settings_page_focus.contains_focused(window, cx)
            || self.terminal_list.as_ref().is_some_and(|list| list.focus.contains_focused(window, cx))
            || self.clone_project.as_ref().is_some_and(|open| open.visible && open.focus.contains_focused(window, cx));
        // **With no body and a project in front, the focus rests on the
        // start screen**, whose keys are its own handle's — M14.5. A menu,
        // a tab's name or the `+` field focuses the root's handle over it
        // and is moved down to the screen's at the next frame; they lose
        // nothing, because the root reads their keys in the capture phase,
        // which passes the root on the way down to the screen as well.
        let start = self.start_showing().is_some();
        let resting = if start {
            self.start_focus.contains_focused(window, cx)
        } else {
            self.focus.is_focused(window)
        };
        if self.active_body().is_none() && !resting && !held_below {
            window.focus(
                if start {
                    &self.start_focus
                } else {
                    &self.focus
                },
                cx,
            );
        }
        if let Some(open) = &self.worktree_removal
            && !open.focus.iter().any(|focus| focus.is_focused(window))
        {
            window.focus(&open.focus[1], cx);
        }
        // The other way the path goes empty: what was focused stopped
        // being drawn. Registered on the first render, because it takes
        // the window and `Root::new` has none.
        if self.focus_lost.is_none() {
            self.focus_lost =
                Some(cx.on_focus_lost(window, |root, window, cx| root.refocus(window, cx)));
        }

        // A file tab whose buffer now exists is told to its server —
        // a restored tab has no editor until it is drawn.
        self.ensure_documents(cx);
        // …and a jump waiting for that buffer lands on it.
        self.place_caret(window, cx);
        // The same for a Back that reopened a file: the tab exists a
        // frame before its editor does.
        self.land_history(window, cx);
        self.land_library(window, cx);
        self.note_place(cx);
        // …and the same reason again: an edit is a place, and a buffer
        // that does not exist yet cannot be listened to.
        self.hear_edits(cx);
        // The disk, on the window's clock — a reload needs the window.
        #[cfg(any(test, feature = "native-preview"))]
        let background_services = !self.isolated_render;
        #[cfg(not(any(test, feature = "native-preview")))]
        let background_services = true;
        if background_services {
            self.start_watching(window, cx);
        }
        // …and the IDE socket, if the constructor had no project to
        // name in its lock file. A window opened empty and given a
        // project announces itself then; one that already listens
        // stops at the first line of `start_ide`.
        if background_services {
            self.start_ide();
        }
        // The code face, from the installed list — once, not per frame.
        self.refresh_code_font(window, cx);
        // Diff tabs restored at start-up get their listeners now.
        self.hear_restored_diffs(window, cx);
        // One scroll handle per strip, made before any strip is drawn and
        // dropped with its area — M12.8.
        self.keep_strip_scrolls();
        // **The tab in front is brought into view.** A stroke can choose
        // a tab the strip is not showing — `cmd-3`, a file opened from a
        // search hit, the neighbour a closed tab leaves in front — and a
        // lit tab nobody can see is worse than a strip that does not
        // scroll at all. Once per change of tab, not once per frame:
        // `scroll_to_item` is a request applied at prepaint, and asking
        // for it sixty times a second would fight a hand on the
        // trackpad. It is the focused area's strip that scrolls: the tab
        // in front is in it.
        //
        // **Every area's strip, once its place has settled** — M12.8's
        // second window check, O5. GPUI judges a request at prepaint
        // against the strip's bounds **from the frame before** — it
        // scrolls before it records the new ones — and consumes it
        // either way. A handle made this frame has no bounds; at a restart
        // the second frame's are the whole room's, the focused area
        // having been drawn alone before the room was measured, and only
        // the third's are the area's own. A request made against either
        // was judged against the wrong edge and lost: the front tab, last
        // of a long strip, stayed behind the chevron. So a strip is asked
        // only once the room is measured and its bounds are the same two
        // frames running, and a frame is asked for meanwhile.
        let measured = self.work_room().is_some();
        let mut waiting = false;
        let strips: Vec<_> = self
            .tab_scroll
            .iter()
            .map(|(area, strip)| (*area, strip.clone()))
            .collect();
        for (area, strip) in strips {
            // Read every frame, so "the same two frames running" means
            // the last two.
            let bounds = strip.bounds();
            let steady = self.strip_bounds.insert(area, bounds) == Some(bounds);
            let front = self.area_front(area);
            if self.tab_drag.active.is_some() {
                // Consume live front changes without competing with the held edge.
                self.tab_scrolled.insert(area, front);
                continue;
            }
            if self.tab_scrolled.get(&area) == Some(&front) {
                continue;
            }
            if front.is_some() && !(measured && steady && bounds.size.width > gpui::px(0.0)) {
                waiting = true;
                continue;
            }
            self.tab_scrolled.insert(area, front);
            if let Some(at) =
                front.and_then(|front| self.area_tabs(area).position(|tab| tab.id == front))
            {
                strip.scroll_to_item(at);
            }
        }
        self.tab_scrolled
            .retain(|area, _| self.tab_scroll.contains_key(area));
        self.strip_bounds
            .retain(|area, _| self.tab_scroll.contains_key(area));
        // A few frames at most: a strip that is never drawn must not keep
        // the window painting (traps.md, a clock that notifies).
        self.strip_waits = if waiting {
            self.strip_waits.saturating_add(1)
        } else {
            0
        };
        if waiting && self.strip_waits <= STRIP_WAITS {
            let root = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(root));
        }
        // A tab that just went behind another is written: autosave's
        // cue for it, before the tree follows the new one.
        if self.tree_followed != self.active_tab {
            self.autosave_files(crate::autosave::Trigger::Behind, cx);
        }
        if self.activation.is_none() {
            self.watch_activation(window, cx);
        }
        if self.appearance.is_none() {
            self.watch_appearance(window, cx);
        }

        let picker = self.render_picker(window, cx);
        // So are the side docks' panels, each its own width.
        // Each in a card of its own beside the areas — M12.8.
        let left = panels.left.open.then(|| {
            let panel = self.side_dock(guild_session::Dock::Left, cx);
            self.render_side_card(panel)
        });
        let right = panels.right.open.then(|| {
            let panel = self.side_dock(guild_session::Dock::Right, cx);
            self.render_side_card(panel)
        });
        // The bottom dock's panel is a region too, inside a card drawn
        // here; its layout node is its own height.
        let bottom = panels.bottom.open.then(|| {
            gpui::AnyView::from(self.region(
                crate::view::region::Part::Dock(guild_session::Dock::Bottom),
                cx,
            ))
            .cached(
                gpui::StyleRefinement::default()
                    .flex_none()
                    .w_full()
                    .h(self.bottom_dock_height()),
            )
        });
        // The pinned column is a view of its own, reused from the last
        // frame until the root or the column itself changes —
        // `view::region`. Its layout node is the style handed here, so the
        // width is `render_column`'s own.
        let column = (!self.settings.column_folded).then(|| {
            gpui::AnyView::from(self.region(crate::view::region::Part::Column, cx)).cached(
                gpui::StyleRefinement::default()
                    .flex_none()
                    .w(self.column_width())
                    .h_full(),
            )
        });
        // **Keys handed over while the window draws repaint the body that
        // takes them.** `window.focus` refreshes nothing during a draw, so a
        // cached terminal would replay the frame it painted without the
        // keys — no text client for the platform until something else
        // repaints it. `view::body` draws it whole for this one frame.
        self.focus_moved_in_draw = window.focused(cx) != focused_before;

        div()
            .track_focus(&self.focus)
            // A panel gets its own context so that Escape closes it there
            // and stays the program's key everywhere else: in a terminal
            // it is how one leaves an editor's insert mode.
            // Action Bar rows reuse the contextual Escape guard of worktree rows.
            .key_context(
                match (
                    self.recording.is_some(),
                    self.worktree_dragging
                        || self.action_bar_dragging
                        || self.project_drag.active.is_some()
                        || self.tab_drag.active.is_some(),
                ) {
                    (true, true) => "guild panel recording column-drag",
                    (false, true) => "guild column-drag",
                    (true, false) => "guild panel recording",
                    (false, false) => "guild",
                },
            )
            // **The capture phase, not the bubble one**, for the fields
            // and menus that do not hold the focus themselves: since
            // M12.7 one reader, `context_menu_key`, covers every menu of
            // the engine — the right button's, the tab strip's three, a
            // dock head's ⋮, the agents list, the project popup, the
            // Log's filter dropdowns and Find in Files' history — plus
            // the `+` field, Go to Line and a tab's name, none of the
            // engine's. A key event is dispatched from the focused node
            // *upwards*, so handled on the way up the terminal behind
            // would already have typed the letter into a live `claude`;
            // captured on the way down, the root sees it first and stops
            // it there.
            //
            // **A popup is not among them** since M8.9: it takes the focus
            // on its own card and reads its keys there, after this whole
            // phase — which is why `context_menu_key` finds nothing to do
            // while one is up: the two are mutually exclusive by
            // construction (`put_fields_away`), not by a second reader
            // standing down.
            //
            // Bindings are dispatched before either phase, so this does not
            // take a shortcut away.
            .capture_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if root.tab_launcher_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if root.tab_shortcut_key(event, cx) {
                    return;
                }
                if root.worktree_removal_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if (root.worktree_dragging
                    || root.action_bar_dragging
                    || root.project_drag.active.is_some()
                    || root.tab_drag.active.is_some())
                    && event.keystroke.key == "escape"
                {
                    root.clear_project_drag(cx);
                    root.clear_tab_drag(cx);
                    root.action_bar_dragging = false;
                    root.worktree_dragging = false;
                    cx.stop_active_drag(window);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                // Native fields own editing and route host keys from their wrapper.
                if crate::text_field::focused(window, cx)
                    && root.context_menu.is_none()
                    && root
                        .clone_project
                        .as_ref()
                        .is_none_or(|open| !open.visible || !open.phase.busy())
                {
                    return;
                }
                if root.clone_project_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }

                // **A balloon goes at the next key**, and
                // lets the key through — except Escape, which was pressed
                // for the balloon. M8.15.
                if root.balloon_heard(Some(event.keystroke.key.as_str()), cx) {
                    cx.stop_propagation();
                    return;
                }
                // The right button's menu first: while it is up every key
                // is its own. M12.2.
                if root.context_menu_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if root.new_worktree_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if root.workspace_field_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if root.workspace_icon_picker_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                // *What's new* — M5.16: Esc puts it away; every other key
                // goes on to what has the focus.
                if root.update.greeting().is_some() && event.keystroke.key == "escape" {
                    root.dismiss_whats_new(cx);
                    cx.stop_propagation();
                    return;
                }
                if root.showing_usage && event.keystroke.key == "escape" {
                    root.showing_usage = false;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if root.showing_servers && event.keystroke.key == "escape" {
                    root.showing_servers = false;
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if root.goto_line_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                // A tab's name being typed — M12.1 — over a terminal that
                // would otherwise take every letter.
                // Backspace over a Log filter dropdown is
                // `context_menu_key`'s since M12.7 task 6: the dropdowns
                // are menus of the engine, read at the top of this phase.
                if root.rename_tab_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if root.process_tab_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                root.record_stroke(event, window, cx)
            }))
            // Escape closes the theme customizer wherever the focus is —
            // its card, the button that opened it, another Settings
            // control, or the window's own handle that Settings rests on —
            // and drops its preview (code review r1, bugs+impl-2). The
            // bubble phase, so the hex field's own Escape, which restores
            // the field, stops it first.
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && root.customizer_drawn() && root.customizing()
                {
                    cx.stop_propagation();
                    root.dismiss_customizer(window, cx);
                }
            }))
            // **The double Shift's second ear: the modifiers** — M8.9,
            // decision 8. A `ModifiersChangedEvent` is dispatched along the
            // focus path from the focused node up, and the root is on every
            // path; nothing on it stops one (task 0 read M5.12's hover).
            .on_modifiers_changed(cx.listener(
                |root, event: &gpui::ModifiersChangedEvent, window, cx| {
                    root.tab_launcher_modifiers(event.modifiers, window, cx);
                    root.shift_heard(event, window, cx)
                },
            ))
            .map(|root| self.bind_tabs(root, cx))
            .map(|root| self.bind_files(root, cx))
            .map(|root| self.bind_git(root, cx))
            .map(|root| self.bind_push(root, cx))
            .map(|root| self.bind_run(root, cx))
            .map(|root| self.bind_navigation(root, cx))
            .map(|root| self.bind_recent_files(root, cx))
            .map(|root| self.bind_everywhere(root, cx))
            .map(|root| self.bind_find_in_files(root, cx))
            .map(|root| self.bind_panels(root, cx))
            .map(|root| self.bind_projects(root, cx))
            .map(|root| self.bind_workspaces(root, cx))
            .map(|root| self.bind_window(root, cx))
            .flex()
            .flex_col()
            .size_full()
            // **One surface for the whole frame** — the column, the docks,
            // the strip and the status bar are all `surface.panel`, and the
            // only thing that divides them is a hairline. Until M12's task
            // 12 the window had three greys where the meaning was one:
            // `app` behind the frame, `panel` on the panels and `sunken`
            // under the status bar, which read as a strip belonging to
            // something else. What a tab draws inside itself — a terminal,
            // an editor, a diff — still paints its own ground: that is the
            // work, and it is allowed to differ from the frame around it.
            .bg(rgb(theme.surface.panel))
            .text_color(rgb(theme.text.primary))
            .text_size(theme::size::UI)
            // A person's own family when they named one, and the design
            // system's otherwise. A name that is not installed is GPUI's
            // problem to fall back from, the same as any missing family.
            .font_family(
                self.settings
                    .ui_font
                    .clone()
                    .unwrap_or_else(|| crate::fonts::DEFAULT.ui.to_owned()),
            )
            .relative()
            .child(crate::view::resize::capture(
                cx,
                Self::resize_motion,
                Self::resize_release,
            ))
            // Dragging is followed on the window rather than on the grip:
            // a pointer moving faster than the frames leaves the three
            // pixels behind, and a handle that only works slowly is a
            // handle that looks broken.
            .capture_any_mouse_up(cx.listener(|root, event: &gpui::MouseUpEvent, window, cx| {
                if event.button == gpui::MouseButton::Left {
                    // The column's drop line outlives the capture phase: a
                    // release in the gap between rows is the list's drop,
                    // which reads it in the bubble phase. It goes once the
                    // release has been dispatched, whoever took it.
                    let hint = root.column_drop_hint.take();
                    root.clear_project_drag(cx);
                    root.clear_tab_drag(cx);
                    if hint.is_some() {
                        root.column_drop_hint = hint;
                        cx.defer_in(window, |root, _, cx| root.set_column_drop_hint(None, cx));
                    }
                }
                if event.button == gpui::MouseButton::Left
                    && (root.worktree_dragging || root.action_bar_dragging)
                {
                    root.action_bar_dragging = false;
                    root.worktree_dragging = false;
                    cx.notify();
                }
            }))
            .on_mouse_move(
                cx.listener(|root, event: &gpui::MouseMoveEvent, window, cx| {
                    if event.pressed_button.is_none()
                        && (root.project_drag.active.is_some() || root.tab_drag.active.is_some())
                    {
                        root.clear_tab_drag(cx);
                        root.clear_project_drag(cx);
                        cx.stop_active_drag(window);
                    }
                    if event.pressed_button.is_none()
                        && (root.worktree_dragging || root.action_bar_dragging)
                    {
                        root.action_bar_dragging = false;
                        root.worktree_dragging = false;
                        cx.stop_active_drag(window);
                        cx.notify();
                    }
                    if event.pressed_button.is_some() {
                        root.wake_project_drag(window, cx);
                        root.wake_tab_drag(window, cx);
                    }
                    // A tool drag whose release never arrived — let go
                    // outside the window — ends on the first move with no
                    // button held, M5.8's lesson.
                    // Not a drag from another application: that one is
                    // AppKit's until it drops, and GPUI's drag is what the
                    // drop is matched against.
                    if event.pressed_button.is_none()
                        && root.file_operations.drag.is_some()
                        && !root.tree_drag_inbound()
                    {
                        cx.stop_active_drag(window);
                        root.clear_tree_drag(cx);
                    }
                    if event.pressed_button.is_none() && root.tool_drag.take().is_some() {
                        cx.notify();
                    }
                    // The hidden column comes out at the window's left
                    // edge and goes when the pointer leaves it — M12.
                    root.follow_column_pointer(event.position, cx);
                    // The Find tool's list-and-preview split. M8.15.
                    if root.drag_find_split(event, cx) {
                        return;
                    }
                    root.follow_pointer(event, window, cx)
                }),
            )
            // A tool being dragged: the zones are drawn for it at the
            // three edges, and only then.
            .on_drag_move(cx.listener(
                |root, event: &gpui::DragMoveEvent<crate::view::drag::DragTool>, _window, cx| {
                    let tool = event.drag(cx).0;
                    if root.tool_drag != Some(tool) {
                        root.tool_drag = Some(tool);
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|root, _event, _window, cx| {
                    root.stop_dragging(cx);
                    root.drop_picker(cx);
                    root.end_find_split(cx);
                }),
            )
            // **The double Shift's third ear: every press of the mouse** —
            // M8.9, decision 8. A window listener registered by a canvas
            // painted first, rather than `capture_any_mouse_down`, which
            // fires only while the root's own hitbox is hovered — and an
            // occluding card is exactly where two shift-clicks on its rows
            // land. A canvas inserts no hitbox, so it hides nothing.
            .child({
                let heard = self.heard.clone();
                gpui::canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let heard = heard.clone();
                        window.on_mouse_event(move |_: &gpui::MouseDownEvent, phase, _, _| {
                            if phase == gpui::DispatchPhase::Capture {
                                heard.set(heard.get().wrapping_add(1));
                            }
                        });
                    },
                )
                .absolute()
                .size_full()
            })
            // **A press anywhere takes a balloon down**, heard the same way
            // and for the same reason: the press may land on a card that
            // occludes the root. M8.15.
            .when(self.balloon.is_some(), |root| {
                let this = cx.entity().downgrade();
                root.child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let this = this.clone();
                            window.on_mouse_event(move |_: &gpui::MouseDownEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Capture {
                                    this.update(cx, |root, cx| {
                                        root.balloon_heard(None, cx);
                                    })
                                    .ok();
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .child(self.render_titlebar(window, cx))
            // The column runs the whole height beside the docks and the
            // bottom one, because it is where a person changes *what*
            // they are working on rather than *how* they are looking at
            // it. It replaced the row of projects M5.1 put here: a row
            // was right while a project was a directory and there were
            // three of them, and a project is a repository with several
            // worktrees now. M12.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    // The peeking column hangs from this row's own top
                    // left, so the panel starts under the title bar and
                    // runs to the status bar like the pinned one does.
                    .relative()
                    .children(column)
                    // Hidden, and the pointer has brought it out: it is
                    // drawn **over** the work rather than beside it, so
                    // nothing reflows for a panel that is about to go
                    // away again. `deferred`, because an absolutely
                    // placed element is still painted in its own place in
                    // the tree — the trap the branch popup and the Log's
                    // filters both met — and this one has to outrank
                    // everything the row draws after it.
                    .when(self.settings.column_folded && self.column_peeking, |row| {
                        row.child(gpui::deferred(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .child(self.render_column(cx)),
                        ))
                    })
                    .child(
                        div()
                            .id("work-surface")
                            .debug_selector(|| "work-surface".into())
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .debug_selector(|| "upper-work-surface".into())
                                    .flex()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    // **The work is cards on the window's colour** —
                                    // section 4.1, M12.6; since
                                    // M12.8 a card per area, with the side docks in
                                    // cards of their own beside them (decision 18).
                                    // The row touches the column; with the column
                                    // hidden it stands in from the window's left as
                                    // from its right.
                                    .mr(theme::space::WORK_INSET)
                                    .when(self.settings.column_folded, |row| {
                                        row.ml(theme::space::WORK_INSET)
                                    })
                                    // The side docks frame the window rather than
                                    // living in it. Their tops are the areas' tops,
                                    // so a panel's heading sits on the strips' line.
                                    .when(self.tool_drag.is_some(), |row| {
                                        row.child(
                                            self.render_drop_zone(guild_session::Dock::Left, cx),
                                        )
                                    })
                                    .when_some(left, |row, card| {
                                        row.child(card)
                                            .child(div().flex_none().w(theme::space::WORK_BETWEEN))
                                    })
                                    .child(self.render_areas(window, cx))
                                    .when_some(right, |row, card| {
                                        row.child(div().flex_none().w(theme::space::WORK_BETWEEN))
                                            .child(card)
                                    })
                                    .when(self.tool_drag.is_some(), |row| {
                                        row.child(
                                            self.render_drop_zone(guild_session::Dock::Right, cx),
                                        )
                                    })
                                    // The side docks' grips, over the gaps: after
                                    // the cards, before every popup. `side_grips`
                                    // says why.
                                    .relative()
                                    .children(self.side_grips(cx)),
                            )
                            .children(bottom.map(|panel| self.render_bottom_card(panel, cx)))
                            .when(self.tool_drag.is_some(), |surface| {
                                surface
                                    .child(self.render_drop_zone(guild_session::Dock::Bottom, cx))
                            }),
                    )
                    // Above both sides of the boundary, below deferred popups.
                    .when(!self.settings.column_folded, |row| {
                        row.child(self.render_column_edge(cx))
                    }),
            )
            // One footer baseline below every dock, including column-owned controls.
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_end()
                    .when(!self.settings.column_folded, |footer| {
                        footer.child(self.render_column_foot(cx).w(self.column_width()))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.render_status(window, cx)),
                    )
                    .relative()
                    .when(
                        self.settings.column_folded && self.column_peeking,
                        |footer| {
                            footer.child(gpui::deferred(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .w(self.column_width())
                                    .occlude()
                                    .child(self.render_column_foot(cx)),
                            ))
                        },
                    )
                    .child({
                        let height = self.footer_height.clone();
                        let root = cx.entity().downgrade();
                        gpui::canvas(
                            move |bounds, _, cx| {
                                if height.replace(bounds.size.height) != bounds.size.height {
                                    let root = root.clone();
                                    cx.defer(move |cx| {
                                        let _ = root.update(cx, |_, cx| cx.notify());
                                    });
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    }),
            )
            // Whichever popup is up — M8.9's one picker. It reads its own
            // keys on its own card, so nothing of it is in the capture
            // handler above.
            .children(picker)
            .when(self.goto_line_query().is_some(), |root| {
                root.child(self.render_goto_line(cx))
            })
            .children(self.render_context_menu(cx))
            .children(self.render_new_worktree(cx))
            .children(self.render_clone_project(cx))
            .children(self.render_worktree_removal(cx))
            .children(self.render_workspace_field(cx))
            .children(self.render_workspace_icon_picker(cx))
            .children(self.render_servers(cx))
            .children(self.render_usage(cx))
            // Recovery controls must stay above the deferred terminal cards
            // whose processes they let the person cancel or reattach.
            .children(self.render_terminal_jobs(cx))
            .children(self.render_whats_new(window, cx))
            .children(self.render_balloon(cx))
            // Last, so a notice is not covered by the panel that raised it.
            .child(self.render_notices(window, cx))
    }
}

impl Root {
    /// A side dock's panel, drawn by its cached region — `view::region`.
    /// Its layout node is the panel's own width, full height, as
    /// `render_panel` draws it.
    fn side_dock(
        &mut self,
        dock: guild_session::Dock,
        cx: &mut Context<Self>,
    ) -> gpui::ViewElement<gpui::AnyView> {
        gpui::AnyView::from(self.region(crate::view::region::Part::Dock(dock), cx)).cached(
            gpui::StyleRefinement::default()
                .flex_none()
                .w(self.side_dock_width(dock))
                .h_full(),
        )
    }

    /// Whether the window is in front, from the activation itself — once
    /// from the window as it is, then on every change.
    ///
    /// **Never from a paint.** Until 2026-09-24 the flag was set at the
    /// top of `render`, and a window that loses activation while it is
    /// covered — another application in full screen, another Space,
    /// minimised — has its display link stopped and paints nothing: it
    /// went on believing it was in front, the worktree cadence went on
    /// forking git and the tab in front went on being marked seen. GPUI
    /// calls the observer whether a frame follows or not.
    fn watch_activation(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        self.window_focused = window.is_window_active();
        self.activation = Some(cx.observe_window_activation(window, |root, window, cx| {
            root.window_focused = window.is_window_active();
            // The window losing activation is a blur of every buffer.
            if !root.window_focused {
                root.cancel_tab_launcher(window, cx);
                for focus in root.overlay_focus.borrow_mut().values_mut() {
                    focus.presentation.gesture = None;
                }
                root.autosave_files(crate::autosave::Trigger::Deactivate, cx);
                return;
            }
            // Coming back is where a worktree made in another window — or
            // by an agent in a terminal — is noticed at once rather than
            // within the three seconds of the cadence. M12.
            root.ask_worktrees();
            // …and the merge requests not asked lately (GLD-063).
            root.forge_focused();
        }));
    }
}

#[cfg(test)]
mod tests {
    /// Every `.rs` under `src/`, with its text up to its tests — a test
    /// that spells what it looks for is not the code it looks at.
    fn sources(dir: &std::path::Path, found: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).expect("src is readable") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file");
                let code = text
                    .split("#[cfg(test)]\nmod tests")
                    .next()
                    .unwrap_or_default();
                found.push((path, code.to_owned()));
            }
        }
    }

    /// **Whether the window is focused is written by its activation, never
    /// by its paint.** 2026-09-24: it was set at the top of `render`, and
    /// a window that loses activation while it is covered — another
    /// application's full-screen window, another Space, minimised — has
    /// its display link stopped and paints nothing, so it went on
    /// believing it was in front: the worktree cadence kept forking git
    /// and the tab in front kept being marked seen by nobody. GPUI calls
    /// the activation observers whether a frame follows or not.
    ///
    /// Read from the source because no test here has a platform window to
    /// cover: the rule is where the assignment is, and a paint that writes
    /// the flag again is the bug coming back.
    #[test]
    fn focus_is_written_by_the_activation_and_never_by_a_paint() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        sources(&src, &mut found);
        let writes: Vec<String> = found
            .iter()
            .flat_map(|(path, text)| {
                text.lines()
                    .filter(|line| line.contains(".window_focused ="))
                    .map(move |line| format!("{}: {}", path.display(), line.trim()))
            })
            .collect();
        let render = &found
            .iter()
            .find(|(path, _)| path.ends_with("root/render.rs"))
            .expect("render.rs")
            .1;
        let start = render
            .find("fn watch_activation(")
            .expect("the activation is watched in a function of its own");
        let body = &render[start..];
        let body = &body[..body.find("\n    }\n").expect("the function ends")];
        assert_eq!(
            body.matches(".window_focused =").count(),
            writes.len(),
            "the flag is written only where the activation is watched: {writes:#?}"
        );
        assert!(
            body.contains("observe_window_activation"),
            "and that is where the observer is registered"
        );
    }

    /// **The column and the docks are drawn by views of their own,
    /// cached.** The root is not cached and renders on every frame a
    /// terminal in front prints: measured on 2026-09-24 with twelve
    /// projects, the column was 0.7 ms and the bottom dock 0.34 of a 2.7 ms
    /// frame that changed nothing in either. `view::region` wraps the same
    /// `Root` methods in views GPUI reuses until the root or the region
    /// itself is notified. Read from the source because no test here has a
    /// window: a region drawn inline again is the cost coming back, and a
    /// region that stops observing the root is one that goes stale.
    #[test]
    fn the_regions_are_drawn_by_their_cached_views() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        sources(&src, &mut found);
        let text = |file: &str| {
            found
                .iter()
                .find(|(path, _)| path.ends_with(file))
                .map(|(_, text)| text.clone())
                .expect("the file")
        };
        let render = text("root/render.rs");
        // Once: the column peeking over the work, which is drawn over
        // everything and goes again — the pinned one is the region's.
        assert_eq!(
            render.matches("self.render_column(cx)").count(),
            1,
            "only the peeking column is drawn inline"
        );
        assert!(
            render.contains("Part::Column"),
            "the pinned column is the region's"
        );
        assert!(render.contains(".cached("), "and the region is cached");
        assert!(
            !render.contains("render_bottom_panel("),
            "the bottom dock is its region's, inside a card drawn inline"
        );
        assert!(
            render.contains("Part::Dock(guild_session::Dock::Bottom)"),
            "and that is its region"
        );
        for side in ["render_left_panel(", "render_right_panel("] {
            assert!(
                !render.contains(side),
                "the side docks are their regions': {side}"
            );
        }
        assert!(
            render.contains("fn side_dock("),
            "drawn through one helper for both sides"
        );
        let region = text("view/region.rs");
        assert!(
            region.contains("cx.observe(root"),
            "a region re-renders on every notify of the root"
        );
    }

    /// **What the window draws on every frame binds no session's entity.**
    /// GPUI records every entity read or updated during a draw and
    /// invalidates the window at that entity's next notify; the root is not
    /// cached, so that is the whole window again. 2026-09-24: `render` read
    /// every session view on every frame — the strip's
    /// names, the agents counter, `sync_from_views` — and two terminals
    /// printing in tabs nobody could see kept an unchanging window at 20–35
    /// frames a second and 10 % of a core against 3 % idle. The facts the
    /// frame needs are published beside the view — `terminal::facts`.
    ///
    /// Read from the source because no test here has a window to draw: a
    /// pattern that binds `view` out of a session's body, in the code a
    /// frame runs, is the bug coming back. Two files are let through:
    /// `view/body.rs` draws the tab in front, which is on screen and must
    /// redraw with its output, and `view/menu/terminal.rs` reads the
    /// terminal a menu was opened on, at the press.
    #[test]
    fn a_frame_binds_no_session_view() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        sources(&src.join("view"), &mut found);
        sources(&src.join("root"), &mut found);
        let framed = |path: &std::path::Path| {
            let view = path.starts_with(src.join("view"))
                && !path.ends_with("view/body.rs")
                && !path.ends_with("view/menu/terminal.rs");
            view || path.ends_with("root/render.rs")
        };
        let mut bindings = Vec::new();
        for (path, text) in found.iter().filter(|(path, _)| framed(path)) {
            for variant in ["Body::Terminal {"] {
                for (at, _) in text.match_indices(variant) {
                    let pattern = &text[at..];
                    let pattern = &pattern[..pattern.find('}').unwrap_or(pattern.len())];
                    if pattern.contains("view") {
                        bindings.push(format!("{}: {pattern}}}", path.display()));
                    }
                }
            }
        }
        assert!(
            bindings.is_empty(),
            "a frame reads a session's view: {bindings:#?}"
        );

        let render = &found
            .iter()
            .find(|(path, _)| path.ends_with("root/render.rs"))
            .expect("render.rs")
            .1;
        assert!(
            !render.contains("sync_from_views("),
            "the views are synced on the watch loop's clock, not on a frame's"
        );

        // The two a frame asks for every tab cannot reach an entity at all:
        // they are given no `App` to read one with.
        for (file, name) in [
            ("root/tabs.rs", "fn session_tab_name("),
            ("view/agents.rs", "fn open_agents("),
        ] {
            let text = &found
                .iter()
                .find(|(path, _)| path.ends_with(file))
                .expect("the file")
                .1;
            let start = text.find(name).expect("the function");
            let signature = &text[start..];
            let signature = &signature[..signature.find('{').expect("a body")];
            assert!(
                !signature.contains("App"),
                "{name} takes no App, so that a frame cannot read a terminal through it: {signature}"
            );
        }
    }
}

#[cfg(test)]
#[path = "render/footer_tests.rs"]
mod footer_tests;

#[cfg(test)]
#[path = "render/terminal_cache_tests.rs"]
mod terminal_cache_tests;
