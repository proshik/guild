//! What Find Usages can be asked about — M8.15.
//!
//! The grammar answers before any server does: task 0 measured
//! rust-analyzer finding `fn` as a usage of itself, jdtls answering
//! `public` with the method it modifies, and gopls refusing `func`. Out
//! here because `target_at` is the door the action goes through.

use guild_desktop::usages::target_at;
/// The target at the first `^` in `marked`, with the mark taken out.
fn at(language: &str, marked: &str) -> Option<String> {
    let offset = marked.find('^').expect("a caret");
    let text = marked.replacen('^', "", 1);
    target_at(language, &text, offset).map(|target| {
        assert_eq!(&text[target.range.clone()], target.name);
        target.name
    })
}

#[test]
fn a_name_is_a_target_wherever_the_caret_is_in_it() {
    assert_eq!(at("rust", "use a::^Shape;").as_deref(), Some("Shape"));
    assert_eq!(at("rust", "use a::Sh^ape;").as_deref(), Some("Shape"));
    assert_eq!(at("rust", "use a::Shape^;").as_deref(), Some("Shape"));
}

#[test]
fn a_keyword_is_not_a_target() {
    assert_eq!(at("rust", "f^n main() {}"), None);
    assert_eq!(at("go", "package main\nfu^nc main() {}"), None);
    assert_eq!(at("java", "class A { pub^lic void run() {} }"), None);
    assert_eq!(at("kotlin", "cla^ss A"), None);
}

#[test]
fn a_space_a_comment_and_a_string_are_not_targets() {
    assert_eq!(at("rust", "fn main() {  ^  }"), None);
    assert_eq!(at("rust", "// a Sh^ape here\nfn main() {}"), None);
    assert_eq!(at("rust", "const X: &str = \"Sh^ape\";"), None);
    assert_eq!(
        at("java", "class A { /** a Sh^ape */ void run() {} }"),
        None
    );
}

#[test]
fn a_literal_and_a_primitive_are_not_targets() {
    assert_eq!(at("rust", "fn main() { f(4^2); }"), None);
    assert_eq!(at("rust", "const X: u^32 = 1;"), None);
}

#[test]
fn the_six_languages_with_a_server_name_their_names() {
    assert_eq!(
        at("rust", "fn main() { s.ar^ea(); }").as_deref(),
        Some("area")
    );
    assert_eq!(at("rust", "fn f(s: Sh^ape) {}").as_deref(), Some("Shape"));
    assert_eq!(
        at("go", "package main\ntype Sh^ape struct{}").as_deref(),
        Some("Shape")
    );
    assert_eq!(
        at("go", "package main\nfunc main() { fmt.Pri^ntln(1) }").as_deref(),
        Some("Println")
    );
    assert_eq!(
        at("java", "class A extends Ba^se {}").as_deref(),
        Some("Base")
    );
    assert_eq!(
        at("java", "class A { int area() { return si^de; } }").as_deref(),
        Some("side")
    );
    assert_eq!(
        at("kotlin", "import feign.Con^tract").as_deref(),
        Some("Contract")
    );
    assert_eq!(
        at("kotlin", "fun use(): Con^tract? = null").as_deref(),
        Some("Contract")
    );
    assert_eq!(at("kotlin", "@Be^an fun use() {}").as_deref(), Some("Bean"));
    assert_eq!(
        at("ruby", "class Sh^ape < Base\nend").as_deref(),
        Some("Shape")
    );
    assert_eq!(
        at("ruby", "def area\n  @si^de\nend").as_deref(),
        Some("@side")
    );
    assert_eq!(
        at("elixir", "defmodule Demo.Sh^ape do\nend").as_deref(),
        Some("Demo.Shape")
    );
}

#[test]
fn a_language_without_a_grammar_has_no_targets() {
    assert_eq!(target_at("no-such-grammar-anywhere", "Shape", 1), None);
}

#[test]
fn a_caret_past_the_end_is_on_the_last_name() {
    assert_eq!(target_at("rust", "fn main() { area }", 99), None);
    assert_eq!(
        target_at("rust", "use a::Shape", 99)
            .map(|target| target.name)
            .as_deref(),
        Some("Shape")
    );
}

mod answers {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use guild_desktop::search_tree;
    use guild_desktop::usages::{self, Folds, Group, Row};
    use lsp_types::{LocationLink, Position, Range, Uri};

    fn link(uri: &str, line: u32, from: u32, to: u32) -> LocationLink {
        let range = Range::new(Position::new(line, from), Position::new(line, to));
        LocationLink {
            origin_selection_range: None,
            target_uri: uri.parse::<Uri>().expect("a uri"),
            target_range: range,
            target_selection_range: range,
        }
    }

    const ROOT: &str = "/work/in-action";

    fn files() -> HashMap<String, String> {
        HashMap::from([
            (
                format!("file://{ROOT}/app/src/Config.kt"),
                "package a\nimport feign.Contract\nfun use(): Contract? {\n    return Contract.Default()\n}\n"
                    .to_owned(),
            ),
            (
                format!("file://{ROOT}/app/src/Other.kt"),
                "val x: Contract? = null // Контракт\n".to_owned(),
            ),
        ])
    }

    /// The answer task 0 recorded for `Contract` in `in-action`, with a
    /// jar beside it and the lines out of order.
    fn answer() -> Vec<LocationLink> {
        vec![
            link(&format!("file://{ROOT}/app/src/Config.kt"), 3, 11, 19),
            link(&format!("file://{ROOT}/app/src/Config.kt"), 1, 13, 21),
            link(&format!("file://{ROOT}/app/src/Other.kt"), 0, 7, 15),
            link(
                "jar:///m2/feign-core-13.6-sources.jar!/feign/Contract.java",
                37,
                17,
                25,
            ),
            link(&format!("file://{ROOT}/app/src/Config.kt"), 2, 11, 19),
        ]
    }

    #[test]
    fn a_project_file_is_filed_by_its_path_and_a_jar_under_its_name() {
        let texts = files();
        let found = usages::found(answer(), Path::new(ROOT), |uri| texts.get(uri).cloned());
        assert_eq!(found.project, 4);
        let (project, _) = found.of(Group::Project);
        let places: Vec<(PathBuf, u32)> = project
            .iter()
            .map(|hit| (hit.path.clone(), hit.line))
            .collect();
        assert_eq!(
            places,
            vec![
                (PathBuf::from("app/src/Config.kt"), 2),
                (PathBuf::from("app/src/Config.kt"), 3),
                (PathBuf::from("app/src/Config.kt"), 4),
                (PathBuf::from("app/src/Other.kt"), 1),
            ],
            "sorted by file and line, whatever order the server sent"
        );
        let (libraries, first) = found.of(Group::Libraries);
        assert_eq!(first, 4);
        assert_eq!(
            libraries[0].path,
            PathBuf::from("feign-core-13.6-sources.jar/feign/Contract.java")
        );
        assert_eq!(
            found.links[4].target_uri.as_str(),
            "jar:///m2/feign-core-13.6-sources.jar!/feign/Contract.java"
        );
    }

    #[test]
    fn file_selection_tracks_file_groups_and_folding_without_selecting_match_rows() {
        let found = usages::found(answer(), Path::new(ROOT), |_| None);
        let mut folds = usages::Folds::default();
        let rows = usages::rows(&found, &folds);
        let order: Vec<_> = rows.iter().filter_map(usages::file_of).collect();
        assert_eq!(order.len(), 3);
        let mut selected = guild_desktop::file_selection::Selection::default();
        selected.click(&order[0], &order, false, false);
        selected.click(&order[2], &order, false, true);
        assert_eq!(selected.selected.len(), 3);
        assert_eq!(selected.selected[2].0, Group::Libraries);
        folds.groups.insert(Group::Libraries);
        let visible: Vec<_> = usages::rows(&found, &folds)
            .iter()
            .filter_map(usages::file_of)
            .collect();
        selected.retain(&visible);
        assert_eq!(selected.selected.len(), 2);
        assert!(selected.action_target().is_none());
        assert!(
            rows.iter()
                .filter(|r| usages::usage_of(r).is_some())
                .all(|r| usages::file_of(r).is_none())
        );
        assert_eq!(
            found.links[4].target_uri.as_str(),
            "jar:///m2/feign-core-13.6-sources.jar!/feign/Contract.java"
        );
    }

    #[test]
    fn a_usage_draws_its_line_with_the_name_placed_in_it() {
        let texts = files();
        let found = usages::found(answer(), Path::new(ROOT), |uri| texts.get(uri).cloned());
        let hit = &found.hits[2];
        assert_eq!(hit.text, "    return Contract.Default()");
        assert_eq!(&hit.text[hit.ranges[0].clone()], "Contract");
        assert_eq!(hit.column, 11);
    }

    #[test]
    fn a_file_that_cannot_be_read_keeps_its_usages() {
        let found = usages::found(answer(), Path::new(ROOT), |_| None);
        assert_eq!(found.len(), 5);
        assert!(
            found
                .hits
                .iter()
                .all(|hit| hit.text.is_empty() && hit.ranges.is_empty())
        );
    }

    #[test]
    fn a_file_is_read_once_however_many_usages_it_holds() {
        let texts = files();
        let mut asked = Vec::new();
        usages::found(answer(), Path::new(ROOT), |uri| {
            asked.push(uri.to_owned());
            texts.get(uri).cloned()
        });
        asked.sort();
        let before = asked.len();
        asked.dedup();
        assert_eq!(before, asked.len());
        assert_eq!(asked.len(), 3);
    }

    #[test]
    fn a_place_named_twice_is_one_usage() {
        let mut twice = answer();
        twice.push(twice[0].clone());
        let found = usages::found(twice, Path::new(ROOT), |_| None);
        assert_eq!(found.len(), 5);
    }

    /// UTF-16 units in, bytes out — and a Cyrillic comment after the
    /// name must not move it.
    #[test]
    fn a_range_is_counted_in_utf16_units() {
        let text = "/* ё */ val x: Contract".to_owned();
        let found = usages::found(
            vec![link("file:///work/in-action/a.kt", 0, 15, 23)],
            Path::new(ROOT),
            |_| Some(text.clone()),
        );
        let hit = &found.hits[0];
        assert_eq!(&hit.text[hit.ranges[0].clone()], "Contract");
    }

    #[test]
    fn the_tree_is_the_target_then_each_group_then_its_files() {
        let texts = files();
        let found = usages::found(answer(), Path::new(ROOT), |uri| texts.get(uri).cloned());
        let rows = usages::rows(&found, &Folds::default());
        assert_eq!(rows[0], Row::Target);
        assert_eq!(
            rows[1],
            Row::Group {
                group: Group::Project,
                count: 4
            }
        );
        let libraries = rows
            .iter()
            .position(|row| {
                *row == Row::Group {
                    group: Group::Libraries,
                    count: 1,
                }
            })
            .expect("the libraries' group");
        let usages: Vec<usize> = rows.iter().filter_map(usages::usage_of).collect();
        assert_eq!(
            usages,
            vec![0, 1, 2, 3, 4],
            "a usage indexes the whole list"
        );
        assert_eq!(usages::usage_of(rows.last().unwrap()), Some(4));
        assert!(libraries > 1);
    }

    #[test]
    fn a_group_with_nothing_in_it_is_not_drawn() {
        let texts = files();
        let mut answer = answer();
        answer.remove(3);
        let found = usages::found(answer, Path::new(ROOT), |uri| texts.get(uri).cloned());
        let rows = usages::rows(&found, &Folds::default());
        assert!(!rows.iter().any(|row| matches!(
            row,
            Row::Group {
                group: Group::Libraries,
                ..
            }
        )));
    }

    #[test]
    fn a_fold_shuts_a_group_or_a_file_and_folding_all_leaves_the_heads() {
        let texts = files();
        let found = usages::found(answer(), Path::new(ROOT), |uri| texts.get(uri).cloned());
        let mut folds = Folds::default();
        let rows = usages::rows(&found, &folds);
        let file = rows
            .iter()
            .find(|row| matches!(row, Row::Item { row: search_tree::Row::File { name, .. }, .. } if name == "Config.kt"))
            .cloned()
            .expect("Config.kt");
        assert!(folds.toggle(&file));
        assert!(folds.is_folded(&file));
        let shut = usages::rows(&found, &folds);
        assert_eq!(shut.len(), rows.len() - 3, "its three usages are gone");

        assert!(!folds.toggle(&Row::Target), "the target does not fold");
        folds.fold_all(&found);
        let heads = usages::rows(&found, &folds);
        assert_eq!(heads.len(), 3, "the target and the two groups");
    }
}

/// Balloons and gutter clicks follow the actual editor geometry after upgrades.
#[gpui::test]
fn the_balloon_knows_where_the_editor_puts_its_text(cx: &mut gpui::TestAppContext) {
    use gpui::{AppContext, px};
    use guild_desktop::{
        file_tab::{FileTab, Look},
        i18n::Ui,
        theme::Theme,
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("test.rs"),
        "let имя = 42;\nlet second = 2;\n",
    )
    .unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let view = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("test.rs"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        file = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(400.)));
    let file = file.unwrap();
    let (letter, first) = cx.update(|window, cx| {
        let file = file.read(cx);
        (
            file.bounds_of(4..6, window, cx).unwrap(),
            file.bounds_of(0..1, window, cx).unwrap(),
        )
    });
    cx.update(|_, cx| {
        assert!(!file.read(cx).in_gutter(first.center()));
        assert!(
            file.read(cx)
                .in_gutter(first.origin - gpui::point(px(1.), px(0.)))
        );
    });
    cx.simulate_click(letter.center(), gpui::Modifiers::default());
    cx.update(|_, cx| {
        let cursor = file.read(cx).editor().unwrap().read(cx).cursor_position();
        assert_eq!(cursor.line, 0);
        assert!(
            (4..=5).contains(&cursor.character),
            "click must land on the Cyrillic name: {cursor:?}"
        );
    });
}
