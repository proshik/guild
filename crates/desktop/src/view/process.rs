//! The native, graph-driven Process page. Inspection and selection own no effects.
use crate::view::tooltip::Hint as _;
use crate::{Root, i18n::Text, process_view, theme};
use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, SharedString, div, rgb};
use gpui_component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use guild_session::{
    SessionId,
    workflow::{Definition, OriginBinding, ReviewLedger, RunId},
};
use serde_json::Value;

fn handoff_text(handoff: &Value, acknowledged: bool) -> Text {
    if handoff["continued"] == true {
        Text::ProcessHandoffContinued
    } else {
        match handoff["stage"].as_str() {
            Some("reserved") => Text::ProcessHandoffReserved,
            Some("submitted") if handoff["path"] == "startup" => {
                Text::ProcessHandoffStartupSubmitted
            }
            Some("submitted") => Text::ProcessHandoffSubmitted,
            Some("observed") if acknowledged => Text::ProcessHandoffAcknowledged,
            Some("observed") => Text::ProcessHandoffObserved,
            Some("unknown") => Text::ProcessHandoffUnknown,
            _ => match handoff["reason"].as_str() {
                Some("busy_or_question") => Text::ProcessHandoffBusy,
                Some("draft_input") => Text::ProcessHandoffDraft,
                Some("user_input") => Text::ProcessHandoffInputUsed,
                Some("opencode_input_unqualified") => Text::ProcessHandoffInputUnknown,
                Some("origin_generation") => Text::ProcessHandoffHolder,
                Some("covered_or_sender") => Text::ProcessHandoffCovered,
                Some("prompt_transport_unqualified") => Text::ProcessHandoffTransport,
                Some("snapshot_recheck" | "stale_snapshot" | "stale_generation") => {
                    Text::ProcessHandoffSnapshot
                }
                _ => Text::ProcessHandoffWaiting,
            },
        }
    }
}

fn short(value: &Value) -> String {
    value
        .as_str()
        .map(|s| s.chars().take(12).collect())
        .unwrap_or_else(|| "—".into())
}
fn leaf(value: &Value) -> String {
    value
        .as_str()
        .and_then(|s| std::path::Path::new(s).file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "—".into())
}
fn state_color(t: theme::Theme, state: &str) -> u32 {
    match state {
        "finished" => t.accent.green,
        "active" | "ready" => t.accent.brass,
        "waiting" => t.accent.rust,
        _ => t.text.secondary,
    }
}
fn section(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(theme::size::BODY)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.into())
}
fn meta(t: theme::Theme, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme::space::TREE_INSET)
        .child(
            div()
                .text_size(theme::size::META)
                .text_color(rgb(t.text.secondary))
                .child(label.into()),
        )
        .child(div().whitespace_normal().child(value.into()))
}
fn muted(t: theme::Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(theme::size::META)
        .whitespace_normal()
        .text_color(rgb(t.text.secondary))
        .child(text.into())
}

impl Root {
    pub(crate) fn render_process(
        &self,
        run: RunId,
        pane: guild_session::PaneId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let ui = self.ui();
        let entry = self.processes.runs.get(&run);
        let viewport = entry.map(|entry| {
            let mut views = entry.viewports.borrow_mut();
            views.retain(|pane, _| self.tabs.iter().any(|t| matches!(t.panes.body_of(*pane), Some(crate::Body::Process {run:r}) if *r==run)));
            let view=views.entry(pane).or_default();
            view.focus.get_or_insert_with(||cx.focus_handle());
            view.clone()
        });
        let compact = viewport
            .as_ref()
            .is_none_or(|v| v.width.get() < theme::space::LOG_WIDE.0 * crate::scale::BASE);
        let mut page = div()
            .id(SharedString::from(format!("process-{run}-{pane:?}")))
            .debug_selector(|| "process-page".into())
            .tab_group()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .bg(rgb(t.surface.app))
            .text_color(rgb(t.text.primary))
            .text_size(theme::size::UI);
        if let Some(viewport) = &viewport {
            if let Some(focus) = &viewport.focus {
                page = page.track_focus(focus);
            }
            let width = viewport.width.clone();
            let owner = cx.entity_id();
            page = page.track_scroll(&viewport.scroll).child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        let measured = f32::from(bounds.size.width);
                        if width.replace(measured) != measured {
                            window.refresh();
                            cx.defer(move |cx| cx.notify(owner));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        }
        let mut body = div()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .min_w_0()
            .p(theme::space::PANEL_PADDING)
            .gap(theme::space::PANEL_PADDING);
        if let Some(gap) = entry.and_then(|e| e.gap.as_ref()) {
            body = body.child(
                div()
                    .p(theme::space::CARD_PADDING)
                    .bg(rgb(t.accent.rust_bg))
                    .rounded(theme::radius::ROW)
                    .child(section(ui.text(Text::ProcessUnavailable)))
                    .child(muted(t, gap.clone()))
                    .when(entry.is_some_and(|e| e.value.is_some()), |n| {
                        n.child(muted(t, ui.text(Text::ProcessStaleSnapshot)))
                    }),
            );
        }
        let Some(snapshot) = entry.and_then(|e| e.value.as_ref()) else {
            return page
                .child(
                    body.child(ui.text(if entry.is_some_and(|e| e.gap.is_some()) {
                        Text::ProcessUnavailable
                    } else {
                        Text::ProcessLoading
                    })),
                )
                .into_any_element();
        };
        let Ok((definition, phases)) = process_view::phases(snapshot) else {
            return page
                .child(body.child(ui.text(Text::ProcessUnavailable)))
                .into_any_element();
        };
        let selected = entry
            .and_then(|e| e.selection.step.as_deref())
            .or_else(|| process_view::current(&phases));
        let current: Vec<_> = phases
            .iter()
            .filter(|p| p.state == "active")
            .map(|p| p.step.label.as_str())
            .collect();
        let status = if snapshot["graph"]["cancelled"] == true {
            "cancelled"
        } else if !current.is_empty() {
            "active"
        } else if phases.iter().any(|p| p.state == "waiting") {
            "waiting"
        } else if !phases.is_empty()
            && phases
                .iter()
                .all(|p| matches!(p.state.as_str(), "finished" | "skipped"))
        {
            "finished"
        } else if phases.iter().any(|p| p.state == "ready") {
            "ready"
        } else {
            "unknown"
        };
        let attention = snapshot["recovery"]["state"]["parent_attention_required"] == true;
        let origin = serde_json::from_value::<OriginBinding>(snapshot["origin"].clone()).ok();
        let subtitle = format!(
            "{} · {} · {} {}",
            leaf(&snapshot["target"]),
            definition.id,
            ui.text(Text::ProcessRevision),
            entry
                .and_then(|e| e.revision)
                .map_or_else(|| "—".into(), |r| r.to_string())
        );
        body = body.child(
            div()
                .flex()
                .min_w_0()
                .gap(theme::space::CARD_PADDING)
                .when(compact, |h| h.flex_col())
                .when(!compact, |h| h.justify_between().items_start())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(theme::size::SECTION)
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .whitespace_normal()
                                .child(definition.label.clone()),
                        )
                        .child(muted(t, subtitle))
                        .child(muted(t, run.to_string())),
                )
                .child(
                    div()
                        .flex_none()
                        .px(theme::space::CARD_GAP)
                        .py(theme::space::TREE_INSET)
                        .rounded(theme::radius::ROW)
                        .bg(rgb(if attention {
                            t.accent.brass_bg
                        } else {
                            t.surface.panel
                        }))
                        .text_size(theme::size::META)
                        .text_color(rgb(if attention {
                            t.accent.brass
                        } else {
                            state_color(t, status)
                        }))
                        .child(if attention {
                            ui.text(Text::ProcessParentAttention)
                        } else {
                            ui.process_state(status)
                        }),
                ),
        );
        if !current.is_empty() {
            body = body.child(muted(
                t,
                format!(
                    "{}: {}",
                    ui.text(Text::ProcessExecuting),
                    current.join(" · ")
                ),
            ));
        }
        let mut strip = div()
            .debug_selector(|| "process-phases".into())
            .flex()
            .flex_wrap()
            .min_w_0()
            .gap(theme::space::CARD_INNER_GAP);
        for (index, phase) in phases.iter().enumerate() {
            let id = phase.step.id.clone();
            let selected = selected == Some(id.as_str());
            let state = phase.state.as_str();
            let selector = format!("process-phase-{id}");
            strip = strip.child(
                div()
                    .flex_1()
                    .min_w(theme::space::PROCESS_PHASE_MIN)
                    .when(compact, |p| p.min_w_0().w_full().flex_none())
                    .border_t_2()
                    .border_color(rgb(state_color(t, state)))
                    .child(
                        Button::new(SharedString::from(format!("process-phase-{run}-{id}")))
                            .debug_selector(move || selector.clone())
                            .accessibility_label(format!(
                                "{} · {}{}",
                                phase.step.label,
                                ui.process_state(state),
                                if selected {
                                    format!(" · {}", ui.text(Text::ProcessSelectedStep))
                                } else {
                                    String::new()
                                }
                            ))
                            .toggled(selected)
                            .small()
                            .custom(super::controls::ghost(t, selected, cx))
                            .w_full()
                            .h_auto()
                            .py(theme::space::CARD_GAP)
                            .justify_start()
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .gap(theme::space::TREE_INSET)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(theme::space::TREE_INSET)
                                            .text_size(theme::size::META)
                                            .text_color(rgb(state_color(t, state)))
                                            .child(if state == "finished" {
                                                crate::icons::glyph(
                                                    "ui/check.svg",
                                                    theme::size::META,
                                                )
                                                .text_color(rgb(state_color(t, state)))
                                                .into_any_element()
                                            } else if state == "active" {
                                                crate::icons::dot(state_color(t, state))
                                                    .into_any_element()
                                            } else {
                                                crate::icons::ring(state_color(t, state))
                                                    .into_any_element()
                                            })
                                            .child(format!(
                                                "{} · {}",
                                                index + 1,
                                                ui.process_state(state)
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme::size::UI)
                                            .whitespace_normal()
                                            .text_color(rgb(if selected {
                                                t.accent.patina
                                            } else {
                                                t.text.primary
                                            }))
                                            .child(phase.step.label.clone()),
                                    ),
                            )
                            .on_click(cx.listener(move |root, _, _, cx| {
                                root.select_process_step(run, &id, cx)
                            })),
                    ),
            );
        }
        body = body.child(strip);
        if let Some(removed) = entry.and_then(|e| e.selection.removed.as_ref()) {
            body = body.child(muted(
                t,
                format!(
                    "{} {} → {}",
                    ui.text(Text::ProcessSelectedFallback),
                    removed,
                    selected.unwrap_or("—")
                ),
            ));
        }
        // Preserve operational recovery guards. Live-only navigation adds no
        // card; an earlier actual resume still explains its retained wait.
        let recovery = &snapshot["recovery"]["state"];
        if attention || recovery["phase"] == "recovery_gap" {
            let mut notice = div()
                .flex()
                .flex_col()
                .gap(theme::space::TREE_INSET)
                .p(theme::space::CARD_PADDING)
                .border_l_2()
                .border_color(rgb(t.accent.brass))
                .bg(rgb(t.accent.brass_bg));
            if attention {
                notice = notice.child(section(ui.text(Text::ProcessParentAttention)));
            }
            notice = notice
                .child(muted(
                    t,
                    ui.text(if snapshot["handoff"]["ready"] == true {
                        Text::ProcessHandoffReady
                    } else {
                        match recovery["phase"].as_str() {
                            Some("resume_reserved") => Text::ProcessRecoveryPending,
                            Some("origin_live_reused") => Text::ProcessRecoveryLive,
                            Some("resume_launched") => Text::ProcessRecoveryLaunched,
                            _ => Text::ProcessRecoveryGap,
                        }
                    }),
                ))
                .when(recovery["placement"]["fallback"] == true, |n| {
                    n.child(muted(t, ui.text(Text::ProcessRecoveryAreaFallback)))
                });
            if let Some(reason) = recovery["reason"].as_str() {
                notice = notice.child(muted(t, reason.to_owned()));
            }
            if attention {
                notice = notice.child(section(ui.text(Text::ProcessReconcileChecklist)));
                let assessment = &snapshot["reconciliation"];
                if assessment["stale"] == true {
                    notice = notice.child(muted(t, ui.text(Text::ProcessReconcileStale)));
                }
                if !assessment["blockers"].is_array() {
                    notice = notice.child(muted(t, ui.text(Text::ProcessReconcileUnknown)));
                }
                for blocker in assessment["blockers"].as_array().into_iter().flatten() {
                    let text = match blocker["code"].as_str() {
                        Some("source_changed") => Text::ProcessReconcileSource,
                        Some("contract_changed") => Text::ProcessReconcileContract,
                        Some("origin_unknown") => Text::ProcessReconcileOrigin,
                        Some("holder_unknown" | "child_identity") => Text::ProcessReconcileHolder,
                        Some("permission") => Text::ProcessReconcilePermission,
                        Some("question") => Text::ProcessReconcileQuestion,
                        Some("effect_unknown") => Text::ProcessReconcileEffect,
                        Some("handoff_unknown") => Text::ProcessHandoffUnknown,
                        Some("delivery_unknown") => Text::ProcessReconcileDelivery,
                        Some("report_pending") => Text::ProcessReconcileReport,
                        Some("child_source_unknown" | "child_report_stale") => {
                            Text::ProcessReconcileChildSource
                        }
                        Some(
                            "review_pending" | "review_usage_unknown" | "review_archive_unknown",
                        ) => Text::ProcessReconcileReview,
                        Some("resource_owner" | "resource_changed") => {
                            Text::ProcessReconcileResource
                        }
                        Some("status_unknown") => Text::ProcessReconcileStatus,
                        _ => Text::ProcessReconcileUnknown,
                    };
                    let mut row = div()
                        .flex()
                        .flex_col()
                        .gap(theme::space::TREE_INSET)
                        .child(muted(t, ui.text(text)));
                    if let Some(session) = blocker["session"]
                        .as_str()
                        .and_then(|s| s.parse::<SessionId>().ok())
                    {
                        row = row.child(muted(t, session.to_string()));
                        if self.process_session_available(run, session) {
                            row =
                                row.child(
                                    Button::new(SharedString::from(format!(
                                        "reconciliation-session-{run}-{session}-{}",
                                        blocker["code"]
                                    )))
                                    .label(ui.text(Text::ProcessOpenSession))
                                    .small()
                                    .custom(super::controls::ghost(t, false, cx))
                                    .on_click(cx.listener(move |root, _, _, cx| {
                                        root.open_process_session(run, session, cx)
                                    })),
                                );
                        }
                    }
                    notice = notice.child(row);
                }
                if assessment["clear"] == true {
                    notice = notice.child(muted(t, ui.text(Text::ProcessReconcileClear)));
                }
                if let Some(hash) = assessment["fingerprint"].as_str() {
                    notice = notice.child(muted(
                        t,
                        format!("{} · {}", run, &hash[..hash.len().min(12)]),
                    ));
                }
            }
            body = body.child(notice);
        }
        if snapshot["reconciliation"]["acknowledged"] == true {
            body = body.child(muted(t, ui.text(Text::ProcessReconcileRecorded)));
        }
        if snapshot["handoff"].is_object() {
            let handoff = &snapshot["handoff"];
            let text = handoff_text(handoff, snapshot["reconciliation"]["acknowledged"] == true);
            let mut card = div()
                .flex()
                .flex_col()
                .gap(theme::space::TREE_INSET)
                .p(theme::space::CARD_PADDING)
                .bg(rgb(t.surface.raised))
                .child(section(ui.text(Text::ProcessHandoffTitle)))
                .child(muted(t, ui.text(text)));
            if handoff["path"] == "startup" {
                card = card.child(muted(t, ui.text(Text::ProcessHandoffStartup)));
            }
            if let Some(digest) = handoff["digest"].as_str() {
                card = card.child(muted(
                    t,
                    format!("{} · {}", run, &digest[..digest.len().min(12)]),
                ));
            }
            if let Some(origin) = snapshot["origin"]["session"]
                .as_str()
                .and_then(|s| s.parse::<SessionId>().ok())
                && self.process_session_available(run, origin)
            {
                card = card.child(
                    Button::new(SharedString::from(format!("handoff-origin-{run}")))
                        .label(ui.text(Text::ProcessOpenSession))
                        .small()
                        .custom(super::controls::ghost(t, false, cx))
                        .on_click(cx.listener(move |root, _, _, cx| {
                            root.open_process_session(run, origin, cx)
                        })),
                );
            }
            body = body.child(card);
        }
        let Some(phase) = phases.iter().find(|p| Some(p.step.id.as_str()) == selected) else {
            return page
                .child(body.child(ui.text(Text::ProcessUnavailable)))
                .into_any_element();
        };
        let role = definition.roles.get(&phase.step.role);
        let mut main = div()
            .debug_selector(|| "process-main".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(theme::space::PANEL_PADDING)
            .when(compact, |m| m.flex_none().w_full());
        let mut workers = div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(section(ui.text(Text::ProcessWorker)));
        let mut sessions = std::collections::HashSet::new();
        for assignment in snapshot["assignments"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
        {
            let observed = &assignment["observation"];
            if observed["bound"] != true {
                continue;
            }
            if let Some(session) = observed["session"]
                .as_str()
                .and_then(|s| s.parse::<SessionId>().ok())
                .filter(|s| sessions.insert(*s))
            {
                workers = workers.child(
                    self.process_worker(
                        run,
                        session,
                        definition
                            .steps
                            .iter()
                            .find(|s| assignment["plan"]["step"] == s.id)
                            .and_then(|s| definition.roles.get(&s.role))
                            .map(|r| r.label.as_str())
                            .unwrap_or(ui.text(Text::ProcessUnknown)),
                        cx,
                    ),
                );
            }
        }
        if sessions.is_empty() {
            let worker_phase = process_view::current(&phases)
                .and_then(|id| phases.iter().find(|p| p.step.id == id))
                .unwrap_or(phase);
            let worker_role = definition.roles.get(&worker_phase.step.role);
            if let Some(role) =
                worker_role.filter(|r| r.owner == guild_session::workflow::Owner::Parent)
            {
                if let Some(origin) = &origin {
                    workers = workers.child(self.process_worker(
                        run,
                        origin.session,
                        role.label.as_str(),
                        cx,
                    ));
                }
            } else {
                workers = workers.child(muted(t, ui.text(Text::ProcessNoAssignment)));
                if let Some(assignment) = snapshot["assignments"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .rev()
                    .find(|a| a["plan"]["step"] == worker_phase.step.id)
                {
                    let path = assignment["observation"]
                        .get("checkout")
                        .or_else(|| assignment["observation"].get("path"));
                    if let Some(path) = path {
                        workers =
                            workers.child(meta(t, ui.text(Text::ProcessCheckout), leaf(path)));
                    }
                    if let Some(detail) = assignment["detail"].as_str() {
                        workers = workers.child(muted(t, detail.to_owned()));
                    }
                }
            }
        }
        main = main
            .child(workers)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .child(muted(t, ui.text(Text::ProcessSelectedStep)))
                    .child(
                        div()
                            .debug_selector(|| "process-selected-label".into())
                            .text_size(theme::size::TITLE)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .whitespace_normal()
                            .child(phase.step.label.clone()),
                    )
                    .child(muted(
                        t,
                        format!(
                            "{} · {} · {}",
                            phase.step.id,
                            role.map(|r| r.label.as_str()).unwrap_or(&phase.step.role),
                            ui.process_state(&phase.state)
                        ),
                    ))
                    .children(phase.detail.as_ref().map(|detail| muted(t, detail.clone())))
                    .child(meta(
                        t,
                        ui.text(Text::ProcessOutcomes),
                        phase.step.outcomes.join(" · "),
                    ))
                    .when(!phase.step.depends_on.is_empty(), |m| {
                        m.child(meta(
                            t,
                            ui.text(Text::ProcessDependencies),
                            phase
                                .step
                                .depends_on
                                .iter()
                                .map(|id| {
                                    definition
                                        .steps
                                        .iter()
                                        .find(|s| s.id == *id)
                                        .map(|s| s.label.as_str())
                                        .unwrap_or(id)
                                })
                                .collect::<Vec<_>>()
                                .join(" · "),
                        ))
                    }),
            )
            .child(muted(t, ui.text(Text::ProcessControlsUnavailable)));
        let mut events = div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(section(ui.text(Text::ProcessEvents)));
        let receipts = snapshot["events"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let recent: Vec<_> = receipts
            .iter()
            .rev()
            .filter(|e| {
                e["observation"].is_object()
                    || e["request"].is_object()
                        && !receipts
                            .iter()
                            .any(|o| o["attempt"] == e["attempt"] && o["observation"].is_object())
            })
            .take(8)
            .collect();
        if recent.is_empty() {
            events = events.child(muted(t, ui.text(Text::ProcessNoEvents)));
        }
        for receipt in recent {
            let observed = &receipt["observation"];
            let op = observed["operation"].as_str();
            let label = match op {
                Some("started") => Text::ProcessEventStart,
                Some("checkpoint") => Text::ProcessEventCheckpoint,
                Some("assignment_observed") => Text::ProcessEventAssignment,
                Some("assignment_delivered") => Text::ProcessEventDelivery,
                Some("recovery_observed") => Text::ProcessEventRecovery,
                Some("origin_handoff_submission" | "origin_handoff_observed") => {
                    Text::ProcessHandoffTitle
                }
                Some("parent_reconciled") => Text::ProcessReconcileEvent,
                Some("child_reported") => Text::ProcessEventReport,
                Some(
                    "review_observed"
                    | "review_qualified"
                    | "review_result"
                    | "review_environment_released",
                ) => Text::ProcessEventReview,
                Some("correction_delivery") => Text::ProcessEventCorrection,
                Some("cancel_requested") => Text::ProcessEventCancel,
                None => Text::ProcessEventReservation,
                _ => Text::ProcessEventUnknown,
            };
            let phase = observed["recovery"]["phase"].as_str();
            events = events.child(
                div()
                    .flex()
                    .gap(theme::space::CARD_PADDING)
                    .py(theme::space::CARD_GAP)
                    .border_b_1()
                    .border_color(rgb(t.surface.divider))
                    .child(muted(t, format!("#{}", receipt["revision"])))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().whitespace_normal().child(ui.text(label)))
                            .children(observed["step"].as_str().map(|s| muted(t, s.to_owned())))
                            .children((op == Some("correction_delivery")).then(|| {
                                muted(
                                    t,
                                    ui.text(match observed["stage"].as_str() {
                                        Some("observed") => Text::ProcessCorrectionObserved,
                                        Some("submitted") => Text::ProcessCorrectionSubmitted,
                                        Some("parent_checkpoint_required") => {
                                            Text::ProcessCorrectionParent
                                        }
                                        Some("refused") => Text::ProcessCorrectionRefused,
                                        _ => Text::ProcessCorrectionUnknown,
                                    }),
                                )
                            }))
                            .children(phase.map(|p| {
                                muted(
                                    t,
                                    ui.text(match p {
                                        "origin_live_reused" => Text::ProcessRecoveryLive,
                                        "resume_launched" => Text::ProcessRecoveryLaunched,
                                        _ => Text::ProcessRecoveryGap,
                                    }),
                                )
                            })),
                    ),
            );
        }
        main = main.child(events);
        let aside = self.process_context(run, &phase.step.id, snapshot, &definition, compact, cx);
        body = body.child(
            div()
                .flex()
                .min_w_0()
                .gap(theme::space::CARD_PADDING * 2.0)
                .when(compact, |g| g.flex_col())
                .child(main)
                .child(aside),
        );
        page.child(body).into_any_element()
    }

    fn process_worker(
        &self,
        run: RunId,
        session: SessionId,
        role: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let ui = self.ui();
        let t = self.theme;
        let available = self.process_session_available(run, session);
        let record = self.desk.session(session);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .p(theme::space::CARD_PADDING)
            .bg(rgb(t.surface.panel))
            .rounded(theme::radius::ROW);
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP);
        if let Some(harness) = record.and_then(|r| r.runs.harness()) {
            row = row.child(super::kind::harness_mark(
                crate::agents::Agent::from_harness(harness).mark(),
                t,
                theme::space::ICON,
                super::kind::Shade::Full,
            ));
        }
        row = row
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(section(record.and_then(|r| r.name.clone()).unwrap_or_else(
                        || {
                            record
                                .and_then(|r| r.runs.harness())
                                .map_or_else(|| session.to_string(), |h| format!("{h:?} · {role}"))
                        },
                    )))
                    .child(muted(t, format!("{role} · {session}")))
                    .children(record.map(|r| {
                        muted(
                            t,
                            r.cwd
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        )
                    })),
            )
            .child(
                Button::new(SharedString::from(format!(
                    "process-session-{run}-{session}"
                )))
                .debug_selector(move || format!("process-session-{session}"))
                .small()
                .custom(super::controls::ghost(t, false, cx))
                .label(ui.text(Text::ProcessOpenSession))
                .disabled(!available)
                .on_click(
                    cx.listener(move |root, _, _, cx| root.open_process_session(run, session, cx)),
                ),
            );
        card = card.child(row);
        if available {
            let status = self.status_of(session);
            card = card.child(muted(t, ui.activity_state(status)));
            use guild_session::status::{Status, Why};
            if let Status::Waiting(why) = status {
                card = card.child(muted(
                    t,
                    ui.text(match why {
                        Why::Permission => Text::ProcessPermissionUnavailable,
                        Why::Question => Text::ProcessQuestionUnavailable,
                        Why::Auth | Why::Unstated => Text::ProcessWaitingUnavailable,
                    }),
                ));
            }
        } else {
            card = card.child(muted(t, ui.text(Text::ProcessSessionUnavailable)));
        }
        card
    }

    fn process_context(
        &self,
        run: RunId,
        step: &str,
        snapshot: &Value,
        definition: &Definition,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let ui = self.ui();
        let t = self.theme;
        let origin = serde_json::from_value::<OriginBinding>(snapshot["origin"].clone()).ok();
        let mut aside = div()
            .debug_selector(|| "process-context".into())
            .flex()
            .flex_col()
            .flex_none()
            .min_w_0()
            .w(theme::space::SIDE_LIST)
            .gap(theme::space::CARD_PADDING)
            .border_color(rgb(t.surface.divider))
            .when(compact, |a| {
                a.w_full().border_t_1().pt(theme::space::PANEL_PADDING)
            })
            .when(!compact, |a| a.border_l_1().pl(theme::space::PANEL_PADDING))
            .child(section(ui.text(Text::ProcessContext)))
            .child(meta(
                t,
                ui.text(Text::ProcessTarget),
                leaf(&snapshot["target"]),
            ))
            .child(meta(
                t,
                ui.text(Text::ProcessDefinition),
                definition.id.clone(),
            ));
        if let Some(assignment) = snapshot["assignments"]
            .as_array()
            .into_iter()
            .flatten()
            .rev()
            .find(|a| a["observation"]["bound"] == true)
        {
            aside = aside.child(meta(
                t,
                ui.text(Text::ProcessCheckout),
                leaf(&assignment["observation"]["checkout"]),
            ));
        }
        if let Some(origin) = origin.as_ref() {
            let available = self.process_session_available(run, origin.session);
            aside = aside
                .child(meta(
                    t,
                    ui.text(Text::ProcessOrigin),
                    format!(
                        "{:?} · {}",
                        origin.harness,
                        origin.cwd.file_name().unwrap_or_default().to_string_lossy()
                    ),
                ))
                .child(
                    Button::new("process-parent-link")
                        .small()
                        .custom(super::controls::ghost(t, false, cx))
                        .label(ui.text(Text::ProcessOpenSession))
                        .disabled(!available)
                        .on_click(cx.listener({
                            let session = origin.session;
                            move |root, _, _, cx| root.open_process_session(run, session, cx)
                        })),
                )
                .when(!available, |a| {
                    a.child(muted(t, ui.text(Text::ProcessSessionUnavailable)))
                });
        } else {
            aside = aside.child(meta(
                t,
                ui.text(Text::ProcessOrigin),
                ui.text(Text::ProcessUnrecorded),
            ));
        }
        aside = aside
            .child(meta(
                t,
                ui.text(Text::ProcessContract),
                match snapshot["contract"]["kind"]
                    .as_str()
                    .or_else(|| snapshot["contract_kind"].as_str())
                {
                    Some("lightweight") => ui.text(Text::ProcessLightweight),
                    Some("openspec") => "OpenSpec",
                    _ => ui.text(Text::ProcessUnrecorded),
                },
            ))
            .child(meta(
                t,
                ui.text(Text::ProcessRecordedSource),
                format!(
                    "{} · {}",
                    short(&snapshot["source"]["commit"]),
                    short(&snapshot["source"]["sha256"])
                ),
            ))
            .child(section(ui.text(Text::ProcessEvidence)));
        let artifacts = process_view::artifacts(snapshot, step);
        if artifacts.is_empty() {
            aside = aside.child(muted(t, ui.text(Text::ProcessNoEvidence)));
        }
        for (index, artifact) in artifacts.into_iter().enumerate() {
            let available = self
                .process_file_relative(run, step, &artifact.path)
                .is_some();
            let path = artifact.path;
            let step = step.to_owned();
            aside = aside
                .child(
                    Button::new(SharedString::from(format!("process-artifact-{index}")))
                        .small()
                        .custom(super::controls::ghost(t, false, cx))
                        .h_auto()
                        .w_full()
                        .justify_start()
                        .label(artifact.label)
                        .hint(path.display().to_string(), t)
                        .accessibility_label(format!(
                            "{} · {}",
                            ui.text(Text::ProcessOpenFile),
                            path.display()
                        ))
                        .disabled(!available)
                        .on_click(cx.listener(move |root, _, window, cx| {
                            root.open_process_file(run, &step, &path, window, cx)
                        })),
                )
                .child(muted(
                    t,
                    artifact
                        .sha256
                        .as_ref()
                        .map(|s| s.chars().take(12).collect::<String>())
                        .unwrap_or_else(|| ui.text(Text::ProcessUnrecorded).into()),
                ))
                .when(artifact.different_source, |a| {
                    a.child(muted(t, ui.text(Text::ProcessSourceDifferent)))
                })
                .when(!available, |a| {
                    a.child(muted(t, ui.text(Text::ProcessFileUnavailable)))
                });
        }
        aside = aside.child(section(ui.text(Text::ProcessResources)));
        let skills = definition
            .steps
            .iter()
            .find(|s| s.id == step)
            .map(|s| s.skills.as_slice())
            .unwrap_or_default();
        if skills.is_empty() {
            aside = aside.child(muted(t, ui.text(Text::ProcessNoResources)));
        }
        for skill in skills {
            let pin = &snapshot["skills"][skill];
            aside = aside.child(meta(
                t,
                skill.clone(),
                pin["reason"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| short(&pin["skill"]["sha256"])),
            ));
        }
        for review in snapshot["reviews"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["step"] == step)
        {
            aside = aside
                .child(section(ui.text(Text::ProcessReviewUsage)))
                .child(meta(
                    t,
                    ui.text(Text::ProcessArchive),
                    leaf(&review["round"]["archive"]),
                ));
            if let Ok(job) = serde_json::from_value::<guild_session::terminal_job::JobReceipt>(
                review["job"].clone(),
            ) {
                let available = self.process_job_target(run, step, job.id).is_some();
                let step = step.to_owned();
                let id = job.id;
                aside = aside
                    .child(muted(
                        t,
                        format!("{} · {}", job.title, ui.terminal_status(&job)),
                    ))
                    .child(
                        Button::new(SharedString::from(format!("process-review-job-{id}")))
                            .small()
                            .custom(super::controls::ghost(t, false, cx))
                            .label(ui.text(Text::ProcessOpenJob))
                            .disabled(!available)
                            .on_click(cx.listener(move |root, _, window, cx| {
                                root.open_process_job(run, &step, id, window, cx)
                            })),
                    )
                    .when(!available, |a| {
                        a.child(muted(t, ui.text(Text::ProcessJobUnavailable)))
                    });
            } else {
                aside = aside.child(muted(t, ui.text(Text::ProcessNoJob)));
            }
        }
        aside = aside
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(t.surface.divider))
                    .pt(theme::space::PANEL_PADDING)
                    .child(section(ui.text(Text::ProcessReviewUsage)))
                    .child(
                        match serde_json::from_value::<ReviewLedger>(
                            snapshot["review_ledger"].clone(),
                        ) {
                            Ok(ledger) => muted(
                                t,
                                ui.process_review_usage(
                                    ledger.used(),
                                    ledger.reserved(),
                                    ledger.remaining(),
                                    definition.limits.review_calls,
                                    ledger.corrections_used(),
                                    definition.limits.correction_rounds,
                                ),
                            ),
                            Err(_) => muted(t, ui.text(Text::ProcessUnknown)),
                        },
                    ),
            )
            .child(muted(t, ui.text(Text::ProcessCloseKeepsRunning)));
        aside
    }
}

#[cfg(test)]
mod tests;
