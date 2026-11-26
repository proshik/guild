//! What git says, read — and written by named actions, three of them by
//! the end of M8.3, each an argument list behind [`Write`] and nothing
//! else: decision 9 of that plan says why three and no more.
//!
//! The branch is read from `.git/HEAD` rather than through a git library.
//! One line of a file against a dependency that brings libgit2 with it is
//! not a close call, and nothing here needs more: the branch is shown,
//! never changed. What needs more than a file — the status of the working
//! tree, a blob out of `HEAD` — comes from `git` itself, run as a command
//! through [`run`]: decision 1 of the M8.2 plan says
//! why, and that the git panel and blame to come go through the same
//! door. Nothing in this module runs on the window's thread.
//!
//! Worktrees are handled because they are where this product is going. A
//! task gets its own worktree — section 6.3 of the spec — and there `.git`
//! is a *file* pointing at the real directory, not a directory itself. A
//! reader that assumes a directory shows no branch at all in exactly the
//! place branches start to matter.

use std::path::{Path, PathBuf};

pub mod auto_ff;
pub mod blame;
pub mod branch_update;
pub mod branches;
pub mod clone;
pub mod hunk;
pub mod inspection;
pub mod log;
mod read_budget;
pub mod stashes;
pub mod status;
pub mod worktree_removal;
pub mod worktrees;

pub use hunk::Hunk;

/// Why git had no answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    /// No `git` on this machine: no marks, no diff, one quiet line.
    #[error("git is not installed")]
    NoGit,
    /// The directory is not inside a repository.
    #[error("not a git repository")]
    NoRepository,
    /// Git ran and refused, in its own words.
    #[error("git failed: {0}")]
    Failed(String),
}

/// `git`, looked up on the login shell's `PATH` and run with it — the
/// `git` a terminal tab would run, which in a `Guild.app` opened from
/// Finder is not launchd's `/usr/bin` one by default. Both doors, the read
/// and the write, start here. Decision 22 of M5.11.
fn git_command() -> std::process::Command {
    let mut command = std::process::Command::new("git");
    if let Some(path) = guild_session::login_path::path() {
        command.env("PATH", path);
    }
    command
}

/// Run `git` in `root` and answer its standard output.
///
/// Every command this crate runs goes through here: a write, when a pass
/// brings one, is one more argument list behind one more action rather
/// than a second way in, and there is one place to say that none of it
/// runs on the window's thread. `core.quotePath=false` so that a path
/// with a non-ASCII name comes back as bytes, not as octal escapes.
pub fn run(root: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    run_with_env(root, args, None)
}
pub(crate) fn run_with_env(
    root: &Path,
    args: &[&str],
    invocation: Option<&std::process::Command>,
) -> Result<Vec<u8>, GitError> {
    #[cfg(test)]
    ran::record(root, args);
    let _permit = read_budget::Permit::acquire();
    let mut command = git_command();
    if let Some(invocation) = invocation {
        for (key, value) in invocation.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
    }
    let output = command
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .current_dir(invocation.and_then(|c| c.get_current_dir()).unwrap_or(root))
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                GitError::NoGit
            } else {
                GitError::Failed(err.to_string())
            }
        })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.contains("not a git repository") {
        Err(GitError::NoRepository)
    } else {
        Err(GitError::Failed(stderr))
    }
}

/// Every command `run` was handed, and where — in a test build only.
///
/// **What a worker does not run is only seen by counting.** The cheap
/// poll of 2026-09-24 is a promise about processes — the list, and no
/// `merge-base`, `diff` or `show` behind it — and an answer that looks the
/// same says nothing about how many forks it took. Tests run in parallel,
/// so a test reads only the commands run under its own folder.
#[cfg(test)]
mod ran {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static RAN: Mutex<Vec<(PathBuf, Vec<String>)>> = Mutex::new(Vec::new());

    pub(super) fn record(root: &Path, args: &[&str]) {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect();
        RAN.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((root.to_path_buf(), args));
    }

    /// The argument lists run under `folder`, spelled as given or
    /// `canonical` — git prints a worktree's path resolved, and the tests
    /// hand it one that is not.
    pub(crate) fn under(folder: &Path, canonical: &Path) -> Vec<Vec<String>> {
        RAN.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|(root, _)| root.starts_with(folder) || root.starts_with(canonical))
            .map(|(_, args)| args.clone())
            .collect()
    }
}

#[cfg(test)]
pub(crate) use ran::under as ran_under;

/// The status of the working tree, as porcelain v2 bytes for
/// [`status::parse`]. Untracked directories are reported as one entry
/// each, not descended into: a fresh `target/` is one line.
pub fn status(root: &Path) -> Result<Vec<u8>, GitError> {
    run(
        root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=normal"],
    )
}

/// A write to the repository. Three by the end of M8.3 — decision 9 —
/// eight by the end of M8.3.3, and **the count stopped being the rule
/// there**: decision 21 of the spec lifted it and put a set of guards in
/// its place. Each write is a verb that refuses what it does not mean,
/// each reference is a string git printed, and where git can refuse, its
/// refusal is the guard and its words are the notice.
///
/// The verbs are spelled here and nowhere else in the crate, which is
/// what `tests/all/git_writes.rs` pins and what the finishing checklist
/// greps for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    /// Fetch only the selected source into its remote-tracking ref, without
    /// tags or submodules — and, through `--refmap=`, into nothing else: a
    /// configured `remote.<name>.fetch` refspec otherwise applies beside the
    /// explicit one and can force-move another ref.
    ///
    /// `refuse_forced` is auto fast-forward's: `fetch.showForcedUpdates=true`
    /// on the argv, so a forced upstream is a refused fetch whatever the
    /// person's config says. The manual Update branch keeps git's own
    /// handling of a forced upstream.
    FetchSource {
        remote: String,
        refspec: String,
        refuse_forced: bool,
    },
    /// Merge a verified source commit into the originating current branch.
    MergeSource { oid: String, message: String },
    /// Move a branch that is checked out nowhere to a commit, only as a
    /// fast-forward: a local `fetch .` without `+`, which git refuses for a
    /// non-fast-forward and for a branch checked out in any worktree.
    FastForwardBranch { branch: String, oid: String },
    /// Fast-forward the checked-out branch to a commit, and nothing else:
    /// the branch's configured merge options are emptied and the strategy
    /// named, because a configured `--squash`, `-s ours` or
    /// `pull.twohead=ours` otherwise turns `--ff-only` into a squash or a
    /// merge commit. `--no-overwrite-ignore`: an ignored file the commit
    /// starts to track is refused rather than overwritten, as nothing of it
    /// is in git.
    FastForwardCheckout { branch: String, oid: String },

    /// Clone only through the private staging and cancellation gateway.
    Clone {
        /// A validated credential-free remote URL.
        url: String,
        /// Internal staging destination.
        destination: PathBuf,
    },
    /// `git add`: the paths into the index — a change, a deletion or an
    /// untracked file, which is what `add` means for each of them.
    Stage(Vec<PathBuf>),
    /// `git restore --staged`: the paths' index entries back to what
    /// `HEAD` has, the working tree untouched.
    Unstage(Vec<PathBuf>),
    /// `git restore --source=HEAD --staged --worktree`: the paths back
    /// to what the last commit holds, in the index and on the disk both
    /// — Rollback.
    ///
    /// **The same verb as [`Write::Unstage`] and not the same write.**
    /// One takes the index back and leaves the disk as it is; this one
    /// takes both. `--source=HEAD` is what makes the second half say
    /// *the last commit* rather than *the index*, which is what
    /// `--worktree` means on its own.
    ///
    /// **`restore`, not `checkout HEAD --`.** `checkout` is the verb M8.3.3
    /// refused for `switch`'s sake: restoring a path is one of the five other
    /// things it does, so spelling it here would put the whole of `checkout`
    /// behind a menu item that means one of them.
    ///
    /// **A file that `HEAD` does not have is never in this list.** git
    /// takes `restore --source=HEAD --worktree` on a path outside
    /// `HEAD` to mean *there should be nothing here* and **deletes it
    /// from the disk** — measured, not reasoned about. The expected
    /// rollback of a newly added file is to unstage it and leave it, so
    /// `root::changes` sends those to [`Write::Unstage`] instead and an
    /// unversioned row is offered no rollback at all.
    Rollback(Vec<PathBuf>),
    /// `git commit -F <file>`: the index as a commit, with the message
    /// read from a file so that quotes, newlines and `#` lines arrive
    /// intact, and `--cleanup=whitespace` because the default cleanup
    /// strips a line that starts with `#` as a comment.
    Commit { message_file: PathBuf },
    /// `git switch -c <name>`: a branch made from what was typed into
    /// the popup's field, and moved onto.
    ///
    /// The name is a person's own typing, which is why it is the only
    /// write here that takes one. `branch_list::offers_create` keeps
    /// the obvious mistakes out of the list; everything else is git's
    /// to refuse, and `git check-ref-format`'s rule is git's alone.
    Create(String),
    /// `git switch <branch>`: what the branch popup does when a branch
    /// is chosen.
    ///
    /// **`switch`, not `checkout`.** `checkout` does five other things
    /// besides changing branch, and one of them — restoring a path — is
    /// the destructive gesture decision 21 puts behind a sheet. `switch`
    /// refuses everything but a branch, so the verb itself is the guard.
    /// The name is one `git branch` printed, never a person's typing.
    Switch(String),
    /// `git branch -d <name>`: a branch removed, and only where git
    /// agrees nothing is lost by it.
    ///
    /// **`-d`, never `-D`.** The lower case refuses a branch whose
    /// commits sit on no other, and that refusal *is* the guard: the
    /// window does not work out what is merged, git does, and says so in
    /// its own words. The upper case is the spelling that loses work and
    /// is spelled nowhere in this crate.
    DeleteBranch(String),
    /// `git stash apply <ref>`: an entry's changes onto the working
    /// tree, the entry left where it is.
    ///
    /// **`apply`, not `pop`.** `pop` drops the entry when it succeeds,
    /// so a half-applied conflict would leave a person with the mess and
    /// no stash to return to. Dropping is a gesture of its own, with a
    /// bin of its own.
    ///
    /// The reference is one `git stash list` printed. Never an index we
    /// counted: the list shifts under a drop.
    ApplyStash(String),
    /// `git stash drop <ref>`: an entry gone.
    ///
    /// **Carried by its commit, spelled by its position.** git takes only
    /// `stash@{n}`, and the list shifts under it, so the reference here is
    /// the one the person was shown and the commit is the entry itself;
    /// [`settle`] rewrites the reference to where that commit is at the
    /// moment of the drop, and refuses one no longer listed. The window
    /// asks in a sheet first — git keeps a dropped entry's commit only
    /// until it collects garbage. Decision 17 of M5.11.
    DropStash {
        /// `stash@{n}` as the list the person saw printed it.
        reference: String,
        /// The entry's commit, as that list printed it.
        sha: String,
    },
    /// `git push [--set-upstream] <remote> <refspec>`: the branch's
    /// commits to a remote. The tenth write, and the first that reaches
    /// another machine.
    ///
    /// **No force in any spelling.** `--force` overwrites what somebody
    /// else pushed and `--force-with-lease` only narrows the window in
    /// which it can; neither is in `args`, and
    /// `no_write_carries_a_destructive_spelling` matches both by
    /// substring. A push git rejects is a rejection in git's own words
    /// — *Updates were rejected because the remote contains work* — and
    /// what to do about it is a pull, which is a pass of its own.
    ///
    /// **Both names are strings git printed**: the remote is one of the
    /// names `git remote` gave, and the refspec is built from the branch
    /// `git symbolic-ref` gave and the upstream `git rev-parse` gave.
    /// Nothing here is typed, which is decision 21's rule met by the one
    /// write that leaves this machine.
    Push {
        /// One of the names `git remote` printed.
        remote: String,
        /// `<branch>` when both sides carry the same name, and
        /// `<branch>:<onto>` when they do not — see [`Destination`].
        refspec: String,
        /// The branch has no upstream on this remote, so the push sets
        /// one.
        set_upstream: bool,
        /// Which tags go with it — M8.3.6, *Push tags*.
        tags: PushTags,
        /// Whether the repository's pre-push hook runs. Defaults to true.
        run_hooks: bool,
    },
    /// `git worktree add -b <branch> <path>`: a checkout of its own for a
    /// branch of its own — the eleventh write, M12.
    ///
    /// **`-b`, never `-B`.** The lower case refuses a branch that already
    /// exists; the upper case resets it to the base and loses the commits
    /// on it. The refusal is the guard, as `branch -d`'s is, and git's
    /// words are the notice.
    ///
    /// The path is Guild's own — `<repository>-<name>` beside the
    /// repository, which is git's manual's `../hotfix` with the
    /// repository's name in front of it — and the branch is what was
    /// typed into the branch popup, the second write to take a person's
    /// typing after [`Write::Create`]. The base is git's own start point,
    /// the last word; without one it is `HEAD` of the root this runs in,
    /// which is what `add` uses when it is given none — the column's `+`.
    /// `guild worktree create` gives the repository's main branch unless
    /// told otherwise, M13.1.
    WorktreeAdd {
        /// Where the checkout goes.
        path: PathBuf,
        /// The branch it is made on.
        branch: String,
        /// Where the branch starts; `None` is `HEAD`.
        base: Option<String>,
    },
    /// `git worktree remove <path>`: a checkout gone, its branch left
    /// where it is — the twelfth write, M12.
    ///
    /// **No `--force`, in any spelling.** git refuses a worktree with a
    /// modified or an untracked file in it, and that refusal is the
    /// guard. What git does *not* refuse is the ignored files — an
    /// `.env`, a `target/` — which go with the folder without a word;
    /// that is the window's to ask about first, which is decision 6 of
    /// M12's plan and decision 30 of the spec met from the side that is
    /// easy to miss.
    WorktreeRemove(PathBuf),
    /// GUI-only consent: bind the folder/branch/commit and optionally remove the
    /// confirmed branch using the existing safe branch-deletion gateway.
    WorktreeRemoveConfirmed {
        target: worktree_removal::Target,
        delete_branch: bool,
    },
    /// `git worktree unlock <path>`: the thirteenth write, M12 task 18.
    ///
    /// **A lock outlives whoever took it.** git locks a checkout on
    /// request — a tool writes its name and its pid into the reason — and
    /// nothing removes that when the process dies, which is the trap the
    /// IDE lock file already carries in this window. A dead session's lock
    /// then refuses every removal for ever, and git's own advice is
    /// `remove -f -f`, which is exactly the spelling this repository bars.
    ///
    /// So the window unlocks instead: it loses nothing — a lock is a note
    /// saying *somebody is using this*, not a change to the tree — and it
    /// is a deliberate act by a person who has been shown who wrote the
    /// note.
    WorktreeUnlock(PathBuf),
    /// `git apply --cached` of a binary patch with a full index line: the
    /// index entry's new content, computed by the window from the entry it
    /// read, swapped in by git only over that entry — decision 55. The
    /// shown working lines go into the index; the disk is not touched.
    StageHunk(Hunk),
    /// The same write putting `HEAD`'s lines back into the index for one
    /// shown block. **Not `--reverse`:** the patch is the entry's new
    /// content either way, so both writes are one spelling and the
    /// difference is what was computed — and that `HEAD` joins what is
    /// checked.
    UnstageHunk(Hunk),
}

impl Write {
    /// The push a destination means.
    ///
    /// A constructor rather than a struct literal at the call site,
    /// because the refspec is the one thing about a push that is easy
    /// to get quietly wrong: `push origin bar` means `bar:bar` whatever
    /// `bar` tracks, so a branch whose upstream is `origin/foo` needs
    /// the pair spelled or it grows a second remote branch and leaves
    /// the one it tracks behind.
    pub fn push(to: &Destination) -> Self {
        Self::push_with(to, PushTags::None)
    }

    /// A push that takes tags with it — M8.3.6.
    pub fn push_with(to: &Destination, tags: PushTags) -> Self {
        Self::push_with_hooks(to, tags, true)
    }

    /// The dialog's explicit hook choice; other callers run hooks.
    pub fn push_with_hooks(to: &Destination, tags: PushTags, run_hooks: bool) -> Self {
        Write::Push {
            tags,
            run_hooks,
            remote: to.remote.clone(),
            refspec: if to.onto == to.branch {
                to.branch.clone()
            } else {
                format!("{}:{}", to.branch, to.onto)
            },
            set_upstream: to.set_upstream,
        }
    }

    /// The hunk a hunk write carries.
    pub fn hunk(&self) -> Option<&Hunk> {
        match self {
            Self::StageHunk(hunk) | Self::UnstageHunk(hunk) => Some(hunk),
            _ => None,
        }
    }

    /// Explicit project-relative identities carried separately from argv.
    pub fn pathspecs(&self) -> Option<&[PathBuf]> {
        match self {
            Self::Stage(paths) | Self::Unstage(paths) | Self::Rollback(paths) => Some(paths),
            _ => None,
        }
    }

    /// Actual bounded argv. Pathspec writes read literal NUL-delimited paths
    /// from an owned stdin file; paths never become process arguments.
    pub fn args(&self) -> Vec<String> {
        let verb: &[&str] = match self {
            Write::FetchSource {
                remote,
                refspec,
                refuse_forced,
            } => {
                if *refuse_forced {
                    return exact_fetch(remote, refspec);
                }
                return vec![
                    "fetch".into(),
                    "--no-tags".into(),
                    "--no-prune".into(),
                    "--no-recurse-submodules".into(),
                    "--refmap=".into(),
                    "--".into(),
                    remote.clone(),
                    refspec.clone(),
                ];
            }
            Write::FastForwardBranch { branch, oid } => {
                return exact_fetch(".", &format!("{oid}:refs/heads/{branch}"));
            }
            Write::FastForwardCheckout { branch, oid } => {
                return vec![
                    "-c".into(),
                    "merge.autoStash=false".into(),
                    "-c".into(),
                    format!("branch.{branch}.mergeOptions="),
                    "merge".into(),
                    "--ff-only".into(),
                    "--no-squash".into(),
                    "--commit".into(),
                    "--no-autostash".into(),
                    "--no-overwrite-ignore".into(),
                    "--strategy=ort".into(),
                    "--".into(),
                    oid.clone(),
                ];
            }
            Write::MergeSource { oid, message } => {
                return vec![
                    "-c".into(),
                    "merge.autoStash=false".into(),
                    "merge".into(),
                    "--no-edit".into(),
                    "--no-autostash".into(),
                    "-m".into(),
                    message.clone(),
                    "--".into(),
                    oid.clone(),
                ];
            }
            Write::Clone { url, destination } => {
                return vec![
                    "clone".into(),
                    "--progress".into(),
                    "--".into(),
                    url.clone(),
                    destination.to_string_lossy().into_owned(),
                ];
            }
            Write::Stage(_) => &[
                "--literal-pathspecs",
                "add",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            Write::Unstage(_) => &[
                "--literal-pathspecs",
                "restore",
                "--staged",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            Write::Rollback(_) => &[
                "--literal-pathspecs",
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--pathspec-from-file=-",
                "--pathspec-file-nul",
            ],
            Write::Commit { message_file } => {
                return vec![
                    "commit".to_owned(),
                    "-F".to_owned(),
                    message_file.to_string_lossy().into_owned(),
                    "--cleanup=whitespace".to_owned(),
                ];
            }
            Write::Switch(branch) => {
                return vec!["switch".to_owned(), branch.clone()];
            }
            Write::Create(name) => {
                return vec!["switch".to_owned(), "-c".to_owned(), name.clone()];
            }
            Write::DeleteBranch(name) => {
                return vec![
                    "branch".to_owned(),
                    "-d".to_owned(),
                    "--".to_owned(),
                    name.clone(),
                ];
            }
            Write::ApplyStash(reference) => {
                return vec!["stash".to_owned(), "apply".to_owned(), reference.clone()];
            }
            Write::DropStash { reference, .. } => {
                return vec!["stash".to_owned(), "drop".to_owned(), reference.clone()];
            }
            Write::Push {
                remote,
                refspec,
                set_upstream,
                tags,
                run_hooks,
            } => {
                let mut args = vec!["push".to_owned()];
                if !run_hooks {
                    args.push("--no-verify".to_owned());
                }
                if *set_upstream {
                    args.push("--set-upstream".to_owned());
                }
                match tags {
                    PushTags::None => {}
                    PushTags::All => args.push("--tags".to_owned()),
                    PushTags::CurrentBranch => args.push("--follow-tags".to_owned()),
                }
                args.push(remote.clone());
                args.push(refspec.clone());
                return args;
            }
            Write::WorktreeAdd { path, branch, base } => {
                let mut args = vec![
                    "worktree".to_owned(),
                    "add".to_owned(),
                    "-b".to_owned(),
                    branch.clone(),
                    path.to_string_lossy().into_owned(),
                ];
                args.extend(base.clone());
                return args;
            }
            Write::WorktreeUnlock(path) => {
                return vec![
                    "worktree".to_owned(),
                    "unlock".to_owned(),
                    path.to_string_lossy().into_owned(),
                ];
            }
            Write::WorktreeRemoveConfirmed { target, .. } => {
                // The GUI-only command also has a distinct argv identity, so
                // its completion cannot answer a simultaneous legacy CLI ask.
                let mut args = Write::WorktreeRemove(target.path.clone()).args();
                args.insert(2, "--".into());
                return args;
            }
            Write::WorktreeRemove(path) => {
                return vec![
                    "worktree".to_owned(),
                    "remove".to_owned(),
                    path.to_string_lossy().into_owned(),
                ];
            }
            Write::StageHunk(_) | Write::UnstageHunk(_) => &hunk::ARGS,
        };
        verb.iter().map(|word| (*word).to_owned()).collect()
    }
}

/// A fetch that writes the named destination and nothing else, and only
/// as a fast-forward — whatever `fetch.showForcedUpdates` and the remote's
/// configured refspecs say. Measured on git 2.50.1: with
/// `fetch.showForcedUpdates=false` a refspec without `+` replaces a
/// diverged ref, and a configured refspec applies as a refmap beside an
/// explicit one.
fn exact_fetch(remote: &str, refspec: &str) -> Vec<String> {
    vec![
        "-c".into(),
        "fetch.showForcedUpdates=true".into(),
        "fetch".into(),
        "--no-tags".into(),
        "--no-prune".into(),
        "--no-recurse-submodules".into(),
        "--refmap=".into(),
        "--".into(),
        remote.into(),
        refspec.into(),
    ]
}

/// Everything git printed about one command, both channels.
///
/// [`run`] answers with stdout and drops stderr, which is right for the
/// ten things that *read*: their answer is on stdout and a failure is an
/// error. A write is the other way round — *Switched to branch 'main'*
/// and *Deleted branch spike* are **stderr, on success** — and the
/// Console shows what git said, not what git returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    pub out: String,
    pub err: String,
}

impl Said {
    /// Both channels as one block, in the order git wrote them.
    pub fn text(&self) -> String {
        let mut text = self.out.trim_end().to_owned();
        if !self.err.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(self.err.trim_end());
        }
        text
    }
}

/// A write as it may be carried out now — decision 17 of M5.11.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled {
    /// Run it: any write but a drop as it came in, and a drop with the
    /// reference its commit has now.
    Ready(Write),
    /// A drop whose commit no entry of the stash is any more. Nothing is
    /// run. **Not a [`GitError`]**: git refused nothing, and that enum is
    /// matched with no wildcard in places this has no business changing.
    StashGone(Write),
    /// The stash list could not be read, in git's words. Nothing is run.
    Failed(Write, GitError),
    /// A hunk write whose entry, disk or `HEAD` is no longer what it was
    /// computed from. Nothing is run — decision 55.
    Stale(Write),
    /// A hunk write git would store converted. Nothing is run.
    Converted(Write),
    /// A hunk write past the size hunks are computed for. Nothing is run.
    WholeFile(Write),
}

/// What a write means at the moment it runs.
///
/// Every write but a drop is `Ready` as it came, and git is asked
/// nothing. A drop asks `git stash list` and is `Ready` with the position
/// its commit has **now** — a stash pushed after the person's list was
/// read has moved it — or `StashGone` when no entry is that commit. Read
/// on the git thread, just before the write, so nothing can move the list
/// between the two but another process in the same few milliseconds.
pub fn settle(root: &Path, write: Write) -> Settled {
    if let Some(hunk) = write.hunk() {
        return match hunk::settle(root, hunk) {
            Ok(()) => Settled::Ready(write),
            Err(hunk::Refusal::Stale) => Settled::Stale(write),
            Err(hunk::Refusal::Converted) => Settled::Converted(write),
            Err(hunk::Refusal::WholeFile) => Settled::WholeFile(write),
            Err(hunk::Refusal::Failed(err)) => Settled::Failed(write, err),
        };
    }
    let Write::DropStash { sha, .. } = &write else {
        return Settled::Ready(write);
    };
    let listed = stashes(root)
        .and_then(|bytes| stashes::parse(&bytes).map_err(|err| GitError::Failed(err.to_string())));
    let listed = match listed {
        Ok(listed) => listed,
        Err(err) => return Settled::Failed(write, err),
    };
    match stashes::reference_of(&listed, sha) {
        Some(reference) => Settled::Ready(Write::DropStash {
            reference: reference.to_owned(),
            sha: sha.clone(),
        }),
        None => Settled::StashGone(write),
    }
}

/// Carry out one write, keeping what git said about it.
///
/// The refusal is still the point — it becomes a notice in git's own
/// words — and now the words of a *success* are kept too, for the
/// Console. Nothing else about the call changes: same process, same
/// thread, same status walk after it.
///
/// A write that touches the checkout waits for its [`turn`] in it: two
/// roots opened in one checkout have a git thread each.
pub fn write(root: &Path, write: &Write) -> Result<Said, GitError> {
    let _turn = match write {
        // Refs and objects only: no checkout of anybody's to wait for, and
        // a fetch may take its whole deadline.
        Write::FetchSource { .. } | Write::FastForwardBranch { .. } | Write::Clone { .. } => None,
        _ => turn(root),
    };
    write_in_turn(root, write)
}

/// [`write`] for a caller that already holds the checkout's turn.
pub(crate) fn write_in_turn(root: &Path, write: &Write) -> Result<Said, GitError> {
    if let Write::WorktreeRemoveConfirmed {
        target,
        delete_branch,
    } = write
    {
        return worktree_removal::write(root, target, *delete_branch);
    }
    write_one(root, write)
}

/// Guild's writes in one checkout take turns — review r1 of auto
/// fast-forward. Every root has a git thread of its own, and two roots can
/// be one checkout (the checkout and a folder in it), so the threads alone
/// serialize nothing between them: a `switch` from one could land between
/// another's preflight and its `merge --ff-only`, and the merge would move
/// the branch switched to. git has no single command that checks both the
/// branch `HEAD` names and the checkout before a fast-forward, so the
/// window takes turns per canonical checkout instead. A process outside
/// Guild is not covered; the fast-forward's post-check reports what it did.
///
/// `None` when git cannot say which checkout `root` is: the write then
/// goes ahead as it always did, and git refuses what it refuses.
pub(crate) fn turn(root: &Path) -> Option<Turn> {
    let top = checkout_of(root)?;
    let (turns, freed) = &*TURNS;
    let mut turns = turns
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    while turns.taken.contains(&top) {
        *turns.waiting.entry(top.clone()).or_default() += 1;
        turns = freed
            .wait(turns)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(count) = turns.waiting.get_mut(&top) {
            *count -= 1;
        }
    }
    turns.taken.insert(top.clone());
    Some(Turn(top))
}

/// How many writers wait for this checkout's turn — for the tests that
/// must know a writer is really waiting, not merely started.
#[cfg(test)]
pub(crate) fn waiting_for_turn(checkout: &Path) -> usize {
    let (turns, _) = &*TURNS;
    turns
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .waiting
        .get(checkout)
        .copied()
        .unwrap_or_default()
}

/// The checkout `root` is in, canonical: what git calls its toplevel. Only
/// the line's own newline is taken off — a path may end in a space, and a
/// trimmed one names a folder that does not exist — and the bytes stay
/// bytes: a path need not be UTF-8.
pub(crate) fn checkout_of(root: &Path) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let top = run(root, &["rev-parse", "--show-toplevel"]).ok()?;
    let top = top.strip_suffix(b"\n").unwrap_or(&top);
    Path::new(std::ffi::OsStr::from_bytes(top))
        .canonicalize()
        .ok()
}

#[derive(Default)]
struct Turns {
    taken: std::collections::HashSet<PathBuf>,
    waiting: std::collections::HashMap<PathBuf, usize>,
}

static TURNS: std::sync::LazyLock<(std::sync::Mutex<Turns>, std::sync::Condvar)> =
    std::sync::LazyLock::new(Default::default);

/// A checkout's turn, given back when dropped — a panic included.
pub(crate) struct Turn(PathBuf);

impl Drop for Turn {
    fn drop(&mut self) {
        let (turns, freed) = &*TURNS;
        turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .taken
            .remove(&self.0);
        freed.notify_all();
    }
}

/// One named command; the composite gateway calls this for its removal step.
fn write_one(root: &Path, write: &Write) -> Result<Said, GitError> {
    if matches!(write, Write::FetchSource { .. }) {
        return branch_update::write_fetch(root, write, std::time::Duration::from_secs(120));
    }
    if matches!(write, Write::Clone { .. }) {
        return Err(GitError::Failed(
            "Clone requires its owned staging gateway.".into(),
        ));
    }
    if let Some(hunk) = write.hunk() {
        return hunk::write(root, hunk);
    }
    // Closing tabs queues their final state. Drain it before git starts,
    // and exclude late saves (including .gitignore) until git has finished.
    let _removal = match write {
        Write::WorktreeRemove(path)
        | Write::WorktreeRemoveConfirmed {
            target: worktree_removal::Target { path, .. },
            ..
        } => Some(
            guild_session::project_state::prepare_removal(path)
                .map_err(|err| GitError::Failed(err.to_string()))?,
        ),
        _ => None,
    };
    // Reject unsupported identities before constructing or executing any command.
    // Status parsing promises UTF-8; a replacement-character sibling is never a substitute.
    let input = if let Some(paths) = write.pathspecs() {
        use std::io::{Seek, Write as _};
        if paths
            .iter()
            .any(|path| path.as_os_str().as_encoded_bytes().contains(&0))
        {
            return Err(GitError::Failed("Path contains NUL".into()));
        }
        if paths.iter().any(|path| path.to_str().is_none()) {
            return Err(GitError::Failed("Unsupported non-UTF-8 path".into()));
        }
        let mut input = tempfile::tempfile().map_err(|err| GitError::Failed(err.to_string()))?;
        for path in paths {
            input
                .write_all(path.as_os_str().as_encoded_bytes())
                .and_then(|()| input.write_all(&[0]))
                .map_err(|err| GitError::Failed(err.to_string()))?;
        }
        input
            .rewind()
            .map_err(|err| GitError::Failed(err.to_string()))?;
        Some(input)
    } else {
        None
    };
    let args = write.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut command = git_command();
    let proxy = if let Write::Push { remote, .. } = write {
        crate::network::git(&mut command, root, remote, true, true)?
    } else {
        crate::network::GitPolicy::default()
    };
    if let Some(input) = input {
        command.stdin(input);
    }
    let output = command
        .args(["-c", "core.quotePath=false"])
        .args(&args)
        .current_dir(root)
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                GitError::NoGit
            } else {
                GitError::Failed(err.to_string())
            }
        })?;

    let said = Said {
        out: proxy.redact(&String::from_utf8_lossy(&output.stdout)),
        err: proxy.redact(&String::from_utf8_lossy(&output.stderr)),
    };
    if output.status.success() {
        return Ok(said);
    }
    let stderr = said.text();
    if stderr.contains("not a git repository") {
        Err(GitError::NoRepository)
    } else {
        Err(GitError::Failed(stderr))
    }
}

/// Which local branches contain a commit.
///
/// `git branch --contains <sha>`, one name a line, the current one
/// marked with `*` which is stripped here. A **read**: it moves nothing,
/// and it is what the opened commit's footer says — *In 2 branches*.
pub fn contains(root: &Path, sha: &str) -> Result<Vec<String>, GitError> {
    let bytes = run(
        root,
        &["branch", "--contains", sha, "--format=%(refname:short)"],
    )?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The marked format the log is read in — decision 8 of M8.3: a NUL between
/// fields, `\x01` after each commit, and `-z` so nothing in a message has to
/// be escaped.
///
/// `%D` joined it in M8.3.4 rather than becoming a second ask: task 0
/// priced it at three milliseconds on top of a twenty-seven millisecond
/// page, against a second `git log` at twenty-seven.
const LOG_FORMAT: &str = "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%D%x00%s%x00%b%x00%x01";

/// What the log's toolbar narrows the list by.
///
/// **Arguments of `git log`, not a pass over what is loaded** —
/// decision 3 of M8.3.4. A page is two hundred commits, and a filter that
/// searched only those would lie about the rest; task 0 priced every one
/// of these within four milliseconds of an unfiltered page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// What was typed into the field: words, or a hash.
    pub text: String,
    /// An author's name, as `git log --author` matches it.
    pub author: Option<String>,
    /// A date as git reads one — `1.week`, `2026-08-01`.
    pub since: Option<String>,
}

impl Filter {
    /// A filter that is only what was typed. For tests and for the field.
    pub fn text(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::default()
        }
    }

    /// Whether what was typed is a hash rather than words.
    ///
    /// **Four or more hex characters and nothing else** — decision 4.
    /// Pasting a hash and getting the commit is what the gesture is for,
    /// and `--grep=8f607b1` finds only a message that mentions it. Three
    /// is too few: a hash that short matches half a repository, and `abc`
    /// is something a person wrote.
    pub fn hash(&self) -> Option<&str> {
        let text = self.text.trim();
        (text.len() >= 4 && text.chars().all(|c| c.is_ascii_hexdigit())).then_some(text)
    }
}

/// What a page of the log is asked about.
///
/// **Three states, not two.** `All` is every ref git knows — its own
/// `--all` — and is the log unnarrowed; `Head` is the head of the branch
/// checked out, which is git's default and therefore no argument at all;
/// `Named` is one branch. `All` and `Head` were one state while the
/// scope was an `Option<String>`, so *Branch: HEAD* was both the default
/// and the choice and there was nothing for a cross to take off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every branch and tag: `--all`.
    #[default]
    All,
    /// Wherever `HEAD` points, which is what `git log` alone answers.
    Head,
    /// One branch, by one of the names `git branch` printed.
    Named(String),
}

impl Scope {
    /// The branch this scope names, if it names one.
    pub fn branch(&self) -> Option<&str> {
        match self {
            Self::Named(name) => Some(name),
            _ => None,
        }
    }
}

/// The arguments of a log ask.
///
/// Pure, so the shape can be read without running anything. A named ref
/// goes **after** the options and is followed by `--`: `git log` takes
/// `[<options>] [<revision-range>] [[--] <path>…]`, and without the
/// separator a branch that shares its name with a file is ambiguous —
/// git says so and refuses. The name itself is never typed by a person:
/// it is one of the names `git branch` printed.
///
/// **A hash beats a chosen branch.** A person who picked a branch and
/// then pasted a hash means the hash: it names one commit and the branch
/// names a range, and the narrower answer is the one just typed.
pub fn log_args(scope: &Scope, filter: &Filter, skip: usize, n: usize) -> Vec<String> {
    let mut args: Vec<String> = ["log", "--topo-order", LOG_FORMAT, "-z", "-n"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    args.push(n.to_string());
    args.push("--skip".to_owned());
    args.push(skip.to_string());

    let hash = filter.hash().map(str::to_owned);
    if hash.is_none() {
        let text = filter.text.trim();
        if !text.is_empty() {
            args.push(format!("--grep={text}"));
            args.push("--regexp-ignore-case".to_owned());
        }
    }
    if let Some(author) = &filter.author {
        args.push(format!("--author={author}"));
    }
    if let Some(since) = &filter.since {
        args.push(format!("--since={since}"));
    }

    // `--all` is an option and goes among the options; a name is a
    // revision and goes after them, before `--`.
    match (hash.as_deref(), scope) {
        (Some(revision), _) => {
            args.push(revision.to_owned());
            args.push("--".to_owned());
        }
        (None, Scope::Named(name)) => {
            args.push(name.clone());
            args.push("--".to_owned());
        }
        (None, Scope::All) => args.push("--all".to_owned()),
        (None, Scope::Head) => {}
    }
    args
}

/// A page of the log, newest first, as bytes for [`log::parse`]: `n`
/// commits after skipping `skip` — the next page of a list is "the same
/// walk, further on", which `--skip` says exactly. A repository with no
/// commit yet has an empty log, not an error.
pub fn log(
    root: &Path,
    scope: &Scope,
    filter: &Filter,
    skip: usize,
    n: usize,
) -> Result<Vec<u8>, GitError> {
    let args = log_args(scope, filter, skip, n);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(root, &args) {
        Err(GitError::Failed(message)) if message.contains("does not have any commits") => {
            Ok(Vec::new())
        }
        other => other,
    }
}

/// One commit's files with their status letters, as bytes for
/// [`log::files_of`]. The message is left out: it is already known.
pub fn commit_files(root: &Path, sha: &str) -> Result<Vec<u8>, GitError> {
    if sha.contains(',') {
        return inspection::read(root, sha).map(|changes| inspection::name_status(&changes));
    }
    run(root, &["show", "--name-status", "-z", "--format=", sha])
}

/// The local branches with the current one marked and each tip's author,
/// age and subject, as bytes for [`branches::parse`]: 8.4 ms on this
/// repository, task 0 of M8.3.3 — three tenths of a millisecond over the
/// name and the mark alone, which is why there is one format for the
/// popup and the Log's column rather than two.
pub fn branches(root: &Path) -> Result<Vec<u8>, GitError> {
    run(
        root,
        &[
            "branch",
            "--format=%(refname:short)%00%(HEAD)%00%(authorname)%00%(committerdate:relative)%00%(contents:subject)",
        ],
    )
}

/// The branch a task's new worktree starts from, by
/// [`crate::branch_list::main_branch`]'s rule — M13.1. Two reads: what
/// `origin/HEAD` names, which a repository without that remote or that
/// ref answers with nothing, and the local branches by name. `None` when
/// git will not say, which leaves the write its `HEAD`.
pub fn main_branch(root: &Path) -> Option<String> {
    let origin = run(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .ok()
    .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned());
    let locals = run(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .ok()?;
    let locals: Vec<String> = String::from_utf8_lossy(&locals)
        .lines()
        .map(str::to_owned)
        .collect();
    crate::branch_list::main_branch(origin.as_deref(), &locals)
}

/// The stash, newest first, as bytes for [`stashes::parse`]: 7.2 ms on
/// this repository, task 0 of M8.3.3. The entry's commit is the second
/// field since M5.11, because a drop is carried by it. `%x00` because the stash list
/// speaks the log's format, where that is the NUL — `git branch` above
/// speaks ref-filter's, where the same byte is `%00`.
pub fn stashes(root: &Path) -> Result<Vec<u8>, GitError> {
    run(root, &["stash", "list", "--format=%gd%x00%H%x00%ar%x00%gs"])
}

/// What a push would send, and where it could go — every field a
/// string git printed.
///
/// Read in one ask of the project's git thread, because the four
/// questions are one question: a head that named a remote from one walk
/// and counted commits from another would be two truths free to
/// disagree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ahead {
    /// The branch checked out, as `git symbolic-ref` printed it.
    /// `None` on a detached `HEAD`, where there is nothing to push.
    pub branch: Option<String>,
    /// Every remote `git remote` printed, in git's order.
    pub remotes: Vec<String>,
    /// The branch's upstream as `<remote>/<branch>`, when it has one.
    pub upstream: Option<String>,
    /// The commits the push would send, newest first — the Log's own
    /// rows, from the Log's own format.
    pub commits: Vec<log::Commit>,
}

/// Which tags a push takes with it — *Push tags* and its select.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PushTags {
    /// None: the box is clear.
    #[default]
    None,
    /// Every tag — `--tags`.
    All,
    /// The annotated tags on the commits being pushed — `--follow-tags`,
    /// *Current Branch*.
    CurrentBranch,
}

/// Where a push would go: three names and the one bool that decides
/// whether it also sets an upstream.
///
/// Three fields rather than one sentence, because the head draws them
/// differently — the remote is a button and the two branch names are
/// text, where the local branch is the part that cannot be
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// One of the names `git remote` printed.
    pub remote: String,
    /// The branch being pushed, as `git symbolic-ref` printed it.
    pub branch: String,
    /// The branch it lands on: the upstream's own name when the
    /// upstream is on this remote, the local name otherwise.
    ///
    /// **Not the same thing as `branch`, and that is the point.**
    /// `git push origin bar` means `bar:bar` whatever `bar` tracks, so
    /// a branch whose upstream is `origin/foo` would quietly grow a
    /// second remote branch and leave the one it tracks behind.
    pub onto: String,
    /// The branch has no upstream on this remote, so the push sets one.
    pub set_upstream: bool,
}

impl Ahead {
    /// Where a push would go, given the remote chosen in the head — or
    /// nothing, when there is nowhere: no remote at all, or a detached
    /// `HEAD` with no branch to send.
    ///
    /// **A chosen name that `git remote` did not print is ignored**, not
    /// passed on. The head offers only what git listed; a choice left
    /// over from a remote that has since been removed falls back to the
    /// upstream's rather than being sent as typed.
    pub fn destination(&self, chosen: Option<&str>) -> Option<Destination> {
        let branch = self.branch.clone()?;
        let tracked = self.upstream.as_deref().and_then(split_upstream);
        let remote = chosen
            .filter(|name| self.remotes.iter().any(|known| known == name))
            .or(tracked.map(|(remote, _)| remote))
            .filter(|name| self.remotes.iter().any(|known| known == name))
            .map(str::to_owned)
            .or_else(|| self.remotes.first().cloned())?;
        // The upstream's name counts only on the remote it is on: a push
        // to a second remote is a new upstream under the branch's own
        // name.
        let onto = match tracked {
            Some((tracked_remote, tracked_branch)) if tracked_remote == remote => {
                tracked_branch.to_owned()
            }
            _ => branch.clone(),
        };
        let set_upstream =
            !matches!(tracked, Some((tracked_remote, _)) if tracked_remote == remote);
        Some(Destination {
            remote,
            branch,
            onto,
            set_upstream,
        })
    }
}

/// An upstream split into its remote and its branch.
///
/// **At the first slash.** A remote's name cannot hold one and a
/// branch's can, so splitting from the other end would read
/// `origin/feature/lists` as a branch `lists` on a remote
/// `origin/feature`.
pub fn split_upstream(upstream: &str) -> Option<(&str, &str)> {
    let (remote, branch) = upstream.split_once('/')?;
    (!remote.is_empty() && !branch.is_empty()).then_some((remote, branch))
}

/// The remotes, one name a line. Empty for a repository with none,
/// which is a different thing from having nothing to push.
pub fn remotes(root: &Path) -> Result<Vec<String>, GitError> {
    let bytes = run(root, &["remote"])?;
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The branch `HEAD` points at, or `None` on a detached one.
///
/// `symbolic-ref` rather than `branch()` above, which reads the file and
/// answers a short hash for a detached `HEAD`. A hash is the right thing
/// to *show* in a title bar and the wrong thing to hand to `push`: there
/// is no branch, and the honest answer is that there is nothing to send.
pub fn head_branch(root: &Path) -> Result<Option<String>, GitError> {
    match run(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) {
        Ok(bytes) => {
            let name = String::from_utf8_lossy(&bytes).trim().to_owned();
            Ok((!name.is_empty()).then_some(name))
        }
        // `--quiet` makes a detached HEAD an exit code with nothing on
        // either channel, which arrives here as a failure with no words.
        Err(GitError::Failed(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

/// The upstream of the branch checked out, as `<remote>/<branch>`.
///
/// `None` when there is none: git says *no upstream configured* and
/// exits 128, which is an answer rather than a fault — a branch nobody
/// has pushed yet is the ordinary case this whole tab exists for.
pub fn upstream(root: &Path) -> Result<Option<String>, GitError> {
    match run(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) {
        Ok(bytes) => {
            let name = String::from_utf8_lossy(&bytes).trim().to_owned();
            Ok((!name.is_empty()).then_some(name))
        }
        Err(GitError::Failed(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

/// The arguments that ask for the commits a push would send.
///
/// Pure, so the shape can be read without running anything.
///
/// **With an upstream it is a range**, `<upstream>..HEAD`, which is
/// exactly what `push` would send. **Without one there is no range to
/// take**, and inventing one would be a guess: the ask is everything on
/// `HEAD` that no ref of the chosen remote already holds, which is the
/// question `push` itself answers before it sends anything.
pub fn ahead_args(upstream: Option<&str>, remote: &str, n: usize) -> Vec<String> {
    let mut args: Vec<String> = ["log", LOG_FORMAT, "-z", "-n"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
    args.push(n.to_string());
    match upstream {
        Some(upstream) => args.push(format!("{upstream}..HEAD")),
        None => {
            args.push("HEAD".to_owned());
            args.push("--not".to_owned());
            args.push(format!("--remotes={remote}"));
        }
    }
    // The separator for the same reason `log_args` carries one: without
    // it a ref that shares its name with a file is ambiguous, and git
    // refuses rather than choosing.
    args.push("--".to_owned());
    args
}

/// The commits a push would send, as bytes for [`log::parse`]. A
/// repository with no commit yet has none, not an error.
pub fn ahead(
    root: &Path,
    upstream: Option<&str>,
    remote: &str,
    n: usize,
) -> Result<Vec<u8>, GitError> {
    let args = ahead_args(upstream, remote, n);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(root, &args) {
        Err(GitError::Failed(message)) if message.contains("does not have any commits") => {
            Ok(Vec::new())
        }
        other => other,
    }
}

/// Read-only blame of a named revision, with a path relative to `root`.
/// Historical callers pass the repository root and the blob's exact path.
pub fn blame_at(root: &Path, revision: &str, path: &Path) -> Result<Vec<u8>, GitError> {
    run(
        root,
        &[
            "blame",
            "--porcelain",
            revision,
            "--",
            &path.to_string_lossy(),
        ],
    )
}

/// Who wrote each line of a file and when, as bytes for [`blame::parse`].
/// The working tree's text, so a line not committed yet is marked as
/// such; renames are followed, which is git's default.
pub fn blame(root: &Path, path: &Path) -> Result<Vec<u8>, GitError> {
    run(
        root,
        &["blame", "--porcelain", "--", &path.to_string_lossy()],
    )
}

/// What the index holds outside the folder `root` is, from the
/// repository's top — empty when `root` is the top. Asked by a folder
/// project's commit under the checkout's turn, right before it runs: a
/// change staged outside after the window planned the commit would ride
/// along with it (review r3 `arch+quality-2`). Renames are listed as their
/// two paths, `diff.relative` cannot make the names relative, and
/// `diff.ignoreSubmodules` cannot hide a staged gitlink (review r4).
pub fn staged_outside(root: &Path) -> Result<Vec<PathBuf>, GitError> {
    let prefix = run(root, &["rev-parse", "--show-prefix"])?;
    let prefix = String::from_utf8_lossy(&prefix)
        .trim_end_matches('\n')
        .to_owned();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let names = run(
        root,
        &[
            "-c",
            "diff.relative=false",
            "diff",
            "--cached",
            "--name-only",
            "--no-renames",
            "--ignore-submodules=none",
            "-z",
        ],
    )?;
    Ok(names
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .filter(|name| !name.starts_with(&prefix))
        .map(PathBuf::from)
        .collect())
}

/// The last commit's short hash and summary — what a notice says after
/// a commit lands.
pub fn last_commit(root: &Path) -> Result<(String, String), GitError> {
    let bytes = run(root, &["log", "-1", "--format=%h%n%s"])?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    let short = lines.next().unwrap_or_default().to_owned();
    let summary = lines.next().unwrap_or_default().to_owned();
    Ok((short, summary))
}

/// A file as it is in `HEAD`, or `None` when `HEAD` has no such file —
/// a new file, or a repository with no commit yet, where everything is
/// new.
pub fn head_text(root: &Path, path: &Path) -> Result<Option<Vec<u8>>, GitError> {
    // `./`: the path is the project's, and a project may be a folder of
    // its repository — `HEAD:path` would read the root's file of the same
    // name without a word. Decision 18 of M5.11.
    blob(root, &format!("HEAD:./{}", path.display()))
}

/// An immutable, size-checked HEAD blob for editor markers. Missing paths and
/// unborn HEAD are empty baselines; all actual Git failures remain errors.
/// Resolve the object before checking size, so a branch change cannot swap in
/// a larger blob between the size check and the read. All IO uses the gateway.
pub(crate) fn bounded_head_text(
    root: &Path,
    path: &Path,
    limit: usize,
) -> Result<Option<Vec<u8>>, GitError> {
    if run(root, &["rev-parse", "--is-inside-work-tree"])? != b"true\n" {
        return Err(GitError::NoRepository);
    }
    // Only a missing HEAD path needs index/untracked eligibility. An existing
    // HEAD blob stays the base even if its index entry was removed meanwhile.
    let new_file = || -> Result<Option<Vec<u8>>, GitError> {
        run(
            root,
            &[
                "--literal-pathspecs",
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "--error-unmatch",
                "--",
                &path.to_string_lossy(),
            ],
        )?;
        Ok(None)
    };
    let head = match run(root, &["rev-parse", "--verify", "HEAD"]) {
        Ok(head) => String::from_utf8_lossy(&head).trim().to_owned(),
        Err(error) => {
            let branch = run(root, &["symbolic-ref", "-q", "HEAD"])?;
            let branch = String::from_utf8_lossy(&branch);
            if run(
                root,
                &["for-each-ref", "--format=%(objectname)", branch.trim()],
            )?
            .is_empty()
            {
                return new_file();
            }
            return Err(error);
        }
    };
    let entries = run(
        root,
        &[
            "--literal-pathspecs",
            "ls-tree",
            "-z",
            &head,
            "--",
            &path.to_string_lossy(),
        ],
    )?;
    if entries.is_empty() {
        return new_file();
    }
    let record = String::from_utf8_lossy(&entries);
    let mut fields = record.split_whitespace();
    if !matches!(fields.next(), Some("100644" | "100755")) {
        return Err(GitError::Failed("Not a regular source blob".into()));
    }
    if fields.next() != Some("blob") {
        return Err(GitError::Failed("Not a source blob".into()));
    }
    let object = fields
        .next()
        .ok_or_else(|| GitError::Failed("Missing source object".into()))?;
    let length = run(root, &["cat-file", "-s", object])?;
    let length: usize = String::from_utf8_lossy(&length)
        .trim()
        .parse()
        .map_err(|_| GitError::Failed("Invalid source size".into()))?;
    if length > limit {
        return Err(GitError::Failed(
            "Source marker byte budget exceeded".into(),
        ));
    }
    run(root, &["cat-file", "blob", object]).map(Some)
}

/// A blob by its spec — `HEAD:path`, `:path` for the index, `sha:path`,
/// `sha^:path` — or `None` when the spec names nothing: a file the
/// revision does not have, a repository with no commit yet, a first
/// commit's parent.
pub fn blob(root: &Path, spec: &str) -> Result<Option<Vec<u8>>, GitError> {
    match run(root, &["show", spec]) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(GitError::Failed(message))
            if [
                "does not exist in",
                "exists on disk, but not in",
                "invalid object name",
                "bad revision",
                "ambiguous argument",
                "unknown revision",
            ]
            .iter()
            .any(|sign| message.contains(sign)) =>
        {
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

/// The repository's root directory for a directory inside it: a project
/// that is a subdirectory of its repository gets its status from here,
/// with paths made relative to it.
pub fn toplevel(dir: &Path) -> Result<PathBuf, GitError> {
    let bytes = run(dir, &["rev-parse", "--show-toplevel"])?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(PathBuf::from(text.trim()))
}

/// How much of a commit hash stands in for a branch name.
///
/// Seven is what `git` itself prints and what people paste to each other.
const SHORT_HASH: usize = 7;

/// The branch of the repository at `dir`, if it is one.
///
/// `None` when there is no repository — a project need not be one, and an
/// empty widget is better than the word "none" in the title bar.
pub fn branch(dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(dir)?.join("HEAD")).ok()?;
    describe(head.trim())
}

/// Where the repository's data actually lives.
///
/// Usually `<dir>/.git`. In a worktree that path is a file whose contents
/// are `gitdir: <somewhere else>`, and the branch lives at the far end.
///
/// **Asked of `dir` and then of each directory above it, the nearest
/// answer winning** — git's own discovery. A project opened on a folder
/// of its repository has no `.git` of its own, and its title bar was
/// empty while every git command in it worked. Decision 18 of M5.11.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find_map(git_dir_here)
}

/// Whether `dir`'s own `.git` is a regular file — a linked worktree's or a
/// submodule's checkout, whose repository lives elsewhere. Asked of the
/// file's kind only, never its contents, so nothing waits on what is there.
/// GLD-055 refuses such a folder as a project group's.
pub fn dot_git_is_file(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir.join(".git")).is_ok_and(|meta| meta.file_type().is_file())
}

/// `git_dir` for exactly this directory, with no walking.
fn git_dir_here(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");

    if dot_git.is_dir() {
        return Some(dot_git);
    }

    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let target = pointer.trim().strip_prefix("gitdir:")?.trim();

    let path = PathBuf::from(target);
    Some(if path.is_absolute() {
        path
    } else {
        dir.join(path)
    })
}

/// What to show for the contents of `HEAD`.
///
/// A branch by name; a detached head by a short hash, because the full
/// forty characters say no more and take the width of the whole widget.
fn describe(head: &str) -> Option<String> {
    if let Some(reference) = head.strip_prefix("ref:") {
        let name = reference.trim().rsplit('/').next()?.trim();
        return (!name.is_empty()).then(|| name.to_owned());
    }

    let hash: String = head.chars().take(SHORT_HASH).collect();
    (hash.len() == SHORT_HASH && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_editor_baseline_resolves_project_prefix_and_checks_blob_size_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .current_dir(dir.path())
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("name.txt"), "root\n").unwrap();
        std::fs::write(dir.path().join("sub/name.txt"), "nested\n").unwrap();
        std::fs::write(
            dir.path().join("sub/large.txt"),
            vec![b'x'; crate::line_changes::BYTE_LIMIT + 1],
        )
        .unwrap();
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "base"]);
        let root = dir.path().join("sub");
        assert_eq!(
            bounded_head_text(
                &root,
                Path::new("name.txt"),
                crate::line_changes::BYTE_LIMIT
            )
            .unwrap(),
            Some(b"nested\n".to_vec())
        );
        let before = ran_under(dir.path(), &dir.path().canonicalize().unwrap()).len();
        assert!(
            bounded_head_text(
                &root,
                Path::new("large.txt"),
                crate::line_changes::BYTE_LIMIT
            )
            .is_err()
        );
        let calls = ran_under(dir.path(), &dir.path().canonicalize().unwrap());
        assert!(
            calls[before..]
                .iter()
                .any(|args| args.starts_with(&["cat-file".into(), "-s".into()]))
        );
        assert!(
            !calls[before..]
                .iter()
                .any(|args| args.starts_with(&["cat-file".into(), "blob".into()])),
            "oversized HEAD is never materialized"
        );
    }

    /// A half-written or foreign HEAD must not become a nonsense label.
    #[test]
    fn an_unreadable_head_shows_nothing() {
        assert_eq!(describe(""), None);
        assert_eq!(describe("ref:"), None);
        assert_eq!(describe("ref: "), None);
        assert_eq!(describe("not a hash"), None);
    }

    /// Review r2: a checkout whose name ends in a space. Trimming git's
    /// answer took the space with the newline, the path did not exist, and
    /// the write went ahead without the checkout's turn.
    #[test]
    fn a_checkout_named_with_a_trailing_space_has_a_turn() {
        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path().join("repo ");
        std::fs::create_dir(&dir).unwrap();
        run(&dir, &["init", "-q"]).unwrap();
        let canonical = dir.canonicalize().unwrap();
        assert_eq!(checkout_of(&dir), Some(canonical.clone()));
        std::fs::create_dir(dir.join("sub")).unwrap();
        assert_eq!(checkout_of(&dir.join("sub")), Some(canonical));
        assert!(turn(&dir).is_some());
    }

    /// Review r1, sharpened in r2: a write from a folder of the checkout —
    /// what a second root opened there does — must wait for the checkout's
    /// fast-forward, which holds the turn from its preflight to its
    /// post-check. The fast-forward goes on only once that writer is seen
    /// waiting; a turn keyed by the root's own path rather than its
    /// checkout would never make it wait, and its switch would land
    /// between the preflight and the merge.
    #[test]
    fn a_write_from_a_folder_of_the_checkout_waits_for_the_fast_forward() {
        let folder = tempfile::tempdir().unwrap();
        let dir = folder.path().join("repo");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=Guild",
                    "-c",
                    "user.email=guild@example.invalid",
                ])
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("sub/file"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "a"]);
        let a = git(&["rev-parse", "HEAD"]);
        git(&["branch", "feature"]);
        git(&["switch", "-q", "-c", "next"]);
        std::fs::write(dir.join("sub/file"), "b\n").unwrap();
        git(&["commit", "-q", "-am", "b"]);
        let b = git(&["rev-parse", "HEAD"]);
        git(&["switch", "-q", "main"]);
        let checkout = dir.canonicalize().unwrap();

        let (go, ready) = std::sync::mpsc::channel();
        let sub = dir.join("sub");
        let other = std::thread::spawn(move || {
            ready.recv().unwrap();
            write(&sub, &Write::Switch("feature".into()))
        });
        let landed = auto_ff::fast_forward_checkout(
            &dir,
            "main",
            &a,
            &b,
            || {
                go.send(()).unwrap();
                let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while waiting_for_turn(&checkout) == 0 && std::time::Instant::now() < until {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                true
            },
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        );
        other.join().unwrap().unwrap();
        assert_eq!(landed, auto_ff::Landed::FastForwarded);
        assert_eq!(git(&["rev-parse", "main"]), b, "main moved");
        assert_eq!(git(&["rev-parse", "feature"]), a, "feature did not");
        assert_eq!(
            git(&["branch", "--show-current"]),
            "feature",
            "the other writer ran, after"
        );
    }
}
