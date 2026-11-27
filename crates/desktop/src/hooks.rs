//! The hook Guild gives every agent CLI it starts — M5.10.
//!
//! A terminal tab's output is characters, so what its agent is doing has
//! to be told to the window by someone else, and Claude Code's hooks are
//! that someone. This is the package: a script that posts a hook's JSON to
//! the window, and the settings that name it for eight hooks. Both are
//! written to [`directory`] when the window starts and handed to each
//! session by `--settings` — **never** to `~/.claude/settings.json`, which
//! is the person's.
//!
//! OpenCode has no hook command to name on its command line; its status
//! comes through a *plugin* in opencode's own plugins directory — the
//! first deliberate write to another tool's config. Codex's hooks live in
//! its `config.toml`, the second. Both are no-ops outside a session Guild
//! started, and the exceptions are explained at [`install_opencode`] and
//! [`install_codex`]. Neither is written for an agent never run on this
//! machine until the person starts it from Guild — [`install_door_if_known`],
//! GLD-078.
//!
//! A command hook with a script rather than the CLI's own HTTP hook, by the
//! person's choice: a script is the common, proven form, and the HTTP hook
//! carries a guard against a proxy environment that differs from the person's
//! — which is exactly what a window opened from Finder has. Decision 1 of
//! M5.10's plan.

pub mod identity;
pub mod server;

pub use server::sweep;

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The script, whole. The repository is its one source: it is written out
/// from here rather than shipped in the bundle, because `just desktop`
/// runs without one.
pub const SCRIPT: &str = include_str!("../assets/hooks/guild-hook.sh");
/// OpenCode's status plugin, whole — M14. Written into opencode's own
/// plugins directory by [`install_opencode`] (opencode has no per-launch
/// plugin flag); a no-op outside a session Guild started.
pub const OPENCODE_PLUGIN: &str = include_str!("../assets/hooks/opencode-status.js");
/// The status line that taps the plan's limits on their way to the
/// person's own, whole — written beside the hook's script, for the same
/// reason.
pub const STATUSLINE_SCRIPT: &str = include_str!("../assets/hooks/guild-statusline.sh");

const SCRIPT_NAME: &str = "guild-hook.sh";
const SETTINGS_NAME: &str = "claude-settings.json";
const USAGE_POST_SCRIPT: &str = include_str!("../assets/hooks/guild-usage-post.sh");
const USAGE_POST_NAME: &str = "guild-usage-post.sh";
const STATUSLINE_NAME: &str = "guild-statusline.sh";
const STATUSLINE_SETTINGS_NAME: &str = "claude-settings-statusline.json";
/// Where the status line keeps a stamp per session: when it last posted.
const STAMPS_NAME: &str = "statusline";
const OPENCODE_PLUGIN_NAME: &str = "opencode-status.js";

/// The most a socket's path may hold: `sun_path` is 104 bytes on macOS,
/// the terminating NUL included.
const SUN_PATH: usize = 104;

/// Where the window writes the package: `<state_dir>/hooks`.
pub fn directory() -> PathBuf {
    crate::paths::state_dir().join("hooks")
}

/// The settings a session is started with: two hooks, both the script.
///
/// What a session is doing comes from the program itself, through OSC 7501;
/// the hooks carry only what it cannot say. `SessionStart` names the
/// conversation, and again on an in-TUI `/resume`; `UserPromptSubmit` says
/// the person submitted a prompt — the project sort's authorship and a
/// continuation's turn-start acknowledgement read it.
pub fn settings(script: &Path) -> Value {
    let command = shell_quote(script);
    let hook = |extra: Value| {
        let mut hook = json!({ "type": "command", "command": command });
        if let (Some(hook), Some(extra)) = (hook.as_object_mut(), extra.as_object()) {
            hook.extend(extra.clone());
        }
        json!([{ "hooks": [hook] }])
    };
    json!({
        // Per-launch settings override a saved dark/light preference only
        // inside Guild. Claude detects our current background through OSC 11.
        "theme": "auto",
        "hooks": {
            "UserPromptSubmit": hook(json!({})),
            // How `/resume` is heard at once, with the resumed
            // conversation's id — M13.3, measured in its task 0.
            "SessionStart": hook(json!({})),
        }
    })
}

/// [`settings`], and Guild's status line — what a session is started
/// with when the person has a status line of their own to chain to.
pub fn settings_with_statusline(script: &Path, statusline: &Path) -> Value {
    let mut settings = settings(script);
    settings["statusLine"] = json!({ "type": "command", "command": shell_quote(statusline) });
    settings
}

/// The person's own status line for a session started in `cwd`: the
/// first `statusLine.command` in `<cwd>/.claude/settings.local.json`,
/// `<cwd>/.claude/settings.json`, then `<config>/settings.json` — Claude's
/// own precedence. A file that
/// cannot be read, or names no command, is passed over.
///
/// Read once, at the launch: a status line changed while a session runs
/// reaches the next one, as it does in Claude.
pub fn user_statusline(cwd: &Path, config: &Path) -> Option<String> {
    [
        cwd.join(".claude").join("settings.local.json"),
        cwd.join(".claude").join("settings.json"),
        config.join("settings.json"),
    ]
    .iter()
    .find_map(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        let settings: Value = serde_json::from_str(&text).ok()?;
        settings
            .get("statusLine")?
            .get("command")?
            .as_str()
            .filter(|command| !command.trim().is_empty())
            .map(str::to_owned)
    })
}

/// Claude's own folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".claude")
        })
}

/// A path as a shell reads it back: single-quoted, with a quote inside
/// closed, escaped and reopened. A hook's command is a line a shell runs,
/// and a home directory may hold a space or an apostrophe.
pub fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// Where [`install`] put the two files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The script.
    pub script: PathBuf,
    /// The settings naming it.
    pub settings: PathBuf,
    /// The status line's script.
    pub statusline: PathBuf,
    /// The settings naming both.
    pub statusline_settings: PathBuf,
}

/// Write the script and its settings into `dir`, touching neither when it
/// already says what it should.
///
/// Each is written beside itself and renamed into place, so a `claude`
/// starting at that moment reads the old file or the new one and never
/// half of either. Not through `Store`, whose write keeps a `.bak`: a file
/// rewritten from the binary at every start has nothing worth backing up.
pub fn install(dir: &Path) -> io::Result<Installed> {
    std::fs::create_dir_all(dir)?;

    let script = dir.join(SCRIPT_NAME);
    write_if_changed(&script, SCRIPT, 0o755)?;

    let settings_path = dir.join(SETTINGS_NAME);
    write_json_if_changed(&settings_path, &settings(&script))?;

    // Install the adapter before publishing a script that invokes it.
    write_if_changed(&dir.join(USAGE_POST_NAME), USAGE_POST_SCRIPT, 0o755)?;
    let statusline = dir.join(STATUSLINE_NAME);
    write_if_changed(&statusline, STATUSLINE_SCRIPT, 0o755)?;
    let statusline_settings = dir.join(STATUSLINE_SETTINGS_NAME);
    write_json_if_changed(
        &statusline_settings,
        &settings_with_statusline(&script, &statusline),
    )?;
    // The stamps of sessions a window before this one started: a session
    // that outlived it posts at its next tick rather than fifteen seconds
    // on, and the folder does not grow a file per session for ever.
    let stamps = dir.join(STAMPS_NAME);
    let _ = std::fs::remove_dir_all(&stamps);
    std::fs::create_dir_all(&stamps)?;

    Ok(Installed {
        script,
        settings: settings_path,
        statusline,
        statusline_settings,
    })
}

/// Where opencode loads its local plugins: `<config>/plugins`, under
/// opencode's config directory ([`OpenCodeHarness::config_dir`]:
/// `$XDG_CONFIG_HOME/opencode` or `~/.config/opencode`).
///
/// A local `.js` plugin need only be dropped into this directory to be
/// loaded — opencode's plugin command lists one that is simply there, and
/// other tools may keep their own status plugins beside it. Guild's is
/// written here by [`install_opencode`] under its own name, so it never
/// touches theirs.
///
/// [`OpenCodeHarness::config_dir`]: guild_session::harness::OpenCodeHarness::config_dir
pub fn opencode_plugins_dir() -> PathBuf {
    guild_session::harness::OpenCodeHarness::config_dir().join("plugins")
}

/// Write opencode's status plugin into opencode's own plugins directory.
///
/// **One of the two places the hooks rule is bent** — Guild writes to the
/// person's opencode config, where Claude writes only its own `--settings`
/// and never `~/.claude/settings.json`. There is no per-launch plugin flag
/// in opencode V2, so the global plugins directory is the only door its
/// status can come through. The plugin is a strict no-op without the socket
/// Guild sets on what it launches, so the person's own opencode runs bear
/// the file and nothing else.
pub fn install_opencode() -> io::Result<PathBuf> {
    install_opencode_into(&guild_session::harness::OpenCodeHarness::config_dir())
}

/// [`install_opencode`] into an opencode config directory of the caller's
/// choosing — a test's scratch dir, as [`install_codex_into`] is.
pub fn install_opencode_into(config: &Path) -> io::Result<PathBuf> {
    let dir = config.join("plugins");
    std::fs::create_dir_all(&dir)?;
    write_if_changed(&dir.join(OPENCODE_PLUGIN_NAME), OPENCODE_PLUGIN, 0o644)?;
    Ok(dir.join(OPENCODE_PLUGIN_NAME))
}

/// Whether the window may write the agents' global status doors at all.
/// `GUILD_NO_GLOBAL_HOOKS` says no — an isolated dev window may use the
/// real harnesses without rewriting their global integration files — at
/// the start and at a launch alike.
pub fn global_hooks_allowed() -> bool {
    std::env::var_os("GUILD_NO_GLOBAL_HOOKS").is_none()
}

/// Write `harness`'s status door into its directory in `dirs` — the Codex
/// hook block or the opencode plugin — known or not: what a launch from
/// Guild does before the process starts, so the first session's status
/// moves (GLD-078). `None` for Claude, which has no door to write.
pub fn install_door(
    dirs: &guild_session::harness::AgentDirs,
    harness: guild_session::HarnessKind,
) -> Option<io::Result<PathBuf>> {
    let dir = dirs.of(harness)?;
    let written = match harness {
        guild_session::HarnessKind::Codex => install_codex_into(dir),
        _ => install_opencode_into(dir),
    };
    if written.is_ok() {
        doors_written()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((harness, dir.to_path_buf()));
    }
    Some(written)
}

/// The doors this process has written, by agent and directory — the
/// window's own memory, so a launch after the first does no disk work for
/// its door at all.
fn doors_written()
-> &'static std::sync::Mutex<std::collections::HashSet<(guild_session::HarnessKind, PathBuf)>> {
    static WRITTEN: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashSet<(guild_session::HarnessKind, PathBuf)>>,
    > = std::sync::OnceLock::new();
    WRITTEN.get_or_init(Default::default)
}

/// What a launch from Guild writes before its process starts:
/// [`install_door`], known or not — the person is starting this agent now
/// (GLD-078) — but only the first time in this window, and only for an
/// agent on the launch's `path`.
///
/// **Once per window, on the window's thread.** A launch is built and
/// spawned on the window's thread, and the door must be on disk before the
/// spawn, so its disk work is done there — and therefore done once: a door
/// [`install_door`] wrote here, at the window's start for an agent already
/// known or at an earlier launch, is remembered and not read, compared or
/// written again. What is left on that thread is one install the first time
/// an agent never run on this machine is started from Guild — after it the
/// agent is known, and the next window's start writes its door. A door
/// removed behind a running window's back comes back at the next start, as
/// before GLD-078.
///
/// **Not on the `PATH`, nothing.** A saved tab restored for an agent that is
/// gone resumes nothing; writing its door would make the directory the
/// start left alone (decision 58).
pub fn prepare_door(
    dirs: &guild_session::harness::AgentDirs,
    harness: guild_session::HarnessKind,
    path: &str,
) -> Option<io::Result<PathBuf>> {
    let dir = dirs.of(harness)?;
    let remembered = doors_written()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&(harness, dir.to_path_buf()));
    if remembered || !harness.is_on(path) {
        return None;
    }
    install_door(dirs, harness)
}

/// [`install_door`], only for an agent [known] here — what the window's
/// start does. An agent never run on this machine gets no directory and no
/// file from a window that merely opened (GLD-078).
///
/// [known]: guild_session::harness::known
pub fn install_door_if_known(
    dirs: &guild_session::harness::AgentDirs,
    harness: guild_session::HarnessKind,
) -> Option<io::Result<PathBuf>> {
    if !dirs.known(harness) {
        return None;
    }
    install_door(dirs, harness)
}

/// The start of the block the window owns in Codex's config.
const CODEX_BEGIN: &str = "# >>> guild hooks >>>";
/// The end of the block the window owns in Codex's config.
const CODEX_END: &str = "# <<< guild hooks <<<";
/// The events Guild asks Codex to report. The script is told the event's
/// own name on its command line — that name becomes `hook_event_name`,
/// which [`crate::status::from_hook`] reads back.
const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
];

/// Why Guild writes here at all, and how it stays out of the way.
///
/// **The hooks rule is bent a second time** — Guild writes to
/// `~/.codex/config.toml`, the person's file, which other tools may share.
/// Codex has no per-launch settings file as Claude's `--settings` is: its
/// hooks live in this one config, beside anyone else's, and there is no other
/// door. The write is **only the block between [`CODEX_BEGIN`] and
/// [`CODEX_END`]**: everything before it and after it — the model, the trust
/// table, another tool's hooks — is read and written back whole, never
/// reordered. And the script is a no-op without the socket Guild sets on what
/// it launches, so the person's own Codex sessions and other tools' run this
/// file and nothing else, the same guard as the opencode plugin.
pub fn install_codex() -> io::Result<PathBuf> {
    // `$CODEX_HOME`, else `~/.codex` — the rule the start screen reads
    // Codex's sessions by, kept in one place so the two cannot part.
    let dir = guild_session::harness::CodexHarness::home();
    install_codex_into(&dir)
}

/// [`install_codex`] into a directory of the caller's choosing — a test's
/// scratch dir, or wherever `$CODEX_HOME` says. Public so the effect is
/// checkable without moving the person's real config around.
pub fn install_codex_into(dir: &Path) -> io::Result<PathBuf> {
    install_codex_with(dir, &codex_block(&codex_script()), || {})
}

/// How many times an install reads Codex's config again after finding that
/// Codex wrote it between Guild's read and Guild's write.
const CODEX_ATTEMPTS: usize = 3;

/// [`install_codex_into`] with `block` as the block, and `meanwhile` run
/// just after each read — where Codex may write too.
///
/// **What was read is what is compared, and what is replaced.** A block
/// that is already there is the end of it: nothing is read a second time and
/// nothing is written, so a Codex running beside the window — which writes
/// `[hooks.state]` into this file — never has its write replaced by the
/// window's older copy. A block that must change is written beside and
/// renamed over the file only while the file still holds the bytes the
/// block was put into; otherwise the file is read again and the block put
/// into what Codex wrote. POSIX has no compare-and-rename, so the instant
/// between the last read and the rename is still open — a microsecond,
/// against the whole of a read, a merge and a write before.
fn install_codex_with(dir: &Path, block: &str, mut meanwhile: impl FnMut()) -> io::Result<PathBuf> {
    let path = dir.join("config.toml");
    std::fs::create_dir_all(dir)?;

    for _ in 0..CODEX_ATTEMPTS {
        let existing = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err),
        };
        meanwhile();
        let text = upsert_codex_block(existing.clone().unwrap_or_default(), block);
        if existing.as_deref() == Some(text.as_str()) {
            return Ok(path);
        }
        // The mode of a file that already exists is its owner's — a shared
        // config is not rewritten to Guild's taste just because Guild
        // writes a block into it.
        let mut mode = 0o644;
        if existing.as_deref().is_some_and(|text| !text.is_empty())
            && let Ok(meta) = std::fs::metadata(&path)
        {
            mode = meta.permissions().mode() & 0o777;
        }
        if replace_if_unchanged(&path, existing.as_deref(), &text, mode)? {
            return Ok(path);
        }
    }
    Err(io::Error::other(
        "Codex's config.toml kept changing while Guild wrote its hooks into it",
    ))
}

/// Put `text` at `path`, written beside and renamed over it, only if `path`
/// still holds `expected` (`None`: no file) — `false`, and nothing written,
/// when something else changed it since it was read.
fn replace_if_unchanged(
    path: &Path,
    expected: Option<&str>,
    text: &str,
    mode: u32,
) -> io::Result<bool> {
    let beside = path.with_extension("writing");
    let written = std::fs::write(&beside, text)
        .and_then(|()| std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(mode)));
    let now = written.and_then(|()| match std::fs::read_to_string(path) {
        Ok(now) => Ok(Some(now)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    });
    match now {
        Ok(now) if now.as_deref() == expected => std::fs::rename(&beside, path).map(|()| true),
        other => {
            let _ = std::fs::remove_file(&beside);
            other.map(|_| false)
        }
    }
}

/// The script's own path, as Guild wrote it out — what the hook command names.
fn codex_script() -> PathBuf {
    directory().join(SCRIPT_NAME)
}

/// The `[[hooks.*]]` block Guild owns, as TOML, between its markers.
///
/// Pure, so the shape on disk is a test. Each hook is the script with the
/// event's name on the command line; the event's own argument is what the
/// script puts in `hook_event_name`.
fn codex_block(script: &Path) -> String {
    let script = shell_quote(script);
    let mut out = format!("{CODEX_BEGIN}\n");
    for event in CODEX_EVENTS {
        out.push_str(&format!(
            "[[hooks.{event}]]\n[[hooks.{event}.hooks]]\ntype = \"command\"\ncommand = \"{script} {event}\"\n"
        ));
    }
    out.push_str(CODEX_END);
    out
}

/// Put Guild's block into Codex's config without touching what is around it.
///
/// A previous block of ours is replaced whole; where none exists the block
/// is appended after a blank line. Everything the file held that is not our
/// own marker rounds is left byte for byte, a file other tools share being
/// one Guild must not reorder.
fn upsert_codex_block(existing: String, block: &str) -> String {
    if let Some(begin) = existing.find(CODEX_BEGIN) {
        // Codex may append trust tables before our closing comment. They
        // belong to Codex, not to this block, and must survive a refresh.
        let Some(relative_end) = existing[begin..].find(CODEX_END) else {
            return existing; // An incomplete marker is not permission to erase the tail.
        };
        let end = begin + relative_end;
        let mut trust = String::new();
        let mut keeping = false;
        for line in existing[begin..end].split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') {
                keeping =
                    trimmed.starts_with("[hooks.state]") || trimmed.starts_with("[hooks.state.");
            }
            if keeping {
                trust.push_str(line);
            }
        }
        let head = &existing[..begin];
        let tail = &existing[end + CODEX_END.len()..];
        let block = block.strip_suffix(CODEX_END).unwrap_or(block);
        return format!("{head}{block}{trust}{CODEX_END}{tail}");
    }
    let sep = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{existing}{sep}{block}\n")
}

fn write_json_if_changed(path: &Path, value: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    text.push('\n');
    write_if_changed(path, &text, 0o644)
}

fn write_if_changed(path: &Path, text: &str, mode: u32) -> io::Result<()> {
    let current = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o777);
    if current == Some(mode) && std::fs::read_to_string(path).is_ok_and(|now| now == text) {
        return Ok(());
    }
    let beside = path.with_extension("writing");
    std::fs::write(&beside, text)?;
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&beside, path)
}

/// The socket the window with `pid` listens on for its hooks.
///
/// Under `$TMPDIR` rather than beside the script: the state directory is
/// under the home directory, and a path that does not fit `sun_path`
/// cannot be bound at all. `$TMPDIR` is per user on macOS; when even it is
/// too long, `/tmp`.
pub fn socket_path(pid: u32) -> PathBuf {
    let name = format!("guild-hooks-{pid}.sock");
    let candidate = std::env::temp_dir().join(&name);
    if candidate.as_os_str().len() < SUN_PATH {
        candidate
    } else {
        Path::new("/tmp").join(name)
    }
}

/// The Codex install's race with Codex itself, which needs the private seam
/// between Guild's read of `config.toml` and its write: the crate promises
/// [`install_codex_into`], not the moment inside it a test can stop at.
#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "# >>> guild hooks >>>\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"/scratch/guild-hook.sh Stop\"\n# <<< guild hooks <<<";

    /// What Codex writes while Guild works: trust inside Guild's markers.
    fn codex_writes(path: &Path) {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let text = text.replace(
            CODEX_END,
            "[hooks.state.\"stop\"]\ntrusted_hash = \"sha256:concurrent\"\n# <<< guild hooks <<<",
        );
        std::fs::write(path, text).unwrap();
    }

    /// An install that would change nothing writes nothing — not even the
    /// snapshot it read, over what Codex wrote after that read.
    #[test]
    fn an_install_that_changes_nothing_never_writes_over_codex() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, format!("model = \"o3\"\n{BLOCK}\n")).unwrap();
        install_codex_with(dir.path(), BLOCK, || codex_writes(&path)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("sha256:concurrent"),
            "Codex's write was lost:\n{text}"
        );
        assert!(!dir.path().join("config.writing").exists());
    }

    /// An install that does change the file, raced by Codex, writes its block
    /// over what Codex wrote — read again — rather than over its old snapshot.
    #[test]
    fn a_needed_install_keeps_what_codex_wrote_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let old = BLOCK.replace("/scratch/guild-hook.sh", "/old/guild-hook.sh");
        std::fs::write(&path, format!("model = \"o3\"\n{old}\n")).unwrap();
        let mut once = true;
        install_codex_with(dir.path(), BLOCK, || {
            if std::mem::take(&mut once) {
                codex_writes(&path);
            }
        })
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("sha256:concurrent"),
            "Codex's write was lost:\n{text}"
        );
        assert!(text.contains("/scratch/guild-hook.sh Stop"), "{text}");
        assert!(!text.contains("/old/"), "{text}");
        assert!(text.starts_with("model = \"o3\"\n"), "{text}");
        assert!(!dir.path().join("config.writing").exists());
    }
}
