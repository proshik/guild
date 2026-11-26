//! Which icon a name gets. Out here because the rule is the public door:
//! the tree and the tab strip both ask it, and both must get the same
//! answer for the same name.

use guild_desktop::icons::{self, Palette};

/// The rule is the only hand-written half: whole names
/// beat extensions, the longest suffix wins, and an unmatched name draws
/// **A directory's icon says what it is for, not what it is called** —
/// M8.12. The Material Icon Theme picked by name, so a `configuration`
/// package wore a gear and `web` a globe; what deserves a colour is what
/// the build system knows, and the shape of the path stands
/// in for a build system here.
#[test]
fn a_directory_is_drawn_by_what_it_is_for() {
    use std::path::Path;
    let dir = |path: &str, expanded: bool| icons::for_dir(Path::new(path), expanded, Palette::Dark);

    assert_eq!(dir("src", false), "folder-src");
    assert_eq!(dir("src", true), "folder-src-open");
    assert_eq!(dir("app/src/main/kotlin", false), "folder-src");
    assert_eq!(dir("app/src/main/java", false), "folder-src");
    assert_eq!(dir("app/src/test/kotlin", false), "folder-test");
    assert_eq!(dir("tests", false), "folder-test");
    assert_eq!(dir("app/src/main/resources", false), "folder-resource");

    // Everything else is a plain folder, and that is most of a tree:
    // these are the names the old table dressed up.
    for plain in [
        "app/src/main/kotlin/com/example/inaction/configuration",
        "app/src/main/kotlin/com/example/inaction/controller/web",
        "crates",
        "build",
        ".github",
        "node_modules",
        "nowhere-in-the-table",
    ] {
        assert_eq!(dir(plain, false), "folder", "{plain}");
    }
    assert_eq!(dir("nowhere-in-the-table", true), "folder-open");
    // `main` on its own is not a source root: the code is one level in.
    assert_eq!(dir("app/src/main", false), "folder");
}

/// The light palette takes the light variant where one exists and the
/// ordinary icon where none does — never a missing file.
///
/// **The examples changed with the set in M8.13** and the reason is
/// worth keeping: the earlier snapshot shipped a `_light` for xml, rust and
/// every folder, and 5.29.0 ships fifty-two in all — the theme now draws
/// one icon that reads on both canvases and keeps the variant only where
/// the artwork is nearly white. So none of the four stems `for_dir`
/// answers has one, and that is the assertion rather than an omission.
#[test]
fn light_variants_are_used_only_where_they_exist() {
    assert_eq!(icons::for_file("Cargo.toml", Palette::Light), "toml_light");
    assert_eq!(icons::for_file("deno.json", Palette::Light), "deno_light");
    assert_eq!(
        icons::for_file("x.py", Palette::Light),
        "python",
        "no light variant in the set, so the icon itself"
    );
    assert_eq!(icons::for_file("lib.rs", Palette::Light), "rust");
    for (path, expanded, stem) in [
        ("src", false, "folder-src"),
        ("src", true, "folder-src-open"),
        ("anything", false, "folder"),
    ] {
        assert_eq!(
            icons::for_dir(std::path::Path::new(path), expanded, Palette::Light),
            stem,
            "no folder this rule answers has a light variant any more"
        );
    }
}

/// Every stem the rule can answer is a file in the set: the failure mode
/// of a missing icon is a row that silently draws nothing, which no
/// screenshot review reliably catches.
#[test]
fn every_stem_the_rule_can_return_exists_in_the_set() {
    let missing: Vec<String> = icons::stems()
        .filter(|stem| !icons::has(&format!("{stem}.svg")))
        .collect();
    assert!(missing.is_empty(), "stems with no file: {missing:?}");
}

/// The set travels with its licence, and the licence is the one the
/// artwork is under.
#[test]
fn the_licence_is_in_the_set() {
    let licence = icons::licence();
    assert!(licence.contains("MIT"), "not MIT: {licence}");
    assert!(
        licence.contains("Material Extensions"),
        "not the theme's: {licence}"
    );
}

/// **Every icon file of ours is reachable, and every name in the table
/// is a file.**
///
/// M8.4 added three toolbar icons, wired the buttons and drew three empty
/// squares: `AssetSource::load` answers `None` for a name it does not
/// know, and GPUI then draws nothing at all — no error, no line in the
/// log, no gap in the layout. The panel simply looked as though the icons
/// had no colour. This is the test that would have said so.
#[test]
fn every_icon_of_ours_is_wired_up() {
    use std::collections::HashSet;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/ui");
    let on_disk: HashSet<String> = std::fs::read_dir(&dir)
        .expect("the icon directory is beside the crate")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg"))
        .collect();
    let in_table: HashSet<String> = icons::UI
        .iter()
        .map(|(name, _)| {
            name.strip_prefix("ui/")
                .expect("every name is under ui/")
                .to_owned()
        })
        .collect();

    assert_eq!(
        on_disk.difference(&in_table).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "an icon file nobody can load draws nothing, silently"
    );
    assert_eq!(
        in_table.difference(&on_disk).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "a name in the table with no file behind it would not compile — \
         but if the check ever moves off include_bytes, it would"
    );
    assert!(
        icons::UI.iter().all(|(_, bytes)| !bytes.is_empty()),
        "an empty file loads and draws nothing"
    );
}

#[test]
fn workspace_icon_names_are_stable_and_unknown_names_fall_back() {
    assert_eq!(icons::WORKSPACE_NAMES.len(), 38);
    assert!(icons::is_workspace_name("coffee"));
    assert!(icons::is_workspace_name("lightbulb-sparkle"));
    assert!(!icons::is_workspace_name("coffee-cup"));

    assert_eq!(
        icons::workspace_path("folder"),
        Some("ui/workspace-folder.svg")
    );
    assert_eq!(
        icons::workspace_path("history"),
        Some("ui/workspace-history.svg")
    );
    assert_eq!(
        icons::workspace_path("coffee"),
        Some("ui/workspace-coffee.svg")
    );
    assert_eq!(icons::workspace_path("unknown"), None);

    assert!(
        icons::WORKSPACE_NAMES
            .iter()
            .all(|name| icons::workspace_path(name).is_some()),
        "every offered workspace icon has a vendored asset"
    );
}

/// **Every icon the component asks us for is one we serve**, and every
/// file in that directory is one it asks for.
///
/// `gpui-component` draws its own controls with `IconName`, whose path is
/// `icons/<name>.svg`, and it ships no assets — the application serves
/// that namespace or the button is blank. Guild never did, so the
/// editor's find panel had five invisible buttons for as long as the
/// editor has existed; task 0 of M8.7 found them by pressing `cmd-f`.
///
/// The names are the component's, not ours, so a rename upstream is a
/// button that silently goes blank again. This test cannot see that — it
/// checks the table against the directory — which is why the names are
/// written down in the module's own documentation as *what the component
/// asks for* rather than as icons of ours.
#[test]
fn every_icon_the_component_asks_for_is_served() {
    use std::collections::HashSet;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/component");
    let on_disk: HashSet<String> = std::fs::read_dir(&dir)
        .expect("the component's icon directory is beside the crate")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg"))
        .collect();
    let in_table: HashSet<String> = icons::COMPONENT
        .iter()
        .map(|(name, _)| {
            name.strip_prefix("icons/")
                .expect("every name the component asks for is under icons/")
                .to_owned()
        })
        .collect();

    assert_eq!(
        on_disk, in_table,
        "a file nobody serves draws nothing, and a name with no file behind it is a blank button"
    );
    assert!(
        icons::COMPONENT.iter().all(|(_, bytes)| !bytes.is_empty()),
        "an empty file loads and draws nothing"
    );
}

/// **The component's names are answered before the file-type set.**
///
/// Both live under `icons/`. The Material Icon Theme is 422 names from
/// somebody else's repository, and the day one of them is `close.svg` the
/// component's close button would be handed a file icon. The order is the
/// guard, and this is the test that pins it.
#[test]
fn review_selectors_have_a_visible_dropdown_caret() {
    use gpui::AssetSource;
    use gpui_component::{IconName, IconNamed};

    let path = IconName::ChevronDown.path();
    assert!(
        icons::Icons.load(&path).unwrap().is_some(),
        "the selector's dropdown affordance must be served by the asset source"
    );
}

#[test]
fn the_component_namespace_wins_over_the_file_icons() {
    use gpui::AssetSource;

    let source = icons::Icons;
    for (name, bytes) in icons::COMPONENT {
        let served = source
            .load(name)
            .expect("the asset source does not fail")
            .unwrap_or_else(|| panic!("{name} is served"));
        assert_eq!(
            served.as_ref(),
            *bytes,
            "{name} must come from the component's table, not from the file-type set"
        );
    }
}

/// **Every language Guild knows has an icon of its own.**
///
/// This is the guard for M8.13, and the reason the set stopped coming
/// from a hand-picked snapshot. It was 92 stems of the theme's eleven hundred,
/// and eleven of Guild's thirty-seven languages were not among them:
/// kotlin, java, ruby, swift, c#, elixir, php, lua, nix, hcl and just
/// all drew the blank default page. Nothing said so — a language is
/// added to `highlight`, its grammar and its server are tested, and
/// nobody thinks to look at the tree.
///
/// Each language is asked by a **file name it would really have**, so
/// the rule is exercised end to end rather than the table read back —
/// its whole name where it has one, because that is what a `justfile`
/// or a `Dockerfile` is called, and its first suffix otherwise. The
/// theme knows `just` by that name and by no extension, and neither does
/// its host's language registry, so a `build.just` draws the default page.
#[test]
fn every_language_guild_knows_draws_something_of_its_own() {
    use guild_desktop::highlight::Language;

    let mut plain = vec![];
    for language in Language::ALL {
        let Some(name) = language
            .filenames
            .first()
            .map(|name| (*name).to_owned())
            .or_else(|| {
                language
                    .suffixes
                    .first()
                    .map(|suffix| format!("guild-fixture.{suffix}"))
            })
        else {
            continue;
        };
        if icons::for_file(&name, Palette::Dark) == "file" {
            plain.push(format!("{} ({name})", language.name));
        }
    }
    assert!(
        plain.is_empty(),
        "these languages draw the default page: {plain:?}"
    );
}

/// **IntelliJ's declaration icons are all wired, and every one the rule
/// can name is there** — M8.13.
///
/// A third directory with a third licence, walked on its own for the
/// reason the other two are: a file nobody loads draws nothing, and a
/// stem the rule names with no file behind it draws nothing too. The
/// LICENSE and PROVENANCE are asserted because they are the two files a
/// person adding an icon by hand leaves out.
#[test]
fn every_declaration_icon_is_wired_and_licensed() {
    use guild_desktop::declaration::Kind;
    use std::collections::HashSet;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/nodes");
    let on_disk: HashSet<String> = std::fs::read_dir(&dir)
        .expect("the node icons are beside the crate")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg"))
        .collect();
    let in_table: HashSet<String> = icons::NODES
        .iter()
        .map(|(name, _)| {
            name.strip_prefix("icons/nodes/")
                .expect("every name is under icons/nodes/")
                .to_owned()
        })
        .collect();
    assert_eq!(on_disk, in_table, "the directory and the table disagree");
    assert!(
        dir.join("LICENSE").is_file(),
        "Apache-2.0 travels with the files"
    );
    assert!(dir.join("PROVENANCE").is_file());

    // Every kind in both palettes resolves to a file the table serves.
    for kind in [
        Kind::Class,
        Kind::Abstract,
        Kind::Interface,
        Kind::Enum,
        Kind::Annotation,
        Kind::Record,
    ] {
        for palette in [Palette::Dark, Palette::Light] {
            let stem = icons::for_declaration(kind, palette);
            let path = format!("icons/{stem}.svg");
            assert!(
                icons::NODES.iter().any(|(name, _)| *name == path),
                "{kind:?} in {palette:?} names {path}, which nothing serves"
            );
        }
    }
    // **The dark palettes take the `_dark` file** — JetBrains names the
    // variant for the canvas it is drawn on, the Material set for the one
    // it is not.
    assert_eq!(
        icons::for_declaration(Kind::Class, Palette::Dark),
        "nodes/class_dark"
    );
    assert_eq!(
        icons::for_declaration(Kind::Class, Palette::Light),
        "nodes/class"
    );
    // External Libraries' two, M8.14, by the same rule.
    for jdk in [false, true] {
        for palette in [Palette::Dark, Palette::Light] {
            let path = format!("icons/{}.svg", icons::for_library(jdk, palette));
            assert!(
                icons::NODES.iter().any(|(name, _)| *name == path),
                "{path}, which nothing serves"
            );
        }
    }
}
