//! The title bar's Open In — M12.10, decision 46: a menu of the engine.
//!
//! Enabled installed applications in the configured user order, with real icons,
//! then Copy Path, and the folder all of it acts on as the footnote.
//! `Target::OpenIn` carries the folder and the applications found at the
//! press; a row's deed names an application by its place in that list,
//! since a deed is `Copy`.

use std::path::Path;

use super::{Choice, Deed, Entry, Label, Mark, Target};
use crate::Root;
use crate::i18n::Text;

/// What a row of Open In does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenInDeed {
    /// The folder in Finder.
    Finder,
    /// The folder in the application at this place of the target's list.
    App(usize),
    /// The folder's path on the clipboard.
    CopyPath,
}

/// The menu: a heading, Finder, the external applications, Copy Path
/// and the folder. `apps` is `(name, bundle id)`, in the table's order.
pub fn open_in(
    apps: &[(String, String)],
    folder: &Path,
    selected: &guild_session::settings::OpenInTarget,
) -> Vec<Entry> {
    let row = |label: Label, mark: Mark, deed: OpenInDeed, detail: Option<Label>| {
        Entry::Choice(Choice {
            label,
            mark,
            deed: Deed::OpenIn(deed),
            check: match deed {
                OpenInDeed::CopyPath => None,
                OpenInDeed::Finder => Some(matches!(
                    selected,
                    guild_session::settings::OpenInTarget::Finder
                )),
                OpenInDeed::App(at) => Some(
                    matches!(selected, guild_session::settings::OpenInTarget::App(id) if apps.get(at).is_some_and(|(_, candidate)| candidate == id)),
                ),
            },
            enabled: true,
            detail,
            trailing: None,
        })
    };
    let mut entries = vec![Entry::Heading(Label::Text(Text::MenuOpenIn))];
    entries.extend(apps.iter().enumerate().map(|(at, (name, id))| {
        row(
            Label::Named(name.clone()),
            Mark::App(id.clone()),
            if id == "com.apple.finder" {
                OpenInDeed::Finder
            } else {
                OpenInDeed::App(at)
            },
            None,
        )
    }));
    entries.push(Entry::Separator);
    entries.push(row(
        Label::Text(Text::MenuCopyPath),
        Mark::Icon("ui/menu-copy.svg"),
        OpenInDeed::CopyPath,
        None,
    ));
    entries.push(Entry::Note(Label::Named(crate::open_in::shown(
        &crate::short_path(folder),
    ))));
    entries
}

impl Root {
    /// The Open In button's press: the menu under it, on the root of the
    /// project in front. Display resources are cached before its first render.
    pub(crate) fn open_open_in(
        &mut self,
        from_keyboard: bool,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Root>,
    ) {
        let Some(folder) = self
            .active_project
            .and_then(|id| self.desk.project(id))
            .map(|project| project.path.clone())
        else {
            return;
        };
        self.revalidate_open_in_target(cx);
        let rows = self.open_in_apps.as_ref().expect("loaded applications");
        let apps = rows
            .iter()
            .filter(|row| self.settings.open_in_apps.enabled(&row.key))
            .map(|row| (row.name.to_owned(), row.id.to_owned()))
            .collect();
        let anchor = match self.open_in_at.get() {
            Some(bounds) => super::Anchor::Below(bounds),
            None => super::Anchor::Pointer(gpui::point(
                crate::theme::space::CARD_GAP.to_pixels(window.rem_size()),
                crate::theme::space::TITLEBAR.to_pixels(window.rem_size()),
            )),
        };
        let target = Target::OpenIn { folder, apps };
        let first = if from_keyboard {
            super::step(&self.entries_for_target(&target, cx), None, true)
        } else {
            None
        };
        self.open_menu(target, anchor, window, cx);
        if let Some(menu) = self.context_menu.as_mut() {
            menu.lit = first;
        }
    }

    /// Carry out a row of Open In on the folder the target names.
    pub(super) fn run_open_in_deed(
        &mut self,
        deed: OpenInDeed,
        target: &Target,
        cx: &mut gpui::Context<Root>,
    ) {
        let Target::OpenIn { folder, apps } = target else {
            return;
        };
        use guild_session::settings::OpenInTarget;
        let selection = match deed {
            OpenInDeed::Finder => OpenInTarget::Finder,
            OpenInDeed::App(at) => {
                let Some((_, id)) = apps.get(at) else {
                    return;
                };
                OpenInTarget::App(id.clone())
            }
            OpenInDeed::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    folder.display().to_string(),
                ));
                return;
            }
        };
        self.settings.open_in_target = selection;
        self.revalidate_open_in_target(cx);
        self.store_settings();
        self.open_in_target_at(&self.selected_open_in_target(), folder, cx);
        cx.notify();
    }

    /// Load display resources before the first titlebar render, not the first menu.
    pub(crate) fn ensure_open_in_apps(&mut self) {
        if self.open_in_apps.is_some() {
            return;
        }
        let config = self.settings.open_in_apps.clone();
        let mut rows = Vec::new();
        for key in config.keys(&crate::open_in::default_keys()) {
            if let Some(app) = crate::open_in::APPS.iter().find(|app| app.ids[0] == key) {
                let saved = match &self.settings.open_in_target {
                    guild_session::settings::OpenInTarget::App(id) => Some(id.as_str()),
                    _ => None,
                };
                let id = saved
                    .filter(|id| app.ids.contains(id) && self.open_in_application_available(id))
                    .or_else(|| {
                        app.ids
                            .iter()
                            .copied()
                            .find(|id| self.open_in_application_available(id))
                    });
                if let Some(id) = id {
                    rows.push(crate::open_in::Connection {
                        key,
                        id: id.into(),
                        name: app.name.into(),
                        bundle_id: id.into(),
                        path: crate::system::application(id)
                            .map(|path| std::fs::canonicalize(&path).unwrap_or(path)),
                    });
                }
            } else if let Some(custom) = config.custom.iter().find(|row| row.id == key)
                && let Some(path) = &custom.path
                && let Some(app) = crate::system::application_bundle(path)
                && custom.bundle_id.as_deref() == Some(app.bundle_id.as_str())
            {
                rows.push(crate::open_in::Connection {
                    key: key.clone(),
                    id: key,
                    name: if custom.name.trim().is_empty() {
                        app.name
                    } else {
                        custom.name.clone()
                    },
                    bundle_id: app.bundle_id,
                    path: Some(app.path),
                });
            }
        }
        for row in &rows {
            if let Some(path) = &row.path {
                self.open_in_icon(&row.id, path);
            }
        }
        if let Some(path) = crate::system::application("com.apple.finder") {
            self.open_in_icon("com.apple.finder", &path);
        }
        if matches!(
            self.settings.open_in_target,
            guild_session::settings::OpenInTarget::Terminal
        ) {
            self.settings.open_in_target =
                crate::open_in::resolve_configured(&self.settings.open_in_target, &config, &rows);
            self.store_settings();
        }
        self.open_in_apps = Some(rows);
    }

    pub(crate) fn open_in_icon(
        &self,
        key: &str,
        path: &Path,
    ) -> Option<std::sync::Arc<gpui::Image>> {
        let path_key = format!("path:{}", path.display());
        let cached = self.open_in_icons.borrow().get(&path_key).cloned();
        let image = cached.or_else(|| {
            crate::system::icon_png(path, 32.0).map(|png| {
                std::sync::Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png))
            })
        })?;
        self.open_in_icons
            .borrow_mut()
            .insert(path_key, image.clone());
        self.open_in_icons
            .borrow_mut()
            .insert(key.to_owned(), image.clone());
        Some(image)
    }

    fn open_in_application_available(&self, id: &str) -> bool {
        if id.starts_with("custom:") {
            return self
                .open_in_apps
                .as_ref()
                .and_then(|rows| rows.iter().find(|row| row.id == id))
                .is_some_and(|row| {
                    row.path
                        .as_ref()
                        .and_then(|path| crate::system::application_bundle(path))
                        .is_some_and(|app| app.bundle_id == row.bundle_id)
                });
        }
        #[cfg(test)]
        if let Some(available) = &self.open_in_availability {
            return id == "com.apple.finder" || available.contains(id);
        }
        // Launch Services may retain an entry after the bundle has been removed.
        crate::system::application(id).is_some_and(|path| path.is_dir())
    }

    /// Recheck only on an opening gesture; cached render resources do not poll.
    pub(crate) fn revalidate_open_in_target(&mut self, cx: &mut gpui::Context<Root>) {
        use guild_session::settings::OpenInTarget;
        self.ensure_open_in_apps();
        let selected = self.selected_open_in_target();
        let resolved = match selected {
            OpenInTarget::App(ref id) if !self.open_in_application_available(id) => {
                OpenInTarget::Finder
            }
            target => target,
        };
        if resolved == self.settings.open_in_target {
            return;
        }
        if let OpenInTarget::App(id) = &self.settings.open_in_target {
            self.open_in_icons.borrow_mut().remove(id);
        }
        self.settings.open_in_target = resolved;
        // Refresh the selector too, including an available sibling of a removed EAP.
        self.open_in_apps = None;
        self.ensure_open_in_apps();
        self.store_settings();
        cx.notify();
    }

    pub(crate) fn selected_open_in_target(&self) -> guild_session::settings::OpenInTarget {
        crate::open_in::resolve_configured(
            &self.settings.open_in_target,
            &self.settings.open_in_apps,
            self.open_in_apps.as_deref().unwrap_or_default(),
        )
    }

    pub(crate) fn open_selected_app(&mut self, cx: &mut gpui::Context<Root>) {
        self.revalidate_open_in_target(cx);
        let Some(folder) = self
            .active_project
            .and_then(|id| self.desk.project(id))
            .map(|p| p.path.clone())
        else {
            return;
        };
        self.open_in_target_at(&self.selected_open_in_target(), &folder, cx);
    }

    fn open_in_target_at(
        &mut self,
        target: &guild_session::settings::OpenInTarget,
        folder: &Path,
        cx: &mut gpui::Context<Root>,
    ) {
        #[cfg(test)]
        if let Some(launches) = self.open_in_launches.as_mut() {
            launches.push((target.clone(), folder.to_owned()));
            return;
        }
        let spawn = |command: &mut std::process::Command| {
            let mut child = command.spawn()?;
            std::thread::spawn(move || child.wait());
            Ok(())
        };
        let custom_path = match target {
            guild_session::settings::OpenInTarget::App(id) if id.starts_with("custom:") => self
                .open_in_apps
                .as_ref()
                .and_then(|rows| rows.iter().find(|row| &row.id == id))
                .and_then(|row| row.path.clone()),
            _ => None,
        };
        let result = if let Some(path) = custom_path {
            crate::open_in::launch_path_with(&path, folder, spawn)
        } else {
            crate::open_in::launch_with(target, folder, spawn)
        };
        if let Err(err) = result {
            self.warn(format!("open: {err}"));
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "open_in_tests.rs"]
mod tests;
