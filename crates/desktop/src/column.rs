//! The column as a model: groups, rows, names — M12.
//!
//! **A worktree is a root, and a project in the column is a group of
//! roots** — decision 1 of M12's plan. Inside the code a `Project` is an
//! opened directory; here, and on the screen, a project is a repository,
//! and its rows are its worktrees: the opened ones, and the ones git lists
//! that nobody has opened yet. A directory outside git is a group of one.
//!
//! Everything here is a question about lists, so that a test can ask it
//! without a window: which groups, which rows in what order, what a row is
//! called, where a new worktree's folder goes, and whether one may be
//! removed. Opened roots remain reachable even when their directories are
//! temporarily unavailable, so their tabs can be recovered or closed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use guild_session::ProjectId;

use crate::git::worktrees::Worktree;

/// Trim a worktree branch name before either native entry path reaches Git.
/// Git still validates the remaining refname rules; option-like names must
/// never reach its internal branch command, which can open a help manual.
pub(crate) fn worktree_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a worktree needs a name".to_owned());
    }
    if name.starts_with('-') {
        return Err("a worktree name cannot begin with '-'".to_owned());
    }
    Ok(name)
}

/// An opened root, as the column needs to know it.
///
/// Paths are compared as they are given, so a caller hands both sides in
/// the same form — git's list is absolute, and so is a project's path.
#[derive(Debug, Clone, Copy)]
pub struct Opened<'a> {
    /// Which project.
    pub project: ProjectId,
    /// Its directory.
    pub path: &'a Path,
    /// Its display name.
    pub name: &'a str,
    /// Its repository's common git directory, when it is in one.
    pub repository: Option<&'a Path>,
}

/// One project of the column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The repository's folder, or the directory's own name — or what a
    /// person named it, once [`name_groups`] has been asked.
    pub name: String,
    /// What a name given to it is kept by: the common git directory, or the
    /// directory itself outside git. M12.2.
    pub key: PathBuf,
    /// The common git directory; `None` for a directory outside git.
    pub repository: Option<PathBuf>,
    /// A directory outside git: its one row is the group itself.
    pub plain: bool,
    /// Its worktrees, the primary first.
    pub rows: Vec<Row>,
}

/// One worktree of a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A root that is open.
    Opened {
        /// Which project.
        project: ProjectId,
        /// What the row is called.
        title: Title,
        /// Its branch, once git has said.
        branch: Option<String>,
        /// When its `HEAD` was committed — M12 task 16. `None` until git
        /// has answered, and for a root git has not listed.
        committed: Option<i64>,
        /// The lines its branch added and removed, with what is not
        /// committed — M12.4. `None` until git has counted them.
        lines: Option<(u32, u32)>,
    },
    /// A worktree git lists that has not been opened: drawn dimmed, and
    /// opened on its first click.
    Unopened {
        /// Its folder.
        path: PathBuf,
        /// What the row is called.
        title: Title,
        /// Its branch.
        branch: Option<String>,
        /// When its `HEAD` was committed — M12 task 16.
        committed: Option<i64>,
        /// The lines its branch added and removed — M12.4.
        lines: Option<(u32, u32)>,
    },
}

/// Which of five colours a repository's tile is — M12.4.
///
/// The model names the colour and the view maps it to an accent, so
/// nothing here knows a hex value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    /// `accent.brass`.
    Brass,
    /// `accent.patina`.
    Patina,
    /// `accent.violet`.
    Violet,
    /// `accent.rust`.
    Rust,
    /// `accent.green`.
    Green,
}

/// A repository's tile, the same for its name every time — M12.4, a
/// letter on a coloured tile.
///
/// **FNV-1a of the name's bytes, not the standard library's hasher**: Rust
/// does not promise `DefaultHasher` gives the same output from one release
/// to the next, and a colour that moved with the compiler would be a
/// project that changed colour for no reason a person could see.
pub fn tile_colour(name: &str) -> Tile {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    match hash % 5 {
        0 => Tile::Brass,
        1 => Tile::Patina,
        2 => Tile::Violet,
        3 => Tile::Rust,
        _ => Tile::Green,
    }
}

/// What a worktree's row is called — M12.2 task 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title {
    /// Its branch, or its folder when it is on none.
    pub name: String,
    /// The repository's own checkout, which the row marks with a star.
    pub primary: bool,
}

/// Whether a worktree may be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// The main worktree is the repository; it is not offered.
    Primary,
    /// An agent is working or waiting in it: stop it first.
    AgentBusy,
    /// git holds a lock on it, with the reason whoever locked it gave.
    Locked,
    /// It has changes git would refuse to lose.
    Changed,
    /// Nothing is lost that git can see; ask, because git deletes the
    /// ignored files without a word.
    Ask,
}

/// What the group a root is drawn in is kept by — its repository's common
/// git directory, or the directory itself outside git: [`Group::key`]
/// without building the column. A fold and a name are asked by it.
pub fn group_key<'a>(path: &'a Path, repository: Option<&'a Path>) -> &'a Path {
    repository.unwrap_or(path)
}

/// The repository whose worktree list names `path`, by its common git
/// directory — the key git itself will answer with.
///
/// 2026-09-24: a worktree made by hand or by `guild worktree create`, then
/// opened from its row, stood for a moment as a group of its own, because
/// a root's repository was learnt only when its own git thread answered.
/// Its path was in the list already.
pub fn listed_in<'a>(lists: &'a HashMap<PathBuf, Vec<Worktree>>, path: &Path) -> Option<&'a Path> {
    lists
        .iter()
        .find(|(_, list)| list.iter().any(|worktree| worktree.path == path))
        .map(|(common, _)| common.as_path())
}

/// The opened roots of a group, in its rows' order.
pub fn opened_in(group: &Group) -> impl Iterator<Item = ProjectId> + '_ {
    group.rows.iter().filter_map(|row| match row {
        Row::Opened { project, .. } => Some(*project),
        Row::Unopened { .. } => None,
    })
}

/// The column: a group per repository, in the order its first root was
/// opened, and a group of one per directory outside git.
pub fn groups(roots: &[Opened<'_>], lists: &HashMap<PathBuf, Vec<Worktree>>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for root in roots {
        match root.repository {
            None => groups.push(Group {
                name: root.name.to_owned(),
                key: root.path.to_path_buf(),
                repository: None,
                plain: true,
                rows: vec![Row::Opened {
                    project: root.project,
                    title: Title {
                        name: root.name.to_owned(),
                        primary: false,
                    },
                    branch: None,
                    // A directory outside git has no commit to date.
                    committed: None,
                    lines: None,
                }],
            }),
            Some(common) => {
                if groups
                    .iter()
                    .any(|group| group.repository.as_deref() == Some(common))
                {
                    continue;
                }
                groups.push(repository(common, roots, lists.get(common)));
            }
        }
    }
    groups
}

/// One repository's group: the primary first across both Git-listed and
/// remembered roots, then the remaining Git-listed rows in discovery order,
/// then other opened roots Git has not listed, in workspace order. At start
/// nothing has landed, and only remembered opened roots are drawn.
///
/// Opened roots stay listed even if a volume is disconnected. Their seat
/// may hold unsaved buffers; only the person can decide to close it.
fn repository(common: &Path, roots: &[Opened<'_>], list: Option<&Vec<Worktree>>) -> Group {
    let name = repository_name(common);
    let is_primary = |path: &Path| is_primary_checkout(path, Some(common));
    let members: Vec<&Opened<'_>> = roots
        .iter()
        .filter(|root| root.repository == Some(common))
        .collect();

    let mut rows = Vec::new();
    let mut placed = Vec::new();
    for worktree in list
        .into_iter()
        .flatten()
        .filter(|worktree| worktree.shown())
    {
        let title = title(
            &name,
            &worktree.path,
            is_primary(&worktree.path),
            worktree.branch.as_deref(),
        );
        match members.iter().find(|root| root.path == worktree.path) {
            Some(root) => {
                placed.push(root.project);
                rows.push(Row::Opened {
                    project: root.project,
                    title,
                    branch: worktree.branch.clone(),
                    committed: worktree.committed,
                    lines: worktree.lines,
                });
            }
            None => rows.push(Row::Unopened {
                path: worktree.path.clone(),
                title,
                branch: worktree.branch.clone(),
                committed: worktree.committed,
                lines: worktree.lines,
            }),
        }
    }

    let mut unlisted: Vec<&&Opened<'_>> = members
        .iter()
        .filter(|root| !placed.contains(&root.project))
        .collect();
    // `false` sorts first, and the sort is stable: the primary, then the
    // workspace's own order.
    unlisted.sort_by_key(|root| !is_primary(root.path));
    rows.extend(unlisted.into_iter().map(|root| Row::Opened {
        project: root.project,
        title: title(&name, root.path, is_primary(root.path), None),
        branch: None,
        // Git has not listed it, so it has said nothing about its head.
        committed: None,
        lines: None,
    }));

    // A remembered primary may not have landed in Git's list yet.
    rows.sort_by_key(|row| match row {
        Row::Opened { title, .. } | Row::Unopened { title, .. } => !title.primary,
    });
    Group {
        name,
        key: common.to_path_buf(),
        repository: Some(common.to_path_buf()),
        plain: false,
        rows,
    }
}

/// Call each group what a person named it, where they named it — M12.2,
/// *Rename Project*. `name_of` answers by a group's key.
pub fn name_groups(groups: &mut [Group], name_of: impl Fn(&Path) -> Option<String>) {
    for group in groups {
        if let Some(name) = name_of(&group.key) {
            group.name = name;
        }
    }
}

/// The folder a repository is named by: the one its `.git` is in, or a
/// bare repository's own name without `.git`.
pub(crate) fn repository_name(common: &Path) -> String {
    let name = match main_root(common) {
        Some(main) => main
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        None => common
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned()),
    };
    name.unwrap_or_else(|| common.display().to_string())
}

/// Primary identity survives a missing checkout or an absent git listing.
/// Plain folders have no removable linked checkout; git roots use common-dir metadata.
pub fn is_primary_checkout(path: &Path, repository: Option<&Path>) -> bool {
    repository.is_none_or(|common| main_root(common).as_deref() == Some(path))
}

/// Which open root owns a path, and the path within it: **the deepest**
/// root containing it — GLD-055 decision 10. With a project group's folder
/// and its members both open, a member's file belongs to the member, where
/// its language server and git markers are, and only a file outside every
/// member — the group's `AGENTS.md` — to the folder. A linked worktree
/// nested inside its main checkout is the same shape.
pub fn owning_root<'a>(
    roots: impl IntoIterator<Item = (ProjectId, &'a Path)>,
    path: &Path,
) -> Option<(ProjectId, PathBuf)> {
    roots
        .into_iter()
        .filter_map(|(project, root)| {
            path.strip_prefix(root)
                .ok()
                .map(|relative| (root.components().count(), project, relative.to_path_buf()))
        })
        .max_by_key(|(depth, _, _)| *depth)
        .map(|(_, project, relative)| (project, relative))
}

/// The main worktree: the folder a `.git` directory sits in. A bare
/// repository has none.
pub fn main_root(common: &Path) -> Option<PathBuf> {
    if common.file_name()? != ".git" {
        return None;
    }
    common.parent().map(Path::to_path_buf)
}

/// What a worktree's row is called — M12.2 task 2: **its branch**, and a
/// star on the repository's own checkout.
///
/// M12's decision 9 called the main one *primary* and every other one by
/// its folder, with the branch on a second line; the person asked why a
/// word was invented where the branch says what the worktree is. A worktree
/// on no branch — detached, or one git has not listed yet — is still called
/// by its folder, without the `<repository>-` prefix Guild gives the ones
/// it makes.
pub fn title(repository: &str, path: &Path, primary: bool, branch: Option<&str>) -> Title {
    let name = match branch {
        Some(branch) => branch.to_owned(),
        None => {
            let folder = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            match folder.strip_prefix(&format!("{repository}-")) {
                Some(rest) if !rest.is_empty() => rest.to_owned(),
                _ => folder,
            }
        }
    };
    Title { name, primary }
}

/// Where Guild makes a worktree called `name` — decision 4 of M12's plan:
/// beside the repository, `../<repository>-<name>`, git's own manual's
/// example with the repository's name before it. A `/` becomes `-` in the
/// folder and stays in the branch.
pub fn folder_for(main_root: &Path, name: &str) -> PathBuf {
    let repository = main_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    main_root.with_file_name(format!("{repository}-{}", name.replace('/', "-")))
}

/// Whether a worktree may be removed — decision 6 of M12's plan. An agent
/// at work is said first: losing its turn is the thing to prevent. Changes
/// next: git would refuse them anyway, after the window had closed the tabs
/// for nothing.
///
/// **A lock is between the two, and it was missing until M12 task 18.** git
/// refuses a locked worktree before it looks at anything else, so the
/// window walked a person through a question and answered it with git's
/// own *cannot remove a locked working tree* — six lines of terminal
/// English in a notice. The list already says which worktrees are locked
/// and why; a guard that has the fact and waits for git to say it is a
/// guard that is not doing its job.
pub fn removal(primary: bool, agent_busy: bool, locked: bool, changed: bool) -> Removal {
    if primary {
        Removal::Primary
    } else if agent_busy {
        Removal::AgentBusy
    } else if locked {
        Removal::Locked
    } else if changed {
        Removal::Changed
    } else {
        Removal::Ask
    }
}

/// Whether an opened root is a worktree removed outside the window, and
/// may be closed without asking.
///
/// A tool outside Guild removing worktrees left the column drawing them
/// for as long as the window held them open. **Two
/// witnesses, and both must agree**: `list` is git's answer for the root's
/// repository — which only arrives when the repository can be reached, so
/// a disconnected volume never gets this far — and the folder has no
/// `.git`, so a checkout made a moment ago that the list has not caught up
/// with is left alone. A listed entry git calls prunable is git saying the
/// folder is gone.
///
/// **Nothing is decided for the person**: unsaved text or an agent at work
/// keeps the root, and its row's *Remove Worktree…* then closes it with
/// the usual questions. This is what `0d23546` took out on 2026-09-20 —
/// the old test was the folder alone, which closed a seat on a volume
/// nobody had mounted, buffers and all.
pub fn removed_outside(list: &[Worktree], path: &Path, unsaved: bool, live: bool) -> bool {
    !unsaved
        && !live
        && !list
            .iter()
            .any(|worktree| worktree.path == path && !worktree.prunable)
        && !checkout_at(path)
}

/// Whether a folder holds a git checkout: a `.git` directory in the
/// repository's own, a `.git` file in a linked worktree.
pub fn checkout_at(path: &Path) -> bool {
    path.join(".git").exists()
}

/// What the window knows about a worktree a command has asked to remove —
/// M13 task 8.
#[derive(Debug, Clone, Copy)]
pub struct Removing<'a> {
    /// The worktree's folder.
    pub path: &'a Path,
    /// The asking session was itself started with a task.
    pub caller_is_task: bool,
    /// It is the repository's own checkout.
    pub primary: bool,
    /// An agent in it is working or waiting for an answer.
    pub agent_busy: bool,
    /// A tab of a session a person opened is showing in it.
    pub persons_session: bool,
    /// A file of it is open with text the disk does not have.
    pub unsaved: bool,
    /// Why git holds a lock on it, when it does — git's own words.
    pub lock: Option<&'a str>,
    /// It has changes git would refuse to lose.
    pub changed: bool,
    /// What git ignores in it, which goes with the folder without a word.
    pub ignored: &'a [String],
    /// Whether the caller said the ignored paths may go.
    pub delete_ignored: bool,
}

/// How many ignored paths a refusal names before it says how many more.
const IGNORED_NAMED: usize = 20;

/// Why a command may not remove a worktree, in words it can act on — or
/// `None` when nothing stands in the way. M13 task 8.
///
/// **Who is asking comes first**, before any fact about the folder: a task
/// may not remove a worktree whatever state the worktree is in. Then the
/// column's own order — [`removal`] — with two refusals set between an
/// agent at work and a lock, which a person pressing the sheet does not
/// need and an agent does: a session a person opened, and a file with
/// unsaved text, both closed with the folder. **What git ignores is last**,
/// because it is the one refusal a flag answers, and a caller told to pass
/// the flag should not then meet a lock.
pub fn refuse_removal(asked: &Removing<'_>) -> Option<String> {
    let path = asked.path.display();
    if asked.caller_is_task {
        return Some(
            "a session started with a task may not remove a worktree — tell the person \
             it can go"
                .to_owned(),
        );
    }
    match removal(
        asked.primary,
        asked.agent_busy,
        asked.lock.is_some(),
        asked.changed,
    ) {
        Removal::Primary => {
            return Some(format!(
                "{path} is the repository's own checkout, which git does not remove"
            ));
        }
        Removal::AgentBusy => {
            return Some(format!(
                "an agent in {path} is working or waiting for an answer — let it finish first"
            ));
        }
        _ => {}
    }
    if asked.persons_session {
        return Some(format!(
            "{path} has a session a person opened — it is theirs to close first"
        ));
    }
    if asked.unsaved {
        return Some(format!(
            "{path} has a file open with unsaved changes — it is the person's to save or \
             discard first"
        ));
    }
    if let Some(lock) = asked.lock {
        return Some(format!(
            "{path} is locked: {} — whoever locked it has to let go, or the person unlocks \
             it from the column",
            lock.trim()
        ));
    }
    if asked.changed {
        return Some(format!(
            "{path} has tracked or untracked changes git would refuse to lose — run \
             `git status --short --untracked-files=all` in that worktree to identify them, \
             then commit them or ask the person. Ignored-file consent does not allow \
             these changes to be deleted"
        ));
    }
    if !asked.ignored.is_empty() && !asked.delete_ignored {
        let mut said = format!(
            "removing {path} also deletes what git ignores in it, which git does not ask about:"
        );
        for name in asked.ignored.iter().take(IGNORED_NAMED) {
            said.push_str("\n  ");
            said.push_str(name);
        }
        if asked.ignored.len() > IGNORED_NAMED {
            said.push_str(&format!(
                "\n  and {} more",
                asked.ignored.len() - IGNORED_NAMED
            ));
        }
        said.push_str("\nrun it again with --delete-ignored if they may go");
        return Some(said);
    }
    None
}

/// What a command is told when git refuses to remove a worktree — git's
/// own words, and what to do instead of what git advises. M13 task 8.
///
/// **git's refusal is the guard, and its advice is not.** A changed
/// checkout is refused with *use --force to delete it*, and an agent reads
/// advice as an instruction: the next thing it runs is `git worktree remove
/// --force` beside the window, which throws away exactly what git was
/// protecting. The window never forces, so it says so.
pub fn refused_removal(git_said: &str) -> String {
    format!(
        "{}\nthe window never forces a removal: commit or discard those changes, or ask the person",
        git_said.trim()
    )
}

/// What tells one row from another for the window's element state.
///
/// **Not the row's name, which is what M12 first used.** The primary
/// worktree of every repository is called by the same word of the
/// interface, so with two repositories open both rows were
/// `column-row-primary-false` — one `ElementId`, and the groups above them
/// carry no id to tell the two paths apart. GPUI keeps a stateful
/// element's state by that id, so the second row's press was recorded
/// against the first and clicking it never changed the project. Found by
/// the person in the running window, not by a test, which is why the test
/// beside this function exists.
///
/// A project's id and a worktree's folder are unique by construction —
/// the one thing a name is not.
pub fn row_key(workspace: guild_session::WorkspaceId, row: &Row) -> String {
    match row {
        Row::Opened { project, .. } => format!("column-row-{workspace}-p-{project}"),
        Row::Unopened { path, .. } => {
            format!("column-row-{workspace}-w-{}", path.display())
        }
    }
}

/// How near the window's left edge the pointer must come to bring a
/// hidden column out — M12 task 10.
///
/// Six points, which is what the panels' own grips took after three were
/// measured to be what the eye needs and not what a hand needs. Narrower
/// and the panel is a game of accuracy; wider and it arrives while a
/// person is reaching for the first character of a line.
pub const EDGE: f32 = 6.0;

/// Whether a hidden column should be showing itself for this pointer.
///
/// **The zone it leaves by is not the zone it arrives by.** Coming out is
/// the edge alone, so the column does not appear whenever a person's hand
/// passes the left of the window; going away is the whole of its own
/// width, so crossing the panel to reach its button — which is at the
/// panel's far side — cannot take the panel out from under the pointer.
/// The same asymmetry every auto-hiding dock has, and the reason it reads
/// as one rule here rather than as two numbers spread over a renderer.
///
/// `x` is the pointer in the window's own coordinates, `width` the
/// column's. A column that is not folded never peeks: it is already there.
pub fn peek(folded: bool, peeking: bool, x: f32, width: f32) -> bool {
    if !folded {
        return false;
    }
    if peeking { x <= width } else { x <= EDGE }
}

/// What the column may be dragged between, in points.
///
/// Narrower than `NARROWEST` and a worktree's row is its own name cut
/// short — the thing the column exists to show; wider than `WIDEST` and
/// the panel is taking room from the work it is there to get to. The
/// panels' own bounds are worked out from the window because a dock may
/// take half of it; the column holds one list of short names and does
/// not grow with the window.
pub const NARROWEST: f32 = 180.0;
/// The widest the column may be dragged — see [`NARROWEST`].
pub const WIDEST: f32 = 420.0;

/// The narrowest the column may be at interface size `ui_font`: the rows'
/// own floor, or what the title bar draws over the column if that is more —
/// the traffic lights' inset, the column button, the history pair and the
/// mode button standing in the column's corner, one gap between areas short
/// of its edge, with the gaps between them.
/// Those controls scale with the interface and the rows' floor does not,
/// so at a large size the title bar is what decides.
pub fn narrowest(ui_font: f32) -> f32 {
    let rem = f32::from(crate::scale::rem_size(ui_font));
    let token = |value: gpui::Rems| value.0 * rem;
    let traffic =
        token(crate::theme::space::TRAFFIC_LIGHTS).max(crate::theme::space::TRAFFIC_LIGHTS_MIN);
    let button = token(crate::theme::space::STATUS_BUTTON);
    let gap = token(crate::theme::space::CARD_GAP);
    let pair = 2.0 * button + token(crate::theme::space::STATUS_BUTTON_GAP);
    let between = token(crate::theme::space::WORK_BETWEEN);
    // traffic | column | gap | back · forward | gap | mode | to the edge
    let titlebar = traffic + button + gap + pair + gap + button + between;
    // Never past the widest: a settings file can hold an interface size
    // past the settings' range, and a floor above the ceiling is no width.
    NARROWEST.max(titlebar.ceil()).min(WIDEST)
}

/// The column's width for a border dragged to `x`, at interface size
/// `ui_font`.
///
/// The column is against the window's left edge, so the pointer's own x
/// **is** the width — there is no offset to carry from where the drag
/// began, and none to get wrong when the pointer outruns the frames.
pub fn width(x: f32, ui_font: f32) -> f32 {
    x.clamp(narrowest(ui_font), WIDEST)
}

/// Bounded Git facts preserve branch space in the narrow column.
pub fn compact_lines(count: u32) -> String {
    if count > 999 {
        "999+".into()
    } else {
        count.to_string()
    }
}

/// Order repository groups only; stable equal/unknown scores retain manual order.
/// Each row contributes the observed activity of its own checkout.
///
/// `active` says when a group became active ([`guild_session::project_sort::Active`]),
/// and only Recent activity reads it: the active groups first, the earliest
/// to become active first, then the rest by their latest started turn.
pub fn sort_groups(
    groups: &mut [Group],
    mode: guild_session::project_sort::Mode,
    activity: impl Fn(&Row) -> guild_session::project_sort::Activity,
    active: impl Fn(&Group) -> Option<u64>,
) {
    use guild_session::project_sort::{Activity, Mode};
    use std::cmp::Reverse;
    if mode == Mode::Manual {
        return;
    }
    groups.sort_by_cached_key(|group| {
        if mode == Mode::RecentActivity
            && let Some(since) = active(group)
        {
            return (false, since, Reverse(None));
        }
        let mut total = Activity::default();
        for row in &group.rows {
            total.include(activity(row));
        }
        let score = match mode {
            Mode::RecentActivity => total.last_turn,
            Mode::MostActivity => {
                (total.last_activity.is_some() || total.turns > 0).then_some(total.turns)
            }
            Mode::LastUserMessage => total.last_user,
            Mode::Manual => None,
        };
        (true, 0, Reverse(score))
    });
}

/// A project group's header, as the column draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// Which group.
    pub id: guild_session::project_group::ProjectGroupId,
    /// What the person called it.
    pub name: String,
    /// Whether its repositories are hidden under it.
    pub collapsed: bool,
    /// How many of its repositories are drawn under it — or would be, while
    /// it is collapsed.
    pub count: usize,
    /// The folder it is bound to — GLD-055: drawn as the first row under
    /// the header, before its repositories.
    pub folder: Option<PathBuf>,
}

/// One section of the column: a project group's repositories under its
/// header, or the ungrouped repositories, which have none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The group's header; `None` for the ungrouped repositories.
    pub head: Option<Head>,
    /// Its repositories, in the order they came in.
    pub repositories: Vec<Group>,
}

/// Put already ordered repositories into their workspace's project groups.
///
/// **A group stands where its first member comes**: the repositories are
/// walked in the order they arrive — the workspace's manual order, or the
/// sort mode's — and a group's header goes where the first of its members
/// is met, with every member after it in the order they came. So under a
/// sort mode a group is one item ranked by its most active member, and in
/// the manual order it stands where the person put its roots. Consecutive
/// ungrouped repositories share one section without a header. A group none
/// of whose repositories is shown draws no header.
pub fn arrange(
    repositories: Vec<Group>,
    saved: &[guild_session::project_group::ProjectGroup],
) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for repository in repositories {
        let member_of = saved
            .iter()
            .find(|group| group.repositories.contains(&repository.key));
        match member_of {
            Some(group) => {
                match sections.iter_mut().find(|section| {
                    section
                        .head
                        .as_ref()
                        .is_some_and(|head| head.id == group.id)
                }) {
                    Some(section) => section.repositories.push(repository),
                    None => sections.push(Section {
                        head: Some(Head {
                            id: group.id,
                            name: group.name.clone(),
                            collapsed: group.collapsed,
                            count: 0,
                            folder: group.folder.clone(),
                        }),
                        repositories: vec![repository],
                    }),
                }
            }
            None => match sections.last_mut() {
                Some(section) if section.head.is_none() => section.repositories.push(repository),
                _ => sections.push(Section {
                    head: None,
                    repositories: vec![repository],
                }),
            },
        }
    }
    for section in &mut sections {
        let count = section.repositories.len();
        if let Some(head) = &mut section.head {
            head.count = count;
        }
    }
    if sections.is_empty() {
        sections.push(Section {
            head: None,
            repositories: Vec::new(),
        });
    }
    sections
}

/// One element of the column's list, in the order it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A project group's header.
    Head(Head),
    /// A repository, with the group it is drawn in.
    Repository {
        /// The repository.
        group: Group,
        /// Its project group; `None` among the ungrouped.
        section: Option<guild_session::project_group::ProjectGroupId>,
    },
}

/// The column's list as it is drawn: a header per group, then its
/// repositories unless it is collapsed, and the ungrouped repositories
/// where they stand.
pub fn items(sections: &[Section]) -> Vec<Item> {
    let mut drawn = Vec::new();
    for section in sections {
        let id = section.head.as_ref().map(|head| head.id);
        if let Some(head) = &section.head {
            drawn.push(Item::Head(head.clone()));
            if head.collapsed {
                continue;
            }
        }
        drawn.extend(section.repositories.iter().map(|group| Item::Repository {
            group: group.clone(),
            section: id,
        }));
    }
    drawn
}

/// One item at the column's top level: a project group, or a repository in
/// no group. A dragged group takes one of their places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Top {
    /// A project group.
    Group(guild_session::project_group::ProjectGroupId),
    /// An ungrouped repository, by its key.
    Repository(PathBuf),
}

/// The column's top-level items in the order they are drawn.
pub fn tops(sections: &[Section]) -> Vec<Top> {
    let mut found = Vec::new();
    for section in sections {
        match &section.head {
            Some(head) => found.push(Top::Group(head.id)),
            None => found.extend(
                section
                    .repositories
                    .iter()
                    .map(|repository| Top::Repository(repository.key.clone())),
            ),
        }
    }
    found
}

/// What the column's search can land on: a project group's header, or a
/// repository's head.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Listed {
    /// A project group, by its name.
    Group(guild_session::project_group::ProjectGroupId),
    /// A repository, by its key.
    Repository(PathBuf),
}

/// The column's list as it is drawn while the search holds `query`: as
/// [`items`], except that a collapsed group with a repository whose name
/// the query finds is drawn open, so the match can be seen and reached.
/// Nothing is saved — the group is collapsed again once the query is gone.
pub fn items_found(sections: &[Section], query: &str) -> Vec<Item> {
    if query.is_empty() {
        return items(sections);
    }
    let opened: Vec<Section> = sections
        .iter()
        .map(|section| {
            let mut section = section.clone();
            if let Some(head) = &mut section.head
                && head.collapsed
                && section.repositories.iter().any(|repository| {
                    !crate::speed_search::ranges(&repository.name, query).is_empty()
                })
            {
                head.collapsed = false;
            }
            section
        })
        .collect();
    items(&opened)
}

/// What kind of row a drag is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropRow {
    /// An ungrouped repository's block — its head, worktrees and tabs.
    Repository,
    /// A project group's header.
    Head {
        /// Whether the group is drawn collapsed.
        collapsed: bool,
    },
    /// A repository's block inside a group.
    Member {
        /// Whether it is the last of its group's drawn repositories.
        last: bool,
    },
}

/// What is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dragged {
    /// A repository, by its head.
    Project,
    /// A project group, by its header.
    Group,
}

/// Where on a row a drag would land: the place before the row's top-level
/// item, the row itself, or the place after the top-level item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Before the row's top-level item.
    Before,
    /// The row itself.
    Middle,
    /// After the row's top-level item — for a group's last project, after
    /// the whole group.
    After,
}

/// How far into a row from its top or bottom an edge reaches, in points:
/// a quarter of a head, so the middle stays the larger target.
pub const DROP_EDGE: f32 = 7.0;

/// Where a drag at `y` over a row spanning `top..bottom` lands, or `None`
/// when that row takes nothing of this kind there.
///
/// **A row's edges are the places between items**, and the only places a
/// drag reorders by. A top-level row's middle is the row itself: a group's
/// header takes a project into the group, an ungrouped project offers to
/// merge with the one dropped on it. An open group's bottom edge rests on
/// its first project, so it is the group too. Inside a group the edges are
/// places in the group, except the last project's bottom edge, which is the
/// place after the whole group; a project's middle there is no target. A
/// dragged group goes into nothing: rows split into before and after.
pub fn drop_zone(row: DropRow, dragged: Dragged, top: f32, bottom: f32, y: f32) -> Option<Zone> {
    let upper = y < top + DROP_EDGE;
    let lower = y > bottom - DROP_EDGE;
    let lower_half = y >= (top + bottom) / 2.0;
    match (dragged, row) {
        (Dragged::Project, DropRow::Repository | DropRow::Head { collapsed: true }) => {
            Some(if upper {
                Zone::Before
            } else if lower {
                Zone::After
            } else {
                Zone::Middle
            })
        }
        (Dragged::Project, DropRow::Head { collapsed: false }) => {
            Some(if upper { Zone::Before } else { Zone::Middle })
        }
        // Inside a group the places are its lines; a project's middle there
        // is no target.
        (Dragged::Project, DropRow::Member { .. }) => {
            if upper {
                Some(Zone::Before)
            } else if lower {
                Some(Zone::After)
            } else {
                None
            }
        }
        (Dragged::Group, DropRow::Repository | DropRow::Head { collapsed: true }) => {
            Some(if lower_half {
                Zone::After
            } else {
                Zone::Before
            })
        }
        (Dragged::Group, DropRow::Head { collapsed: false }) => Some(Zone::Before),
        (Dragged::Group, DropRow::Member { last }) => (last && lower).then_some(Zone::After),
    }
}

/// Where a drag in the column would land now: the row under the pointer
/// and where on it. The column draws a line for a place between items and
/// lights the row for its middle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropHint {
    /// The workspace whose column it is.
    pub workspace: guild_session::WorkspaceId,
    /// The row under the pointer.
    pub row: Listed,
    /// Where on it.
    pub zone: Zone,
    /// The top-level item its edges are beside: the row's own, or for a
    /// repository inside a group, the group.
    pub beside: Top,
    /// What kind of row it is, which says what its edges mean.
    pub kind: DropRow,
}

/// Whether a zone of a row is a place inside a group — beside one of its
/// projects — rather than beside a top-level item.
pub fn inside_group(kind: DropRow, zone: Zone) -> bool {
    match kind {
        DropRow::Member { last } => match zone {
            Zone::Before => true,
            Zone::After => !last,
            Zone::Middle => false,
        },
        _ => false,
    }
}

/// Where a dragged project was let go of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOn {
    /// A project group's header.
    Head(guild_session::project_group::ProjectGroupId),
    /// A repository's header, in its group or among the ungrouped.
    Repository {
        /// The group it is drawn in.
        group: Option<guild_session::project_group::ProjectGroupId>,
        /// Whether it is the dragged project itself.
        itself: bool,
    },
    /// The list's empty end, below the ungrouped repositories.
    Ungrouped,
}

/// What a drop does to the dragged project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    /// The group it moves to — `Some(None)` out of its group — or `None`
    /// when it stays in the section it is in.
    pub group: Option<Option<guild_session::project_group::ProjectGroupId>>,
    /// Whether it also takes the target repository's place in the order.
    pub reorder: bool,
}

/// What dropping a project of group `from` on `on` means; `None` when it
/// changes nothing.
pub fn landing(
    from: Option<guild_session::project_group::ProjectGroupId>,
    on: &DropOn,
) -> Option<Landing> {
    match *on {
        DropOn::Head(group) => (from != Some(group)).then_some(Landing {
            group: Some(Some(group)),
            reorder: false,
        }),
        DropOn::Repository { itself: true, .. } => None,
        DropOn::Repository { group, .. } => Some(Landing {
            group: (group != from).then_some(group),
            reorder: true,
        }),
        DropOn::Ungrouped => from.is_some().then_some(Landing {
            group: Some(None),
            reorder: false,
        }),
    }
}

/// One tab as the column's tab list needs to know it — `T` is the
/// window's tab identity, which nothing outside the window can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabFact<T> {
    /// Which tab.
    pub id: T,
    /// The worktree and workspace it belongs to.
    pub seat: guild_session::Seat,
    /// The area whose strip it is on.
    pub area: guild_session::AreaId,
    /// Whether any of its panes holds a session — an agent or a shell.
    pub session: bool,
}

/// The tabs listed under a worktree's row: the seat's tabs that hold a
/// session — agents and shells, not files, diffs or pages — in the strips'
/// order, area by area as `areas` lists them; a tab of an area the stage
/// has not named comes last, in the window's order. `tabs` is the window's
/// own list, whose order within an area is that area's strip.
pub fn worktree_tabs<T: Copy>(
    tabs: &[TabFact<T>],
    seat: guild_session::Seat,
    areas: &[guild_session::AreaId],
) -> Vec<T> {
    let mut listed: Vec<&TabFact<T>> = tabs
        .iter()
        .filter(|tab| tab.seat == seat && tab.session)
        .collect();
    // Stable: within an area, the window's order is the strip's.
    listed.sort_by_key(|tab| {
        areas
            .iter()
            .position(|area| *area == tab.area)
            .unwrap_or(areas.len())
    });
    listed.into_iter().map(|tab| tab.id).collect()
}

/// The groups running agent sessions keep active, by workspace and
/// [`group_key`]: a session counts for the group its root is drawn in, a
/// worktree's for its repository's.
pub fn active_groups<'a>(
    sessions: impl IntoIterator<Item = (guild_session::WorkspaceId, &'a Path, Option<&'a Path>)>,
) -> std::collections::HashSet<(guild_session::WorkspaceId, PathBuf)> {
    sessions
        .into_iter()
        .map(|(workspace, path, repository)| (workspace, group_key(path, repository).to_path_buf()))
        .collect()
}
