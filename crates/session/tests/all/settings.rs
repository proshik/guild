//! Application settings persist independently of session state.

use crate::scratch::Scratch;
use guild_session::{Panels, PermissionMode, Settings, Store};

fn temp_dir(name: &str) -> Scratch {
    Scratch::absent(&format!("guild-settings-{name}-"))
}

/// M12: whether the column is folded is the window's own choice, and it
/// is kept — folding it every morning would be a setting that does not
/// settle.
#[test]
fn the_folded_column_survives_a_restart() {
    let dir = temp_dir("column");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));

    store
        .save(&Settings {
            column_folded: true,
            ..Settings::default()
        })
        .expect("the settings are saved");

    assert!(store.load().expect("and read back").column_folded);
}

/// The column's mode and Settled's threshold are the window's, kept
/// across a restart; a file written before reads as Projects and 3 days.
#[test]
fn the_column_mode_and_settle_threshold_survive_a_restart() {
    let dir = temp_dir("column-mode");
    let path = dir.join("settings.json");
    let store: Store<Settings> = Store::new(path.clone());
    let default = Settings::default();
    assert_eq!(
        default.column_mode,
        guild_session::settings::ColumnMode::Projects
    );
    assert_eq!(default.settle_after_days, 3);
    store
        .save(&Settings {
            column_mode: guild_session::settings::ColumnMode::Sessions,
            settle_after_days: 7,
            ..Settings::default()
        })
        .expect("the settings are saved");
    let restored = store.load().expect("and read back");
    assert_eq!(
        restored.column_mode,
        guild_session::settings::ColumnMode::Sessions
    );
    assert_eq!(restored.settle_after_days, 7);

    let mut old = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("column_mode");
    old.as_object_mut().unwrap().remove("settle_after_days");
    std::fs::write(&path, old.to_string()).unwrap();
    let restored = store.load().expect("an old file reads");
    assert_eq!(
        restored.column_mode,
        guild_session::settings::ColumnMode::Projects
    );
    assert_eq!(restored.settle_after_days, 3);
}

/// A settings file written before M12 has no such key, and reads as a
/// column that is open: a person who has never folded it should see what
/// it holds.
#[test]
fn a_settings_file_without_the_column_reads_as_open() {
    let dir = temp_dir("pre-m12");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let file = dir.join("settings.json");
    std::fs::write(&file, "{}").expect("a file with none of the keys");

    let restored = Store::<Settings>::new(file).load().expect("it still reads");

    assert!(!restored.column_folded);
}

#[test]
fn settings_survive_a_restart() {
    let dir = temp_dir("restart");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));

    store
        .save(&Settings {
            column_folded: true,
            ..Settings::default()
        })
        .expect("the settings are saved");

    let restored = store.load().expect("and read back");

    assert!(restored.column_folded);
}

/// The same law as for the session file: corrupted settings must not stop
/// the application from opening. The difference is that the cost of a
/// mistake is lower here — one setting is lost, not the session list.
#[test]
fn broken_settings_file_falls_back_to_defaults() {
    let dir = temp_dir("broken");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, "{this is not json").unwrap();

    let settings = Store::<Settings>::new(path)
        .load()
        .expect("startup must not fail");

    assert!(!settings.column_folded);
}

/// An unknown field in the file is no reason to lose the settings: Guild
/// versions drift apart, and settings from a newer version must be
/// readable by an older one.
#[test]
fn unknown_fields_do_not_discard_the_file() {
    let dir = temp_dir("forward");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, r#"{"column_folded":true,"theme_colour":"dark"}"#).unwrap();

    let settings = Store::<Settings>::new(path).load().unwrap();

    assert!(
        settings.column_folded,
        "a known field must survive an unknown neighbour"
    );
}

/// The permission policy is chosen **in advance**, not on the fly.
///
/// There is nothing to approve a request with as it happens: CLI 2.1.251
/// has no `--permission-prompt-tool`, and without it a request becomes a
/// refusal at once. So the only thing a person can decide is what is
/// allowed from the start.
///
/// **It opens on `auto` since the person asked for it**, and the reason
/// the strict default lasted as long as it did is worth keeping: refusing
/// is safer than allowing silently. What changed is who is asking — a
/// window built for agents that work, where every new tab said *manual
/// mode on* and the first act was always the same keystroke. `claude`
/// still asks about whatever its own settings say to ask about.
#[test]
fn a_new_session_opens_in_auto() {
    assert_eq!(Settings::default().permission_mode, PermissionMode::Auto);
}

#[test]
fn permission_mode_survives_a_restart() {
    let dir = temp_dir("permissions");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));

    store
        .save(&Settings {
            permission_mode: PermissionMode::AcceptEdits,
            ..Settings::default()
        })
        .expect("the settings are saved");

    assert_eq!(
        store.load().unwrap().permission_mode,
        PermissionMode::AcceptEdits
    );
}

/// The value goes to the `claude` command line as it is, so it must match
/// what `claude` accepts. The list comes from `claude --help` version
/// 2.1.251.
#[test]
fn permission_mode_matches_the_cli_vocabulary() {
    let pairs = [
        (PermissionMode::Manual, "manual"),
        (PermissionMode::AcceptEdits, "acceptEdits"),
        (PermissionMode::Auto, "auto"),
        (PermissionMode::Plan, "plan"),
        (PermissionMode::DontAsk, "dontAsk"),
        (PermissionMode::BypassPermissions, "bypassPermissions"),
    ];

    for (mode, expected) in pairs {
        assert_eq!(mode.as_cli_value(), expected);
    }
}

/// Select and it is copied. A habit from other terminals: no separate
/// action is needed to reach the clipboard.
///
/// A setting rather than a fixed behaviour: those it annoys, it annoys a
/// lot — every selection overwrites the clipboard, including one made just
/// to read.
#[test]
fn copy_on_select_is_on_by_default() {
    assert!(Settings::default().copy_on_select);
}

#[test]
fn copy_on_select_survives_a_restart() {
    let dir = temp_dir("copying");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));

    store
        .save(&Settings {
            copy_on_select: false,
            ..Settings::default()
        })
        .unwrap();

    assert!(!store.load().unwrap().copy_on_select);
}

/// The font size is changed with a shortcut, and without a limit a held
/// `Cmd+-` drives it to zero: the interface becomes invisible, and there
/// is nothing left to bring it back with — what would bring it back is not
/// visible either.
#[test]
fn the_font_size_is_kept_within_readable_limits() {
    let dir = temp_dir("font-size");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");

    std::fs::write(&path, r#"{"terminal_font_size": 0.0}"#).unwrap();
    let tiny = Store::<Settings>::new(path.clone()).load().unwrap();
    assert_eq!(tiny.terminal_font_size, guild_session::MIN_FONT_SIZE);

    std::fs::write(&path, r#"{"terminal_font_size": 900.0}"#).unwrap();
    let huge = Store::<Settings>::new(path.clone()).load().unwrap();
    assert_eq!(huge.terminal_font_size, guild_session::MAX_FONT_SIZE);
    let store = Store::<Settings>::new(path);
    for size in [12.5, 13.0, 13.5, 13.0, 12.5] {
        store
            .save(&Settings {
                terminal_font_size: size,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            store.load().unwrap().terminal_font_size,
            size,
            "half points survive restart"
        );
    }
}

#[test]
fn the_default_font_size_is_the_one_from_the_design_system() {
    assert_eq!(Settings::default().terminal_font_size, 12.5);
}

/// A window restored off-screen, or collapsed to a dot, looks like an
/// application that failed to start: there is an icon in the Dock but no
/// window. Monitors get unplugged and resolutions change — it has to be
/// checked.
#[test]
fn a_window_place_that_would_hide_the_window_is_rejected() {
    use guild_session::WindowPlace;

    let normal = WindowPlace {
        x: 100.0,
        y: 100.0,
        width: 1180.0,
        height: 760.0,
    };
    assert!(normal.is_usable());

    let collapsed = WindowPlace {
        width: 12.0,
        ..normal
    };
    assert!(!collapsed.is_usable(), "a collapsed window is not restored");

    let broken = WindowPlace {
        x: f32::NAN,
        ..normal
    };
    assert!(!broken.is_usable(), "a non-number is not a place");
}

#[test]
fn the_window_place_survives_a_restart() {
    use guild_session::WindowPlace;

    let dir = temp_dir("window");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));
    let place = WindowPlace {
        x: 40.0,
        y: 60.0,
        width: 1200.0,
        height: 800.0,
    };

    store
        .save(&Settings {
            window: Some(place),
            ..Settings::default()
        })
        .unwrap();

    assert_eq!(store.load().unwrap().window, Some(place));
}

/// The three switches of Projects & windows survive a restart, and a file
/// written before they existed still reads — with the defaults a person
/// would expect rather than with `false` for all three.
#[test]
fn the_project_switches_survive_a_restart() {
    let dir = temp_dir("project-switches");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));

    let mut settings = Settings::default();
    assert!(settings.reopen_last_project, "on by default");
    assert!(settings.confirm_closing_live_project, "on by default");
    assert!(
        !settings.forget_sessions_on_close,
        "off by default: a reopened project comes back with its work"
    );

    settings.reopen_last_project = false;
    settings.forget_sessions_on_close = true;
    store.save(&settings).expect("saved");

    let back = store.load().expect("loaded");
    assert!(!back.reopen_last_project);
    assert!(back.confirm_closing_live_project);
    assert!(back.forget_sessions_on_close);
}

/// A settings file written before these existed must not read as three
/// switches turned off: two of them being off changes what the window
/// does, and nobody asked for that.
#[test]
fn a_file_without_the_project_switches_keeps_the_defaults() {
    let dir = temp_dir("project-switches-absent");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, r#"{"language":"english","terminal_font_size":12.5}"#).unwrap();

    let settings = Store::<Settings>::new(path).load().expect("loads");

    assert!(settings.reopen_last_project);
    assert!(settings.confirm_closing_live_project);
    assert!(!settings.forget_sessions_on_close);
}

/// Auto fast-forward of the default branch is on unless a person turns it
/// off — including for a file written before it existed — and the choice
/// survives a restart.
#[test]
fn auto_fast_forward_is_on_by_default_and_its_choice_survives() {
    assert!(Settings::default().auto_fast_forward_default_branch);

    let dir = temp_dir("auto-fast-forward");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, r#"{"language":"english"}"#).unwrap();
    let store = Store::<Settings>::new(path);
    let mut settings = store.load().expect("loads");
    assert!(settings.auto_fast_forward_default_branch, "absent reads on");

    settings.auto_fast_forward_default_branch = false;
    store.save(&settings).expect("saved");
    assert!(
        !store
            .load()
            .expect("loaded")
            .auto_fast_forward_default_branch
    );
}

/// GitLab merge request status in the column is on unless a person turns
/// it off — including for a file written before it existed — and the choice
/// survives a restart. GLD-063.
#[test]
fn gitlab_merge_requests_are_on_by_default_and_their_choice_survives() {
    assert!(Settings::default().gitlab_merge_requests);

    let dir = temp_dir("gitlab-merge-requests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, r#"{"language":"english"}"#).unwrap();
    let store = Store::<Settings>::new(path);
    let mut settings = store.load().expect("loads");
    assert!(settings.gitlab_merge_requests, "absent reads on");

    settings.gitlab_merge_requests = false;
    store.save(&settings).expect("saved");
    assert!(!store.load().expect("loaded").gitlab_merge_requests);
}

/// Autosave is on unless a person turns it off, and the choice survives
/// a restart.
#[test]
fn autosave_is_on_by_default_and_survives_a_restart() {
    let dir = temp_dir("autosave");
    let store = Store::<Settings>::new(dir.join("settings.json"));
    let mut settings = store.load().unwrap();
    assert!(settings.autosave, "on by default");

    settings.autosave = false;
    store.save(&settings).unwrap();
    let back = store.load().unwrap();
    assert!(!back.autosave);
}

/// The diff is side by side unless a person chose otherwise, and the
/// choice survives a restart; a settings file from before the choice
/// existed reads back as side by side.
#[test]
fn the_diff_view_is_side_by_side_by_default_and_survives_a_restart() {
    use guild_session::DiffView;
    let dir = temp_dir("diff-view");
    let store = Store::<Settings>::new(dir.join("settings.json"));
    let mut settings = store.load().unwrap();
    assert_eq!(settings.diff_view, DiffView::SideBySide);

    settings.diff_view = DiffView::Unified;
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap().diff_view, DiffView::Unified);
    assert_eq!(DiffView::Unified.toggled(), DiffView::SideBySide);
}

/// **The panels are the window's, and a project's layout is taken once** —
/// M12.2 task 3. Which docks are open and how wide were kept per project,
/// so switching projects rebuilt the window; every IDE keeps them for the
/// window. A person whose layout lives in a project's file today finds it
/// kept: the project in front gives it the first time, and no project
/// after that overrides it.
#[test]
fn the_panels_are_the_window_s_and_a_project_s_layout_is_taken_once() {
    let mut settings = Settings::default();
    assert_eq!(settings.panels, None, "nothing chosen yet");

    let mut chosen = Panels::default();
    chosen.left.open = true;
    chosen.left.size = 300.0;
    assert!(
        settings.adopt_panels(Some(chosen.clone())),
        "taken from the project in front"
    );
    assert_eq!(settings.panels.as_ref(), Some(&chosen));

    let mut other = Panels::default();
    other.bottom.open = true;
    assert!(
        !settings.adopt_panels(Some(other)),
        "a second project changes nothing"
    );
    assert_eq!(settings.panels.as_ref(), Some(&chosen));

    let dir = temp_dir("panels");
    let store = Store::new(dir.join("settings.json"));
    store.save(&settings).unwrap();
    // Read back tidied — its docks spelled out, since M8.3.6 reads the
    // window's layout through the rule a project's goes through.
    let mut tidied = chosen.clone();
    tidied.tidy();
    assert_eq!(
        store.load().unwrap().panels.as_ref(),
        Some(&tidied),
        "survives a restart"
    );

    let mut fresh = Settings::default();
    assert!(
        fresh.adopt_panels(None),
        "no project: the defaults, written down"
    );
    assert_eq!(fresh.panels, Some(Panels::default()));

    // A settings file from before has no key at all.
    let mut old: serde_json::Value = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("panels");
    let read: Settings = serde_json::from_value(old).unwrap();
    assert_eq!(read.panels, None);
}

/// **Recent Files stays where it was dragged** — M12.3 task 3, as Recent
/// Projects has since M5.8. Unmoved it has no place written down, which
/// is *centred*; a file from before has no key and reads the same.
#[test]
fn recent_files_stays_where_it_was_left() {
    let mut settings = Settings::default();
    assert_eq!(settings.recent_files.at, None, "centred until moved");
    settings.recent_files.at = Some(guild_session::Spot { x: 40.0, y: 90.0 });

    let dir = temp_dir("recent-files-place");
    let store = Store::new(dir.join("settings.json"));
    store.save(&settings).unwrap();
    let read = store.load().unwrap();
    assert_eq!(
        read.recent_files.at,
        Some(guild_session::Spot { x: 40.0, y: 90.0 })
    );

    let mut old: serde_json::Value = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("recent_files");
    let from_before: Settings = serde_json::from_value(old).unwrap();
    assert_eq!(from_before.recent_files.at, None);
}

/// **The size the window's tab terminals were last fitted to** survives a
/// restart — a tab restored behind another is spawned before anything is
/// drawn, and `claude` prints its first screen at whatever the pty says.
/// A file from before has none, and a tab starts at 80 × 24 as it did.
#[test]
fn the_tab_grid_survives_a_restart_and_an_old_file_has_none() {
    let dir = temp_dir("tab-grid");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));
    let grid = guild_session::TabGrid {
        cols: 122,
        rows: 29,
    };
    store
        .save(&Settings {
            tab_grid: Some(grid),
            ..Settings::default()
        })
        .expect("the settings are saved");
    assert_eq!(store.load().expect("and read back").tab_grid, Some(grid));

    std::fs::write(dir.join("settings.json"), "{}").expect("an old file");
    assert_eq!(store.load().expect("an old file reads").tab_grid, None);
}

/// **Closing an agent's tab asks, by default** — 2026-09-16 for Claude
/// Code, every harness since 2026-09-28. A file from before has no such
/// key and asks too: the person asked for it.
#[test]
fn closing_an_agent_tab_asks_by_default_and_in_an_old_file() {
    assert!(Settings::default().confirm_closing_agent_tab);
    let old: Settings = serde_json::from_str("{}").expect("an old file reads");
    assert!(old.confirm_closing_agent_tab);
    let off: Settings = serde_json::from_str(r#"{"confirm_closing_agent_tab": false}"#)
        .expect("a file that turned it off reads");
    assert!(!off.confirm_closing_agent_tab);
}

/// **A file written while the setting was Claude's keeps its answer.**
/// The key was `confirm_closing_claude_tab` until the question covered
/// Codex and opencode too; a person who turned it off is not asked again
/// because it was renamed.
#[test]
fn the_setting_s_old_key_is_still_read() {
    let off: Settings = serde_json::from_str(r#"{"confirm_closing_claude_tab": false}"#)
        .expect("a file from before the rename reads");
    assert!(!off.confirm_closing_agent_tab);
}

/// **A satellite window's place is remembered** — M8.3.6, as the main
/// window's is — and a file from before has none.
#[test]
fn the_satellite_windows_places_survive_and_an_old_file_has_none() {
    let place = guild_session::WindowPlace {
        x: 10.0,
        y: 20.0,
        width: 900.0,
        height: 600.0,
    };
    let written = serde_json::to_string(&Settings {
        push_window: Some(place),
        diff_window: Some(place),
        ..Settings::default()
    })
    .expect("it writes");
    let read: Settings = serde_json::from_str(&written).expect("it reads");
    assert_eq!(read.push_window, Some(place));
    assert_eq!(read.diff_window, Some(place));

    let splits: Settings = serde_json::from_str(
        &serde_json::to_string(&Settings {
            push_splits: Some((0.6, 0.55)),
            ..Settings::default()
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(splits.push_splits, Some((0.6, 0.55)));

    let old: Settings = serde_json::from_str("{}").expect("an old file reads");
    assert_eq!(old.push_splits, None);
    assert_eq!(old.push_window, None);
    assert_eq!(old.diff_window, None);
}

/// **The window's docks lose a retired tool when read** — M8.3.6: the
/// person's own settings had Push shown in the bottom dock.
#[test]
fn a_window_whose_bottom_dock_showed_push_reads_without_it() {
    let dir = temp_dir("retired-push");
    std::fs::create_dir_all(&dir).expect("a directory");
    let file = dir.join("settings.json");
    std::fs::write(
        &file,
        // The person's own `panels`, taken off the disk on 2026-09-16.
        r#"{"panels": {"face": "log", "docks": {"placed": [["log", "bottom"], ["output", "bottom"], ["search", "bottom"], ["problems", "bottom"], ["commit", "right"], ["push", "bottom"], ["explorer", "left"]], "shown": [["right", "commit"], ["left", "explorer"], ["bottom", "push"]]}, "left": {"open": false, "size": 375.79834}, "right": {"open": false, "size": 490.76172}, "bottom": {"open": true, "size": 394.90625}, "columns": {"branches": 288.2461, "details": 357.92188, "message": 113.01953, "push_files": 710.3047}, "git_face": "log"}}"#,
    )
    .expect("an M12.3 file");
    let read = Store::<Settings>::new(file).load().expect("it reads");
    let docks = read.panels.expect("panels").docks().clone();
    assert!(!docks.docked(guild_session::Tool::Push));
    assert_eq!(
        docks.shown(guild_session::Dock::Bottom),
        Some(guild_session::Tool::Git)
    );
}

/// **A file from before the split keeps its face in the editor** — M12.11,
/// decision 2. One setting named the face of the terminal and of code
/// alike; the person who chose it meant both, and the split must not take
/// it off their files without a word.
#[test]
fn an_old_file_carries_its_face_into_code() {
    let dir = temp_dir("code-font-old");
    let path = dir.join("settings.json");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, r#"{"terminal_font":"Menlo"}"#).unwrap();

    let settings = Store::<Settings>::new(path).load().expect("loads");

    assert_eq!(settings.terminal_font.as_deref(), Some("Menlo"));
    assert_eq!(settings.code_font.as_deref(), Some("Menlo"));
}

/// **And only once.** A person who, after the split, puts code back on no
/// choice has made a choice, and the next start must not copy the
/// terminal's face over it again.
#[test]
fn a_new_file_keeps_code_apart() {
    let dir = temp_dir("code-font-new");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));
    assert!(Settings::default().code_font_own, "a new file starts split");

    store
        .save(&Settings {
            terminal_font: Some("Menlo".to_owned()),
            code_font: None,
            ..Settings::default()
        })
        .expect("the settings are saved");
    let restored = store.load().expect("and read back");

    assert_eq!(restored.terminal_font.as_deref(), Some("Menlo"));
    assert_eq!(restored.code_font, None, "no choice stays no choice");
}

#[test]
fn development_workflows_are_opt_in_and_preserve_other_settings() {
    let legacy: Settings =
        serde_json::from_str(r#"{"copy_on_select":false,"ui_font_size":15.0}"#).unwrap();
    let mut value = serde_json::to_value(&legacy).unwrap();
    assert_eq!(value["development_workflows"], false);
    value["development_workflows"] = true.into();
    let enabled: Settings = serde_json::from_value(value).unwrap();
    let dir = temp_dir("experimental");
    let store = Store::new(dir.join("settings.json"));
    store.save(&enabled).unwrap();
    let restored: Settings = store.load().unwrap();
    let mut restored = serde_json::to_value(restored).unwrap();
    assert_eq!(restored["development_workflows"], true);
    assert_eq!(restored["copy_on_select"], false);
    assert_eq!(restored["ui_font_size"], 15.0);
    restored["development_workflows"] = false.into();
    store
        .save(&serde_json::from_value::<Settings>(restored).unwrap())
        .unwrap();
    let off = serde_json::to_value(store.load().unwrap()).unwrap();
    assert_eq!(off["development_workflows"], false);
    assert_eq!(off["copy_on_select"], false);
}

/// Newer action-bar identifiers must not discard any of the person's settings.
#[test]
fn action_bar_order_store_roundtrip_preserves_unknown_ids_and_other_settings() {
    let dir = temp_dir("action-bar");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    let raw = serde_json::json!({
        "action_bar_order": ["terminal_jobs", "future_control", "usage", "usage"],
        "action_bar_excluded": ["open_in", "future_hidden"],
        "column_folded": true,
        "terminal_font_size": 19.0,
        "language": "russian"
    });
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let store = Store::<Settings>::new(path.clone());
    let settings = store.load().unwrap();
    assert!(settings.column_folded);
    assert_eq!(settings.terminal_font_size, 19.0);
    store.save(&settings).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["action_bar_order"], raw["action_bar_order"]);
    assert_eq!(saved["action_bar_excluded"], raw["action_bar_excluded"]);
    assert_eq!(saved["language"], raw["language"]);
    assert_eq!(store.load().unwrap(), settings);
}

#[test]
fn action_bar_order_defaults_normalizes_and_moves_without_hiding_controls() {
    use guild_session::action_bar::{self, Item::*};
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(
        action_bar::effective(&old.action_bar_order),
        [Usage, OpenIn, TerminalJobs]
    );
    let mut saved = vec![
        "terminal_jobs".into(),
        "future".into(),
        "usage".into(),
        "usage".into(),
    ];
    assert_eq!(action_bar::effective(&saved), [TerminalJobs, Usage, OpenIn]);
    assert!(!action_bar::move_to(&mut saved, &[], TerminalJobs, 0));
    assert!(!action_bar::move_to(&mut saved, &[], TerminalJobs, 3));
    assert!(action_bar::move_to(&mut saved, &[], OpenIn, 0));
    assert_eq!(action_bar::effective(&saved), [OpenIn, TerminalJobs, Usage]);
    assert!(saved.contains(&"future".into()));
    action_bar::reset(&mut saved, &mut Vec::new());
    assert_eq!(action_bar::effective(&saved), [Usage, OpenIn, TerminalJobs]);
}

#[test]
fn action_bar_composition_subsets_empty_add_and_restore() {
    use guild_session::action_bar::{self, Item::*};
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(
        action_bar::visible(&old.action_bar_order, &old.action_bar_excluded),
        [Usage, OpenIn, TerminalJobs]
    );
    let mut order = vec!["future".into(), "terminal_jobs".into(), "usage".into()];
    let mut excluded = vec!["future_hidden".into()];
    assert!(action_bar::remove(&mut excluded, OpenIn));
    assert!(!action_bar::remove(&mut excluded, OpenIn));
    assert_eq!(
        action_bar::visible(&order, &excluded),
        [TerminalJobs, Usage]
    );
    assert!(!action_bar::move_to(&mut order, &excluded, OpenIn, 0));
    assert!(!action_bar::move_to(&mut order, &excluded, Usage, 2));
    assert!(action_bar::move_to(&mut order, &excluded, Usage, 0));
    assert_eq!(
        action_bar::visible(&order, &excluded),
        [Usage, TerminalJobs]
    );
    assert!(action_bar::add(&mut order, &mut excluded, OpenIn));
    assert!(!action_bar::add(&mut order, &mut excluded, OpenIn));
    assert_eq!(
        action_bar::visible(&order, &excluded),
        [Usage, TerminalJobs, OpenIn]
    );
    for item in [Usage, OpenIn, TerminalJobs] {
        assert!(action_bar::remove(&mut excluded, item));
    }
    assert!(action_bar::visible(&order, &excluded).is_empty());
    let settings = Settings {
        action_bar_order: order,
        action_bar_excluded: excluded,
        ..Default::default()
    };
    let dir = temp_dir("composition");
    let path = dir.join("settings.json");
    let store = Store::<Settings>::new(path);
    store.save(&settings).unwrap();
    let mut loaded = store.load().unwrap();
    assert_eq!(loaded, settings);
    assert!(action_bar::visible(&loaded.action_bar_order, &loaded.action_bar_excluded).is_empty());
    assert!(action_bar::add(
        &mut loaded.action_bar_order,
        &mut loaded.action_bar_excluded,
        Usage
    ));
    assert_eq!(
        action_bar::visible(&loaded.action_bar_order, &loaded.action_bar_excluded),
        [Usage]
    );
    assert!(loaded.action_bar_order.contains(&"future".into()));
    assert!(loaded.action_bar_excluded.contains(&"future_hidden".into()));
    action_bar::reset(
        &mut loaded.action_bar_order,
        &mut loaded.action_bar_excluded,
    );
    assert!(loaded.action_bar_order.is_empty() && loaded.action_bar_excluded.is_empty());
    assert_eq!(
        action_bar::visible(&loaded.action_bar_order, &loaded.action_bar_excluded),
        [Usage, OpenIn, TerminalJobs]
    );
}

#[test]
fn codex_independent_permissions_persist_without_changing_arguments() {
    let dir = temp_dir("codex-permissions");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));
    let mut settings = Settings::default();
    settings.launch_arguments.codex =
        "  --sandbox danger-full-access -a never --model 'multi word'  ".into();
    let original = settings.launch_arguments.codex.clone();
    for sandbox in [true, false] {
        for approvals in [true, false] {
            settings.codex_sandbox = sandbox;
            settings.codex_command_approvals = approvals;
            store.save(&settings).unwrap();
            settings = store.load().unwrap();
            assert_eq!(
                (settings.codex_sandbox, settings.codex_command_approvals),
                (sandbox, approvals)
            );
            assert_eq!(settings.launch_arguments.codex, original);
        }
    }
}

#[test]
fn launcher_order_preserves_preferences_and_repairs_duplicates() {
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.tab_launcher_order(), guild_session::DefaultTab::ALL);
    let custom: Settings =
        serde_json::from_str(r#"{"tab_launcher_order":["terminal","codex","terminal"]}"#).unwrap();
    assert_eq!(
        custom.tab_launcher_order(),
        vec![
            guild_session::DefaultTab::Terminal,
            guild_session::DefaultTab::Codex,
            guild_session::DefaultTab::Claude,
            guild_session::DefaultTab::OpenCode
        ]
    );
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
    assert_eq!(restored.tab_launcher_order(), custom.tab_launcher_order());
}

#[test]
fn launcher_order_survives_a_store_restart() {
    let dir = temp_dir("launcher-order");
    let store: Store<Settings> = Store::new(dir.join("settings.json"));
    let settings = Settings {
        tab_launcher_order: vec![
            guild_session::DefaultTab::Terminal,
            guild_session::DefaultTab::Codex,
        ],
        ..Default::default()
    };
    store.save(&settings).unwrap();
    assert_eq!(
        store.load().unwrap().tab_launcher_order(),
        settings.tab_launcher_order()
    );
}

/// `two-themes-customize`: two palettes, and a mode that names one or
/// follows the system.
mod themes {
    use super::temp_dir;
    use guild_session::settings::{Customization, ThemeCustom, ThemeMode, Tint};
    use guild_session::{Settings, Store, ThemeKind};

    /// Write `json` as a settings file and read it the way the window does.
    fn load(name: &str, json: &str) -> (Settings, std::path::PathBuf, super::Scratch) {
        let dir = temp_dir(name);
        std::fs::create_dir_all(&*dir).expect("a directory to write into");
        let file = dir.join("settings.json");
        std::fs::write(&file, json).expect("the file");
        let settings = Store::<Settings>::new(file.clone())
            .load()
            .expect("it reads");
        (settings, file, dir)
    }

    /// Nothing beside the file was made: it was not moved aside as unreadable.
    fn nothing_moved_aside(file: &std::path::Path) {
        assert!(file.exists(), "the file is where it was");
        let siblings: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("unreadable"))
            .collect();
        assert!(siblings.is_empty(), "moved aside: {siblings:?}");
    }

    #[test]
    fn two_palettes_dark_by_default_light_is_light() {
        assert_eq!(ThemeKind::ALL, &[ThemeKind::Light, ThemeKind::Dark]);
        assert_eq!(ThemeKind::default(), ThemeKind::Dark);
        assert!(ThemeKind::Light.is_light());
        assert!(!ThemeKind::Dark.is_light());
        assert_eq!(ThemeKind::Light.name(), "Light");
        assert_eq!(ThemeKind::Dark.name(), "Dark");
    }

    #[test]
    fn a_mode_resolves_against_the_system() {
        for system_dark in [false, true] {
            assert_eq!(ThemeMode::Light.resolve(system_dark), ThemeKind::Light);
            assert_eq!(ThemeMode::Dark.resolve(system_dark), ThemeKind::Dark);
        }
        assert_eq!(ThemeMode::System.resolve(false), ThemeKind::Light);
        assert_eq!(ThemeMode::System.resolve(true), ThemeKind::Dark);
    }

    #[test]
    fn a_fresh_install_follows_the_system() {
        let fresh = Settings::default();
        assert_eq!(fresh.theme_mode(), ThemeMode::System);
        assert_eq!(fresh.theme_kind(true), ThemeKind::Dark);
        assert_eq!(fresh.theme_kind(false), ThemeKind::Light);
        let written = serde_json::to_value(&fresh).unwrap();
        assert_eq!(written["theme_mode"], "system");
        assert!(
            written.get("theme").is_none(),
            "no legacy key is invented: {written}"
        );
    }

    /// Spec "Migration of the old theme setting", through `Store::load`.
    #[test]
    fn every_old_palette_opens_light_or_dark() {
        for (old, mode) in [
            ("cloud", ThemeMode::Light),
            ("light", ThemeMode::Light),
            ("guild", ThemeMode::Dark),
            ("cide", ThemeMode::Dark),
            ("darcula", ThemeMode::Dark),
            ("ghostty", ThemeMode::Dark),
            ("slate", ThemeMode::Dark),
            ("islands", ThemeMode::Dark),
            ("sepia", ThemeMode::Dark),
        ] {
            let json = format!(r#"{{"theme":"{old}","copy_on_select":false}}"#);
            let (settings, file, _dir) = load(&format!("migrate-{old}"), &json);
            assert_eq!(settings.theme_mode(), mode, "{old}");
            assert!(
                !settings.copy_on_select,
                "{old}: the rest of the file was read"
            );
            nothing_moved_aside(&file);
        }
    }

    #[test]
    fn a_file_with_no_theme_opens_dark() {
        let (settings, _file, _dir) = load("migrate-none", r#"{"copy_on_select":false}"#);
        assert_eq!(settings.theme_mode(), ThemeMode::Dark);
        let direct: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(
            direct.theme_mode(),
            ThemeMode::Dark,
            "without normalise too"
        );
    }

    #[test]
    fn a_saved_mode_is_not_migrated_again() {
        let (settings, _file, _dir) =
            load("no-remigrate", r#"{"theme":"cloud","theme_mode":"system"}"#);
        assert_eq!(settings.theme_mode(), ThemeMode::System);
    }

    #[test]
    fn an_unknown_mode_reads_as_system_and_keeps_the_rest() {
        let (settings, file, _dir) = load(
            "unknown-mode",
            r#"{"theme_mode":"sepia","copy_on_select":false}"#,
        );
        assert_eq!(settings.theme_mode(), ThemeMode::System);
        assert!(!settings.copy_on_select);
        nothing_moved_aside(&file);
    }

    /// Byte for byte: an older build reading the file keeps its own choice.
    #[test]
    fn the_old_value_survives_a_save() {
        let (settings, file, _dir) = load("legacy-kept", r#"{"theme":"cloud"}"#);
        Store::<Settings>::new(file.clone())
            .save(&settings)
            .unwrap();
        guild_session::flush();
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(written["theme"], "cloud");
        assert_eq!(written["theme_mode"], "light");
    }

    #[test]
    fn a_customization_round_trips_as_hex() {
        let mut settings = Settings::default();
        settings.theme_custom.dark.accent = Some(Tint {
            color: 0xF97316,
            intensity: 100,
        });
        settings.theme_custom.light.background = Some(Tint {
            color: 0x3b82f6,
            intensity: 40,
        });
        let written = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            written["theme_custom"]["dark"]["accent"],
            serde_json::json!({"color": "#f97316", "intensity": 100})
        );
        let read: Settings = serde_json::from_value(written).unwrap();
        assert_eq!(read.theme_custom, settings.theme_custom);
        assert!(read.theme_custom.dark.background.is_none());
    }

    /// Spec "Malformed customization": one entry goes, nothing else does.
    #[test]
    fn a_malformed_customization_is_dropped_alone() {
        let (settings, file, _dir) = load(
            "malformed",
            r##"{"copy_on_select":false,"theme_mode":"dark","theme_custom":{
                "dark":{"accent":{"color":"blue","intensity":50},
                        "background":{"color":"#3b82f6","intensity":250},
                        "highlight":{"color":"#ABC","intensity":-3}},
                "light":7}}"##,
        );
        assert!(!settings.copy_on_select);
        assert_eq!(settings.theme_mode(), ThemeMode::Dark);
        let dark = settings.theme_custom.dark;
        assert_eq!(dark.accent, None, "not a colour");
        assert_eq!(
            dark.background,
            Some(Tint {
                color: 0x3b82f6,
                intensity: 100
            }),
            "clamped"
        );
        assert_eq!(
            dark.highlight,
            Some(Tint {
                color: 0xaabbcc,
                intensity: 0
            }),
            "short hex, clamped up"
        );
        assert_eq!(settings.theme_custom.light, Customization::default());
        nothing_moved_aside(&file);

        let whole: Settings = serde_json::from_str(r#"{"theme_custom":"x"}"#).unwrap();
        assert_eq!(whole.theme_custom, ThemeCustom::default());
    }

    #[test]
    fn a_kind_names_its_customization() {
        let mut custom = ThemeCustom::default();
        custom.of_mut(ThemeKind::Light).accent = Some(Tint {
            color: 1,
            intensity: 1,
        });
        assert!(custom.of(ThemeKind::Light).accent.is_some());
        assert!(custom.of(ThemeKind::Dark).is_empty());
        assert!(!custom.of(ThemeKind::Light).is_empty());
    }
}

/// The Sessions mode's folds are the window's, like the mode itself: which
/// status sections a person collapsed into the stack at the bottom and
/// whether Settled is open survive a restart, and a file written before
/// reads as every section expanded and Settled closed.
#[test]
fn the_session_folds_survive_a_restart_and_an_old_file_reads_expanded() {
    use guild_session::settings::{SessionFolds, SessionSection};

    let dir = temp_dir("session-folds");
    let path = dir.join("settings.json");
    let store: Store<Settings> = Store::new(path.clone());
    assert_eq!(Settings::default().session_folds, SessionFolds::default());
    assert!(SessionFolds::default().collapsed.is_empty());
    assert!(!SessionFolds::default().settled_open);

    let folds = SessionFolds {
        collapsed: [SessionSection::Done, SessionSection::Idle]
            .into_iter()
            .collect(),
        settled_open: true,
    };
    store
        .save(&Settings {
            session_folds: folds.clone(),
            ..Settings::default()
        })
        .expect("the settings are saved");
    assert_eq!(store.load().expect("and read back").session_folds, folds);

    let mut old = serde_json::to_value(Settings::default()).unwrap();
    old.as_object_mut().unwrap().remove("session_folds");
    std::fs::write(&path, old.to_string()).unwrap();
    let restored = store.load().expect("an old file reads");
    assert_eq!(restored.session_folds, SessionFolds::default());
}

/// A section name this build does not know — written by a newer one — is
/// dropped alone: the sections it does know stay collapsed and the rest of
/// the file still reads.
#[test]
fn an_unknown_collapsed_section_is_dropped_alone() {
    use guild_session::settings::SessionSection;

    let dir = temp_dir("session-folds-unknown");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let path = dir.join("settings.json");
    let mut value = serde_json::to_value(Settings {
        column_width: 300.0,
        ..Settings::default()
    })
    .unwrap();
    value.as_object_mut().unwrap().insert(
        "session_folds".into(),
        serde_json::json!({ "collapsed": ["working", "someday"], "settled_open": true }),
    );
    std::fs::write(&path, value.to_string()).unwrap();

    let restored = Store::<Settings>::new(path).load().expect("it still reads");

    assert_eq!(
        restored
            .session_folds
            .collapsed
            .into_iter()
            .collect::<Vec<_>>(),
        vec![SessionSection::Working]
    );
    assert!(restored.session_folds.settled_open);
    assert_eq!(restored.column_width, 300.0);
}
