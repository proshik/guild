# GPUI Kit and the native Markdown preview

The desktop stack runs on the unmodified published GPUI Kit/component 0.7.1 and
its matching gpui-pre 0.3.8 snapshot. `gpui` is the workspace alias for that
single runtime; headless crates still have no GPUI. The `gpui_kit::application()`
entry point supplies its platform implementation. `gpui-component` remains the
component crate's published name within GPUI Kit. Read this before raising
these versions.

## Retired patches and compatibility

The old 0.5.1 vendor copy and its `[patch.crates-io]` entry were removed, not
ported. Upstream now clips combined editor highlights before shaping. Public
editor tests exercise diagnostics crossing the viewport and a two-byte
definition hover scrolled above Cyrillic text. Guild's jump centering lives in
`editor/reveal.rs`, using public geometry and deferred scrolling; tests cover
first layout, already-visible rows, edges and supersession. Gutter and balloon
geometry come from public editor bounds, not private constants.

The input split uses EditorState/Editor for files, TextareaState/Textarea for
commit messages and console text, and AnyInputState for review forms. Files
still retain one buffer and the same save/conflict policy. `replace_all`
represents an edit; `set_value` loads text and resets history without a Change
event.

Tree-sitter and tree-sitter-tags move together to 0.26. The old Just binding
pinned 0.25 as a normal dependency. Its replacement, codebook-tree-sitter-just
0.1.3, provides both the parser and a public matching HIGHLIGHTS_QUERY; the
obsolete vendored Just query is removed. Grammar/query and capture tests cover
both engines.

## Markdown behavior

One retained native TextView renders each preview. Paragraphs stay at viewport
width. Tables opt into the public table overflow/nowrap styles. Code blocks use
nowrap and flex-column/start alignment to retain intrinsic line width inside
their own horizontal viewport, with the public code actions hook for Copy. No
Markdown renderer or dependency source is patched. Existing parsing bounds,
image cache, mode persistence, source focus and save/close routes remain in
Guild.

The production Pane fixtures measure a 600 px preview and 564 px content for
both the mixed document and README. Identical prose coordinates copy identical
text; real horizontal wheel/selection events reach CODETAIL and TABLETAIL. These
are GPUI test-platform results, not native gesture acceptance.

## The title bar's tooltip and hover

The component uses a Button's single GPUI hover callback for its managed
tooltip, so a Button that combines a tooltip and `on_hover` panics at startup.
A regression test reproduces that panic. Custom hover observation belongs to the
enclosing Guild hitbox instead, which preserves the button's tooltip and the
review preview without modifying upstream sources.
