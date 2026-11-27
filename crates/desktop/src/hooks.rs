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
    Some(match harness {
        guild_session::HarnessKind::Codex => install_codex_into(dir),
        _ => install_opencode_into(dir),
    })
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
    let path = dir.join("config.toml");
    std::fs::create_dir_all(dir)?;

    let mut mode = 0o644;
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // The mode of a file that already exists is its owner's — a shared
        // config is not rewritten to Guild's taste just because Guild
        // writes a block into it.
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    if !existing.is_empty()
        && let Ok(meta) = std::fs::metadata(&path)
    {
        use std::os::unix::fs::PermissionsExt;
        mode = meta.permissions().mode() & 0o777;
    }
    let text = upsert_codex_block(existing, &codex_block(&codex_script()));
    write_if_changed(&path, &text, mode)?;
    Ok(path)
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
