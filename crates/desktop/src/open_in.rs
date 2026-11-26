//! What the title bar's Open In offers — M12.10, decision 46.
//!
//! The builtin defaults are sixteen common editors, terminals and IDEs, each
//! by every bundle identifier it ships under — IntelliJ IDEA as Ultimate,
//! Community and EAP. Pure: the table and the rows for what is installed;
//! whether an identifier is installed, and its icon, are `desktop::system`'s,
//! where AppKit lives.

/// An application Open In can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct App {
    /// The name the menu shows.
    pub name: &'static str,
    /// Its bundle identifiers, the likeliest first.
    pub ids: &'static [&'static str],
}

/// The builtin integrations, plus Finder and external macOS Terminal.
pub const APPS: &[App] = &[
    App {
        name: "Finder",
        ids: &["com.apple.finder"],
    },
    App {
        name: "Ghostty",
        ids: &["com.mitchellh.ghostty"],
    },
    App {
        name: "Zed",
        ids: &["dev.zed.Zed"],
    },
    App {
        name: "Zed Preview",
        ids: &["dev.zed.Zed-Preview"],
    },
    App {
        name: "VS Code",
        ids: &["com.microsoft.VSCode"],
    },
    App {
        name: "VS Code Insiders",
        ids: &["com.microsoft.VSCodeInsiders"],
    },
    App {
        name: "iTerm2",
        ids: &["com.googlecode.iterm2"],
    },
    App {
        name: "Terminal",
        ids: &["com.apple.Terminal"],
    },
    App {
        name: "Xcode",
        ids: &["com.apple.dt.Xcode"],
    },
    App {
        name: "GoLand",
        ids: &["com.jetbrains.goland", "com.jetbrains.goland-EAP"],
    },
    App {
        name: "IntelliJ IDEA",
        ids: &[
            "com.jetbrains.intellij",
            "com.jetbrains.intellij-EAP",
            "com.jetbrains.intellij.ce",
        ],
    },
    App {
        name: "PyCharm",
        ids: &["com.jetbrains.pycharm", "com.jetbrains.pycharm-EAP"],
    },
    App {
        name: "PyCharm CE",
        ids: &["com.jetbrains.pycharm.ce"],
    },
    App {
        name: "CLion",
        ids: &["com.jetbrains.CLion", "com.jetbrains.CLion-EAP"],
    },
    App {
        name: "Rider",
        ids: &["com.jetbrains.rider", "com.jetbrains.rider-EAP"],
    },
    App {
        name: "RustRover",
        ids: &["com.jetbrains.rustrover", "com.jetbrains.rustrover-EAP"],
    },
    App {
        name: "Android Studio",
        ids: &["com.google.android.studio"],
    },
    App {
        name: "Sublime Text",
        ids: &["com.sublimetext.4", "com.sublimetext.3"],
    },
];

/// A row of the menu: an installed application by the identifier found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub name: &'static str,
    pub id: &'static str,
}

/// The installed applications, in the table's order, each by the first of
/// its identifiers `installed` says yes to.
pub fn rows(installed: impl Fn(&str) -> bool) -> Vec<Row> {
    rows_for_target(&guild_session::settings::OpenInTarget::Finder, installed)
}

/// Keep an installed saved edition (for example IDEA EAP) ahead of its sibling IDs.
/// Otherwise retain the catalog's existing first-installed rule.
pub fn rows_for_target(
    saved: &guild_session::settings::OpenInTarget,
    installed: impl Fn(&str) -> bool,
) -> Vec<Row> {
    APPS.iter()
        .filter_map(|app| {
            let preferred = match saved {
                guild_session::settings::OpenInTarget::App(saved) => app
                    .ids
                    .iter()
                    .copied()
                    .find(|id| *id == saved && installed(id)),
                _ => None,
            };
            let id = preferred.or_else(|| app.ids.iter().copied().find(|id| installed(id)))?;
            Some(Row { name: app.name, id })
        })
        .collect()
}

/// The folder as the menu's footnote says it: its start — `~` or the
/// first name under `/` — an ellipsis, and its last two names, once there
/// are more than three after the start. The menu is about *which* folder,
/// and the last two names say that; the whole road made the card as wide
/// as the screen on 2026-09-25.
pub fn shown(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let (head, rest) = match parts.as_slice() {
        ["", first, rest @ ..] => (format!("/{first}"), rest),
        [first, rest @ ..] => ((*first).to_owned(), rest),
        [] => return path.to_owned(),
    };
    match rest {
        [.., parent, name] if rest.len() > 3 => format!("{head}/…/{parent}/{name}"),
        _ => path.to_owned(),
    }
}

/// Resolve a saved choice against the installed catalog without launching it.
pub fn available_target(
    saved: &guild_session::settings::OpenInTarget,
    rows: &[Row],
) -> guild_session::settings::OpenInTarget {
    use guild_session::settings::OpenInTarget;
    match saved {
        OpenInTarget::Terminal => rows
            .iter()
            .find(|row| row.id == "com.apple.Terminal")
            .map(|row| OpenInTarget::App(row.id.to_owned()))
            .unwrap_or(OpenInTarget::Finder),
        OpenInTarget::App(id) if !rows.iter().any(|row| row.id == id) => OpenInTarget::Finder,
        target => target.clone(),
    }
}

/// The external launch boundary, injectable so tests never open another application.
/// Legacy Terminal choices are resolved through available_target before launch.
pub fn launch_with(
    target: &guild_session::settings::OpenInTarget,
    folder: &std::path::Path,
    spawn: impl FnOnce(&mut std::process::Command) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use guild_session::settings::OpenInTarget;
    let mut command = std::process::Command::new("open");
    match target {
        OpenInTarget::Finder | OpenInTarget::Terminal => {
            command.args(["-b", "com.apple.finder"]);
        }
        OpenInTarget::App(id) => {
            command.args(["-b", id]);
        }
    }
    spawn(command.arg(folder))
}

/// One installed configured destination. Custom connections keep their own identity.
#[derive(Debug, Clone)]
pub(crate) struct Connection {
    pub key: String,
    pub id: String,
    pub name: String,
    pub path: Option<std::path::PathBuf>,
    pub bundle_id: String,
}
impl Connection {
    #[cfg(test)]
    pub(crate) fn fixture(name: &str, id: &str) -> Self {
        Self {
            key: id.into(),
            id: id.into(),
            name: name.into(),
            path: None,
            bundle_id: id.into(),
        }
    }
}

pub(crate) fn default_keys() -> Vec<&'static str> {
    APPS.iter().map(|app| app.ids[0]).collect()
}

pub(crate) fn resolve_configured(
    saved: &guild_session::settings::OpenInTarget,
    config: &guild_session::open_in::Config,
    apps: &[Connection],
) -> guild_session::settings::OpenInTarget {
    use guild_session::settings::OpenInTarget;
    let id = match saved {
        OpenInTarget::Finder => return OpenInTarget::Finder,
        OpenInTarget::Terminal => "com.apple.Terminal",
        OpenInTarget::App(id) => id,
    };
    apps.iter()
        .find(|app| app.id == id && config.enabled(&app.key))
        .map(|app| {
            if app.id == "com.apple.finder" {
                OpenInTarget::Finder
            } else {
                OpenInTarget::App(app.id.clone())
            }
        })
        .unwrap_or(OpenInTarget::Finder)
}

pub(crate) fn matching_applications<'a>(
    apps: &'a [crate::system::InstalledApplication],
    query: &str,
) -> Vec<&'a crate::system::InstalledApplication> {
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    apps.iter()
        .filter(|app| {
            let text =
                format!("{} {} {}", app.name, app.bundle_id, app.path.display()).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .collect()
}

/// Open an exact connected application path, without parsing a command string.
pub fn launch_path_with(
    path: &std::path::Path,
    folder: &std::path::Path,
    spawn: impl FnOnce(&mut std::process::Command) -> std::io::Result<()>,
) -> std::io::Result<()> {
    spawn(
        std::process::Command::new("open")
            .arg("-a")
            .arg(path)
            .arg(folder),
    )
}
