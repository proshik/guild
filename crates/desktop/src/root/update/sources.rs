//! Root owns source preferences, async replies and quit admission; runtime owns bytes/tools.
use super::*;
use crate::update::{
    control::{Identity, Ticket},
    runtime::{self, Event, Request},
    selection,
};
use guild_session::update::{PreferenceStore, Preferences, Source};
use std::{
    future::Future,
    pin::Pin,
    task::{Context as FutureContext, Poll, Waker},
};
impl Root {
    /// Start application-owned updater work. Diagnostic roots never contact a provider.
    pub fn enable_updater(&mut self) -> std::io::Result<()> {
        if self.update.stopped || self.update.runtime.is_some() {
            return Ok(());
        }
        self.update.source_updates = true;
        self.update.error = Some(runtime::Failure::Internal);
        let state = self.integrations.state_dir();
        let provider = self
            .integrations
            .external()
            .then(|| {
                guild_session::update::parse_keys(include_bytes!(
                    "../../../../../assets/update-keys.json"
                ))
                .ok()
                .and_then(|keys| {
                    crate::update::remote::Provider::new(update::REPOSITORY, keys).ok()
                })
            })
            .flatten();
        let runtime = runtime::Runtime::spawn(
            runtime::Config {
                default_stage: state.join("update"),
                work: state.join("update/work"),
                cache: state.join("update/cache"),
            },
            provider,
        )?;
        self.update.state = state;
        self.update.selection = Some(selection::Selection::new(update::REPOSITORY.into()));
        self.update.runtime = Some(runtime);
        self.update.error = None;
        Ok(())
    }
    /// Desired source values, including a choice waiting for durable acknowledgement.
    pub fn update_preferences(&self) -> Preferences {
        self.update
            .selection
            .as_ref()
            .map(|selection| selection.gate().preferences().clone())
            .unwrap_or_else(|| self.update.displayed_preferences.clone())
    }
    /// A pending destination transaction is read on the owned worker before discovery.
    pub fn update_recovery(&self) -> Option<&transaction::Receipt> {
        self.update.recovery.as_deref()
    }
    /// A terminal phase can still have a live helper retiring its active pointer.
    pub fn update_recovery_waiting(&self) -> bool {
        self.update.recovery.as_ref().is_some_and(|receipt| {
            matches!(
                receipt.phase(),
                transaction::Phase::LaunchRequested
                    | transaction::Phase::Acknowledged
                    | transaction::Phase::Restored
                    | transaction::Phase::Aborted
            )
        }) && self
            .update
            .recovery_since
            .is_some_and(|since| since.elapsed() < Duration::from_secs(60))
    }
    /// A foreign or unreadable transaction blocks replacement without following its paths.
    pub fn update_recovery_failure(&self) -> Option<transaction::Failure> {
        self.update.recovery_error
    }
    /// Fully qualified source work needs durable preferences and a clear destination journal.
    pub fn update_ready_to_check(&self) -> bool {
        self.update_preferences_ready()
            && self.update.inspected
            && self.update.recovery.is_none()
            && self.update.recovery_error.is_none()
    }
    pub(crate) fn update_recovery_command(&self) -> Option<String> {
        Some(crate::update::recovery::command(
            &self
                .update
                .bundle
                .as_ref()?
                .join("Contents/MacOS/guild-desktop"),
            &self.update.state,
            self.update.recovery.as_ref()?.id(),
            if self.update.recovery.as_ref()?.phase() == transaction::Phase::Acknowledged {
                transaction::Recovery::KeepCurrent
            } else {
                transaction::Recovery::RestorePrevious
            },
        ))
    }
    pub fn running_update_build(&self) -> Option<&Build> {
        self.update.build.as_ref()
    }
    pub(crate) fn update_source_epoch(&self) -> Option<uuid::Uuid> {
        self.update
            .selection
            .as_ref()
            .map(|selection| selection.gate().selection())
    }
    pub(crate) fn updater_default_directory(&self) -> PathBuf {
        self.update.state.join("update")
    }
    /// Saving coalesces edits; uncertain preference recovery and committed swaps freeze controls.
    pub fn update_source_editable(&self) -> bool {
        !self.update.stopped
            && !self.update.committed
            && self.update.selection.as_ref().is_some_and(|selection| {
                matches!(
                    selection.status(),
                    selection::Status::Ready | selection::Status::Saving
                )
            })
    }
    /// Work is enabled only after matching durable preference acknowledgement.
    pub fn update_preferences_ready(&self) -> bool {
        !self.update.stopped
            && !self.update.committed
            && self
                .update
                .selection
                .as_ref()
                .is_some_and(|selection| selection.status() == &selection::Status::Ready)
    }
    /// Whether the selected artifact is already verified and available to prepare.
    pub fn update_download_ready(&self) -> bool {
        self.update.ready
    }
    /// Current sanitized operation error, without transport diagnostics or credentials.
    pub fn update_failure(&self) -> Option<&runtime::Failure> {
        self.update.error.as_ref()
    }
    /// Coalesced transfer/verification progress from the selected-source worker.
    pub fn update_progress(&self) -> Option<&crate::update::remote::Progress> {
        self.update.progress.as_ref()
    }
    /// Invalidate current callbacks immediately; persist the choice on the background executor.
    pub fn set_update_preferences(&mut self, value: Preferences, cx: &mut Context<Self>) -> bool {
        if self.update.stopped
            || self.update.committed
            || value
                .local_directory
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return false;
        }
        let Some(selection) = &mut self.update.selection else {
            return false;
        };
        if !selection.change(value.clone()) {
            return false;
        }
        self.update.inspecting = false;
        self.update.displayed_preferences = value;
        self.update.manual_choice = None;
        if let Some(runtime) = &self.update.runtime {
            runtime.clear();
        }
        self.clear_source_candidate();
        self.update.activated = None;
        cx.notify();
        true
    }
    fn clear_source_candidate(&mut self) {
        self.update.ticket = None;
        self.update.candidates.clear();
        self.update.selected = None;
        self.update.staged = None;
        self.update.staged_notes.clear();
        self.update.ready = false;
        self.update.installing = false;
        self.update.progress = None;
        self.update.error = None;
        self.update.warning = None;
        self.update.caching = false;
        self.update.checking = false;
        self.update.downloading = false;
        self.update.unverified = 0;
    }
    fn source_owns(&self, ticket: &Ticket) -> bool {
        self.update_preferences_ready()
            && self
                .update
                .selection
                .as_ref()
                .is_some_and(|selection| selection.gate().owns(ticket))
    }
    fn source_pins(&self, ticket: &Ticket, identity: &Identity) -> bool {
        self.source_owns(ticket)
            && self
                .update
                .selection
                .as_ref()
                .is_some_and(|selection| selection.gate().pins(ticket, identity))
    }
    pub(super) fn poll_source_update(&mut self, cx: &mut Context<Self>) {
        if let Some(work) = &mut self.update.preference_work
            && work.is_ready()
        {
            let mut context = FutureContext::from_waker(Waker::noop());
            if let Poll::Ready(answer) = Pin::new(work).poll(&mut context) {
                self.update.preference_work = None;
                if let Some(selection) = &mut self.update.selection {
                    selection.complete(answer);
                    self.update.displayed_preferences = selection.gate().preferences().clone();
                }
                cx.notify();
            }
        }
        if self.update.preference_work.is_none()
            && let Some(work) = self
                .update
                .selection
                .as_mut()
                .and_then(selection::Selection::take_work)
        {
            let store = PreferenceStore::new(self.update.state.clone());
            self.update.preference_work = Some(
                cx.background_executor()
                    .spawn(async move { work.execute(&store) }),
            );
        }
        // Enqueue replaces reply slots. Consume completed work before a due
        // Local poll, including after a slow first native frame.
        let events = self
            .update
            .runtime
            .as_ref()
            .map(runtime::Runtime::drain)
            .unwrap_or_default();
        for event in events {
            if !self.source_owns(event.ticket()) {
                continue;
            }
            self.settle_source_event(event, cx);
        }
        if self.update_preferences_ready() {
            if !self.update.inspected
                || (self.update.recovery.is_some()
                    && self
                        .update
                        .recovery_checked
                        .is_none_or(|last| last.elapsed() >= LOOK_EVERY))
            {
                if !self.update.inspecting {
                    self.clear_source_candidate();
                    self.update.activated = None;
                    let ticket = self
                        .update
                        .selection
                        .as_mut()
                        .unwrap()
                        .gate_mut()
                        .begin()
                        .unwrap();
                    self.update.ticket = Some(ticket.clone());
                    self.update.inspecting = true;
                    self.update
                        .runtime
                        .as_ref()
                        .unwrap()
                        .submit(Request::Inspect {
                            ticket,
                            state: self.update.state.clone(),
                            destination: self.update.bundle.clone(),
                        });
                }
                return;
            }
            if self.update.recovery.is_some() || self.update.recovery_error.is_some() {
                return;
            }
            let epoch = self.update.selection.as_ref().unwrap().gate().selection();
            if self.update.activated != Some(epoch) {
                self.update.activated = Some(epoch);
                self.check_source_update(cx);
            } else if self.update_preferences().source == Source::Local
                && !self.update.installing
                && self
                    .update
                    .runtime
                    .as_ref()
                    .is_some_and(runtime::Runtime::idle)
                && self
                    .update
                    .looked
                    .is_none_or(|last| last.elapsed() >= LOOK_EVERY)
            {
                self.update.looked = Some(Instant::now());
                if let Some(ticket) = self.update.ticket.clone() {
                    self.update
                        .runtime
                        .as_ref()
                        .unwrap()
                        .submit(Request::PollLocal(ticket));
                } else {
                    self.check_source_update(cx);
                }
            } else if self.update_preferences().source == Source::Remote
                && !self.update.installing
                && !self.asking
                && !self.update_auto_retry_blocked()
                && self
                    .update
                    .runtime
                    .as_ref()
                    .is_some_and(runtime::Runtime::idle)
                && self
                    .update
                    .looked
                    .is_some_and(|last| last.elapsed() >= Duration::from_secs(30 * 60))
            {
                self.check_source_update(cx);
            }
        }
    }
    fn update_auto_retry_blocked(&self) -> bool {
        match self.update.error.as_ref() {
            Some(runtime::Failure::Remote(crate::update::remote::Error::RateLimited(reset))) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                *reset > now
            }
            // Setup/authorization failures need an explicit owner retry.
            Some(runtime::Failure::Remote(
                crate::update::remote::Error::MissingGh | crate::update::remote::Error::Access,
            )) => true,
            _ => false,
        }
    }
    pub(crate) fn check_source_update(&mut self, cx: &mut Context<Self>) {
        if !self.update_ready_to_check() {
            return;
        }
        let ticket = self
            .update
            .selection
            .as_mut()
            .and_then(|selection| selection.gate_mut().begin());
        let Some(ticket) = ticket else {
            return;
        };
        self.update.activated = Some(self.update.selection.as_ref().unwrap().gate().selection());
        self.clear_source_candidate();
        self.update.ticket = Some(ticket.clone());
        self.update.checking = true;
        self.update.looked = Some(Instant::now());
        if let Some(runtime) = &self.update.runtime {
            runtime.submit(Request::Check(ticket));
        }
        cx.notify();
    }
    fn settle_source_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Inspected { receipt, .. } => {
                self.update.inspected = true;
                self.update.inspecting = false;
                self.update.recovery_checked = Some(Instant::now());
                if self.update.recovery.as_ref().map(|receipt| receipt.id())
                    != receipt.as_ref().map(|receipt| receipt.id())
                {
                    self.update.recovery_since = receipt.as_ref().map(|_| Instant::now());
                }
                self.update.recovery = receipt;
                self.update.recovery_error = None;
            }
            Event::Unchanged { .. } => {
                // A prior failed poll revoked GUI install authority. An unchanged
                // cached fingerprint cannot restore it; revalidate the full input.
                if self.update.error.is_some() || !self.update.ready {
                    self.check_source_update(cx);
                }
                return;
            }
            Event::Cached { .. } => {
                self.update.caching = false;
                self.update.warning = None;
            }
            Event::Checked {
                ticket,
                candidates,
                ready,
                cache_warning,
                unverified,
            } => {
                self.update.progress = None;
                self.update.checking = false;
                self.update.error = None;
                self.update.looked = Some(Instant::now());
                self.update.candidates = candidates;
                let selected = self
                    .update
                    .manual_choice
                    .as_ref()
                    .and_then(|id| {
                        self.update
                            .candidates
                            .iter()
                            .find(|view| &view.identity == id)
                    })
                    .or_else(|| self.update.candidates.first())
                    .cloned();
                if let Some(view) = selected {
                    self.update.ready = ready.as_ref() == Some(&view.identity);
                    self.accept_source_view(&ticket, view);
                } else {
                    self.update.staged = None;
                    self.update.selected = None;
                    self.update.ready = false;
                }
                self.update.warning = cache_warning;
                self.update.unverified = unverified;
            }
            Event::Downloaded { ticket, candidate } => {
                self.update.downloading = false;
                if self.update.selected.as_ref() != Some(&candidate.identity) {
                    return;
                }
                if !self.accept_source_view(&ticket, *candidate) {
                    cx.notify();
                    return;
                }
                self.update.ready = true;
                self.update.error = None;
                self.update.warning = None;
                self.update.progress = None;
                if let Some(identity) = &self.update.selected
                    && let Some(permit) = self
                        .update
                        .selection
                        .as_ref()
                        .unwrap()
                        .gate()
                        .permit(&ticket, identity)
                {
                    self.update.caching = true;
                    self.update
                        .runtime
                        .as_ref()
                        .unwrap()
                        .submit(Request::Cache { ticket, permit });
                }
            }
            Event::Prepared {
                ticket, identity, ..
            } => {
                if !self.source_pins(&ticket, &identity) {
                    return;
                }
                let Some(destination) = self.update.bundle.clone() else {
                    self.update.error = Some(runtime::Failure::Unavailable);
                    self.update.runtime.as_ref().unwrap().clear();
                    self.update.installing = false;
                    return;
                };
                self.update
                    .runtime
                    .as_ref()
                    .unwrap()
                    .submit(Request::Journal {
                        ticket,
                        identity,
                        destination,
                        state: self.update.state.clone(),
                    });
            }
            Event::Journaled {
                ticket, identity, ..
            } => {
                if !self.source_pins(&ticket, &identity) {
                    return;
                }
                self.update
                    .runtime
                    .as_ref()
                    .unwrap()
                    .submit(Request::Handoff { ticket, identity });
            }
            Event::HandoffReady {
                ticket,
                identity,
                token,
            } => {
                if !self.source_pins(&ticket, &identity) {
                    return;
                }
                self.update.committed = true;
                match token.arm() {
                    Ok(()) => crate::quit(cx),
                    Err(failure) => {
                        self.update.committed = false;
                        self.update.installing = false;
                        self.update.error = Some(runtime::Failure::Transaction(failure));
                        self.update.runtime.as_ref().unwrap().clear();
                    }
                }
            }
            Event::Progress { progress, .. } => self.update.progress = Some(progress),
            Event::Failed { error, .. } => {
                if self.update.inspecting {
                    self.update.inspecting = false;
                    self.update.inspected = true;
                    self.update.recovery_error = Some(match &error {
                        runtime::Failure::Transaction(failure) => *failure,
                        _ => transaction::Failure::Storage,
                    });
                }
                self.update.checking = false;
                self.update.downloading = false;
                if self.update.caching
                    && let runtime::Failure::Remote(error) = &error
                {
                    self.update.warning = Some(error.clone());
                } else {
                    self.update.error = Some(error);
                    self.update.ready = false;
                }
                self.update.caching = false;
                self.update.progress = None;
                self.update.installing = false;
            }
        }
        cx.notify();
    }
    /// Current persistence/recovery status; absent only before startup or after quit.
    pub fn update_preference_status(&self) -> Option<&selection::Status> {
        self.update
            .selection
            .as_ref()
            .map(selection::Selection::status)
    }
    /// Candidate discovery has not yet completed for the current operation.
    pub fn update_checking(&self) -> bool {
        self.update.checking || self.update.inspecting
    }
    /// Explicit download has not yet completed for the current operation.
    pub fn update_downloading(&self) -> bool {
        self.update.downloading
    }
    /// Final copy/journal/helper admission is in progress.
    pub fn update_installing(&self) -> bool {
        self.update.installing || self.update.committed
    }
    /// Current signed/validated discovery results for release history selection.
    pub fn update_candidates(&self) -> &[runtime::View] {
        &self.update.candidates
    }
    /// Select history by immutable identity and refresh current release availability.
    pub fn select_update(&mut self, identity: &Identity, cx: &mut Context<Self>) -> bool {
        if !self.update_preferences_ready()
            || self.update.installing
            || !self
                .update
                .candidates
                .iter()
                .any(|view| &view.identity == identity)
        {
            return false;
        }
        self.update.manual_choice = Some(identity.clone());
        self.check_source_update(cx);
        true
    }
    /// Cancel only uncommitted updater work. Source selection remains saved.
    pub fn cancel_update(&mut self, cx: &mut Context<Self>) {
        if self.update.stopped || self.update.committed {
            return;
        }
        if let Some(selection) = &mut self.update.selection {
            selection.gate_mut().cancel();
        }
        if let Some(runtime) = &self.update.runtime {
            runtime.clear();
        }
        self.update.activated = self
            .update
            .selection
            .as_ref()
            .map(|selection| selection.gate().selection());
        self.update.inspecting = false;
        self.clear_source_candidate();
        self.update.looked = Some(Instant::now());
        cx.notify();
    }
    /// Explicitly re-read a previously unreadable destination journal.
    pub fn retry_update_recovery(&mut self, cx: &mut Context<Self>) {
        if self.update.stopped || self.update.committed || self.update.installing {
            return;
        }
        self.cancel_update(cx);
        self.update.inspected = false;
        self.update.recovery = None;
        self.update.recovery_error = None;
        self.update.activated = None;
    }
    /// Retry a failed preference operation without silently changing its requested source.
    pub fn retry_update_preferences(&mut self, cx: &mut Context<Self>) -> bool {
        if self.update.stopped || self.update.committed {
            return false;
        }
        let retried = self
            .update
            .selection
            .as_mut()
            .is_some_and(selection::Selection::retry);
        if retried {
            self.update.activated = None;
            cx.notify();
        }
        retried
    }
    /// Resolve exactly the pending preference intent exposed by the recovery UI.
    pub fn resolve_update_preferences(
        &mut self,
        operation_id: uuid::Uuid,
        keep_requested: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.update.stopped || self.update.committed {
            return false;
        }
        let resolved = self
            .update
            .selection
            .as_mut()
            .is_some_and(|selection| selection.resolve(operation_id, keep_requested));
        if resolved {
            self.update.activated = None;
            cx.notify();
        }
        resolved
    }
    /// Running metadata captured by main before any window can start replacement.
    pub fn set_running_update_info(&mut self, info: Option<guild_session::update::BuildInfo>) {
        self.update.running_info = info;
    }
    /// Cached running artifact provenance, independent of the selected source.
    pub fn running_update_info(&self) -> Option<&guild_session::update::BuildInfo> {
        self.update.running_info.as_ref()
    }
    /// Nonfatal cache warning; the in-memory verified payload may still be installed.
    pub fn update_warning(&self) -> Option<&crate::update::remote::Error> {
        self.update.warning.as_ref()
    }
    /// Matching releases which could not be authenticated for this protocol.
    pub fn unverified_update_releases(&self) -> usize {
        self.update.unverified
    }
    /// Explicitly selected release, or the highest eligible chosen-source candidate.
    pub fn selected_update(&self) -> Option<&runtime::View> {
        let identity = self.update.selected.as_ref()?;
        self.update
            .candidates
            .iter()
            .find(|view| &view.identity == identity)
    }
    pub(crate) fn apply_source_update(
        &mut self,
        reinstall: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.update_ready_to_check() || self.update.installing || self.asking {
            return;
        }
        if self.update.bundle.is_none() {
            self.warn(self.ui().text(Text::NoticeUpdateNotABundle).into());
            cx.notify();
            return;
        }
        if self.update.error.is_some() || self.update.selected.is_none() {
            self.check_source_update(cx);
            return;
        }
        let identity = self.update.selected.clone().unwrap();
        let Some(view) = self.selected_update().cloned() else {
            return;
        };
        if !reinstall
            && view
                .info
                .as_ref()
                .is_some_and(|info| self.update.running_info.as_ref() == Some(info))
        {
            self.inform(self.ui().text(Text::UpdaterUpToDate).into());
            cx.notify();
            return;
        }
        if !self.update.ready {
            let ticket = self
                .update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .begin()
                .unwrap();
            if !self
                .update
                .selection
                .as_mut()
                .unwrap()
                .gate_mut()
                .accept(&ticket, identity.clone())
            {
                return;
            }
            self.update.ticket = Some(ticket.clone());
            self.update.error = None;
            self.update.caching = false;
            self.update.downloading = true;
            self.update.progress = None;
            self.update
                .runtime
                .as_ref()
                .unwrap()
                .submit(Request::Download { ticket, identity });
            cx.notify();
            return;
        }
        let Some(ticket) = self.update.ticket.clone() else {
            return;
        };
        if !self.source_pins(&ticket, &identity) {
            return;
        }
        let ui = self.ui();
        let installed = self
            .update
            .build
            .as_ref()
            .map(|build| build.artifact_label(self.update.running_info.as_ref()))
            .unwrap_or_else(|| "unknown".into());
        let source = match identity.source() {
            Source::Local => "Local",
            Source::Remote => "Remote",
        };
        let mut body = ui.update_replacement(
            &installed,
            &view.build.artifact_label(view.info.as_ref()),
            source,
        );
        if view.info.is_none() {
            body.push_str("\n\n");
            body.push_str(ui.text(Text::UpdaterLegacyReplacement));
        }
        let answers = [
            gpui::PromptButton::new(ui.text(Text::UpdaterReplaceAction)),
            gpui::PromptButton::cancel(ui.text(Text::Cancel)),
        ];
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            ui.text(Text::UpdaterReplaceTitle),
            Some(&body),
            &answers,
            cx,
        );
        self.asking = true;
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let answer = asked.await;
                let _ = cx.update(|window, cx| {
                    root.update(cx, |root, cx| {
                        root.asking = false;
                        if answer == Ok(0) && root.source_pins(&ticket, &identity) {
                            root.ask_source_shutdown(ticket, identity, window, cx);
                        }
                        cx.notify();
                    })
                });
            })
            .detach();
    }
    fn ask_source_shutdown(
        &mut self,
        ticket: Ticket,
        identity: Identity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.source_pins(&ticket, &identity) {
            return;
        }
        let root = cx.weak_entity();
        let saved_ticket = ticket.clone();
        let saved_identity = identity.clone();
        let then = move |cx: &mut gpui::App| {
            let _ = root.update(cx, |root, cx| {
                root.begin_source_prepare(saved_ticket, saved_identity, cx)
            });
        };
        if self.may_quit_then(window, cx, then) {
            self.begin_source_prepare(ticket, identity, cx);
        }
    }
    fn begin_source_prepare(
        &mut self,
        confirmation: Ticket,
        identity: Identity,
        cx: &mut Context<Self>,
    ) {
        if !self.source_pins(&confirmation, &identity) {
            return;
        }
        let Some(root) = self
            .update
            .bundle
            .as_ref()
            .and_then(|bundle| bundle.parent())
            .map(PathBuf::from)
        else {
            return;
        };
        let ticket = self
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .begin()
            .unwrap();
        if !self
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .accept(&ticket, identity.clone())
        {
            return;
        }
        self.update.ticket = Some(ticket.clone());
        self.update.installing = true;
        self.update.caching = false;
        self.update.error = None;
        self.update.progress = None;
        self.update
            .runtime
            .as_ref()
            .unwrap()
            .submit(Request::Prepare {
                ticket,
                identity,
                root,
            });
        self.inform(self.ui().text(Text::NoticeRestartingToUpdate).into());
        cx.notify();
    }
    #[cfg(feature = "native-preview")]
    pub(crate) fn retain_updater_fixture_clock(
        &mut self,
        clock: gpui::Task<()>,
        stop: Arc<AtomicBool>,
    ) {
        assert!(!self.integrations.external() && self.update.fixture_clock.is_none());
        self.update.fixture_clock = Some((clock, stop));
    }
    /// Explicit one-shot consent for a task-owned native fixture, never production.
    #[cfg(feature = "native-preview")]
    pub(crate) fn approve_updater_fixture(
        &mut self,
        expected: uuid::Uuid,
        cx: &mut Context<Self>,
    ) -> Result<bool, &'static str> {
        if self.integrations.external() {
            return Err("fixture requires diagnostic policy");
        }
        if self
            .update
            .running_info
            .as_ref()
            .is_some_and(|info| info.build_id == expected)
        {
            return Ok(false);
        }
        if !self.update_preferences_ready() || !self.update.ready || self.update.installing {
            return Ok(false);
        }
        if self.update_preferences().source != Source::Local
            || self.update_preferences().local_directory != Some(self.update.state.join("stage"))
            || self
                .update
                .bundle
                .as_ref()
                .and_then(|path| path.canonicalize().ok())
                != Some(self.update.state.join("apps/Guild.app"))
            || !self.dirty_file_tabs(None, cx).is_empty()
            || self.running_sessions() != 0
        {
            return Err("fixture cannot act outside its empty owned app/state");
        }
        let view = self
            .selected_update()
            .ok_or("fixture has no selected artifact")?;
        if view
            .info
            .as_ref()
            .is_none_or(|info| info.build_id != expected)
        {
            return Err("fixture selected an unexpected artifact");
        }
        let ticket = self.update.ticket.clone().ok_or("fixture has no ticket")?;
        let identity = view.identity.clone();
        self.begin_source_prepare(ticket, identity, cx);
        Ok(true)
    }
    fn accept_source_view(&mut self, ticket: &Ticket, view: runtime::View) -> bool {
        if !self
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .accept(ticket, view.identity.clone())
        {
            return false;
        }
        if view.info.as_ref().is_some_and(|info| {
            // Source provenance may be unknown, so require explicit replacement;
            // UUID reuse with different metadata is always invalid.
            guild_session::update::compare(self.update.running_info.as_ref(), info, true).is_err()
        }) {
            self.update.selected = None;
            self.update.staged = None;
            self.update.staged_notes.clear();
            self.update.ready = false;
            self.update.error = Some(runtime::Failure::Preparation(
                crate::update::preparation::Failure::Invalid,
            ));
            return false;
        }
        self.update.selected = Some(view.identity.clone());
        let comparison = view.info.as_ref().and_then(|info| {
            guild_session::update::compare(
                self.update.running_info.as_ref(),
                info,
                view.identity.source() == Source::Local,
            )
            .ok()
        });
        self.update.staged = match (view.identity.source(), comparison) {
            (Source::Remote, Some(guild_session::update::Comparison::Newer)) => Some(view.build),
            (Source::Local, Some(guild_session::update::Comparison::Installed)) => None,
            (Source::Local, _) => Some(view.build),
            _ => None,
        };
        let sections = guild_session::changelog::parse(&format!("## [Unreleased]\n{}", view.notes));
        self.update.staged_notes =
            guild_session::changelog::merged(&sections.iter().collect::<Vec<_>>());
        true
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(feature = "native-preview")]
impl Root {
    /// Presentation-only fixture: no bundle destination, provider or transaction.
    pub(crate) fn seed_update_card_fixture(&mut self, scenario: &str, cx: &mut Context<Self>) {
        assert!(!self.integrations.external() && self.update.bundle.is_none());
        let mut installed = guild_session::update::BuildInfo::local("1".repeat(40));
        installed.tag = Some("nightly-2026-10-31".into());
        installed.channel = Some(guild_session::update::Channel::Nightly);
        installed.revision_order = Some(50);
        self.set_running_update_info(Some(installed.clone()));
        let mut info = installed;
        if scenario != "current" {
            info.build_id = uuid::Uuid::new_v4();
            info.commit = "2".repeat(40);
            info.tag = Some("nightly-2026-11-01".into());
            info.revision_order = Some(51);
        }
        let mut view = runtime::View {
            identity: Identity {
                artifact: crate::update::control::Artifact::Build(info.build_id),
                origin: crate::update::control::Origin::Remote {
                    repository: update::REPOSITORY.into(),
                    channel: guild_session::update::Channel::Nightly,
                    release_id: 1,
                    sha256: "a".repeat(64),
                },
            },
            build: Build::from(&info),
            info: Some(info),
            notes: "### Added\n- Keep remote and local update sources available.\n### Changed\n- Read update status and release notes in one card.\n### Fixed\n- Preserve update recovery controls.".into(),
        };
        if std::env::var("GUILD_UPDATE_CARD_CONTENT").as_deref() == Ok("long") {
            view.notes = guild_session::changelog::Group::ALL.into_iter().map(|group| {
                let count = if group == guild_session::changelog::Group::Security {2} else {8};
                format!("### {}\n{}", group.name(), (1..=count).map(|index| format!(
                    "- {index}. {}: длинная запись о работе редактора и терминала. Сохраняйте исходные предложения, переносы и сведения о релизе, даже когда текст занимает несколько строк.\n  Дополнительная строка исходной записи без придуманного заголовка.", group.name()
                )).collect::<Vec<_>>().join("\n"))
            }).collect::<Vec<_>>().join("\n");
        }
        if std::env::var("GUILD_UPDATE_CARD_CONTENT").as_deref() == Ok("representative") {
            let releases =
                guild_session::changelog::parse(include_str!("../../../../../CHANGELOG.md"));
            let mut entries = Vec::new();
            for group in guild_session::changelog::Group::ALL {
                entries.extend(
                    releases
                        .iter()
                        .flat_map(|release| release.entries.iter())
                        .filter(|(kind, _)| *kind == group)
                        .take(4)
                        .cloned(),
                );
            }
            view.notes = guild_session::changelog::Release {
                version: "Unreleased".into(),
                date: None,
                entries,
            }
            .markdown_notes();
        }
        if std::env::var("GUILD_UPDATE_CARD_SURFACE").as_deref() == Ok("whats-new") {
            self.update.greeting = Some(crate::update::Greeting {
                version: "nightly-2026-11-01".into(),
                since: Some("nightly-2026-10-29".into()),
                releases: vec![
                    crate::update::Section {
                        version: "nightly-2026-11-01".into(),
                        date: Some("2026-11-01".into()),
                        notes: crate::view::release_notes::parse_body(&view.notes),
                    },
                    crate::update::Section {
                        version: "nightly-2026-10-31".into(),
                        date: Some("2026-10-31".into()),
                        notes: crate::view::release_notes::parse_body(
                            "### Added\n- Изменение пропущенного релиза: сохраняется его собственная версия и дата.\n### Security\n- Проверка доступа из предыдущего релиза.",
                        ),
                    },
                ],
            });
        }
        if std::env::var("GUILD_UPDATE_CARD_CONTENT").as_deref() == Ok("representative") {
            let releases =
                guild_session::changelog::parse(include_str!("../../../../../CHANGELOG.md"));
            if let Some(previous) = releases
                .iter()
                .find(|release| !release.is_unreleased() && !release.entries.is_empty())
                && let Some(greeting) = self.update.greeting.as_mut()
            {
                greeting.releases[1] = crate::update::Section {
                    version: previous.version.clone(),
                    date: previous.date.clone(),
                    notes: guild_session::changelog::merged(&[previous]),
                };
            }
        }
        let ticket = self
            .update
            .selection
            .as_mut()
            .unwrap()
            .gate_mut()
            .begin()
            .unwrap();
        self.accept_source_view(&ticket, view.clone());
        self.update.candidates = vec![view];
        self.update.checking = scenario == "checking";
        self.update.downloading = scenario == "downloading";
        self.update.ready = scenario == "ready";
        self.update.error = (scenario == "error").then_some(runtime::Failure::Remote(
            crate::update::remote::Error::Network,
        ));
        if scenario == "empty" {
            self.clear_source_candidate();
            self.update.candidates.clear();
        }
        self.open_settings_at(crate::view::settings::Section::Updates, cx);
        cx.notify();
    }
}
