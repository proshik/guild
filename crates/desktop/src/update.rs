//! The Guild you work in, updating itself — M5.13, and M5.15.
//!
//! `just bundle` writes into `Contents/Resources`: `commit`, the commit it
//! was built from, and since M5.15 `built`, when. Until 2026-09-25 it also
//! wrote `source`, the checkout, for the status bar's `↑N` — how far
//! `main` had moved past the bundle. The Update button made that a second
//! way to say the same thing, and a wrong one: the commits it counted were
//! not released, so there was nothing to update to.
//!
//! **An update is a bundle already built** — decision 44 of the spec —
//! **and since M5.16 a release** (decision 45). `just release` stages it
//! in `<state dir>/update/Guild.app` while Guild runs, the window offers
//! it when it is a `nightly-*` built later than itself, and a click swaps
//! it in through the helper the bundle carries. The window never builds
//! itself. The bundle carries its `CHANGELOG.md`, which says what an
//! update brings.
//!
//! Pure: reading a bundle and deciding what is offered. The copy and the helper are [`swap`]; the asking and the
//! click are `root::update`.

use std::path::{Path, PathBuf};

use guild_session::changelog::{self, Notes, Release};

/// Generation and confirmation ownership for asynchronous update work.
pub mod control;
pub mod helper;
/// First Remote provider identity, independent of a checkout or gh defaults.
pub const REPOSITORY: &str = "proshik/guild";
pub mod preparation;
/// Remote source discovery and acquisition.
pub mod remote;
pub mod runtime;
pub mod selection;
pub mod swap;

/// What a bundle says about itself, for About and the Update button: which
/// commit, whether the tree had changes, and the `dogfood-<date>` tag when
/// `install` gave it one. Needs only `commit`; `tag` is written by
/// `_installable`, which tags before it bundles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The full hash.
    pub commit: String,
    /// Built from a tree with uncommitted changes.
    pub dirty: bool,
    /// The dogfood tag on that commit, if any.
    pub tag: Option<String>,
    /// When `just bundle` made it, in UTC seconds since the epoch. `None`
    /// for a bundle made before M5.15 — decision 4 of its spec.
    pub built: Option<u64>,
}

impl Build {
    /// The bundle this process runs from; `None` for a debug build.
    pub fn here() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        Self::read(&exe.parent()?.parent()?.join("Resources"))
    }

    pub fn read(resources: &Path) -> Option<Self> {
        let read = |name: &str, limit| {
            String::from_utf8(
                guild_session::update::bundle::read_small(&resources.join(name), limit).ok()?,
            )
            .ok()
        };
        let text = read("commit", 1024)?;
        let mut lines = text.lines();
        let commit = lines.next()?.trim().to_owned();
        if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let dirty = lines.any(|line| line.trim() == "dirty");
        let tag = read("tag", 1024)
            .and_then(|tag| Some(tag.lines().next()?.trim().to_owned()))
            .filter(|tag| !tag.is_empty());
        let built = read("built", 64).and_then(|built| built.lines().next()?.trim().parse().ok());
        Some(Self {
            commit,
            dirty,
            tag,
            built,
        })
    }

    /// A bundle on disk: what its `Resources` say, and only when its
    /// executable is there — a staging folder half-written is not a build.
    pub fn of_bundle(bundle: &Path) -> Option<Self> {
        let contents = bundle.join("Contents");
        if !contents.join("MacOS").join(EXECUTABLE).is_file() {
            return None;
        }
        Self::read(&contents.join("Resources"))
    }

    /// The exact supported Nightly or Stable release tag; dev builds have none.
    pub fn version(&self) -> Option<&str> {
        self.tag.as_deref().filter(|tag| {
            use guild_session::update::Channel;
            Channel::Nightly.accepts(tag, true, false) || Channel::Stable.accepts(tag, false, false)
        })
    }

    /// Distinguish separately built artifacts even when their source commit is equal.
    pub fn artifact_label(&self, info: Option<&guild_session::update::BuildInfo>) -> String {
        match info {
            Some(info) => format!("{} · {}", self.label(), info.build_id),
            None => self.label(),
        }
    }

    /// The commit as git abbreviates it.
    pub fn short(&self) -> &str {
        &self.commit[..7]
    }

    /// `75315fa · dogfood-2026-09-23`, with `· dirty` when it was.
    pub fn label(&self) -> String {
        let mut label = self.short().to_owned();
        if let Some(tag) = &self.tag {
            label.push_str(" · ");
            label.push_str(tag);
        }
        if self.dirty {
            label.push_str(" · dirty");
        }
        label
    }
}

impl From<&guild_session::update::BuildInfo> for Build {
    fn from(info: &guild_session::update::BuildInfo) -> Self {
        Self {
            commit: info.commit.clone(),
            dirty: info.dirty,
            tag: info.tag.clone(),
            built: Some(info.built),
        }
    }
}

/// The window's executable inside a bundle.
pub const EXECUTABLE: &str = "guild-desktop";

/// Where `just install` stages a build while Guild runs — decision 2 of
/// M5.15's spec. In the state folder rather than a checkout: there are as
/// many `target/` folders as worktrees, and a download will have none.
pub fn staged_bundle(state_dir: &Path) -> PathBuf {
    state_dir.join("update").join("Guild.app")
}

/// Whether a staged build is offered over the one running — decision 4
/// of M5.15, narrowed by decision 1 of M5.16.
///
/// **Only a release**: a staged build without a `nightly-*` tag is a dev
/// build, and a dev build is never offered, however late it was made.
/// Then **built later** is the test, and *another build* follows from
/// it: the same bundle has the same time. It needs no repository, and a
/// rollback — an old commit built now — is the update it is meant to be.
/// A staged bundle that does not say when it was built predates the
/// button, so it is older than whatever draws it; a running build that
/// does not say is a debug build, offered anything that does.
pub fn offered(running: Option<&Build>, staged: &Build) -> bool {
    if !staged
        .tag
        .as_deref()
        .is_some_and(|tag| guild_session::update::Channel::Nightly.accepts(tag, true, false))
    {
        return false;
    }
    let Some(staged) = staged.built else {
        return false;
    };
    match running.and_then(|running| running.built) {
        Some(running) => staged > running,
        None => true,
    }
}

/// What a bundle's `Resources/CHANGELOG.md` says — nothing when it has
/// none, which a bundle made before M5.16 does not.
pub fn changelog_of(bundle: &Path) -> Vec<Release> {
    guild_session::update::bundle::read_small(
        &bundle.join("Contents/Resources").join(CHANGELOG),
        4 * 1024 * 1024,
    )
    .ok()
    .and_then(|bytes| String::from_utf8(bytes).ok())
    .map(|text| changelog::parse(&text))
    .unwrap_or_default()
}

/// What an update from `after` to `upto` brings, by `changelog`: the
/// releases between them merged by group — decision 9 of M5.16. `after`
/// unknown gives `upto`'s own section.
pub fn notes(changelog: &[Release], after: Option<&str>, upto: &str) -> Notes {
    changelog::merged(&changelog::between(changelog, after, upto))
}

/// How many entries the card over Update shows before *and N more* —
/// section 5 of the design system.
pub const HOVER_CUT: usize = 8;

/// What the first start of a new release shows once — decision 10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greeting {
    /// The version started.
    pub version: String,
    /// The version greeted before it; `None` on the first greeting.
    pub since: Option<String>,
    /// What it brings, a block per release, newest first — the person
    /// chose a release's own block over one merged list on 2026-09-25,
    /// from the mockup. A release that says nothing has no block.
    pub releases: Vec<Section>,
}

/// One release's block in *What's new*: its name, its date and its
/// entries by group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub version: String,
    pub date: Option<String>,
    pub notes: Notes,
}

/// Where the version last greeted is kept: one line in the state folder.
pub fn seen_file(state_dir: &Path) -> PathBuf {
    state_dir.join("release-seen")
}

/// Whether an earlier Guild worked in this state folder: its desk is
/// there, under today's name or the one before it. **Not
/// `settings.json`**: the run being greeted writes that before it asks —
/// `Root::from_state` saves the panels it adopts — so on a first launch
/// it is already there. The desk is written only by a `Root` that
/// exists, and the greeting is asked while one is being built.
fn held_a_guild(state_dir: &Path) -> bool {
    ["workspace.json", "sessions.json"]
        .iter()
        .any(|name| state_dir.join(name).exists())
}

/// *What's new*, at start: a nightly not greeted before gets its own
/// bundle's changelog since the version seen last, a block per release, and becomes
/// the version seen — written **now**, when the card is about to show,
/// so a window that goes down before it is dismissed does not show it
/// twice. A dev or a debug build gets nothing and writes nothing, so the
/// next nightly still knows which one was seen; a release that says
/// nothing is seen and shows nothing. **A first launch** — no version
/// seen and no earlier Guild in the folder — is seen and shows nothing:
/// before Guild has been seen at all, everything in the changelog is new
/// and none of it is news (GLD-079). A folder an earlier Guild worked in
/// with no record is an install from before the record, and is greeted.
pub fn greet(state_dir: &Path, running: Option<&Build>, bundle: Option<&Path>) -> Option<Greeting> {
    let version = running?.version()?;
    let file = seen_file(state_dir);
    let seen = std::fs::read_to_string(&file)
        .ok()
        .and_then(|seen| Some(seen.lines().next()?.trim().to_owned()))
        .filter(|seen| !seen.is_empty());
    if seen.as_deref() == Some(version) {
        return None;
    }
    let first_launch = seen.is_none() && !held_a_guild(state_dir);
    let changelog = if first_launch {
        Vec::new()
    } else {
        bundle.map(changelog_of).unwrap_or_default()
    };
    let releases: Vec<Section> = changelog::between(&changelog, seen.as_deref(), version)
        .into_iter()
        .map(|release| Section {
            version: release.version.clone(),
            date: release.date.clone(),
            notes: changelog::merged(&[release]),
        })
        .filter(|section| !section.notes.is_empty())
        .collect();
    if let Err(err) = std::fs::create_dir_all(state_dir)
        .and_then(|()| std::fs::write(&file, format!("{version}\n")))
    {
        tracing::warn!("what's new: {} not written: {err}", file.display());
    }
    (!releases.is_empty()).then(|| Greeting {
        version: version.to_owned(),
        since: seen,
        releases,
    })
}

/// The changelog's name, at the root of a checkout and in a bundle.
pub const CHANGELOG: &str = "CHANGELOG.md";

/// The bundle an executable runs from: `…/X.app/Contents/MacOS/<exe>`
/// gives `…/X.app`, whatever `X` is; anything else — a debug build in
/// `target/` — is no bundle.
pub fn running_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_app = bundle.extension().is_some_and(|ext| ext == "app");
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && is_app)
        .then(|| bundle.to_path_buf())
}

/// Explicit owner recovery from the app bundle, independent of a source checkout.
pub mod recovery;
