//! Which agent CLIs the login shell's `PATH` has: one answer the whole
//! window shares — the tiles and the chevron grey by it, the default's
//! mark, tooltip, start-screen card and Settings row are drawn from it —
//! GLD-081's review.
//!
//! **"Not asked yet" is a state of its own.** The window used to start
//! with every agent marked installed until a probe said otherwise, and
//! once the default followed what is installed that optimism chose: an
//! early `cmd-t` on a Mac with Codex and no Claude Code asked for Claude.
//! While nothing is known the tiles are still drawn enabled (a tile greyed
//! for the tens of milliseconds a probe takes would be seen more often
//! than a missing one, and a press on one says why), but **nothing is
//! chosen by it**: a default nobody chose reads as the Terminal until the
//! first answer, and a press asks again on the spot.
//!
//! **Asked twice over.** On the watch loop's clock, on a thread, at the
//! start and every ten seconds while the window is in front — with tabs
//! open as much as on the start screen, so an agent installed from a
//! terminal tab shows up in the `+` without a restart. And at the press of
//! a gesture whose default nobody chose, on the window's thread: `PATH`'s
//! directories stat'ed for three programs, which `Root::harness_missing`
//! already does at every press, and the one wait for the login shell
//! `login_path::path` allows a process. A press never launches what a
//! stale picture says.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use gpui::Context;
use guild_session::{DefaultTab, HarnessKind};

use crate::Root;
use crate::view::menu::Target;

/// How the agents on the `PATH` are found: the login shell's `PATH`
/// stat'ed for each program. A function so a test can stand in for the
/// machine.
pub(crate) type Probe = fn() -> Vec<HarnessKind>;

/// The production probe.
pub(crate) fn on_login_path() -> Vec<HarnessKind> {
    HarnessKind::available_on(&guild_session::login_path::path_or_inherited())
}

/// The diagnostic window's probe: every agent, the picture its tests were
/// written against — never the machine's `PATH`, which differs between a
/// laptop and CI. A test about a missing agent sets its own.
fn every_agent() -> Vec<HarnessKind> {
    HarnessKind::ALL.to_vec()
}

/// What the window knows of the agents installed, and the asking.
pub(crate) struct Availability {
    /// The last answer, with when it was asked — `None` until one has come.
    known: Option<(Vec<HarnessKind>, Instant)>,
    /// A thread's answer on its way, with when it was asked.
    asking: Option<(Instant, Receiver<Vec<HarnessKind>>)>,
    /// When the clock last asked.
    asked_at: Option<Instant>,
    /// How to ask.
    pub(crate) probe: Probe,
}

impl Availability {
    /// Nothing known yet, asked by the machine's `PATH` when `external`
    /// — the production window — and by [`every_agent`] otherwise, and in
    /// this crate's tests, whose production-policy windows launch stand-ins
    /// by a launch prefix and must not depend on what the machine running
    /// them has installed.
    pub(crate) fn new(external: bool) -> Self {
        Self {
            known: None,
            asking: None,
            asked_at: None,
            probe: if external && !cfg!(test) {
                on_login_path
            } else {
                every_agent
            },
        }
    }

    /// What the tiles and the chevron grey by: the last answer, or every
    /// agent while none has come.
    pub(crate) fn shown(&self) -> &[HarnessKind] {
        self.answered().unwrap_or(&HarnessKind::ALL)
    }

    /// The last answer, or `None` while none has come.
    pub(crate) fn answered(&self) -> Option<&[HarnessKind]> {
        self.known.as_ref().map(|(kinds, _)| kinds.as_slice())
    }

    /// Take `kinds` as the answer asked at `asked` — unless a newer one is
    /// already in: a thread's answer that arrives after a press asked
    /// again is older than what the press found. Whether the picture
    /// changed.
    fn record(&mut self, kinds: Vec<HarnessKind>, asked: Instant) -> bool {
        if let Some((_, at)) = &self.known
            && *at >= asked
        {
            return false;
        }
        let changed = self.answered() != Some(kinds.as_slice());
        self.known = Some((kinds, asked));
        changed
    }

    /// Take `kinds` as known now — a test's picture, or a stand-in's.
    #[cfg(test)]
    pub(crate) fn set(&mut self, kinds: Vec<HarnessKind>) {
        self.known = Some((kinds, Instant::now()));
    }

    /// Ask on this thread. What the probe answers, without recording it.
    pub(crate) fn probe(&self) -> Vec<HarnessKind> {
        (self.probe)()
    }

    /// Ask on this thread and record the answer; the answer, and whether
    /// the picture changed.
    fn ask_now(&mut self) -> (Vec<HarnessKind>, bool) {
        let asked = Instant::now();
        let kinds = self.probe();
        let changed = self.record(kinds.clone(), asked);
        (kinds, changed)
    }

    /// A thread's answer, if it has come; whether the picture changed.
    fn settle(&mut self) -> bool {
        let Some((asked, answer)) = &self.asking else {
            return false;
        };
        let asked = *asked;
        match answer.try_recv() {
            Ok(kinds) => {
                self.asking = None;
                self.record(kinds, asked)
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("agent availability: the probe ended without an answer");
                self.asking = None;
                false
            }
        }
    }

    /// Start a thread asking, when one is due.
    fn ask_if_due(&mut self, in_front: bool) {
        let since = self.asked_at.map(|at| at.elapsed());
        if !crate::start::read_due(since, self.asking.is_some(), in_front) {
            return;
        }
        let asked = Instant::now();
        self.asked_at = Some(asked);
        let (send, receive) = mpsc::channel();
        let probe = self.probe;
        let spawned = std::thread::Builder::new()
            .name("guild-agents-on-path".into())
            .spawn(move || {
                // On a thread: the login shell's `PATH` may still be on its
                // way, and `path` waits for it.
                let _ = send.send(probe());
            });
        match spawned {
            Ok(_) => self.asking = Some((asked, receive)),
            Err(err) => tracing::warn!(%err, "agent availability: the probe could not start"),
        }
    }
}

impl Root {
    /// The probe's answer, if one has come, and a new probe when one is
    /// due — from the watch loop, beside `poll_start`, **whatever fills the
    /// middle**: the start screen's own read was once the only refresh, so
    /// with tabs open the picture stood still.
    pub(crate) fn poll_available(&mut self, cx: &mut Context<Root>) {
        let changed = self.available.settle();
        self.publish_available(changed, cx);
        self.available.ask_if_due(self.window_focused);
    }

    /// The agents installed, asked now, on this thread — for a press whose
    /// default nobody chose. The answer is shared, so what the press opened
    /// and what the `+` then shows agree.
    pub(crate) fn available_now(&mut self, cx: &mut Context<Root>) -> Vec<HarnessKind> {
        let (kinds, changed) = self.available.ask_now();
        self.publish_available(changed, cx);
        kinds
    }

    /// A changed answer reaches whatever drew the old one: the chevron's
    /// menu if it is open, its highlight, and the next frame.
    fn publish_available(&mut self, changed: bool, cx: &mut Context<Root>) {
        if !changed {
            return;
        }
        let shown = self.available.shown().to_vec();
        if let Some(open) = self.context_menu.as_mut()
            && let Target::NewTab { available } = &mut open.target
        {
            *available = shown;
        }
        self.repair_tab_launcher_selection();
        cx.notify();
    }

    /// The tab the default is, as drawn — the `+`'s mark and tooltip, the
    /// start screen's card, Settings' row: the person's choice, or the
    /// first agent installed by the last answer, and **the Terminal while
    /// none has come**, which needs nothing on the `PATH`. Never a guess
    /// that every agent is there.
    pub(crate) fn default_tab(&self) -> DefaultTab {
        self.settings
            .default_tab_among(self.available.answered().unwrap_or(&[]))
    }

    /// The tab the default is, for a press: the person's choice, or the
    /// first agent installed **asked now** — not the last answer, which may
    /// not have come or may be ten seconds old.
    pub(crate) fn default_tab_now(&mut self, cx: &mut Context<Root>) -> DefaultTab {
        match self.settings.default_tab {
            Some(chosen) => chosen,
            None => DefaultTab::first_installed(&self.available_now(cx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};
    use guild_session::{Runs, SessionMode};

    fn codex_only() -> Vec<HarnessKind> {
        vec![HarnessKind::Codex]
    }

    fn diagnostic(cx: &mut TestAppContext) -> (tempfile::TempDir, gpui::Entity<Root>) {
        let state = tempfile::tempdir().unwrap();
        let root = cx.new(|cx| Root::diagnostic(Default::default(), state.path().into(), cx));
        (state, root)
    }

    /// GLD-081's review: on a Mac with Codex and no Claude Code, a `cmd-t`
    /// before the first probe has answered opens Codex — the window used
    /// to take "not asked yet" for "everything installed" and asked for
    /// Claude. And the `+`'s mark, drawn meanwhile, guesses no agent; once
    /// the press has asked, it shows what the press opened.
    #[gpui::test]
    fn a_press_before_the_first_probe_answers_opens_the_agent_installed(cx: &mut TestAppContext) {
        let (_state, root) = diagnostic(cx);
        root.update(cx, |root, cx| {
            root.available.probe = codex_only;
            assert_eq!(root.available.answered(), None, "nothing asked yet");
            assert_eq!(root.default_tab(), DefaultTab::Terminal);
            assert_eq!(
                root.runs_for_default_tab(cx),
                Runs::Codex(SessionMode::Terminal)
            );
            assert_eq!(root.default_tab(), DefaultTab::Codex);
            assert_eq!(root.available.shown(), [HarnessKind::Codex]);
        });
    }

    /// Claude Code removed and Codex installed after the window started:
    /// a press asks again rather than trusting the picture it has, and the
    /// picture follows.
    #[gpui::test]
    fn a_press_after_the_agents_changed_follows_what_is_installed_now(cx: &mut TestAppContext) {
        let (_state, root) = diagnostic(cx);
        root.update(cx, |root, cx| {
            root.available.set(vec![HarnessKind::Claude]);
            root.available.probe = codex_only;
            assert_eq!(root.default_tab(), DefaultTab::Claude);
            assert_eq!(
                root.runs_for_default_tab(cx),
                Runs::Codex(SessionMode::Terminal)
            );
            assert_eq!(root.default_tab(), DefaultTab::Codex);
        });
    }

    /// A chosen default is the person's, missing or not: nothing is asked
    /// for it and nothing replaces it.
    #[gpui::test]
    fn a_chosen_default_is_not_replaced_by_what_is_installed(cx: &mut TestAppContext) {
        let (_state, root) = diagnostic(cx);
        root.update(cx, |root, cx| {
            root.settings.default_tab = Some(DefaultTab::Claude);
            root.available.probe = codex_only;
            assert_eq!(
                root.runs_for_default_tab(cx),
                Runs::Claude(SessionMode::Terminal)
            );
            assert_eq!(root.default_tab(), DefaultTab::Claude);
        });
    }

    /// With no start screen up — tabs, Settings, or no project — the
    /// clock still refreshes what the `+` shows: an agent installed from a
    /// terminal tab is the default without a restart.
    #[gpui::test]
    fn the_clock_refreshes_the_default_without_the_start_screen(cx: &mut TestAppContext) {
        let (_state, root) = diagnostic(cx);
        root.update(cx, |root, _| {
            assert_eq!(root.start_showing(), None);
            root.available.set(vec![HarnessKind::Claude]);
            root.available.probe = codex_only;
        });
        let by = Instant::now() + std::time::Duration::from_secs(5);
        while root.read_with(cx, |root, _| root.default_tab()) != DefaultTab::Codex {
            assert!(Instant::now() < by, "the clock never refreshed the default");
            root.update(cx, |root, cx| root.poll_available(cx));
            std::thread::yield_now();
        }
    }

    /// The clock's probe refreshes the picture whatever is in the middle
    /// of the window — an agent installed from a terminal tab reaches the
    /// `+` — and an answer asked before a press asked again does not undo
    /// what the press found.
    #[test]
    fn the_clock_refreshes_the_picture_and_an_older_answer_loses() {
        let mut available = Availability::new(false);
        available.set(vec![HarnessKind::Claude]);
        available.probe = codex_only;
        available.ask_if_due(true);
        let by = Instant::now() + std::time::Duration::from_secs(5);
        while !available.settle() {
            assert!(Instant::now() < by, "the probe never answered");
            std::thread::yield_now();
        }
        assert_eq!(available.answered(), Some(&[HarnessKind::Codex][..]));

        // A slow thread's answer, asked before the press below.
        let (send, receive) = mpsc::channel();
        available.asking = Some((Instant::now(), receive));
        available.probe = || vec![HarnessKind::OpenCode];
        let (now, changed) = available.ask_now();
        assert_eq!(
            (now.as_slice(), changed),
            (&[HarnessKind::OpenCode][..], true)
        );
        send.send(vec![HarnessKind::Claude]).unwrap();
        assert!(!available.settle());
        assert_eq!(available.answered(), Some(&[HarnessKind::OpenCode][..]));
    }
}
