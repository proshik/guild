//! Update controls share the same global source owner in both Settings hosts.
use super::choices::Value;
use crate::{
    Root,
    i18n::Text,
    theme,
    update::{remote, runtime, selection},
};
use gpui::{Context, SharedString, div, prelude::*, rgb};
use guild_session::update::{BuildInfo, PreferenceState, Provenance, Source};
use std::path::PathBuf;
#[derive(Default)]
pub(crate) struct Draft {
    details: bool,
    notes: bool,
    category: Option<guild_session::changelog::Group>,
    notes_scroll: gpui::ScrollHandle,
    focus: std::cell::OnceCell<gpui::FocusHandle>,
    directory: Option<String>,
    invalid: bool,
    picker: Option<gpui::Task<()>>,
}
impl Draft {
    pub(crate) fn holds_focus(&self, window: &gpui::Window, cx: &gpui::App) -> bool {
        self.focus
            .get()
            .is_some_and(|focus| focus.contains_focused(window, cx))
    }
}
impl Root {
    fn commit_update_directory(&mut self, cx: &mut Context<Self>) {
        let Some(value) = self.update_settings.directory.clone() else {
            return;
        };
        if !self.update_source_editable() {
            return;
        }
        let path = (!value.trim().is_empty()).then(|| PathBuf::from(&value));
        if path.as_ref().is_some_and(|path| !path.is_absolute()) {
            self.update_settings.invalid = true;
            cx.notify();
            return;
        }
        let mut preferences = self.update_preferences();
        preferences.local_directory = path;
        self.set_update_preferences(preferences, cx);
        self.update_settings.directory = None;
        self.update_settings.invalid = false;
        cx.notify();
    }
    pub(super) fn update_directory_control(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let preferences = self.update_preferences();
        let enabled = self.update_source_editable();
        let model = preferences
            .local_directory
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let value = self.update_settings.directory.clone().unwrap_or(model);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .child(
                        div()
                            .flex_1()
                            .min_w(theme::settings::LABEL_MIN)
                            .debug_selector(|| "update-directory-field".into())
                            .child(
                                crate::text_field::TextField::new(
                                    "update-directory-input",
                                    value,
                                    cx.listener(|root, value: &String, _, cx| {
                                        root.update_settings.directory = Some(value.clone());
                                        root.update_settings.invalid = false;
                                        cx.notify();
                                    }),
                                )
                                .disabled(!enabled)
                                .vertical_scroll()
                                .placeholder(self.ui().text(Text::UpdaterDefaultDirectory))
                                .on_blur(
                                    cx.listener(|root, _, _, cx| root.commit_update_directory(cx)),
                                )
                                .on_key(cx.listener(
                                    |root, event: &gpui::KeyDownEvent, window, cx| {
                                        match event.keystroke.key.as_str() {
                                            "enter" => {
                                                root.commit_update_directory(cx);
                                                cx.stop_propagation();
                                            }
                                            "escape" => {
                                                root.update_settings.directory = None;
                                                root.update_settings.invalid = false;
                                                window.focus(&root.focus, cx);
                                                cx.notify();
                                                cx.stop_propagation();
                                            }
                                            "tab" => {
                                                if event.keystroke.modifiers.shift {
                                                    window.focus_prev(cx);
                                                } else {
                                                    window.focus_next(cx);
                                                }
                                            }
                                            _ => {}
                                        }
                                    },
                                )),
                            ),
                    )
                    .child(
                        setting_button(
                            self.theme,
                            "update-directory-browse",
                            self.ui().text(Text::CloneChoose).into(),
                            false,
                        )
                        .when(!enabled, |button| button.opacity(0.4))
                        .on_click(cx.listener(|root, _, _, cx| {
                            if !root.update_source_editable()
                                || root
                                    .update_settings
                                    .picker
                                    .as_ref()
                                    .is_some_and(|task| !task.is_ready())
                            {
                                return;
                            }
                            let selected = root.update_source_epoch();
                            let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: false,
                                prompt: None,
                            });
                            root.update_settings.picker = Some(cx.spawn(async move |root, cx| {
                                let Ok(Ok(Some(paths))) = chosen.await else {
                                    return;
                                };
                                let Some(path) = paths.into_iter().next() else {
                                    return;
                                };
                                let _ = root.update(cx, |root, cx| {
                                    if root.update_source_epoch() != selected
                                        || !root.update_source_editable()
                                    {
                                        return;
                                    }
                                    root.update_settings.directory =
                                        Some(path.to_string_lossy().into_owned());
                                    root.commit_update_directory(cx);
                                });
                            }));
                        })),
                    ),
            )
            .when(self.update_settings.invalid, |row| {
                row.child(self.update_detail(self.ui().text(Text::UpdaterAbsolutePath).into()))
            })
            .child(
                self.update_detail(crate::short_path(
                    &preferences
                        .local_directory
                        .unwrap_or_else(|| self.updater_default_directory()),
                )),
            )
            .into_any_element()
    }
    fn update_detail(&self, value: String) -> gpui::AnyElement {
        div()
            .min_w_0()
            .text_size(theme::size::META)
            .text_color(rgb(self.theme.text.secondary))
            .child(value)
            .into_any_element()
    }
    fn update_card_row(
        &self,
        title: Text,
        description: String,
        control: gpui::AnyElement,
    ) -> gpui::AnyElement {
        self.update_card_content_row(title, self.update_detail(description), control)
    }
    fn update_card_content_row(
        &self,
        title: Text,
        description: gpui::AnyElement,
        control: gpui::AnyElement,
    ) -> gpui::AnyElement {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .p(theme::space::CARD_PADDING)
            .border_b_1()
            .border_color(theme::settings::rule(self.theme))
            .child(
                div()
                    .flex_1()
                    .min_w(theme::settings::LABEL_MIN)
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .child(self.ui().text(title))
                    .child(description),
            )
            .child(control)
            .into_any_element()
    }
    fn update_build_detail(&self, info: &BuildInfo) -> String {
        let producer = self.ui().text(match info.provenance {
            Provenance::Local => Text::UpdaterLocalProducer,
            Provenance::GithubActions => Text::UpdaterCiProducer,
        });
        let built = i64::try_from(info.built)
            .ok()
            .and_then(|n| time::OffsetDateTime::from_unix_timestamp(n).ok())
            .map(|t| {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02} UTC",
                    t.year(),
                    t.month() as u8,
                    t.day(),
                    t.hour(),
                    t.minute()
                )
            })
            .unwrap_or_else(|| "—".into());
        format!("{producer} · {built}\n{}\n{}", info.commit, info.build_id)
    }
    pub(super) fn update_status_control(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let ui = self.ui();
        let ready = self.update_ready_to_check();
        let busy = self.update_installing();
        let mut rows = Vec::new();
        match self.update_preference_status() {
            Some(selection::Status::Ready) => {}
            Some(selection::Status::Failed) => {
                rows.push(self.update_detail(ui.text(Text::UpdaterSaveFailed).into()));
                rows.push(
                    setting_button(
                        self.theme,
                        "update-preference-retry",
                        ui.text(Text::ScopedRetry).into(),
                        false,
                    )
                    .on_click(cx.listener(|root, _, _, cx| {
                        root.retry_update_preferences(cx);
                    }))
                    .into_any_element(),
                );
            }
            Some(selection::Status::Recovery(PreferenceState::Recovery {
                operation_id,
                old,
                requested,
                ..
            })) => {
                let operation_id = *operation_id;
                rows.push(self.update_detail(format!(
                    "{}\n{} / {} → {} / {}",
                    ui.text(Text::UpdaterPreferenceRecovery),
                    old.source.name(),
                    old.channel.name(),
                    requested.source.name(),
                    requested.channel.name()
                )));
                rows.push(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(theme::space::CARD_GAP)
                        .children(
                            [
                                (true, Text::UpdaterKeepRequested),
                                (false, Text::UpdaterKeepPrevious),
                            ]
                            .into_iter()
                            .map(|(keep, text)| {
                                setting_button(
                                    self.theme,
                                    if keep {
                                        "update-keep-requested"
                                    } else {
                                        "update-keep-previous"
                                    },
                                    ui.text(text).into(),
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |root, _, _, cx| {
                                        root.resolve_update_preferences(operation_id, keep, cx);
                                    },
                                ))
                            }),
                        )
                        .into_any_element(),
                );
            }
            Some(selection::Status::Saving | selection::Status::Resolving) => {
                rows.push(self.update_detail(ui.text(Text::UpdaterSaving).into()))
            }
            _ => rows.push(self.update_detail(ui.text(Text::UpdaterLoading).into())),
        }
        if let Some(receipt) = self.update_recovery() {
            let waiting = self.update_recovery_waiting();
            rows.push(
                self.update_detail(
                    ui.text(if waiting {
                        Text::UpdaterJournalWaiting
                    } else if receipt.phase()
                        == guild_session::update::transaction::Phase::Acknowledged
                    {
                        Text::UpdaterCleanupRecovery
                    } else {
                        Text::UpdaterRecoveryError
                    })
                    .into(),
                ),
            );
            rows.push(self.update_detail(format!(
                "{}: {}\n{}: {}",
                ui.text(Text::UpdaterDestination),
                receipt.destination().display(),
                ui.text(Text::UpdaterPrevious),
                receipt.previous_path().display()
            )));
            if !waiting && let Some(command) = self.update_recovery_command() {
                rows.push(self.update_detail(ui.text(Text::UpdaterRecoveryHelp).into()));
                rows.push(self.update_detail(command.clone()));
                rows.push(
                    setting_button(
                        self.theme,
                        "update-copy-recovery",
                        ui.text(Text::UpdaterCopyRecovery).into(),
                        false,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(command.clone()))
                    }))
                    .into_any_element(),
                );
            }
        }
        if self.update_recovery_failure().is_some() {
            rows.push(self.update_detail(ui.text(Text::UpdaterRecoveryError).into()));
            rows.push(
                setting_button(
                    self.theme,
                    "update-journal-retry",
                    ui.text(Text::ScopedRetry).into(),
                    false,
                )
                .on_click(cx.listener(|root, _, _, cx| root.retry_update_recovery(cx)))
                .into_any_element(),
            );
        }
        // A negative discovery status needs evidence for every candidate, not
        // merely the absence of a proven newer revision. Local keeps its policy.
        let no_newer_known = self.update_preferences().source == Source::Local
            || self.running_update_info().is_some_and(|running| {
                self.update_candidates().iter().all(|candidate| {
                    candidate.info.as_ref().is_some_and(
                        |info| match guild_session::update::compare(Some(running), info, false) {
                            Ok(guild_session::update::Comparison::Installed) => true,
                            Ok(guild_session::update::Comparison::Replacement) => {
                                running.commit == info.commit
                                    || matches!((running.revision_order, info.revision_order),
                                        (Some(a), Some(b)) if b < a)
                            }
                            _ => false,
                        },
                    )
                })
            });
        if let Some(view) = self.selected_update() {
            let latest = self
                .update_candidates()
                .first()
                .is_some_and(|first| first.identity == view.identity);
            rows.push(
                div()
                    .child(ui.text(if latest {
                        Text::UpdaterLatest
                    } else {
                        Text::UpdaterCandidate
                    }))
                    .child(
                        self.update_detail(
                            view.build
                                .version()
                                .unwrap_or(view.build.short())
                                .to_owned(),
                        ),
                    )
                    .into_any_element(),
            );
            if no_newer_known
                && view
                    .info
                    .as_ref()
                    .is_some_and(|info| self.running_update_info() == Some(info))
            {
                rows.push(
                    div()
                        .debug_selector(|| "update-installed-status".into())
                        .child(self.update_detail(ui.text(Text::UpdaterUpToDate).into()))
                        .into_any_element(),
                );
            } else if self.update_preferences().source == Source::Remote
                && self.running_update_info().is_some()
                && no_newer_known
            {
                rows.push(
                    div()
                        .debug_selector(|| "update-no-newer-status".into())
                        .child(self.update_detail(ui.text(Text::UpdaterNoNewer).into()))
                        .into_any_element(),
                );
            }
        } else if ready && !self.update_checking() && self.update_failure().is_none() {
            rows.push(self.update_detail(ui.text(Text::UpdaterEmpty).into()));
        }
        if self.update_checking() {
            rows.push(self.update_detail(ui.text(Text::UpdaterChecking).into()));
        }
        if busy {
            rows.push(self.update_detail(ui.text(Text::UpdaterInstalling).into()));
        }
        if let Some(progress) = self.update_progress() {
            let text = match progress {
                remote::Progress::Verifying => ui.text(Text::UpdaterVerifying).into(),
                remote::Progress::Downloading { bytes, total } => format!(
                    "{} · {:.1} / {:.1} MiB",
                    ui.text(Text::UpdaterDownload),
                    *bytes as f64 / 1048576.,
                    *total as f64 / 1048576.
                ),
            };
            rows.push(self.update_detail(text));
        }
        if let Some(error) = self.update_failure() {
            rows.push(self.update_detail(update_error(error, ui)));
        }
        if self.update_warning().is_some() {
            rows.push(self.update_detail(ui.text(Text::UpdaterCacheWarning).into()));
        }
        if self.unverified_update_releases() > 0 {
            rows.push(self.update_detail(ui.text(Text::UpdaterUnverified).into()));
        }
        let reinstall = self
            .selected_update()
            .and_then(|view| view.info.as_ref())
            .is_some_and(|info| self.running_update_info() == Some(info));
        let active = self.update_checking() || self.update_downloading() || busy;
        let replaceable = self.selected_update().is_some()
            && !reinstall
            && self.update_failure().is_none()
            && (self.update_preferences().source == Source::Local
                || self.update.staged().is_some());
        let install = ready && !active && replaceable;
        let action = if self.update_download_ready() {
            Text::UpdaterRestart
        } else {
            Text::UpdaterDownload
        };
        let button = if active {
            setting_button(
                self.theme,
                "update-cancel",
                ui.text(Text::Cancel).into(),
                false,
            )
            .debug_selector(|| "update-cancel".into())
            .disabled(!ready)
            .on_click(cx.listener(|root, _, _, cx| root.cancel_update(cx)))
        } else if install {
            setting_button(self.theme, "update-install", ui.text(action).into(), false)
                .debug_selector(|| "update-install".into())
                .on_click(cx.listener(move |root, _, window, cx| {
                    root.apply_source_update(reinstall, window, cx)
                }))
        } else {
            setting_button(
                self.theme,
                "update-check",
                ui.text(Text::UpdaterCheck).into(),
                false,
            )
            .debug_selector(|| "update-check".into())
            .disabled(!ready)
            .on_click(cx.listener(|root, _, _, cx| {
                if root.update_ready_to_check() && !root.update_installing() {
                    root.check_source_update(cx);
                }
            }))
        };
        let status = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .children(rows)
            .into_any_element();
        let installed = self
            .running_update_info()
            .map(|info| {
                let build = crate::update::Build::from(info);
                build.version().unwrap_or(build.short()).to_owned()
            })
            .or_else(|| {
                self.running_update_build()
                    .map(|build| build.version().unwrap_or(build.short()).to_owned())
            })
            .unwrap_or_else(|| "—".into());
        let mut rows = vec![
            self.update_card_row(
                Text::UpdaterCurrentVersion,
                installed,
                self.update_detail(ui.text(Text::UpdaterInstalled).into()),
            ),
            self.update_card_row(
                Text::UpdaterChannel,
                ui.text(Text::UpdaterChannelHint).into(),
                self.settings_choice(
                    "setting-update-channel",
                    Value::UpdateChannel(self.update_preferences().channel),
                    cx,
                ),
            ),
            self.update_card_content_row(Text::UpdaterStatus, status, button.into_any_element()),
        ];
        if let Some(view) = self.selected_update().filter(|view| !view.notes.is_empty()) {
            let notes = crate::view::release_notes::parse_body(&view.notes);
            rows.push(
                div()
                    .w_full()
                    .border_b_1()
                    .border_color(theme::settings::rule(self.theme))
                    .child(
                        disclosure_row(
                            self.theme,
                            "update-notes-toggle",
                            ui.text(Text::UpdaterNotes).into(),
                            self.update_settings.notes,
                        )
                        .debug_selector(|| "update-notes-toggle".into())
                        .on_click(cx.listener(|root, _, _, cx| {
                            root.update_settings.notes = !root.update_settings.notes;
                            cx.notify();
                        })),
                    )
                    .into_any_element(),
            );
            if self.update_settings.notes {
                let selected =
                    crate::view::release_notes::selected(&notes, self.update_settings.category);
                let mut body = div()
                    .debug_selector(|| "update-notes-body".into())
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_PADDING)
                    .p(theme::space::CARD_PADDING)
                    .border_b_1()
                    .border_color(theme::settings::rule(self.theme));
                if !notes.is_empty() {
                    let root = cx.entity().downgrade();
                    body = body.child(crate::view::release_notes::categories(
                        &notes,
                        selected,
                        self.theme,
                        ui,
                        "update-notes",
                        move |group| {
                            let root = root.clone();
                            Box::new(move |_, _, cx| {
                                let _ = root.update(cx, |root, cx| {
                                    root.update_settings.category = Some(group);
                                    root.update_settings
                                        .notes_scroll
                                        .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
                                    cx.notify();
                                });
                            })
                        },
                    ));
                }
                body = body.child(
                    crate::view::release_notes::scroll_content(
                        "settings-notes-scroll",
                        &self.update_settings.notes_scroll,
                        crate::view::release_notes::content(
                            &notes,
                            selected,
                            &view.notes,
                            self.theme,
                            "update-notes",
                        ),
                        self.theme,
                        crate::scale::rem_size(self.settings.ui_font_size),
                        gpui::rems(0.),
                    )
                    .h(theme::space::WHATS_NEW_LIST),
                );
                rows.push(body.into_any_element());
            }
        }
        rows.push(
            disclosure_row(
                self.theme,
                "update-details-toggle",
                ui.text(Text::UpdaterDetails).into(),
                self.update_settings.details,
            )
            .debug_selector(|| "update-details-toggle".into())
            .on_click(cx.listener(|root, _, _, cx| {
                root.update_settings.details = !root.update_settings.details;
                cx.notify();
            }))
            .into_any_element(),
        );
        if self.update_settings.details {
            let mut details = div()
                .id("update-technical-details")
                .debug_selector(|| "update-technical-details".into())
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(theme::space::CARD_GAP);
            details = details
                .child(self.update_card_row(
                    Text::UpdaterSource,
                    ui.text(Text::UpdaterSourceHint).into(),
                    self.settings_choice(
                        "setting-update-source",
                        Value::UpdateSource(self.update_preferences().source),
                        cx,
                    ),
                ))
                .child(self.update_card_row(
                    Text::UpdaterDirectory,
                    ui.text(Text::UpdaterDirectoryHint).into(),
                    self.update_directory_control(cx),
                ));
            // Deliberate rollback/rebuild/reinstall remains reachable without a newer offer.
            if ready
                && !active
                && !replaceable
                && self.selected_update().is_some()
                && self.update_failure().is_none()
            {
                details = details.child(
                    setting_button(
                        self.theme,
                        "update-replace",
                        ui.text(if !self.update_download_ready() {
                            Text::UpdaterDownload
                        } else if reinstall {
                            Text::UpdaterReinstall
                        } else {
                            Text::UpdaterRestart
                        })
                        .into(),
                        false,
                    )
                    .on_click(cx.listener(move |root, _, window, cx| {
                        root.apply_source_update(reinstall, window, cx)
                    })),
                );
            }
            let mut technical = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(theme::space::CARD_GAP)
                .p(theme::space::CARD_PADDING);
            if self.update_preferences().source == Source::Remote {
                technical =
                    technical.child(self.update_detail(ui.text(Text::UpdaterPublicHelp).into()));
            }
            if let Some(info) = self.running_update_info() {
                technical = technical.child(self.update_detail(format!(
                    "{}\n{}",
                    ui.text(Text::UpdaterInstalled),
                    self.update_build_detail(info)
                )));
            } else {
                technical =
                    technical.child(self.update_detail(ui.text(Text::UpdaterLegacyInfo).into()));
            }
            if let Some(view) = self.selected_update() {
                technical = technical.child(self.update_detail(view.build.label()));
                if let Some(info) = &view.info {
                    technical = technical.child(self.update_detail(self.update_build_detail(info)));
                }
            }
            if self.update_candidates().len() > 1 {
                technical =
                    technical.child(self.update_detail(ui.text(Text::UpdaterHistory).into()));
                for (index, view) in self.update_candidates().iter().enumerate() {
                    let identity = view.identity.clone();
                    technical = technical.child(
                        setting_button(
                            self.theme,
                            SharedString::from(format!("update-history-{index}")),
                            view.build.artifact_label(view.info.as_ref()),
                            self.selected_update()
                                .is_some_and(|selected| selected.identity == identity),
                        )
                        .w_full()
                        .min_w_0()
                        .justify_start()
                        .on_click(cx.listener(move |root, _, _, cx| {
                            root.select_update(&identity, cx);
                        }))
                        .into_any_element(),
                    );
                }
            }
            rows.push(details.child(technical).into_any_element());
        }
        div()
            .track_focus(
                self.update_settings
                    .focus
                    .get_or_init(|| cx.focus_handle().tab_stop(false)),
            )
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }
}
fn update_error(error: &runtime::Failure, ui: crate::i18n::Ui) -> String {
    use crate::update::preparation;
    use guild_session::update::transaction;
    let text = match error {
        runtime::Failure::Remote(remote::Error::MissingGh | remote::Error::Access) => {
            Text::UpdaterAccessError
        }
        runtime::Failure::Remote(remote::Error::Network) => Text::UpdaterNetworkError,
        runtime::Failure::Remote(remote::Error::RateLimited(_)) => Text::UpdaterRateError,
        runtime::Failure::Remote(remote::Error::Storage | remote::Error::Space)
        | runtime::Failure::Preparation(
            preparation::Failure::Storage | preparation::Failure::Space,
        ) => Text::UpdaterStorageError,
        runtime::Failure::Transaction(
            transaction::Failure::MissingAck
            | transaction::Failure::InvalidAck
            | transaction::Failure::Conflict
            | transaction::Failure::Interrupted,
        ) => Text::UpdaterRecoveryError,
        _ => Text::UpdaterVerificationError,
    };
    let mut value = ui.text(text).to_owned();
    if let runtime::Failure::Remote(remote::Error::RateLimited(reset)) = error
        && let Some(time) = i64::try_from(*reset)
            .ok()
            .and_then(|n| time::OffsetDateTime::from_unix_timestamp(n).ok())
    {
        value.push_str(&format!(
            "\n{:04}-{:02}-{:02} {:02}:{:02} UTC",
            time.year(),
            time.month() as u8,
            time.day(),
            time.hour(),
            time.minute()
        ));
    }
    value
}

// Native buttons provide Enter/Space activation and visible focus for disclosures.
fn setting_button(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
    on: bool,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .min_h(theme::settings::CONTROL)
        .px(theme::space::CARD_GAP)
        .rounded(theme::radius::ROW)
        .border_1()
        .border_color(theme::settings::outline(palette))
        .bg(if on {
            rgb(palette.surface.raised).into()
        } else {
            gpui::transparent_black()
        })
        .text_color(rgb(if on {
            palette.accent.patina
        } else {
            palette.text.primary
        }))
        .text_size(theme::size::BODY)
        .hover(|style| style.bg(theme::control_hover(palette, palette.surface.inset)))
        .focus(|style| style.border_color(rgb(palette.accent.patina)))
        .child(label)
}

// Keep native activation and focus while sharing the two disclosure rows' geometry.
fn disclosure_row(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
    expanded: bool,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .w_full()
        .min_h(theme::settings::CONTROL)
        .px(theme::space::CARD_PADDING)
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded_none()
        .bg(gpui::transparent_black())
        .text_size(theme::size::BODY)
        .text_color(rgb(palette.text.primary))
        .justify_start()
        .hover(|style| style.bg(theme::control_hover(palette, palette.surface.inset)))
        .focus(|style| style.border_color(rgb(palette.accent.patina)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme::space::CARD_GAP)
                .child(
                    div()
                        .flex_none()
                        .size(theme::space::ICON)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            crate::icons::chevron(expanded).text_color(rgb(palette.text.secondary)),
                        ),
                )
                .child(label),
        )
}
