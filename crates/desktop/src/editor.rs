//! The boundary with `gpui-component`, whose code editor is what a file
//! tab types into. Everything the component needs from us, and everything
//! we take from it, passes through here — one place to read when it is
//! upgraded, and one place to replace if it is ever replaced.
//!
//! Task 0 of M8.1 checked it on six criteria and wrote the terms down:
//!
//! - **Its `Root` is the window's root view, and ours is its child.**
//!   `Root::read` expects it, and it writes the window's rem size from its
//!   theme's `font_size` on every render — so that field is kept equal to
//!   our `ui_font_size`, and our own render sets the rem after it.
//! - **It binds `tab` in that root**, and a debug build binds the
//!   inspector's `cmd-alt-i` and `ctrl-shift-i`. A terminal of ours never
//!   saw Tab until `NoAction` unbound those in the terminal's own context;
//!   since M5.11 [`unbind_in_our_contexts`] does it once, in `guild` — the
//!   window's own context, which every surface of ours sits under — so a
//!   panel added tomorrow cannot forget it, and Tab in the Search panel no
//!   longer jumps into a buffer.
//! - **Its grammar feature stays off** — twenty-eight grammars and 31 MB
//!   for the six a person here reads — and the six are registered by hand
//!   through the registry its own tests use. `tree-sitter` is pinned to
//!   the 0.26 line it depends on.
//! - **The colours go in through its theme global** and a highlight theme
//!   built from the seven roles of section 2.4; the face, the size and the
//!   leading go on the `Editor` element, because the editor ignores its
//!   theme's mono font.

mod selection;
pub(crate) use selection::bindings as selection_bindings;
pub use selection::{ExtendSelection, SelectionHistory, ShrinkSelection, SmartSelection};
mod reveal;
pub use reveal::EditorReveal;
mod occurrences;
pub use occurrences::Occurrences;
pub mod comments;
pub mod editing;
pub(crate) use editing::bindings as editing_bindings;

use std::sync::Arc;

use gpui::{App, KeyBinding};
use gpui_component::Theme as ComponentTheme;
use gpui_component::highlighter::{HighlightTheme, LanguageConfig, LanguageRegistry};

use crate::highlight;
use crate::theme::Theme;

/// Where the component's four root strokes are undone. The component's
/// root binds `tab` and `shift-tab`, and a debug build the inspector's two
/// strokes; a context here gets those four back.
///
/// **One context, the window's own, and not one per surface.** Until M5.11
/// this listed individual surfaces rather than all the
/// contexts that read keys — and Tab in the Search panel, the tree, the
/// Log, the Output's list or the font list reached the component's
/// *focus the next tab stop*, whose only stops are editors and the commit
/// box: the next letters went into a file. Every surface of ours sits
/// under `guild`, and a `NoAction` there is deeper than the component's
/// `Root` and shallower than `Input`, so a file still indents on Tab while
/// every panel keeps it — this one and the next one somebody adds.
/// M5.11's decision 13.
const OUR_CONTEXTS: &[&str] = &["guild"];

/// The four strokes the component takes at its root.
const TAKEN: &[&str] = &["tab", "shift-tab", "cmd-alt-i", "ctrl-shift-i"];

/// The component's *select one character to the right*, by the name GPUI
/// registers it under. M5.11's decision 6 selects a search hit's match
/// with it.
///
/// **A name and not a type, because the type cannot be reached.**
/// `SelectRight` is declared by `actions!(ui, …)` in the component's
/// `src/actions.rs`, inside `pub(crate) mod actions`, and its input state
/// imports it privately, so no public path carries it out; every
/// `select_*` on `InputState` is `pub(super)` too. What is public is GPUI's
/// registry — `App::build_action` by this name — and the price is a string
/// an upgrade can break without the compiler noticing. The test below is
/// what notices: it finds the name among the registered actions.
pub const SELECT_RIGHT: &str = "ui::SelectRight";

/// Everything the component needs before the first window: its own
/// `init`, our grammars in its registry, and its bindings undone where
/// ours read the keys.
///
/// **Runs before `actions::install`, and what it leaves bound is kept.**
/// The keymap holds only the component's bindings and our unbinds at the
/// end of it, and they are taken into [`crate::actions::Inherited`] so that
/// every `actions::rebind` — which clears the keymap — puts them back under
/// and over Guild's own in the order decision 12 of M5.11 gives.
pub fn init(cx: &mut App) {
    gpui_component::init(cx);
    register_grammars();
    unbind_in_our_contexts(cx);
    let inherited: Vec<KeyBinding> = cx.key_bindings().borrow().bindings().cloned().collect();
    cx.set_global(crate::actions::Inherited(inherited));
}

/// `NoAction` in a deeper context beats a binding in a shallower one:
/// that is what the type is for, and it is how a terminal keeps Tab.
fn unbind_in_our_contexts(cx: &mut App) {
    cx.bind_keys(unbinds());
}

/// The component's root strokes undone in each of [`OUR_CONTEXTS`] — pure,
/// so that a test can lay them in a keymap without an `App`.
fn unbinds() -> Vec<KeyBinding> {
    OUR_CONTEXTS
        .iter()
        .flat_map(|context| {
            TAKEN
                .iter()
                .map(move |stroke| KeyBinding::new(stroke, gpui::NoAction {}, Some(context)))
        })
        .collect()
}

/// The grammar name to hand the editor: the one asked for when it is
/// really registered, and plain text when it is not.
///
/// **The registry never answers `None`.** With the component's grammar
/// feature off its own language list is the single variant `Json`, whose
/// `from_str` answers unconditionally, and the lookup falls back through
/// it — so an unregistered name resolves to the *JSON grammar*, silently.
/// Comparing the name that comes back is the only way to ask the
/// question, and without asking it a language whose grammar has not been
/// registered is not plain: it is parsed as JSON, and the colours are
/// confident nonsense.
pub fn grammar_or_plain(grammar: &'static str) -> &'static str {
    if registered(grammar) {
        grammar
    } else {
        crate::highlight::PLAIN
    }
}

/// Whether the registry really holds this grammar, by the name it answers
/// with rather than by the lookup succeeding.
pub fn registered(grammar: &str) -> bool {
    LanguageRegistry::singleton()
        .language(grammar)
        .is_some_and(|config| config.name.as_ref() == grammar)
}

/// Every grammar into the component's registry, its captures renamed on
/// the way in.
fn register_grammars() {
    let registry = LanguageRegistry::singleton();
    for grammar in crate::grammars::all() {
        registry.register(
            grammar.name,
            &LanguageConfig::new(
                grammar.name,
                (grammar.language)(),
                grammar.injects.iter().map(|name| (*name).into()).collect(),
                &crate::highlight::query_for(grammar.name, &grammar.highlights()),
                grammar.injections,
                "",
            ),
        );
    }
}

/// Give the component our palette and our interface size.
///
/// Called at start and on every change of either. The component's own
/// theme has many more colours than these; the ones set are the ones an
/// editor, its search line and its completion menu are drawn with, and
/// the rest keep its defaults until one is seen.
pub fn apply_theme(theme: &Theme, ui_font_size: f32, cx: &mut App) {
    let highlight = highlight_theme(theme);
    // Loading a different mode replaces its registered palette. Apply Guild's
    // overrides afterwards, rather than letting that load overwrite them.
    ComponentTheme::update(cx, |component| {
        component.mode = if theme.kind.is_light() {
            gpui_component::ThemeMode::Light
        } else {
            gpui_component::ThemeMode::Dark
        };
    });
    ComponentTheme::update(cx, |component| {
        component.font_size = gpui::px(ui_font_size);
        // Guild owns solid colors for these roles. Set both representations:
        // update treats an equal color as untouched, retaining a stale token
        // or a registered gradient unless the token is explicitly replaced.
        macro_rules! color {
            ($field:ident, $value:expr) => {{
                let value = hsla($value);
                component.colors.$field = value;
                component.tokens.$field = value.into();
            }};
        }
        let hover = u32::from(crate::theme::control_hover(*theme, theme.surface.raised)) >> 8;
        color!(secondary_hover, hover);
        color!(secondary_active, hover);
        color!(background, theme.surface.app);
        color!(foreground, theme.text.primary);
        color!(link, theme.accent.patina);
        color!(table_head, theme.surface.app);
        color!(table_head_foreground, theme.text.primary);
        color!(secondary, theme.surface.raised);
        color!(secondary_foreground, theme.text.primary);
        color!(border, theme.surface.divider);
        color!(muted, theme.surface.panel);
        // Input placeholders use this role even with appearance(false).
        color!(muted_foreground, theme.text.muted);
        color!(accent, theme.surface.raised);
        color!(accent_foreground, theme.text.primary);
        color!(selection, theme.accent.highlight);
        color!(caret, theme.text.primary);
        color!(popover, theme.surface.card);
        color!(popover_foreground, theme.text.primary);
        color!(list, theme.surface.card);
        color!(list_active, theme.surface.raised);
        color!(
            list_hover,
            u32::from(crate::theme::control_hover(*theme, theme.surface.card)) >> 8
        );
        color!(scrollbar, theme.surface.app);
        color!(scrollbar_thumb, theme.text.faint);
        color!(scrollbar_thumb_hover, theme.text.muted);
        color!(input, theme.surface.divider);
        color!(ring, theme.accent.brass);
        // The customizer's sliders: the filled part is the accent, the
        // thumb the ink — Guild's, not the component's mode defaults.
        color!(slider_bar, theme.accent.patina);
        color!(slider_thumb, theme.text.primary);
        match highlight {
            Ok(highlight) => component.highlight_theme = Arc::new(highlight),
            Err(err) => tracing::warn!(%err, "the highlight theme was not accepted"),
        }
    });
}

/// The UI family shared by component overlays and the satellite frame.
/// Editor and terminal text retain their own explicit font choices.
pub fn apply_ui_font(family: Option<&str>, cx: &mut App) {
    ComponentTheme::update(cx, |component| {
        component.font_family = family.unwrap_or(crate::fonts::DEFAULT.ui).to_owned().into();
    });
}

/// The code face, for what the component draws in its monospace face —
/// a Markdown preview's code spans and blocks, which read the theme's
/// `mono_font_family` and were Menlo while the editor and the diff drew
/// the person's face. Called whenever the window resolves that face.
pub fn apply_code_font(family: &str, cx: &mut App) {
    ComponentTheme::update(cx, |component| {
        component.mono_font_family = family.to_owned().into();
    });
}

/// The component's highlight theme, **built from [`highlight::CAPTURES`]**
/// rather than written beside it.
///
/// Built through JSON because that is the shape it deserialises and its
/// style type has private fields with no constructor; `Hsla` reads
/// `#rrggbb`.
fn highlight_theme(theme: &Theme) -> Result<HighlightTheme, serde_json::Error> {
    serde_json::from_value(theme_value(theme))
}

/// The theme as the component's JSON, so that the captures it names can
/// be read back and compared with the table they came from.
fn theme_value(theme: &Theme) -> serde_json::Value {
    let hex = |value: u32| format!("#{value:06x}");
    let mut syntax = serde_json::Map::new();
    for (name, paint) in highlight::CAPTURES {
        let (colour, italic, bold) = theme.of_paint(*paint);
        let mut style = serde_json::Map::new();
        style.insert("color".into(), hex(colour).into());
        if italic {
            style.insert("font_style".into(), "italic".into());
        }
        if bold {
            style.insert("font_weight".into(), 700.into());
        }
        syntax.insert((*name).to_owned(), serde_json::Value::Object(style));
    }
    serde_json::json!({
        "name": theme.kind.name(),
        "appearance": if theme.kind.is_light() { "light" } else { "dark" },
        "style": {
            // The four a diagnostic is drawn in — M8.5. They flatten
            // into this same object, and without them the component's
            // wavy underlines take its own red rather than ours.
            "error": hex(theme.accent.rust),
            "warning": hex(theme.accent.brass),
            "info": hex(theme.accent.patina),
            "hint": hex(theme.text.faint),
            "editor.background": hex(theme.surface.app),
            "editor.foreground": hex(theme.text.primary),
            "editor.line_number": hex(theme.text.gutter),
            "editor.active_line_number": hex(theme.text.secondary),
            "syntax": serde_json::Value::Object(syntax),
        }
    })
}

/// The capture names the component's theme actually carries, sorted.
///
/// Read back out of the built JSON rather than out of the table, so that
/// a test comparing the two is comparing what the component is handed
/// with what the window believes it painted.
pub fn themed_captures() -> Vec<String> {
    let value = theme_value(&Theme::default());
    let mut names: Vec<String> = value["style"]["syntax"]
        .as_object()
        .expect("the syntax map")
        .keys()
        .cloned()
        .collect();
    names.sort();
    names
}

/// A design-system colour as the component wants it.
fn hsla(value: u32) -> gpui::Hsla {
    gpui::rgb(value).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry answers *something* for every name, so the question
    /// "is this grammar registered" can only be asked by comparing the
    /// name that comes back. Proven in task 0: asking for `kotlin`
    /// returns the JSON configuration.
    #[test]
    fn an_unregistered_grammar_is_plain_text_and_not_json() {
        register_grammars();
        assert!(registered("rust"));
        assert!(registered(crate::highlight::PLAIN));
        // A name no grammar will ever take, so that registering one
        // cannot quietly turn this test into a tautology — which is what
        // happened to its first version when kotlin arrived.
        const NOBODY: &str = "a-language-nobody-registers";
        assert!(
            !registered(NOBODY),
            "the JSON fallback was taken for a registration"
        );
        assert_eq!(grammar_or_plain("rust"), "rust");
        assert_eq!(grammar_or_plain(NOBODY), crate::highlight::PLAIN);
    }

    /// **Every run the editor lays a line out with is inside that line.**
    ///
    /// The crash of 2026-09-18 was `slice_error_fail` inside GPUI's
    /// `layout_line`, which is a byte index either off a character
    /// boundary or past the end of what is being laid out. The component
    /// asks its highlighter for **one line and its newline** at a time
    /// (`TextElement::highlight_lines`) and turns every style it gets back
    /// into a run of `range.len()` bytes, so this walk is the window's own.
    ///
    /// The text is Russian on purpose: a character of two bytes is what
    /// makes the difference between a boundary and a panic visible.
    #[test]
    fn every_run_of_a_line_is_inside_that_line() {
        use gpui_component::highlighter::SyntaxHighlighter;
        use gpui_component::input::Rope;

        register_grammars();
        let theme = highlight_theme(&Theme::dark()).expect("the theme");

        for (grammar, text) in [
            (
                "rust",
                "// Комментарий
fn main() { let s = \"строка\"; }\n",
            ),
            (
                "go",
                "package auth\n\n// Комментарий\nfunc Make() string { return \"строка\" }\n",
            ),
            (
                "kotlin",
                "package com.example\n\n// Комментарий\nfun main() { val s = \"строка\" }\n",
            ),
            ("markdown", "# Заголовок\n\nтекст **жирный**\n"),
        ] {
            let rope = Rope::from(text);
            let mut highlighter = SyntaxHighlighter::new(grammar);
            highlighter.update(None, &rope, None);

            let mut offset = 0usize;
            for line in text.split_inclusive('\n') {
                let line = line.trim_end_matches('\n');
                let range = offset..offset + line.len() + 1;
                for (at, _) in highlighter.styles(&range, &theme) {
                    assert!(
                        at.end <= text.len(),
                        "{grammar}: a run ends at {} past the text's {}",
                        at.end,
                        text.len()
                    );
                    assert!(
                        text.is_char_boundary(at.start) && text.is_char_boundary(at.end),
                        "{grammar}: {at:?} is not on a character boundary"
                    );
                }
                offset = range.end;
            }
        }
    }

    /// **The coverage guard.** Every capture a registered query can name
    /// lands on the table or on the list of names the window leaves grey
    /// deliberately. A name on neither is a colour nobody decided about —
    /// which is how `@escape` stayed grey from M8.1 until it was counted.
    ///
    /// Compiling the query is half the test: the rename rewrites the text
    /// the parser is given, and a rewrite that does not compile would
    /// otherwise be found by a person opening a file.
    #[test]
    fn every_capture_a_query_names_is_painted_or_deliberately_not() {
        for grammar in crate::grammars::all() {
            let language = (grammar.language)();
            // **The text that is actually registered**, since M8.10: the
            // grammar's own paints and whatever `ADDED_PATTERNS` adds. The
            // first version compiled `renamed_captures` instead, which is
            // not what either engine reads — a pattern of ours that did
            // not compile would have been found by a person opening a
            // file.
            let query = crate::highlight::query_for(grammar.name, &grammar.highlights());
            let compiled = tree_sitter::Query::new(&language, &query)
                .unwrap_or_else(|err| panic!("{}: the renamed query: {err}", grammar.name));
            for name in compiled.capture_names() {
                assert!(
                    crate::highlight::paint_of_capture(name).is_some()
                        || crate::highlight::unpainted(name),
                    "{}: @{name} is painted by nothing and is not on the deliberate list",
                    grammar.name
                );
            }
        }
    }

    /// **The guard against the mine.** Every grammar a language row names
    /// is really in the registry, checked by the name that comes back —
    /// the lookup itself never fails, so a missing registration would
    /// otherwise show up as a file parsed by the JSON grammar and
    /// coloured with confident nonsense.
    #[test]
    fn every_grammar_a_language_names_is_registered() {
        register_grammars();
        let missing: Vec<&str> = crate::highlight::Language::ALL
            .iter()
            .filter(|language| !registered(language.grammar))
            .map(|language| language.grammar)
            .collect();
        assert!(
            missing.is_empty(),
            "these grammars are named by a language and registered by nobody: {missing:?}"
        );
    }

    /// **The component compiles the injections and the highlights as one
    /// query, and a failure there costs the whole language.**
    ///
    /// `build_combined_injections_query` concatenates injections, locals
    /// and highlights and calls `Query::new` on the result; if that
    /// fails it falls back to the `text` configuration and logs a line.
    /// So an injections query that does not compile against its own
    /// grammar does not lose the injections — it loses the colour of
    /// every file of that kind, and the only sign is a warning in a log
    /// nobody opens. Compiled here the way the component compiles it,
    /// including the renaming, which only the highlights half gets.
    #[test]
    fn the_query_the_component_builds_compiles_for_every_grammar() {
        for grammar in crate::grammars::all() {
            let language = (grammar.language)();
            let combined = format!(
                "{}{}",
                grammar.injections,
                crate::highlight::renamed_captures(&grammar.highlights())
            );
            tree_sitter::Query::new(&language, &combined).unwrap_or_else(|err| {
                panic!(
                    "{}: the combined query the component builds: {err}",
                    grammar.name
                )
            });
        }
    }

    /// A grammar may only inject a grammar that is registered. The
    /// component resolves an injection against this list alone, so a
    /// name on it that nothing registers is an injection that silently
    /// does not happen — and `syntax::spans` reads the same list.
    #[test]
    fn a_grammar_injects_only_grammars_that_exist() {
        register_grammars();
        for grammar in crate::grammars::all() {
            for injected in grammar.injects {
                assert!(
                    registered(injected),
                    "{} declares that it injects {injected}, which nobody registers",
                    grammar.name
                );
            }
        }
    }

    /// **A row in the table is a name the component can reach.**
    ///
    /// [`highlight::CAPTURES`] feeds two engines, and only one of them
    /// reads it as a table. Ours does, through `paint_of_capture`; the
    /// component's is `SyntaxColors::style`, a `match` over forty names
    /// with `_ => None` at the end, and the JSON it is handed is
    /// deserialised into a struct with no `deny_unknown_fields` — so a
    /// key it has never heard of is dropped without a word and the
    /// capture draws plain.
    ///
    /// A row added for a name outside those forty is therefore a colour
    /// in the diff and no colour in the tab: the drift the one-table rule
    /// exists to prevent, arriving through the table itself. This is the
    /// guard for that direction; the coverage guard above is the other.
    ///
    /// **A slot of its own, not a fallback.** `style` falls back along the
    /// dots, so asking it for `type.builtin` answers `type`'s colour and
    /// looked like a slot; the key had been dropped, and the editor drew
    /// `u32` as a type while the diff drew it as a keyword. So the built
    /// theme is written back out and every row must still be a key of it
    /// with a colour — which `comment_doc`, a theme key rather than a
    /// capture, is too.
    #[test]
    fn every_row_of_the_table_is_a_name_the_component_can_reach() {
        let built = highlight_theme(&Theme::default()).expect("accepted");
        let kept = serde_json::to_value(&built.style.syntax).expect("written back");
        let missing: Vec<&str> = highlight::CAPTURES
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| kept.get(*name).is_none_or(|style| style.is_null()))
            .collect();
        assert!(
            missing.is_empty(),
            "{missing:?} are painted by the table and by no slot of the component's: \
             they would be one colour in a diff and their parent's in a tab. \
             A capture with no slot belongs in RENAMED, not here"
        );
    }

    /// The theme the component is handed carries every one of the seven
    /// roles, and the comment is italic.
    #[test]
    fn the_highlight_theme_carries_the_seven_roles() {
        for theme in Theme::all() {
            let built = highlight_theme(&theme).expect("accepted");
            assert!(
                built.style.syntax.keyword.is_some(),
                "{}",
                theme.kind.name()
            );
            assert!(built.style.syntax.type_.is_some());
            assert!(built.style.syntax.string.is_some());
            assert!(built.style.syntax.number.is_some());
            assert!(built.style.syntax.comment.is_some());
            assert!(built.style.syntax.attribute.is_some());
            assert!(built.style.syntax.function.is_some());
        }
    }

    /// Global find must win even while the component's Input owns focus.
    #[gpui::test]
    fn search_global_find_wins_in_editor(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            init(cx);
            let inherited = &cx.global::<crate::actions::Inherited>().0;
            let bindings = crate::actions::ordered(
                vec![(
                    "cmd-shift-f".into(),
                    crate::actions::keymap::Named::FindInFiles,
                )],
                inherited,
            );
            let keymap = gpui::Keymap::new(bindings);
            let contexts: Vec<gpui::KeyContext> = ["Root", "guild", "Input"]
                .into_iter()
                .map(|context| gpui::KeyContext::parse(context).unwrap())
                .collect();
            let stroke = gpui::Keystroke::parse("cmd-shift-f").unwrap();
            let (bindings, _) = keymap.bindings_for_input(&[stroke], &contexts);
            assert!(
                bindings.first().is_some_and(|binding| {
                    binding
                        .action()
                        .as_any()
                        .is::<crate::actions::FindInFiles>()
                }),
                "Cmd+Shift+F must reach Find in Files before local Replace"
            );
        });
    }

    #[gpui::test]
    fn search_contexts_and_overrides_preserve_neighboring_inputs(cx: &mut gpui::TestAppContext) {
        use crate::actions::keymap::Named;
        cx.update(|cx| {
            init(cx);
            let keymap = gpui::Keymap::new(crate::actions::ordered(
                vec![
                    ("alt-f".into(), Named::Find),
                    ("alt-r".into(), Named::Replace),
                    ("cmd-shift-f".into(), Named::FindInFiles),
                ],
                &cx.global::<crate::actions::Inherited>().0,
            ));
            let first = |stroke: &str, stack: &str| {
                let contexts: Vec<gpui::KeyContext> = stack
                    .split(' ')
                    .map(|context| gpui::KeyContext::parse(context).unwrap())
                    .collect();
                keymap
                    .bindings_for_input(&[gpui::Keystroke::parse(stroke).unwrap()], &contexts)
                    .0
                    .first()
                    .map(|binding| binding.action().name().to_owned())
            };
            assert_eq!(
                first("alt-f", "Root guild guild-file Input"),
                Some("guild::Find".into())
            );
            assert_eq!(
                first("alt-r", "Root guild guild-file Input"),
                Some("guild::Replace".into())
            );
            for stack in [
                "Root guild terminal",
                "Root guild picker Input",
                "Root guild guild-file guild-field Input",
            ] {
                assert_ne!(
                    first("alt-r", stack),
                    Some("guild::Replace".into()),
                    "{stack}"
                );
            }
            for stack in [
                "Root guild Input",
                "Root guild picker Input",
                "Root guild terminal",
                "Root guild tree",
            ] {
                assert_eq!(
                    first("cmd-shift-f", stack),
                    Some("guild::FindInFiles".into()),
                    "{stack}"
                );
            }
            assert_ne!(
                first("cmd-shift-f", "Root guild clone-project Input"),
                Some("guild::FindInFiles".into())
            );
            assert_ne!(
                first("alt-r", "Root guild guild-file recording Input"),
                Some("guild::Replace".into())
            );
        });
    }

    /// **Tab pressed in a surface of ours stays there.** The component's
    /// `Root` binds `tab` to *focus the next tab stop*, and Guild's only tab
    /// stops are editors and the commit box — so a Tab in the Search panel
    /// jumped into a buffer and the next letters were autosaved into a file.
    ///
    /// The component's `root::Tab` is not exported, so its public
    /// `IndentInline` stands in for it in `Root`; the real `IndentInline` is
    /// bound in `Input`, as the component binds it.
    ///
    /// What each answer means: in a panel, no binding at all — the key
    /// reaches the panel's own handler. In a file, `IndentInline` and
    /// nothing after it — the buffer indents. In the commit box,
    /// `IndentInline` and nothing after it too — an `auto_grow` box is not
    /// indentable (indent.rs:58-65), so the action propagates
    /// (indent.rs:255-257) and finds nothing further, where until M5.11 it
    /// went on to `Root`'s `Tab` and moved the focus into a file.
    #[test]
    fn tab_in_a_surface_of_ours_never_reaches_the_components_focus_cycle() {
        use gpui_component::input::IndentInline;
        let mut bindings = vec![
            KeyBinding::new("tab", IndentInline, Some("Root")),
            KeyBinding::new("tab", IndentInline, Some("Input")),
        ];
        bindings.extend(unbinds());
        let keymap = gpui::Keymap::new(bindings);
        let tab = gpui::Keystroke::parse("tab").expect("tab parses");
        let found = |stack: &str| {
            let contexts: Vec<gpui::KeyContext> = stack
                .split(' ')
                .map(|context| gpui::KeyContext::parse(context).expect("a context parses"))
                .collect();
            keymap
                .bindings_for_input(std::slice::from_ref(&tab), &contexts)
                .0
                .into_iter()
                .map(|binding| binding.predicate().map(|p| p.to_string()))
                .collect::<Vec<_>>()
        };
        for panel in [
            "Root guild search",
            "Root guild tree",
            "Root guild panel log",
            "Root guild picker",
        ] {
            let bound = found(panel);
            assert!(
                bound.is_empty(),
                "tab in {panel} found bindings in {bound:?}"
            );
        }
        for buffer in ["Root guild Input", "Root guild changes Input"] {
            assert_eq!(
                found(buffer),
                vec![Some("Input".to_owned())],
                "tab in {buffer} must be the buffer's IndentInline and nothing after it"
            );
        }
    }

    /// **The name [`SELECT_RIGHT`] is a promise the compiler cannot
    /// keep.** The action is built by its name because its type is private
    /// to the component, so a rename upstream would leave a search hit
    /// landing with nothing selected and no error anywhere. The registry
    /// GPUI builds actions from is the one place the name can be checked:
    /// some registered action has it, and building it answers an action
    /// that says the same name back.
    #[test]
    fn the_components_select_right_is_registered_under_its_name() {
        let registered: Vec<gpui::MacroActionData> =
            gpui::generate_list_of_all_registered_actions().collect();
        assert!(
            !registered.is_empty(),
            "the test binary's action registry is empty: inventory kept none of the actions"
        );
        let found = registered
            .iter()
            .find(|action| action.name == SELECT_RIGHT)
            .unwrap_or_else(|| {
                let ui: Vec<&str> = registered
                    .iter()
                    .map(|action| action.name)
                    .filter(|name| name.starts_with("ui::Select"))
                    .collect();
                panic!(
                    "no action is registered as {SELECT_RIGHT}; the component's ui::Select* \
                     actions are {ui:?}"
                )
            });
        let built = (found.build)(serde_json::json!({}))
            .unwrap_or_else(|err| panic!("{SELECT_RIGHT} would not build: {err}"));
        assert_eq!(built.name(), SELECT_RIGHT);
    }
    /// **A Markdown preview's code is in the person's code face** — the
    /// component's text view reads `typography.mono`, which is its
    /// `mono_font_family`; left alone that was Menlo on macOS, while the
    /// editor and the diff drew Lilex. The face applied is what the view
    /// reads, and a palette switch afterwards does not put Menlo back.
    #[gpui::test]
    fn markdown_code_takes_the_code_face_and_keeps_it(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            super::apply_code_font("Lilex", cx);
            let mono =
                |cx: &gpui::App| ComponentTheme::global(cx).semantic_tokens().typography.mono;
            assert_eq!(mono(cx).as_ref(), "Lilex");
            super::apply_theme(&crate::theme::Theme::of(crate::theme::Kind::Light), 13., cx);
            assert_eq!(mono(cx).as_ref(), "Lilex", "a palette switch keeps it");
            super::apply_code_font("JetBrains Mono", cx);
            assert_eq!(
                mono(cx).as_ref(),
                "JetBrains Mono",
                "and a new choice follows"
            );
        });
    }

    #[gpui::test]
    fn ghost_controls_follow_the_selected_light_palette(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let dark = crate::theme::Theme::of(crate::theme::Kind::Dark);
            super::apply_theme(&dark, 14., cx);
            assert!(ComponentTheme::global(cx).mode.is_dark());
            let theme = crate::theme::Theme::of(crate::theme::Kind::Light);
            super::apply_theme(&theme, 18., cx);
            assert!(!ComponentTheme::global(cx).mode.is_dark());
            assert_eq!(ComponentTheme::global(cx).font_size, gpui::px(18.));
            assert_eq!(
                ComponentTheme::global(cx).colors.secondary_foreground,
                super::hsla(theme.text.primary)
            );
        });
    }
}
