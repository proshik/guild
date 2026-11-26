//! Source launch stays with its editable file until a saved snapshot is verified.
use super::*;
use crate::run::{
    source::{self, Runner, Task},
    tabs::Invocation,
};
use crate::text_field::TextField;
use crate::view::tooltip::Hint as _;
#[cfg(test)]
mod tests;

pub(super) struct Form {
    tasks: Vec<Task>,
    selected: usize,
    values: Vec<String>,
    manager: Option<String>,
    error: Option<String>,
    focus: gpui::FocusHandle,
    needs_focus: bool,
    read: Option<gpui::Task<()>>,
}
pub(super) struct Arguments {
    parameters: Vec<source::Parameter>,
    values: Vec<String>,
    manager: Option<String>,
}
#[derive(Clone)]
pub struct LaunchRun(pub Invocation);
impl gpui::EventEmitter<LaunchRun> for FileTab {}

impl FileTab {
    /// Availability follows current discovery and the target's editability.
    pub fn can_run_file(&self) -> bool {
        !self.read_only && !self.relocating && !self.tasks.is_empty()
    }

    pub(crate) fn choose_task(&mut self, tasks: Vec<Task>, cx: &mut Context<Self>) {
        if tasks.is_empty() || self.read_only {
            return;
        }
        self.launch = Some(Form {
            values: vec![String::new(); tasks[0].parameters.len() + 1],
            tasks,
            selected: 0,
            manager: None,
            error: None,
            focus: cx.focus_handle(),
            needs_focus: true,
            read: None,
        });
        cx.notify();
    }
    /// A declaration click already identifies the command. Ask only for missing
    /// inputs, or when the person explicitly chooses to configure this launch.
    pub(crate) fn activate_task(
        &mut self,
        tasks: Vec<Task>,
        configure: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tasks.is_empty() || self.read_only || self.relocating {
            return;
        }
        self.choose_task(tasks, cx);
        let Some(form) = self.launch.as_mut() else {
            return;
        };
        if form.tasks.len() != 1 {
            return;
        }
        let task = &form.tasks[0];
        if let Some(arguments) = self.launch_arguments.get(&task.identity)
            && arguments.parameters == task.parameters
            && arguments
                .manager
                .as_ref()
                .is_none_or(|m| task.managers.contains(m))
        {
            form.values = arguments.values.clone();
            form.manager = arguments.manager.clone();
        }
        let missing = task
            .parameters
            .iter()
            .zip(&form.values)
            .any(|(p, v)| p.required && v.is_empty())
            || (task.identity.runner == Runner::Package
                && task.managers.len() != 1
                && form.manager.is_none());
        if !configure && !missing {
            self.submit_task(window, cx);
        }
    }
    pub(crate) fn run_file(&mut self, cx: &mut Context<Self>) {
        let mut tasks = self.tasks.clone();
        if let Some(index) = tasks.iter().position(|t| t.default) {
            tasks.swap(0, index);
        }
        if let Some(first) = tasks.first().cloned()
            && first.identity.runner == Runner::Make
        {
            let mut default = first;
            default.identity.name.clear();
            tasks.insert(0, default);
        }
        // Just's default recipe needs its name when it accepts arguments;
        // otherwise Just would interpret the first value as a recipe name.
        if let Some(index) = tasks
            .iter()
            .position(|t| t.identity.runner == Runner::Task && t.identity.name == "default")
        {
            tasks.swap(0, index);
        }
        self.choose_task(tasks, cx);
    }
    fn submit_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.launch.as_ref() else {
            return;
        };
        let focus = form.focus.clone();
        let selected = form.selected;
        let values = form.values.clone();
        let manager = form.manager.clone();
        let path = self.path();
        let expected_path = path.clone();
        let clean = self.clean.clone();
        let work = cx.background_executor().spawn(async move { read(&path) });
        self.launch.as_mut().unwrap().read = Some(cx.spawn_in(window, async move |file, cx| {
            let disk = work.await;
            let _ = cx.update(|window, cx| {
                file.update(cx, |file, cx| {
                    let Some(form) = &mut file.launch else {
                        return;
                    };
                    if form.focus != focus
                        || form.selected != selected
                        || form.values != values
                        || form.manager != manager
                    {
                        return;
                    }
                    form.read = None;
                    if file.path() != expected_path
                        || file.clean != clean
                        || disk.1.as_ref().map(|t| &t.0) != Some(&clean)
                    {
                        file.launch.as_mut().unwrap().error =
                            Some(file.ui.text(Label::FileChangedOnDisk).to_owned());
                        cx.notify();
                        return;
                    }
                    file.submit_validated_task(window, cx);
                })
            });
        }));
    }

    fn submit_validated_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.launch.as_ref() else {
            return;
        };
        let task = form.tasks[form.selected].clone();
        let values = form.values.clone();
        let manager = form.manager.clone();
        let launch = (|| {
            if self.changed_on_disk || self.relocating || self.read_only {
                return Err(self.ui.text(Label::FileChangedOnDisk).to_owned());
            }
            let mut args = Vec::new();
            for (parameter, value) in task.parameters.iter().zip(&values) {
                if value.is_empty() {
                    if parameter.required {
                        return Err(format!(
                            "{}: {}",
                            self.ui.text(Label::RunArgumentsRequired),
                            parameter.name
                        ));
                    }
                    break;
                }
                if parameter.variadic {
                    args.extend(source::tokens(value)?);
                } else {
                    args.push(value.clone());
                }
            }
            if matches!(task.identity.runner, Runner::Make | Runner::Task) {
                args = source::tokens(values.last().map_or("", String::as_str))?;
            }
            task.command(&args, manager.as_deref())?;
            self.save(cx)?;
            Ok(args)
        })();
        let args = match launch {
            Ok(args) => args,
            Err(error) => {
                self.launch.as_mut().unwrap().error = Some(error);
                cx.notify();
                return;
            }
        };
        self.launch_arguments.insert(
            task.identity.clone(),
            Arguments {
                parameters: task.parameters.clone(),
                values,
                manager: manager.clone(),
            },
        );
        let root = self.root.clone();
        let path = self.relative.clone();
        let source = self.clean.clone();
        let expected_path = path.clone();
        let expected_source = source.clone();
        self.launch = None;
        window.focus(&self.focus_handle(cx), cx);
        cx.spawn(async move |file, cx| {
            let current = cx
                .background_executor()
                .spawn(async move {
                    if read(&root.join(&path)).1.map(|t| t.0).as_deref() != Some(&source) {
                        return None;
                    }
                    let current = crate::run::discover::tasks(&root)
                        .into_iter()
                        .find(|t| {
                            t.identity.source == task.identity.source
                                && t.identity.runner == task.identity.runner
                                && (t.identity.name == task.identity.name
                                    || task.identity.name.is_empty())
                                && t.parameters == task.parameters
                        })
                        .and_then(|current| {
                            Some(Invocation {
                                cwd: current.cwd(&root),
                                line: if task.identity.name.is_empty() {
                                    task.command(&args, manager.as_deref()).ok()?
                                } else {
                                    current.command(&args, manager.as_deref()).ok()?
                                },
                                task: Some(task.identity),
                            })
                        });
                    if read(&root.join(&path)).1.map(|t| t.0).as_deref() != Some(&source) {
                        return None;
                    }
                    current
                })
                .await;
            let _ = file.update(cx, |file, cx| {
                let buffer = file
                    .editor
                    .as_ref()
                    .map(|e| e.read(cx).text().to_string())
                    .or_else(|| file.pending.clone())
                    .unwrap_or_else(|| file.clean.clone());
                if file.relative != expected_path
                    || buffer != expected_source
                    || file.clean != expected_source
                    || file.changed_on_disk
                    || file.read_only
                    || file.relocating
                {
                    file.write_failed = Some(file.ui.text(Label::RunSourceChanged).to_owned());
                    cx.notify();
                    return;
                }
                match current {
                    Some(invocation) => cx.emit(LaunchRun(invocation)),
                    None => {
                        file.write_failed = Some(file.ui.text(Label::RunSourceChanged).to_owned())
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn render_launch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let form = self.launch.as_mut()?;
        if form.needs_focus {
            window.focus(&form.focus, cx);
            form.needs_focus = false;
        }
        let theme = self.look.theme;
        let ui = self.ui;
        let task = &form.tasks[form.selected];
        let weak = cx.weak_entity();
        let rows = form
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let painted_task = t.clone();
                let painted_form = form.focus.clone();
                div()
                    .id(("task-choice", i))
                    .debug_selector(move || format!("task-choice-{i}"))
                    .px(theme::space::CARD_PADDING)
                    .py(theme::space::CARD_GAP)
                    .cursor_pointer()
                    .when(i == form.selected, |s| s.bg(rgb(theme.surface.raised)))
                    .child(if t.identity.name.is_empty() {
                        ui.text(Label::RunDefault).to_owned()
                    } else {
                        t.identity.name.clone()
                    })
                    .on_click(cx.listener(move |file, _, window, cx| {
                        let task = file
                            .launch
                            .as_ref()
                            .filter(|form| form.focus == painted_form)
                            .and_then(|form| form.tasks.get(i))
                            .filter(|task| **task == painted_task)
                            .cloned();
                        if let Some(task) = task {
                            file.activate_task(vec![task], false, window, cx);
                        }
                    }))
            })
            .collect::<Vec<_>>();
        let mut fields = Vec::new();
        for (i, p) in task.parameters.iter().enumerate() {
            let weak = weak.clone();
            fields.push(
                div()
                    .child(format!("{}{}", p.name, if p.required { " *" } else { "" }))
                    .child(
                        div()
                            .debug_selector(|| "task-parameter-field".into())
                            .rounded(theme::radius::ROW)
                            .bg(rgb(theme.surface.inset))
                            .border_1()
                            .border_color(rgb(theme.surface.divider))
                            .px(theme::space::CARD_GAP)
                            .child(
                                TextField::new(
                                    ("task-arg", i),
                                    form.values[i].clone(),
                                    move |value, _, cx| {
                                        let _ = weak.update(cx, |file, cx| {
                                            if let Some(form) = file.launch.as_mut() {
                                                form.values[i] = value.clone();
                                            }
                                            cx.notify();
                                        });
                                    },
                                )
                                .when(i == 0, |field| field.autofocus()),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if matches!(task.identity.runner, Runner::Make | Runner::Task) {
            let i = task.parameters.len();
            let weak = weak.clone();
            fields.push(
                div()
                    .rounded(theme::radius::ROW)
                    .bg(rgb(theme.surface.inset))
                    .border_1()
                    .border_color(rgb(theme.surface.divider))
                    .px(theme::space::CARD_GAP)
                    .child(
                        TextField::new("task-vars", form.values[i].clone(), move |value, _, cx| {
                            let _ = weak.update(cx, |file, cx| {
                                if let Some(form) = file.launch.as_mut() {
                                    form.values[i] = value.clone();
                                }
                                cx.notify();
                            });
                        })
                        .placeholder(ui.text(Label::RunVariables))
                        .when(task.parameters.is_empty(), |field| field.autofocus()),
                    )
                    .into_any_element(),
            );
        }
        Some(
            crate::view::popup::card(theme)
                .p(theme::space::CARD_PADDING)
                .gap(theme::space::CARD_GAP)
                .absolute()
                .top(theme::space::CARD_PADDING)
                .right(theme::space::CARD_PADDING)
                .w(theme::space::RUN_COMMAND)
                .max_h_full()
                .id("task-launch-form")
                .track_focus(&form.focus)
                .overflow_y_scroll()
                .capture_key_down(cx.listener(|file, event: &gpui::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            file.launch = None;
                            window.focus(&file.focus_handle(cx), cx);
                            cx.notify();
                            cx.stop_propagation();
                        }
                        "enter" => {
                            file.submit_task(window, cx);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                }))
                .child(if form.tasks.len() > 1 {
                    ui.text(Label::RunChoose).to_owned()
                } else {
                    format!(
                        "{}: {} {}",
                        ui.text(Label::RunTaskSettings),
                        task.identity.runner.program(),
                        task.identity.name
                    )
                })
                .when(form.tasks.len() > 1, |card| card.children(rows))
                .children(fields)
                .when(
                    task.identity.runner == Runner::Package && task.managers.len() != 1,
                    |card| {
                        card.children(["npm", "pnpm", "yarn", "bun"].into_iter().map(|manager| {
                            div()
                                .id(manager)
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap(theme::space::STATUS_BUTTON_GAP)
                                .when(form.manager.as_deref() == Some(manager), |row| {
                                    row.child(
                                        crate::icons::glyph("ui/check.svg", theme::size::UI)
                                            .text_color(rgb(theme.text.primary)),
                                    )
                                })
                                .child(manager)
                                .on_click(cx.listener(move |file, _, _, cx| {
                                    if let Some(form) = file.launch.as_mut() {
                                        form.manager = Some(manager.into());
                                    }
                                    cx.notify();
                                }))
                        }))
                    },
                )
                .children(form.error.clone())
                .child(
                    div()
                        .flex()
                        .gap(theme::space::CARD_GAP)
                        .child(
                            div()
                                .id("task-submit")
                                .cursor_pointer()
                                .child(ui.text(Label::RunStart))
                                .on_click(
                                    cx.listener(|file, _, window, cx| file.submit_task(window, cx)),
                                ),
                        )
                        .child(
                            div()
                                .id("task-cancel")
                                .cursor_pointer()
                                .child(ui.text(Label::Cancel))
                                .on_click(cx.listener(|file, _, window, cx| {
                                    file.launch = None;
                                    window.focus(&file.focus_handle(cx), cx);
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
    pub(super) fn task_gutter(
        &self,
        body: gpui::AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(editor) = self.editor.clone() else {
            return body;
        };
        use gpui_component::RopeExt as _;
        let run_width = if self.tasks.is_empty() || self.read_only {
            px(0.)
        } else {
            theme::space::ICON.to_pixels(window.rem_size())
        };
        let blame_width = self.blame_width(window);
        let font = gpui::Font {
            family: self.look.family.clone(),
            ..window.text_style().font()
        };
        let number_padding = theme::space::CARD_GAP.to_pixels(window.rem_size()) / 2.;
        let number_width = gutter::text_width(
            &editor.read(cx).text().lines_len().to_string(),
            px(self.look.font_size),
            font.clone(),
            window,
        ) + number_padding * 2.;
        let marker_lane = self.git_marker_lane(window, cx);
        let marker_width = marker_lane.as_ref().map_or(px(0.), |lane| lane.width);
        let width = blame_width + number_width + run_width + marker_width;
        let theme = self.look.theme;
        let ui = self.ui;
        let mut rows = std::collections::BTreeMap::<usize, Vec<Task>>::new();
        for task in &self.tasks {
            if !self.read_only {
                rows.entry(task.row).or_default().push(task.clone());
            }
        }
        let mut cells: Vec<_> = rows
            .into_iter()
            .map(|(row, tasks)| {
                let rendered_text = editor.read(cx).text().clone();
                let tip = format!(
                    "{} {}\n{}",
                    ui.text(Label::RunStart),
                    tasks
                        .iter()
                        .map(|t| t.identity.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    ui.text(Label::RunConfigure)
                );
                (
                    row,
                    div()
                        .id(gpui::SharedString::from(format!("editor-run-{row}")))
                        .debug_selector(|| "editor-task-control".into())
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hint_delay()
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::view::tooltip::Tooltip::details(tip.clone(), theme))
                                .into()
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(
                            cx.listener(move |file, event: &gpui::ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if !file.gutter_row_contains(row, event.position(), window, cx) {
                                    return;
                                }
                                if !file
                                    .editor
                                    .as_ref()
                                    .is_some_and(|editor| editor.read(cx).text() == &rendered_text)
                                    || !tasks.iter().all(|task| {
                                        file.tasks
                                            .iter()
                                            .any(|current| current.row == row && current == task)
                                    })
                                {
                                    return;
                                }
                                file.activate_task(
                                    tasks.clone(),
                                    event.modifiers().alt,
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .child(
                            crate::icons::toolbar("ui/tool-run.svg")
                                .text_color(rgb(theme.accent.patina)),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        let mut controls: std::collections::BTreeMap<_, _> = cells.into_iter().collect();
        let state = editor.read(cx);
        let visible = state.visible_row_range().unwrap_or(0..0);
        cells = visible
            .map(|row| {
                let control = controls.remove(&row);
                let blame = self.blame_enabled().then(|| self.blame_cell(row, cx));
                let rendered_text = editor.read(cx).text().clone();
                let number = div()
                    .debug_selector(|| "editor-line-number".into())
                    .id(("editor-number", row))
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px(number_padding)
                    // This transparent cell owns selection only. Numbers are
                    // painted separately using the current editor geometry.
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |file, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if !file.gutter_row_contains(row, event.position, window, cx) {
                                return;
                            }
                            let Some(editor) = file.editor.as_ref() else {
                                return;
                            };
                            if editor.read(cx).text() != &rendered_text {
                                return;
                            }
                            editor.update(cx, |state, cx| {
                                let start = state.text().line_start_offset(row);
                                let end = if row + 1 < state.text().lines_len() {
                                    state.text().line_start_offset(row + 1)
                                } else {
                                    state.text().len()
                                };
                                let range = if event.modifiers.shift {
                                    let old = state.selected_range();
                                    old.start.min(start)..old.end.max(end)
                                } else {
                                    start..end
                                };
                                state.set_selected_range(range, cx);
                                window.focus(&state.focus_handle(cx), cx);
                            });
                        }),
                    );
                (
                    row,
                    div()
                        .size_full()
                        .flex()
                        .on_scroll_wheel(cx.listener(Self::scroll_gutter))
                        .child(div().flex_none().w(blame_width).h_full().children(blame))
                        .child(div().flex_none().w(number_width).h_full().child(number))
                        .child(div().flex_none().w(run_width).h_full().children(control))
                        .child(div().flex_none().w(marker_width).h_full())
                        .into_any_element(),
                )
            })
            .collect();
        let body = div()
            .size_full()
            .flex()
            .child(
                div()
                    .flex_none()
                    .w(width)
                    .on_scroll_wheel(cx.listener(Self::scroll_gutter)),
            )
            .child(div().flex_1().min_w_0().h_full().child(body))
            .into_any_element();
        gutter::Gutter::new(
            body,
            editor,
            self.gutter_snapshot.clone(),
            self.look.font_size,
            width,
            cells,
            gutter::Numbers {
                font,
                size: px(self.look.font_size),
                start: blame_width,
                width: number_width,
                padding: number_padding,
                normal: rgb(theme.text.faint).into(),
                current: rgb(theme.text.primary).into(),
            },
        )
        .git_markers(marker_lane)
        .on_git_marker(self.git_marker_click(cx))
        .into_any_element()
    }
}
