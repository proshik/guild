//! *Set up for Xcode*: the button the language servers' list offers
//! beside a Swift server that was not started — 2026-09-28.
//!
//! sourcekit-lsp knows an Xcode project only through the
//! `buildServer.json` that `xcode-build-server config` writes, and the
//! language table's row starts it only where that file or a
//! `Package.swift` is. So where the root holds one workspace or one
//! project, the list offers to run the command — **on a gesture, never on
//! its own**, because it writes a file into the person's project. Nothing
//! is installed: a machine without `xcode-build-server` is told so, in the
//! tool's own name, as a missing server is.
//!
//! The command runs on a thread of its own with a deadline, like a build
//! system being asked (`deps::ask::run`); its answer lands on the watch
//! loop's clock, and on success the server is started at once.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use gpui::Context;
use guild_session::ProjectId;

use crate::Root;
use crate::highlight::Language;
use crate::lsp::progress::Fault;

/// Xcode's developer directory where the App Store puts it.
const XCODE: &str = "/Applications/Xcode.app/Contents/Developer";

/// How long `config` may take. It runs `xcodebuild -list` and
/// `-showBuildSettings`, which took seconds on devdeck and can take much
/// longer on a first run after an Xcode update.
const DEADLINE: Duration = Duration::from_secs(120);

/// The language the button is for — its row's name in the table.
const SWIFT: &str = "swift";

/// A setup of one project, while it runs and after it failed.
pub(crate) enum Setup {
    Running(Receiver<Result<(), String>>),
    Failed(String),
}

/// What the list draws beside the Swift server.
pub(crate) enum Offer {
    /// The button.
    Ready,
    /// The command is running.
    Running,
    /// It ran and did not write the file, in its own words; the button is
    /// offered again beside them.
    Failed(String),
}

impl Root {
    /// The offer for a project, if there is one to make: its Swift server
    /// was not started for want of a file, and its root holds one Xcode
    /// workspace or project to bind.
    pub(crate) fn xcode_offer(&self, project: ProjectId) -> Option<Offer> {
        if !matches!(
            self.serverless.get(&(project, SWIFT)),
            Some(Fault::Unprepared(_))
        ) {
            return None;
        }
        crate::sourcekit::xcode_config(&self.desk.project(project)?.path)?;
        Some(match self.xcode_setups.get(&project) {
            None => Offer::Ready,
            Some(Setup::Running(_)) => Offer::Running,
            Some(Setup::Failed(why)) => Offer::Failed(why.clone()),
        })
    }

    /// Run `xcode-build-server config` in the project's root.
    pub(crate) fn set_up_xcode(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if matches!(self.xcode_setups.get(&project), Some(Setup::Running(_))) {
            return;
        }
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(args) = crate::sourcekit::xcode_config(&root) else {
            return;
        };
        let (answer, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = answer.send(configure(&root, &args));
        });
        self.xcode_setups.insert(project, Setup::Running(heard));
        cx.notify();
    }

    /// Take what the setups have answered: a success starts the server,
    /// a failure is kept for the list to say.
    pub(crate) fn poll_xcode_setups(&mut self, cx: &mut Context<Self>) {
        let mut done: Vec<(ProjectId, Result<(), String>)> = Vec::new();
        for (project, setup) in &self.xcode_setups {
            let Setup::Running(heard) = setup else {
                continue;
            };
            match heard.try_recv() {
                Ok(result) => done.push((*project, result)),
                Err(TryRecvError::Disconnected) => {
                    done.push((*project, Err("the setup ended without an answer".into())))
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        for (project, result) in done {
            match result {
                Ok(()) => {
                    tracing::info!("a project was set up for Xcode");
                    self.xcode_setups.remove(&project);
                    // Not remembered as unprepared any more: the file the
                    // row needs is there now.
                    self.serverless.remove(&(project, SWIFT));
                    if let Some(swift) = Language::ALL.iter().find(|l| l.name == SWIFT) {
                        self.ensure_server(project, swift, cx);
                    }
                }
                Err(why) => {
                    tracing::warn!(%why, "a project was not set up for Xcode");
                    self.xcode_setups.insert(project, Setup::Failed(why));
                }
            }
            cx.notify();
        }
    }
}

/// The command, on its thread: Xcode named where `xcode-select` names the
/// command line tools, then `config`, then the file it should have left.
fn configure(root: &Path, args: &[String]) -> Result<(), String> {
    let selected = crate::deps::ask::run(
        root,
        Path::new("xcode-select"),
        &["-p"],
        Duration::from_secs(10),
    )
    .unwrap_or_default();
    let developer = crate::sourcekit::developer_dir(Path::new(selected.trim()), Path::new(XCODE));
    let env: Vec<(&str, &std::ffi::OsStr)> = developer
        .iter()
        .map(|dir| ("DEVELOPER_DIR", dir.as_os_str()))
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::deps::ask::run_with(root, Path::new("xcode-build-server"), &args, &env, DEADLINE)?;
    // **The file, not the exit code**: asked the wrong way, the tool
    // prints its usage and exits 0.
    if root.join("buildServer.json").is_file() {
        Ok(())
    } else {
        Err("xcode-build-server wrote no buildServer.json".into())
    }
}
