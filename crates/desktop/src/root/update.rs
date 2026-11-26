//! The window's half of Update Guild — M5.13, reshaped in M5.15.
//!
//! The model is [`crate::update`], the copy and the helper are
//! [`crate::update::swap`]. What is here: looking at the staging folder
//! for a newer build, and the click — the refusals, the
//! quit's questions, the copy on a thread, the helper, the quit.
//! **The window never builds itself** — decision 44 of the spec.

use guild_session::update::transaction;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use gpui::{Context, Window};

use crate::Root;
use crate::i18n::Text;
use crate::update::swap::{self, Failure, Prepared};
use crate::update::{self, Build, Greeting};
use guild_session::changelog::Notes;

/// How often the staging folder is looked at — decision 8 of M5.15's
/// spec: two small reads and a stat, on this thread, no git.
const LOOK_EVERY: Duration = Duration::from_secs(3);

/// Off-thread native startup verification owned through completion or quit.
struct Acknowledging {
    cancel: Arc<AtomicBool>,
    worker: std::thread::JoinHandle<Result<(), transaction::Failure>>,
}
/// What the window knows about updating itself.
pub(crate) struct Update {
    pub(crate) presentation: crate::view::release_notes::Presentation,
    #[cfg(feature = "native-preview")]
    fixture_clock: Option<(gpui::Task<()>, Arc<AtomicBool>)>,
    source_updates: bool,
    inspecting: bool,
    inspected: bool,
    recovery_checked: Option<Instant>,
    recovery_since: Option<Instant>,
    recovery: Option<Box<transaction::Receipt>>,
    recovery_error: Option<transaction::Failure>,
    selection: Option<crate::update::selection::Selection>,
    preference_work: Option<gpui::Task<crate::update::selection::Answer>>,
    runtime: Option<crate::update::runtime::Runtime>,
    state: PathBuf,
    displayed_preferences: guild_session::update::Preferences,
    running_info: Option<guild_session::update::BuildInfo>,
    warning: Option<crate::update::remote::Error>,
    caching: bool,
    checking: bool,
    downloading: bool,
    manual_choice: Option<crate::update::control::Identity>,
    unverified: usize,
    activated: Option<uuid::Uuid>,
    ticket: Option<crate::update::control::Ticket>,
    candidates: Vec<crate::update::runtime::View>,
    selected: Option<crate::update::control::Identity>,
    ready: bool,
    installing: bool,
    committed: bool,
    progress: Option<crate::update::remote::Progress>,
    error: Option<crate::update::runtime::Failure>,
    stopped: bool,
    startup: Option<transaction::Startup>,
    acknowledging: Option<Acknowledging>,
    /// Which build this is; `None` for a debug build.
    build: Option<Build>,
    /// The bundle this process runs from; `None` for a debug build.
    bundle: Option<PathBuf>,
    /// The staged build, while it is offered over this one.
    staged: Option<Build>,
    /// What the staged build brings since this one — the hover card's,
    /// read from its changelog when the offer changes, not per hover.
    staged_notes: Notes,
    /// *What's new*, while it is up: what this nightly brought since the
    /// one run before it — decision 10 of M5.16.
    greeting: Option<Greeting>,
    /// Where *What's new*'s list is scrolled to, for its thumb.
    greeting_scroll: gpui::ScrollHandle,
    looked: Option<Instant>,
    /// The copy being made after a yes: its answer starts the helper and
    /// quits, or says why not.
    applying: Option<Receiver<Result<Prepared, Failure>>>,
}

impl Update {
    pub(crate) fn here() -> Self {
        let bundle = std::env::current_exe()
            .ok()
            .and_then(|exe| update::running_bundle(&exe));
        let build = Build::here();
        let greeting = update::greet(
            &crate::paths::state_dir(),
            build.as_ref(),
            bundle.as_deref(),
        );
        Self {
            presentation: Default::default(),
            #[cfg(feature = "native-preview")]
            fixture_clock: None,
            source_updates: false,
            inspecting: false,
            inspected: false,
            recovery_checked: None,
            recovery_since: None,
            recovery: None,
            recovery_error: None,
            selection: None,
            preference_work: None,
            runtime: None,
            state: crate::paths::state_dir(),
            displayed_preferences: Default::default(),
            running_info: None,
            warning: None,
            caching: false,
            checking: false,
            downloading: false,
            manual_choice: None,
            unverified: 0,
            activated: None,
            ticket: None,
            candidates: Vec::new(),
            selected: None,
            ready: false,
            installing: false,
            committed: false,
            progress: None,
            error: None,
            stopped: false,
            startup: None,
            acknowledging: None,
            build,
            bundle,
            staged: None,
            staged_notes: Notes::new(),
            greeting,
            greeting_scroll: gpui::ScrollHandle::new(),
            looked: None,
            applying: None,
        }
    }

    /// Which commit this Guild is, when it is a bundle.
    pub(crate) fn build(&self) -> Option<&Build> {
        self.build.as_ref()
    }

    /// The newer build waiting in the staging folder — what the title
    /// bar's Update button applies.
    pub(crate) fn staged(&self) -> Option<&Build> {
        self.staged.as_ref()
    }

    /// What the staged build brings, for the hover card.
    pub(crate) fn staged_notes(&self) -> &Notes {
        &self.staged_notes
    }

    /// *What's new*, while it is up.
    pub(crate) fn greeting(&self) -> Option<&Greeting> {
        self.greeting.as_ref()
    }

    /// *What's new*'s list's scroll, for its thumb.
    pub(crate) fn greeting_scroll(&self) -> &gpui::ScrollHandle {
        &self.greeting_scroll
    }

    /// Put *What's new* away; `true` when it was up.
    pub(crate) fn dismiss_greeting(&mut self) -> bool {
        self.greeting.take().is_some()
    }
}

impl Root {
    /// Supply the identity cached by main before opening any window.
    pub fn set_update_startup(&mut self, startup: Option<transaction::Startup>) {
        if !self.update.stopped {
            self.update.startup = startup;
        }
    }
    /// on_next_frame runs after this main Root's frame is rendered, not when a
    /// model merely exists. Closing/crashing before that frame preserves backup.
    pub(crate) fn acknowledge_update_after_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(startup) = self.update.startup.take() else {
            return;
        };
        let root = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = root.update(cx, |root, _| {
                if root.update.stopped {
                    return;
                }
                let cancel = Arc::new(AtomicBool::new(false));
                let flag = cancel.clone();
                match std::thread::Builder::new()
                    .name("guild-update-ack".into())
                    .spawn(move || startup.acknowledge(&flag))
                {
                    Ok(worker) => {
                        root.update.acknowledging = Some(Acknowledging { cancel, worker })
                    }
                    Err(_) => tracing::warn!("update startup acknowledgement could not start"),
                }
            });
        });
    }
    /// Drain startup validation off-thread as part of the existing application quit.
    pub(crate) fn finish_update(
        &mut self,
        cx: &mut Context<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        self.update.stopped = true;
        self.update.startup = None;
        let task = self.update.acknowledging.take();
        if let Some(task) = &task {
            task.cancel.store(true, Ordering::Release);
        }
        #[cfg(feature = "native-preview")]
        let fixture_clock = self.update.fixture_clock.take();
        #[cfg(feature = "native-preview")]
        if let Some((_, stopping)) = &fixture_clock {
            stopping.store(true, Ordering::Release);
        }
        let runtime = self
            .update
            .runtime
            .as_mut()
            .and_then(|runtime| runtime.stop());
        self.update.runtime = None;
        let mut selection = self.update.selection.take();
        let preference_work = self.update.preference_work.take();
        let state = self.update.state.clone();
        let executor = cx.background_executor().clone();
        Box::pin(async move {
            #[cfg(feature = "native-preview")]
            if let Some((clock, _)) = fixture_clock {
                clock.await;
            }

            executor
                .spawn(async move {
                    if let Some(runtime) = runtime {
                        let _ = runtime.join();
                    }
                    if let Some(selection) = &mut selection {
                        if let Some(work) = preference_work {
                            selection.complete(work.await);
                        }
                        let store = guild_session::update::PreferenceStore::new(state);
                        while let Some(work) = selection.take_work() {
                            selection.complete(work.execute(&store));
                        }
                    }
                    if let Some(task) = task {
                        let _ = task.worker.join();
                    }
                })
                .await;
        })
    }

    /// Update: the title bar's button and *Update Guild…* — decision 7 of
    /// M5.15's spec. The refusals first, as notices; then the quit's
    /// own questions, and only on a yes the copy — so a *Cancel* leaves
    /// no copy made and no helper waiting.
    pub(crate) fn apply_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update.source_updates {
            self.apply_source_update(false, window, cx);
            return;
        }
        if self.update.applying.is_some() {
            return;
        }
        let refusal = match &self.update.bundle {
            _ if self.update.staged().is_none() => Some(Text::NoticeUpdateNothingStaged),
            None => Some(Text::NoticeUpdateNotABundle),
            Some(bundle) if !swap::helper_of(bundle).is_file() => Some(Text::NoticeUpdateNoHelper),
            Some(_) => None,
        };
        if let Some(refusal) = refusal {
            let text = self.ui().text(refusal).to_owned();
            self.warn(text);
            cx.notify();
            return;
        }
        let root = cx.weak_entity();
        let then = move |cx: &mut gpui::App| {
            let _ = root.update(cx, |root, cx| root.begin_update(cx));
        };
        if self.may_quit_then(window, cx, then) {
            self.begin_update(cx);
        }
    }

    /// After a yes: say so, and copy the staged bundle aside on a thread.
    /// The answer lands in [`Root::settle_update`].
    fn begin_update(&mut self, cx: &mut Context<Self>) {
        let Some(bundle) = self.update.bundle.clone() else {
            return;
        };
        let text = self.ui().text(Text::NoticeRestartingToUpdate).to_owned();
        self.inform(text);
        cx.notify();
        let staged = update::staged_bundle(&crate::paths::state_dir());
        let running = self.update.build.clone();
        let dir = swap::folder(std::process::id());
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(swap::prepare(&staged, &bundle, running.as_ref(), &dir));
        });
        self.update.applying = Some(receive);
    }

    /// On the watch loop's clock: look at the staging folder, and take the
    /// copy's answer.
    pub(crate) fn poll_update(&mut self, cx: &mut Context<Self>) {
        if self.update.stopped {
            return;
        }
        if self
            .update
            .acknowledging
            .as_ref()
            .is_some_and(|task| task.worker.is_finished())
        {
            let task = self.update.acknowledging.take().unwrap();
            match task.worker.join() {
                Ok(Ok(())) => (),
                Ok(Err(failure)) => {
                    tracing::warn!(?failure, "update startup acknowledgement failed")
                }
                Err(_) => tracing::warn!("update startup acknowledgement worker failed"),
            }
        }
        if self.update.source_updates {
            self.poll_source_update(cx);
            return;
        }
        self.look_at_staged(cx);
        self.settle_update(cx);
    }

    /// Whether a newer build is staged. Repaints only when the answer
    /// changes.
    fn look_at_staged(&mut self, cx: &mut Context<Self>) {
        let due = self
            .update
            .looked
            .is_none_or(|looked| looked.elapsed() >= LOOK_EVERY);
        if !due {
            return;
        }
        self.update.looked = Some(Instant::now());
        let bundle = update::staged_bundle(&crate::paths::state_dir());
        let staged = Build::of_bundle(&bundle)
            .filter(|staged| update::offered(self.update.build.as_ref(), staged));
        if staged != self.update.staged {
            // Read once per offer: the card is hovered far more often
            // than a release is staged.
            self.update.staged_notes = match staged.as_ref().and_then(Build::version) {
                Some(version) => update::notes(
                    &update::changelog_of(&bundle),
                    self.update.build.as_ref().and_then(Build::version),
                    version,
                ),
                None => Notes::new(),
            };
            match &staged {
                Some(build) => tracing::info!(
                    "update: {} offered, {} entries in its changelog since this build",
                    build.label(),
                    self.update
                        .staged_notes
                        .iter()
                        .map(|(_, lines)| lines.len())
                        .sum::<usize>()
                ),
                None => tracing::info!("update: nothing offered"),
            }
            self.update.staged = staged;
            cx.notify();
        }
    }

    /// The copy has answered: ready — the helper starts and the window
    /// quits, without asking again, because the questions were answered
    /// before the copy began; not ready — the notice says why and the
    /// window stays.
    fn settle_update(&mut self, cx: &mut Context<Self>) {
        let Some(answer) = &self.update.applying else {
            return;
        };
        let prepared = match answer.try_recv() {
            Ok(prepared) => prepared,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.update.applying = None;
                return;
            }
        };
        self.update.applying = None;
        let (Some(bundle), ui) = (self.update.bundle.clone(), self.ui()) else {
            return;
        };
        let started = prepared
            .map_err(|failure| ui.update_failed(&failure))
            .and_then(|prepared| {
                swap::start_helper(&prepared, &bundle, std::process::id())
                    .map_err(|err| ui.update_helper_failed(&err.to_string()))
            });
        match started {
            Ok(()) => crate::quit(cx),
            Err(text) => {
                self.warn(text);
                cx.notify();
            }
        }
    }
}

mod sources;
