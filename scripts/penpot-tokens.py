"""Read Guild's palettes and measures out of crates/desktop/src/theme and
print Penpot token sets as JSON.

The output is what the Penpot design file's token sets hold: `measures` for
the `common` set and one entry per palette for the `palette/<kind>` sets.
Literals the view code writes inline are listed in VIEW and DERIVED below,
each with the file it comes from, so a re-sync can find them.

Usage: python3 -I scripts/penpot-tokens.py <repo root>
"""

import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
palettes_rs = (root / "crates/desktop/src/theme/palettes.rs").read_text()
mod_rs = (root / "crates/desktop/src/theme/mod.rs").read_text()
settings_rs = (root / "crates/session/src/settings.rs").read_text()

# ThemeKind -> function name in palettes.rs, and the name the settings show.
kinds = re.findall(r"Kind::(\w+) => Self::(\w+)\(\)", palettes_rs)
names = dict(re.findall(r"ThemeKind::(\w+) => \"([^\"]+)\"", settings_rs))
order = re.search(r"pub const ALL: &'static \[ThemeKind\] = &\[(.*?)\];", settings_rs, re.S)
order = re.findall(r"ThemeKind::(\w+)", order.group(1))
enum = settings_rs.split("pub enum ThemeKind {")[1].split("}")[0]
default = re.search(r"#\[default\]\s*(\w+)", enum).group(1)


def hexrgb(v):
    return [(v >> 16) & 255, (v >> 8) & 255, v & 255]


def tohex(c):
    return "#" + "".join(f"{max(0, min(255, round(x))):02X}" for x in c)


def blend(base, over, a):
    # gpui::Rgba::blend: straight sRGB alpha compositing.
    return [b * (1 - a) + o * a for b, o in zip(base, over)]


def lum(c):
    def lin(x):
        x = x / 255
        return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4

    return 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])


def contrast(a, b):
    a, b = lum(a), lum(b)
    return (max(a, b) + 0.05) / (min(a, b) + 0.05)


def card_outline(card, ink, divider):
    target = max(1.5, contrast(card, divider) + 0.05)
    low, high = 0.0, 1.0
    for _ in range(24):
        f = (low + high) / 2
        if contrast(card, blend(card, ink, f)) < target:
            low = f
        else:
            high = f
    return blend(card, ink, high)


alpha = dict(
    (k.lower(), float(v))
    for k, v in re.findall(r"pub const (\w+): f32 = ([0-9.]+);", mod_rs.split("pub mod alpha")[1].split("}")[0])
)

# theme/diff_center.rs: `of()` answers a light set first, then a dark one.
diff_center_rs = (root / "crates/desktop/src/theme/diff_center.rs").read_text()
diff_center = [
    dict((k, int(v, 16)) for k, v in re.findall(r"(\w+): 0x([0-9a-f]{6}),", block))
    for block in re.findall(r"Colors \{(.*?)\}", diff_center_rs.split("pub(crate) fn of")[1], re.S)
][::-1]

palettes = {}
for kind, fn in kinds:
    body = re.search(rf"pub fn {fn}\(\) -> Self \{{(.*?)\n    \}}\n", palettes_rs, re.S).group(1)
    tokens = {}
    for group in ("surface", "text", "accent", "diff"):
        block = re.search(rf"{group}: \w+ \{{(.*?)\}}", body, re.S).group(1)
        for name, value in re.findall(r"(\w+): 0x([0-9a-f]{6})", block):
            tokens[f"color.{group}.{name}"] = int(value, 16)
    syntax = re.search(r"syntax: Syntax \{(.*?)\n            \}", body, re.S).group(1)
    italic = {}
    for name, style, value in re.findall(r"(?:r#)?(\w+): Ink::(plain|italic)\(0x([0-9a-f]{6})\)", syntax):
        tokens[f"color.syntax.{name}"] = int(value, 16)
        italic[name] = style == "italic"
    c = {k: hexrgb(v) for k, v in tokens.items()}
    out = {k: tohex(v) for k, v in c.items()}
    # Colours the frame draws that theme/ computes rather than stores.
    out["color.state.control_hover"] = tohex(
        blend(c["color.surface.card"], c["color.text.primary"], alpha["worktree_hover"])
    )
    out["color.state.control_hover_panel"] = tohex(
        blend(c["color.surface.panel"], c["color.text.primary"], alpha["worktree_hover"])
    )
    # theme/mod.rs control_hover_lit(): the x over a dock tab the pointer lights,
    # control_hover_over the panel's control_hover.
    out["color.state.control_hover_lit"] = tohex(
        blend(
            blend(c["color.surface.panel"], c["color.text.primary"], alpha["worktree_hover"]),
            c["color.text.primary"],
            alpha["worktree_hover"],
        )
    )
    out["color.state.control_hover_inset"] = tohex(
        blend(c["color.surface.inset"], c["color.text.primary"], alpha["worktree_hover"])
    )
    out["color.state.outline"] = tohex(
        blend(c["color.surface.divider"], c["color.text.faint"], 0.5)
    )
    out["color.state.card_outline"] = tohex(
        card_outline(c["color.surface.card"], c["color.text.primary"], c["color.surface.divider"])
    )
    out["color.state.control_hover_raised"] = tohex(
        blend(c["color.surface.raised"], c["color.text.primary"], alpha["worktree_hover"])
    )
    out["color.state.destructive_hover"] = tohex(
        blend(c["color.surface.raised"], c["color.accent.rust"], alpha["worktree_destructive_hover"])
    )
    # theme/mod.rs column_selected(): the selected worktree's card and the tab
    # in front under it, accent.patina over the panel.
    out["color.state.column_selected"] = tohex(
        blend(c["color.surface.panel"], c["color.accent.patina"], alpha["column_selected"])
    )
    # theme/mod.rs Theme::file_row_hover() and file_row(Selected::Unfocused):
    # a file list's row under the pointer and its selection while the keys
    # are elsewhere, primary ink over surface.app — decision 54.
    out["color.state.row_hover"] = tohex(
        blend(c["color.surface.app"], c["color.text.primary"], alpha["row_hover"])
    )
    out["color.state.selection_inactive"] = tohex(
        blend(c["color.surface.app"], c["color.text.primary"], alpha["selection_inactive"])
    )
    # view/start.rs the lit default card: accent.brass at alpha::LIT_OUTLINE over the card.
    out["color.start.lit_outline"] = tohex(blend(c["color.surface.card"], c["color.accent.brass"], alpha["lit_outline"]))
    # theme/settings.rs: the settings page's own blends.
    out["color.settings.outline"] = tohex(blend(c["color.surface.divider"], c["color.text.faint"], 0.5))
    out["color.settings.card"] = tohex(blend(c["color.surface.sunken"], c["color.surface.panel"], 0.35))
    out["color.settings.rule"] = tohex(blend(c["color.surface.divider"], c["color.text.faint"], 0.2))
    # theme::update_ink: the title bar's Update pill in brass while brass reads on every
    # fill the pill takes, else in the primary ink.
    legible = float(re.search(r"pub const LEGIBLE: f32 = ([0-9.]+);", mod_rs).group(1))
    hover = [round(x) for x in blend(c["color.surface.card"], c["color.text.primary"], alpha["worktree_hover"])]
    fills = [c["color.surface.card"], c["color.surface.raised"], hover]
    brass_reads = all(contrast(c["color.accent.brass"], f) >= legible for f in fills)
    out["color.update.ink"] = tohex(c["color.accent.brass"] if brass_reads else c["color.text.primary"])
    # terminal/palette.rs dim(): the foreground kept at DIM_KEEP over the background.
    dim_keep = float(re.search(r"pub const DIM_KEEP: f32 = ([0-9.]+);", mod_rs).group(1))
    out["color.terminal.dim"] = tohex(blend(c["color.surface.app"], c["color.text.primary"], dim_keep))
    # theme/diff_center.rs: the side-by-side diff's centre colours, one set for
    # the light palettes and one for the dark, chosen by Theme::kind.is_light().
    light = sum(c["color.surface.app"]) / 3 > 127
    centre = diff_center[1 if light else 0]
    for name in ("added", "removed", "modified", "empty", "metadata", "number"):
        out[f"color.diff_center.{name}"] = "#%06X" % centre[name]
    # gpui-component's own primary, which Guild leaves alone (editor.rs apply_theme):
    # default-theme.json primary.background, neutral-900 light and neutral-50 dark.
    # A Progress fills with it — the language servers' bars (view/status.rs, servers.rs).
    out["color.component.progress"] = "#171717" if light else "#FAFAFA"
    # gpui-component diagnostic_styles.rs: a diagnostic's card is the component's
    # background (surface.app, editor.rs) blended with its severity at 0.2; the
    # severities are editor.rs's diagnostic colours.
    for sev, ink in (("error", "color.accent.rust"), ("warning", "color.accent.brass"),
                     ("info", "color.accent.patina"), ("hint", "color.text.faint")):
        out[f"color.diagnostic.{sev}_bg"] = tohex(blend(c["color.surface.app"], c[ink], 0.2))
    # AppKit's, not Guild's: the satellite title bar, its title and the traffic
    # lights are drawn by macOS in the system appearance, whatever the palette.
    # Each palette carries the appearance it is normally seen with, measured from
    # window captures: dark with macOS dark, light with macOS light — the Push
    # Commits window's title bar and its key title, and the main window's lights,
    # key and not. The lights' key colours are the same in both appearances. The
    # inactive grey is translucent: #DCDCDC over the white title bar, #444649
    # over the main window's dark title bar, surface.panel (#474B4B over
    # AppKit's own dark bar). Dark was measured on 2026-11-21 (macOS 26): the
    # lights from the dev window, the bar and title from a plain titled
    # NSWindow, which is what GPUI opens for a satellite with
    # appears_transparent: false. AppKit tints its dark bar with the desktop
    # picture, so #242828 is this machine's.
    system = {
        "titlebar": ("#FFFFFF", "#242828"),
        "title": ("#595959", "#A0A4A4"),
        "light_off": ("#C5CACF", "#444649"),
        "close": ("#EC6765", "#EC6765"),
        "minimize": ("#F2CA44", "#F2CA44"),
        "zoom": ("#65C466", "#65C466"),
    }
    for name, (on_light, on_dark) in system.items():
        out[f"color.system.{name}"] = on_light if light else on_dark
    palettes[kind] = {"name": names[kind], "fn": fn, "tokens": out, "italic": italic}


def px(name, module=None):
    src = mod_rs
    if module:
        src = mod_rs.split(f"pub mod {module} {{")[1]
    m = re.search(rf"pub const {name}: Rems = from_px\(([0-9.]+)\);", src)
    return float(m.group(1))


def module(name):
    if f"pub mod {name} {{" not in mod_rs:
        # A module with a file of its own, theme/<name>.rs.
        return (root / f"crates/desktop/src/theme/{name}.rs").read_text()
    body = mod_rs.split(f"pub mod {name} {{")[1]
    depth, end = 1, 0
    for i, ch in enumerate(body):
        depth += ch == "{"
        depth -= ch == "}"
        if depth == 0:
            end = i
            break
    return body[:end]


NUM = r"([0-9]+(?:\.[0-9]*)?)"
SPACING = re.compile(r"PADDING|GAP|INSET|INDENT|OFFSET|BETWEEN|MARGIN|_X$|_Y$")
measures = []


def add(kind, name, value, src):
    measures.append({"type": kind, "name": name, "value": value, "src": src})


def lengths(mod, src_name, prefix, default_kind):
    body = module(mod)
    found = re.findall(
        rf"pub const (\w+): (?:Rems|Pixels|f32) = (?:from_px|px|from_px)\({NUM}\)", body
    ) + re.findall(rf"pub const (\w+): f32 = {NUM};", body)
    seen = set()
    for const, value in found:
        if const in seen or const.endswith(("_FRACTION", "_SECONDS")):
            continue
        seen.add(const)
        kind = default_kind or ("spacing" if SPACING.search(const) else "sizing")
        add(kind, f"{prefix}.{const.lower()}", float(value), f"theme::{src_name}::{const}")


lengths("size", "size", "font.size", "fontSizes")
lengths("space", "space", "space", None)
lengths("column", "column", "column", None)
lengths("activity", "activity", "activity", None)
lengths("terminal", "terminal", "terminal", None)
lengths("tooltip", "tooltip", "tooltip", None)
lengths("radius", "radius", "radius", "borderRadius")
lengths("settings", "settings", "settings", None)
lengths("worktree_confirmation", "worktree_confirmation", "worktree_confirmation", None)
# DIM and HOVER are an opacity each, not a length.
for m in measures:
    if m["name"] in ("worktree_confirmation.dim", "worktree_confirmation.hover"):
        m["type"] = "opacity"
# activity::GLYPH is ICON at the base scale and SLOT_WIDTH is RING: both are
# written in terms of another constant, which the patterns above do not read.
add("sizing", "activity.glyph", "{space.icon}", "theme::activity::GLYPH = space::ICON at the base scale")
add("sizing", "activity.slot_width", "{activity.ring}", "theme::activity::SLOT_WIDTH = RING")
terminal_size = float(re.search(rf"terminal_font_size: {NUM},", settings_rs).group(1))
add("fontSizes", "font.size.terminal", terminal_size, "Settings::default().terminal_font_size")
for const, value in re.findall(rf"pub const (\w+): f32 = {NUM};", module("leading")):
    add("number", f"font.leading.{const.lower()}", round(float(value), 4), f"theme::leading::{const}")
num, den = re.search(rf"pub const TERMINAL: f32 = {NUM} / {NUM};", module("leading")).groups()
add("number", "font.leading.terminal", round(float(num) / float(den), 4), "theme::leading::TERMINAL")
add("borderWidth", "border.hairline", 1.0, "areas.rs card .p(px(1.0)), border_1()")
# Stand-ins until the intended faces are uploaded to Penpot; swap the value only.
add("fontFamilies", "font.family.ui", "Inter", "Intended: SF Pro (fonts::DEFAULT.ui = .SystemUIFont). Stand-in: Inter, Guild's own alternative in fonts.rs, until SF Pro is uploaded.")
add("fontFamilies", "font.family.code", "Lilex", "fonts::DEFAULT.code")
add("fontFamilies", "font.family.terminal", "Lilex Nerd Font", "fonts::DEFAULT.terminal")
for const, value in re.findall(r"pub const (\w+): u32 = 0x([0-9a-f]{6});", module("brand")):
    add("color", f"color.brand.{const.lower()}", "#" + value.upper(), f"theme::brand::{const}")
add("color", "color.on_brass", "#" + re.search(r"ON_BRASS: u32 = 0x([0-9a-f]{6})", mod_rs).group(1).upper(), "theme::ON_BRASS")
for const, value in re.findall(rf"pub const (\w+): f32 = {NUM};", module("alpha")):
    add("opacity", f"alpha.{const.lower()}", float(value), f"theme::alpha::{const}")

# theme/custom.rs PRESETS: the customizer's preset swatches, the same in
# both palettes, so they sit in common.
custom_rs = (root / "crates/desktop/src/theme/custom.rs").read_text()
presets = custom_rs.split("pub const PRESETS")[1].split("];")[0]
for name, value in re.findall(r'\("(\w+)", 0x([0-9a-f]{6})\)', presets):
    add("color", f"color.preset.{name.lower()}", "#" + value.upper(), f"theme::PRESETS {name}: a customizer preset swatch")

# Literals the frame's view code writes inline rather than reading from
# theme/ — named after the file that holds them so a re-sync can find them.
VIEW = [
    ("sizing", "view.column.workspace_head", 30, "view/column.rs workspace page head .h(px(30))"),
    ("spacing", "view.column.head_inset", 16, "view/column.rs .pl(px(16)) of the workspace head and Projects"),
    ("sizing", "view.column.heading", 24, "view/column.rs Projects heading .h(px(24))"),
    ("spacing", "view.column.group_gap", 12, "view/column.rs project list .gap(px(12))"),
    ("spacing", "view.column.list_top", 4, "view/column.rs project list .pt(px(4))"),
    ("sizing", "view.column.workspace_dot", 6, "view/column.rs workspace identity dot"),
    ("borderRadius", "view.column.workspace_dot_radius", 3, "view/column.rs workspace identity dot"),
    ("spacing", "view.column.identity_gap", 8, "view/column.rs workspace identity .gap(px(8))"),
    ("sizing", "view.column.project_head", 28, "view/column.rs project head .h(px(28))"),
    ("spacing", "view.column.project_head_gap", 8, "view/column.rs project head .gap(px(8))"),
    ("borderRadius", "view.column.project_head_radius", 8, "view/column.rs project head .rounded(px(8))"),
    ("sizing", "view.column.tile", 18, "view/column.rs project tile .size(px(18))"),
    ("borderRadius", "view.column.tile_radius", 5, "view/column.rs project tile .rounded(px(5))"),
    ("fontSizes", "view.column.tile_text", 10, "view/column.rs project tile text_size(from_px(10))"),
    ("sizing", "view.column.chevron", 11, "view/column.rs project chevron .size(px(11))"),
    ("spacing", "view.column.row_left", 6, "view/column.rs worktree row .pl(px(6))"),
    ("spacing", "view.column.row_right", 10, "view/column.rs worktree row .pr(px(10))"),
    ("spacing", "view.column.row_top", 4, "view/column.rs worktree row .pt(px(4.0))"),
    ("spacing", "view.column.row_bottom", 4, "view/column.rs worktree row .pb(px(4.0))"),
    ("spacing", "view.column.glyph_margin", 2, "view/column.rs branch glyph .ml(px(2))"),
    ("sizing", "view.column.glyph_width", 14, "view/column.rs branch glyph .w(px(14))"),
    ("sizing", "view.column.glyph_height", 18, "view/column.rs branch glyph .h(px(18))"),
    ("spacing", "view.column.text_margin", 6, "view/column.rs worktree text .ml(px(6))"),
    ("sizing", "view.column.line", 20, "view/column.rs worktree line_height px(20) at 13pt"),
    ("spacing", "view.column.name_gap", 6, "view/column.rs name line .gap(px(6))"),
    ("spacing", "view.column.meta_gap", 4, "view/column.rs meta line .gap(px(4))"),
    ("spacing", "view.column.changes_gap", 5, "view/column.rs changed numbers .gap(px(5))"),
    ("sizing", "view.tabs.mark", 16, "view/tabs.rs TAB_MARK"),
    ("sizing", "view.tabs.close", 18, "view/tabs.rs TAB_CLOSE"),
    ("sizing", "view.tabs.kinds_chevron", 9, "view/tab_list.rs add chevron .size(px(9))"),
    ("sizing", "view.tabs.kinds_width", 14, "view/tab_list.rs add chevron button .w(px(14))"),
    ("sizing", "view.tabs.rule", 1, "view/tab_list.rs strip rule .w(px(1)), view/tabs.rs strip_row line .h(px(1))"),
    ("spacing", "view.usage.gap", 5, "view/usage.rs usage_chip_frame .gap(px(5))"),
    ("sizing", "view.forge.badge", 8, "view/forge.rs glyph badge ring .size(px(8.0)), at .left(px(7.5)).top(px(9.5))"),
    ("sizing", "view.forge.badge_icon", 6, "view/forge.rs glyph badge svg .size(px(6.0))"),
    ("spacing", "view.forge.card_gap", 6, "view/forge.rs PathCard .gap(px(6.0))"),
    ("spacing", "view.forge.card_x", 12, "view/forge.rs PathCard .px(px(12.0))"),
    ("spacing", "view.forge.card_y", 10, "view/forge.rs PathCard .py(px(10.0))"),
    ("sizing", "view.forge.card_width", 420, "view/forge.rs CARD_WIDTH: PathCard .max_w, and .w with a failure line"),
    ("spacing", "view.forge.line_gap", 8, "view/forge.rs PathCard head and line .gap(px(8.0))"),
    ("sizing", "view.forge.line_icon", 12, "view/forge.rs PathCard line icon .size(px(12.0))"),
    ("sizing", "view.window.width", 1180, "main.rs default window size(px(1180), px(760))"),
    ("sizing", "view.window.height", 760, "main.rs default window size"),
]
for kind, name, value, src in VIEW:
    add(kind, name, float(value), src)

# Derived measures: Penpot token expressions over the tokens above, so a change
# to a source token flows through. Values are strings Penpot evaluates.
DERIVED = [
    # Stage 2: the Explorer, the editor, Changes/Commit and the diff.
    ("fontWeights", "font.weight.semibold", "600", "view/panels.rs dock title FontWeight::SEMIBOLD"),
    ("textCase", "font.case.upper", "uppercase", "view/panels.rs dock title, upper-cased at the draw"),
    ("sizing", "view.dock.width", "230", "session project_state.rs Panels default left Panel::of(230.0)"),
    ("sizing", "view.dock.card", "{view.dock.width} + 2 * {border.hairline}", "view/panels.rs render_side_card: the panel and a point of border either side"),
    ("spacing", "view.tree.row_left", "{space.tree_padding} + {space.tree_inset}", "view/tree.rs row .pl(TREE_PADDING + TREE_INSET + TREE_INDENT * depth), depth 0"),
    ("spacing", "view.tree.depth_1", "{view.tree.row_left} + {space.tree_indent}", "view/tree.rs row padding at depth 1; view/changes.rs file rows (depth + 1)"),
    ("spacing", "view.tree.depth_2", "{view.tree.row_left} + 2 * {space.tree_indent}", "view/tree.rs row padding at depth 2"),
    ("spacing", "view.changes.box_inset", "{space.tree_padding} - {space.tree_inset}", "view/changes.rs commit box .px(TREE_PADDING - TREE_INSET)"),
    ("spacing", "view.changes.field_inset", "4", "view/changes.rs message box .py(px(4.0))"),
    ("sizing", "view.editor.line", "{font.size.terminal} * {font.leading.code}", "file_tab.rs line_height(size * leading::CODE), size = terminal_font_size"),
    ("sizing", "view.editor.gutter", "58", "measured: gpui-component editor gutter, card edge to code"),
    ("sizing", "view.editor.numbers", "18", "measured: gpui-component editor, card edge to the line numbers' right edge"),
    ("spacing", "view.editor.marker_gap", "5", "measured: line numbers to file_tab/git_markers stripe"),
    ("sizing", "view.diff.numbers", "35", "measured: view/diff/gutter.rs numbers_cell, number_width + CARD_GAP/2 each side"),
    ("sizing", "view.diff.marker", "{space.tree_gap} * 2", "view/diff/surface.rs marker .w(TREE_GAP * 2)"),
    ("sizing", "view.diff.half", "({view.window.work_inner} - 2 * {space.mark_bar}) / 2", "view/diff/surface.rs left half .w(relative(ratio)), ratio 0.5 by default"),
    ("sizing", "view.diff.half_right", "{view.window.work_inner} - 2 * {space.mark_bar} - {view.diff.half} - {space.diff_connector}", "view/diff/surface.rs right half: what the left half and the connector leave"),
    ("sizing", "view.column.narrowest", "{space.traffic_lights} + 4 * {space.status_button} + {space.status_button_gap} + 2 * {space.card_gap} + {space.work_between_px}", "column.rs narrowest(ui_font) at the default size: the title bar's traffic lights, column button, history pair and mode button in the column's corner, one gap between areas short of its edge (above NARROWEST's 180); frame.rs fit_frame_panels raises a narrower saved width to it"),
    ("sizing", "view.window.file_width", "{view.window.width}", "a file tab in front keeps the window's width: frame::minimum no longer counts the status bar's server line (7e72ff7b), which grew it to 1550"),
    ("sizing", "view.window.file_work", "{view.window.width} - {space.column}", "the room right of the column at its default width"),
    ("sizing", "view.column.tab_row", "28", "view/column/worktree_tabs.rs ROW, a tab row's pitch"),
    ("sizing", "view.column.tab_fill", "25", "view/column/worktree_tabs.rs FILL, a lit tab row's ground"),
    ("sizing", "view.column.tab_elbow", "8", "view/column/worktree_tabs.rs ELBOW"),
    ("sizing", "view.column.tab_elbow_width", "{view.column.tab_elbow}", "the elbow as a size: view/column/worktree_tabs.rs .w(px(ELBOW))"),
    ("sizing", "view.column.tab_line", "{view.column.tab_gap} + {view.column.tab_row} / 2", "view/column/worktree_tabs.rs connector .h(GAP + ROW * (count - 0.5)), for one tab"),
    ("spacing", "view.column.tab_connector", "{border.hairline} + {view.column.row_left} + {activity.slot_width} + {view.column.glyph_margin} + {view.column.glyph_width} / 2", "view/column/worktree_tabs.rs CONNECTOR_X: under the middle of the row's branch glyph"),
    ("spacing", "view.column.tab_left", "{view.column.tab_connector} + {view.column.tab_elbow}", "view/column/worktree_tabs.rs tab row .pl(CONNECTOR_X + ELBOW)"),
    ("spacing", "view.column.tab_gap", "3 - ({view.column.tab_row} - {view.column.tab_fill}) / 2", "view/column/worktree_tabs.rs GAP: card outline to the first lit row, three apart"),
    ("spacing", "view.column.tab_inset_left", "6", "view/column/worktree_tabs.rs tab card .pl(px(6.0)) and .gap(px(6.0))"),
    ("spacing", "view.column.tab_inset_right", "4", "view/column/worktree_tabs.rs tab card .pr(px(4.0))"),
    ("number", "font.leading.phi", "1.618", "GPUI's own line height where a view sets none (phi): menu and picker text"),
    ("opacity", "alpha.backdrop", "0.32", "view/picker.rs dim(): the backdrop under a floating card, surface.backdrop at 0.32"),
    ("spacing", "view.popup.row_inset", "{space.menu_inset} - {radius.row}", "view/popup.rs row_inset(): MENU_INSET - radius::ROW"),
    ("spacing", "view.popup.row_margin", "{radius.row}", "view/popup.rs a row's .mx(radius::ROW)"),
    ("spacing", "view.popup.top", "64", "root/recent.rs picker_top(): a floating card's top"),
    ("sizing", "view.popup.everywhere_height", "518", "measured: Search Everywhere's card at its default size, with the preview on"),
    ("sizing", "view.popup.find_height", "651", "measured: Find in Files' card at its default size"),
    ("sizing", "view.popup.goto_top", "{space.recent_files_top}", "view/goto_line.rs .mt(RECENT_FILES_TOP)"),
    ("spacing", "view.popup.goto_inset_y", "10", "view/goto_line.rs card .py(px(10.0))"),
    ("spacing", "view.popup.preview_inset", "4", "view/preview.rs .py(px(4.0))"),
    ("spacing", "view.popup.preview_number", "8", "view/preview.rs number .pr(px(8.0))"),
    ("spacing", "view.popup.heading_bottom", "4", "view/everywhere.rs heading .pb(px(4.0))"),
    ("spacing", "view.popup.find_foot_y", "6", "view/find_in_files.rs foot .py(px(6.0))"),
    ("sizing", "view.popup.find_place", "260", "view/find_in_files.rs file and line .max_w(px(260.0))"),
    ("sizing", "view.popup.row_width", "{space.menu_min} - 2 * {radius.row}", "a menu row inside the narrowest card: MENU_MIN less the rows' .mx(radius::ROW)"),
    ("sizing", "view.popup.list_row", "{space.everywhere} - 2 * {radius.row}", "a picker row inside Search Everywhere: the card less the rows' .mx(radius::ROW)"),
    ("sizing", "view.popup.preview_line", "{view.editor.line}", "view/preview.rs: Look::line_height(), the editor's line - terminal_font_size at leading::CODE"),
    ("sizing", "view.log.branches", "200", "session project_state.rs Columns::BRANCHES: the Log's branches column"),
    ("sizing", "view.log.details", "320", "session project_state.rs Columns::DETAILS: the opened commit's column"),
    ("sizing", "view.log.message", "140", "session project_state.rs Columns::MESSAGE: the opened commit's message block"),
    ("sizing", "view.log.list", "{view.window.work_inner} - {view.log.branches} - {view.log.details} - 2 * {view.log.grip}", "view/log.rs the commits between the branch column, two column grips and the opened commit"),
    ("sizing", "view.log.row", "{view.log.list} - 2 * {space.tree_inset}", "view/log.rs list .px(TREE_INSET): a commit row"),
    ("sizing", "view.log.author", "{view.log.row_inner} * 0.22", "theme LOG_AUTHOR_FRACTION of the row's content box (max LOG_AUTHOR 160)"),
    ("sizing", "view.log.graph", "{space.log_graph_lane} * 2", "view/log.rs graph_row .w(LOG_GRAPH_LANE * lanes), two lanes in the fixture"),
    ("spacing", "view.log.ref_inset", "5", "view/log.rs ref_chip .px(px(5.0))"),
    ("spacing", "view.log.filter_gap", "3", "view/log_filters.rs filter button .gap(px(3.0))"),
    ("spacing", "view.log.filter_right", "8", "view/log_filters.rs filter button .pr(px(8.0)) while not narrowed"),
    ("sizing", "view.log.dock", "400", "the bottom dock as the person dragged it for this screen (Panel default 198)"),
    ("sizing", "view.log.refs", "{view.log.row_inner} * 0.25", "theme LOG_REFS_FRACTION: a ref chip is at most a quarter of the row's content box (and LOG_REFS), clipped"),
    ("sizing", "view.log.branch_row", "{view.log.branches} - 2 * {space.tree_inset}", "view/log.rs branches column .px(TREE_INSET): a row in it"),
    ("spacing", "view.log.branch_indent", "{space.tree_padding} + {space.tree_indent}", "view/log.rs a branch under its group .pl(TREE_PADDING + TREE_INDENT)"),
    ("sizing", "view.log.detail_row", "{view.log.details} - {border.hairline}", "view/log.rs the opened commit's column inside its .border_l_1()"),
    ("sizing", "view.log.dock_card", "{view.log.dock} + 2 * {border.hairline}", "view/panels.rs render_bottom_card: the dock and a point of border either side"),
    ("sizing", "view.popup.branch_row", "{space.branch_popup} - 2 * {border.hairline} - 2 * {radius.row}", "a branch row inside the branch popup: its border_1 and the rows' .mx(radius::ROW) either side"),
    ("sizing", "view.push.width", "1000", "root/push.rs open_satellite default (1000.0, 620.0)"),
    ("sizing", "view.push.height", "620", "root/push.rs open_satellite default (1000.0, 620.0)"),
    ("sizing", "view.push.left", "({view.push.width} - {view.push.grip}) * 0.55", "theme/push.rs LEFT_SHARE of the dialog less the grip"),
    ("sizing", "view.push.foot", "48", "view/push_dialog.rs FOOT"),
    ("sizing", "view.push.toolbar", "30", "view/push_dialog.rs TOOLBAR"),
    ("sizing", "view.push.branch_row", "28", "view/push_dialog.rs left(): the push target row .h(28) .px(16) .gap(6)"),
    ("sizing", "view.push.commit_row", "26", "view/push_dialog.rs a commit row .h(26) .pl(34) .pr(10)"),
    ("sizing", "view.push.file_row", "24", "view/push_dialog.rs tree: a row .h(24)"),
    ("sizing", "view.system.titlebar", "32", "measured: AppKit's title bar on a satellite window (appears_transparent: false)"),
    ("spacing", "view.push.inset", "16", "view/push_dialog.rs .px(16) of the target row and the foot"),
    ("sizing", "view.push.window", "{view.push.height} + {view.system.titlebar}", "the Push window with its system title bar"),
    ("sizing", "view.push.grip", "9", "view/push_dialog.rs grip(Sides): .w(9) with a 1 px line in its middle"),
    ("spacing", "view.push.commit_inset", "34", "view/push_dialog.rs a commit row .pl(34)"),
    ("spacing", "view.push.tool_gap", "4", "view/push_dialog.rs toolbar .gap(4)"),
    ("spacing", "view.push.tool_inset", "8", "view/push_dialog.rs toolbar .px(8)"),
    ("spacing", "view.push.rule_margin", "2", "view/push_dialog.rs toolbar rule .mx(2)"),
    ("spacing", "view.push.tree_inset", "12", "view/push_dialog.rs tree: the root row .pl(12)"),
    ("spacing", "view.push.tree_indent", "18", "view/push_dialog.rs tree: indent(depth) = 12 + (depth + 1) * 18"),
    ("spacing", "view.push.tree_depth_0", "{view.push.tree_inset} + {view.push.tree_indent}", "indent(0)"),
    ("spacing", "view.push.tree_depth_1", "{view.push.tree_inset} + 2 * {view.push.tree_indent}", "indent(1)"),
    ("spacing", "view.push.tree_pad", "4", "view/push_dialog.rs tree .py(4)"),
    ("sizing", "view.push.tree_top", "{view.push.tree_pad}", "the tree's .py(4) above its first row, as a spacer under the toolbar"),
    ("sizing", "view.push.help", "22", "view/push_dialog.rs foot: the help button .size(22), round, bordered text.faint"),
    ("sizing", "view.push.select_w", "150", "view/push_dialog.rs foot: the tags select .w(150)"),
    ("sizing", "view.push.select_h", "26", "view/push_dialog.rs foot: the tags select .h(26)"),
    ("spacing", "view.push.select_px", "10", "view/push_dialog.rs foot: the tags select .px(10)"),
    ("sizing", "view.push.cancel", "88", "view/push_dialog.rs foot: Cancel .min_w(88)"),
    ("sizing", "view.push.action", "68", "view/push_dialog.rs foot: Push .min_w(68)"),
    ("sizing", "view.push.split", "28", "view/push_dialog.rs foot: the Push button's chevron segment .w(28), border_l"),
    ("sizing", "view.push.chevron", "14", "view/push_dialog.rs foot: the Push chevron .size(14)"),
    ("sizing", "view.window.card_over_dock", "{view.window.column_height} - {view.log.dock_card} - 2 * {space.work_gap}", "view/panels.rs render_bottom_card: the work card over the bottom dock, its gap and its .mb(WORK_GAP)"),
    ("spacing", "view.component.input_px", "10", "gpui-component sizing.rs Size::Medium input_px: a Textarea's own inset, appearance(false) or not"),
    ("spacing", "view.component.input_py", "8", "gpui-component sizing.rs Size::Medium input_py"),
    ("spacing", "view.console.inset_x", "{space.card_padding} + {view.component.input_px}", "view/console.rs .p(CARD_PADDING) around the Textarea and its own px"),
    ("spacing", "view.console.inset_y", "{space.card_padding} + {view.component.input_py}", "view/console.rs .p(CARD_PADDING) around the Textarea and its own py"),
    ("spacing", "view.border_inset", "{border.hairline}", "GPUI border_1 takes its point out of the layout; an inner stroke in Penpot does not, so a bordered card pads by it"),
    ("sizing", "view.log.grip", "9", "view/panels.rs GRIP_REACH: render_column_grip between the log's columns, drawn as nothing"),
    ("sizing", "view.log.row_inner", "{view.log.row} - 2 * {space.tree_padding}", "a log row inside its .px(TREE_PADDING): what gpui::relative() fractions of the row resolve against"),
    ("sizing", "view.log.summary_min", "{space.log_summary_min}", "theme LOG_SUMMARY_MIN: the summary's min_w, four characters and an ellipsis; it gives way before the chips"),
    ("spacing", "view.settings.switch_pad", "3", "view/controls.rs toggle .p(px(3.0))"),
    ("sizing", "view.settings.knob", "{font.size.body}", "view/controls.rs toggle knob .size(theme::size::BODY)"),
    ("sizing", "view.settings.nav_item", "{settings.nav} - 2 * {space.card_gap} - {border.hairline}", "view/settings/mod.rs a section button: the navigation .p(CARD_GAP) and its .border_r_1()"),
    ("spacing", "view.settings.group_y", "4", "view/settings/mod.rs a group under its section .py(px(4.0)); a row's title and hint .gap(px(4.0)); the search field .gap(px(4.0))"),
    ("spacing", "view.settings.nav_gap", "2", "view/settings/mod.rs the section list .gap(px(2.0))"),
    ("sizing", "view.settings.content", "{view.window.work_inner} - {settings.nav}", "view/settings/mod.rs the rows beside the navigation, in a tab of the work card"),
    ("sizing", "view.settings.repo_indent", "{view.settings.nav_item} - {space.card_padding}", "view/settings/scopes.rs a repository under its workspace .ml(CARD_PADDING)"),
    ("sizing", "view.settings.row_inner", "{view.settings.content} - 2 * {settings.padding} - 2 * {border.hairline} - 2 * {space.card_padding}", "view/settings/sections.rs a w_full control under its row: the content, its padding, the card's border_1 and the row's .p(CARD_PADDING)"),
    ("spacing", "view.settings.button_gap", "2", "view/settings/sections.rs the Theme mode buttons .gap(px(2.0)); customizer.rs the Light/Dark switch and the tabs .gap(px(2.0))"),
    ("borderWidth", "view.settings.preset_ring", "2", "view/settings/customizer.rs the chosen preset .border_2() in text.primary"),
    ("opacity", "view.settings.save_idle", "0.5", "view/settings/customizer.rs Save with nothing to save .opacity(0.5)"),
    ("sizing", "view.slider.height", "24", "gpui-component slider.rs a horizontal slider .h_6()"),
    ("sizing", "view.slider.track", "6", "gpui-component slider.rs the track .h_1p5(), rounded"),
    ("sizing", "view.slider.thumb", "16", "gpui-component slider.rs the thumb .size_4(); absent while disabled"),
    ("spacing", "view.slider.thumb_ring", "1", "gpui-component slider.rs the thumb .p(px(1.)) around its slider_thumb disc"),
    ("sizing", "view.slider.disc", "{view.slider.thumb} - 2 * {view.slider.thumb_ring}", "gpui-component slider.rs the slider_thumb disc .size_full() inside the thumb's .p(px(1.))"),
    ("opacity", "view.slider.track_opacity", "0.2", "gpui-component slider.rs the track bg(slider_bar.opacity(0.2)); slider_bar is accent.patina (editor.rs)"),
    ("opacity", "view.slider.thumb_opacity", "0.5", "gpui-component slider.rs the thumb bg(slider_bar.opacity(0.5)); its disc is slider_thumb, text.primary (editor.rs)"),
    ("sizing", "view.keymap.chip", "140", "view/settings/keymap.rs the stroke .min_w(px(140.0))"),
    ("spacing", "view.keymap.chip_y", "3", "view/settings/keymap.rs the stroke and Reset .py(px(3.0))"),
    ("spacing", "view.keymap.row_y", "5", "view/settings/keymap.rs a row .py(px(5.0))"),
    ("spacing", "view.keymap.reset_x", "6", "view/settings/keymap.rs Reset .px(px(6.0))"),
    ("spacing", "view.keymap.chip_pad_y", "{view.keymap.chip_y} + {border.hairline}", "the stroke .py(px(3.0)) and its border_1, which GPUI lays out and an inner stroke does not"),
    ("spacing", "view.keymap.chip_pad_x", "{space.card_gap} + {border.hairline}", "the stroke .px(CARD_GAP) and its border_1"),
    ("sizing", "view.settings.scoped_field", "{settings.content} / 2", "view/settings/scopes.rs a scoped field .w(CONTENT / 2)"),
    ("spacing", "view.settings.scoped_pad", "{space.card_padding} + {border.hairline}", "view/settings/scopes.rs card .p(CARD_PADDING) and its border_1"),
    ("sizing", "view.settings.scoped_field_h", "42", "measured: a scoped field — .p(CARD_GAP), border_1 and the text field's own line (about 24 at UI size)"),
    ("sizing", "view.settings.repo_tile", "16", "view/settings/scopes.rs LIST_TILE: a repository's letter tile in the WORKSPACES list, a 16-point icon's room"),
    ("borderRadius", "view.settings.repo_tile_radius", str(round(5 * 16 / 18, 4)), "view/column.rs tile(side): .rounded(px(5 * side / 18)) at LIST_TILE, computed here because a borderRadius token's formula may not refer to a sizing one"),
    ("fontSizes", "view.settings.repo_tile_text", str(round(10 * 16 / 18, 4)), "view/column.rs tile(side): .text_size(from_px(10 * side / 18)) at LIST_TILE"),
    ("spacing", "view.workspaces.merge_names_top", "4", "view/workspaces.rs render_workspace_field: the merge question's names .pt(px(4.0))"),
    ("spacing", "view.workspaces.merge_buttons_bottom", "8", "view/workspaces.rs merge_projects_buttons .pb(px(8.0))"),
    ("spacing", "view.column.tabs_toggle_gap", "3", "view/column/worktree_tabs.rs worktree_tabs_toggle .gap(px(3.0)) between the count and the chevron"),
    ("spacing", "view.column.tabs_toggle_pad", "4", "view/column/worktree_tabs.rs worktree_tabs_toggle .px(px(4.0)) while folded"),
    ("fontWeights", "font.weight.medium", "500", "view/start.rs FontWeight::MEDIUM: the card title and the chips"),
    ("sizing", "view.start.tile", "({space.start_column} - 3 * {space.card_gap}) / 4", "view/start.rs four tiles .flex_1() in the column, .gap(CARD_GAP)"),
    ("opacity", "view.start.faint_mark", "0.35", "view/kind.rs FAINT: a full-colour harness mark not on PATH"),
    ("sizing", "view.start.rows", "{space.start_column} + 2 * {space.card_gap}", "view/start.rs the recent rows .mx(-CARD_GAP): wider than the column by a gap either side"),
    ("sizing", "view.start.hints", "300", "view/body.rs HINTS: the two columns of keyboard hints"),
    ("number", "view.sheet.leading", str(round(px("LINE", "worktree_confirmation") / px("BODY", "size"), 4)), "view/worktree_removal.rs card .line_height(LINE) at BODY: LINE / BODY, computed here because Penpot applies no number token that refers to a sizing one"),
    ("spacing", "view.problems.depth_1", "{space.tree_padding} + {space.tree_indent}", "view/problems.rs row .pl(TREE_PADDING + TREE_INDENT * depth), depth 1"),
    ("spacing", "view.problems.depth_2", "{space.tree_padding} + 2 * {space.tree_indent}", "view/problems.rs row padding at depth 2: a file's problems under src/"),
    ("sizing", "view.servers.row", "24", "view/servers.rs a server's line .h(px(24.0))"),
    ("spacing", "view.servers.task_gap", "4", "view/servers.rs a task .gap(px(4.0)).py(px(4.0)); the card .position(at.top() - px(4.0))"),
    ("spacing", "view.diagnostic.pad_x", "4", "gpui-component diagnostic_popover.rs .px_1()"),
    ("spacing", "view.diagnostic.pad_y", "2", "gpui-component diagnostic_popover.rs .py_0p5()"),
    ("spacing", "view.diagnostic.inset_x", "{view.diagnostic.pad_x} + {border.hairline}", "the card's .px_1() and its border_1, which GPUI lays out and an inner stroke does not"),
    ("spacing", "view.diagnostic.inset_y", "{view.diagnostic.pad_y} + {border.hairline}", "the card's .py_0p5() and its border_1"),
    ("fontSizes", "view.diagnostic.text", "12", "gpui-component input/popovers editor_popover .text_xs(): 0.75rem, and Guild's rem is 16 at the default UI size (scale.rs)"),
    ("borderRadius", "view.diagnostic.radius", "6", "gpui-component styled.rs popover_style .rounded(theme.radius), the component's default px(6.), which Guild leaves alone"),
    ("borderWidth", "view.diagnostic.squiggle", "1", "measured: the editor's wavy underline is a 1pt line, 9pt to a wave, 2pt high"),
    ("sizing", "view.agents.row", "{space.branch_popup} - 2 * {border.hairline} - 2 * {space.card_gap}", "view/menu/agents.rs a session row .mx(CARD_GAP) inside the navigator .w(BRANCH_POPUP) and its border_1"),
    ("sizing", "view.tabs.compact", "{space.tabs} + {space.card_gap}", "view/tabs.rs a Process tab alone in its tab .w(TABS + CARD_GAP): its mark and close only"),
    ("spacing", "view.tabs.compact_pad", "{space.card_gap} / 2", "view/tabs.rs the compact Process tab .px(CARD_GAP / 2.0)"),
    ("spacing", "view.popup.note_gap", "2", "view/popup.rs note .mt(px(2.0)) above its rule when a row precedes it"),
    ("spacing", "view.tabs.compact_gap", "{space.tab_underline}", "view/tabs.rs compact_process_mark .gap(TAB_UNDERLINE), as a spacing token: Penpot ignores a sizing one on a gap"),
    ("spacing", "view.process.columns", "2 * {space.card_padding}", "view/process.rs the main column and the context .gap(CARD_PADDING * 2.0)"),
    ("borderWidth", "view.process.rule", "2", "view/process.rs a step's .border_t_2() in its state's colour"),
    ("sizing", "view.component.button_h", "24", "gpui-component button.rs Size::Small .h_6()"),
    ("spacing", "view.component.button_px", "8", "gpui-component button.rs Size::Small .px_2()"),
    ("fontSizes", "view.component.button_text", "14", "gpui-component sizing.rs button_text_size Small .text_sm(): 0.875rem at Guild's rem of 16"),
    ("number", "view.component.button_leading", "1.25", "gpui-component button.rs .line_height(relative(1.25)): everything drawn inside a Button"),
    ("opacity", "view.component.disabled", "0.5", "gpui-component button.rs a disabled Button's label: muted_foreground (text.muted, editor.rs) at opacity(0.5)"),
    ("borderRadius", "view.component.radius", "6", "gpui-component theme radius px(6.), which Guild leaves alone: a Button's ButtonRounded::Medium"),
    ("opacity", "view.progress.track_opacity", "0.2", "gpui-component progress.rs the track bg(bar.opacity(0.2)) over the element's own bg, accent.patina in Guild"),
    ("sizing", "view.popup.snap", "8", "anchored().snap_to_window_with_margin(px(8.0)): a popup keeps this far from the window's edge"),
    ("sizing", "view.new_worktree.base", "24", "view/new_worktree.rs the 'from <branch>' row .h(px(24.0))"),
    ("spacing", "view.popup.title_top", "8", "view/workspaces.rs a field's or the picker's title .pt(px(8.0))"),
    ("sizing", "view.icon_picker.width", "260", "view/workspaces.rs the icon picker card .w(px(260.0))"),
    ("sizing", "view.icon_picker.cell", "30", "view/workspaces.rs an icon cell .size(px(30.0))"),
    ("spacing", "view.icon_picker.gap", "4", "view/workspaces.rs the grid .gap(px(4.0))"),
    ("spacing", "view.icon_picker.padding", "8", "view/workspaces.rs the grid .p(px(8.0))"),
    ("spacing", "view.icon_picker.badge", "1", "view/workspaces.rs the current icon's check .top(px(1.0)).right(px(1.0))"),
    ("sizing", "view.clone.rule", "2", "view/clone_project.rs the status block .border_l_2()"),
    ("spacing", "view.usage.bar_top", "3", "view/usage.rs a window's bar .mt(px(3.0))"),
    ("spacing", "view.usage.reason_top", "6", "view/usage.rs a provider's reason .pt(px(6.0))"),
    ("sizing", "view.usage.anchor_gap", "4", "view/usage.rs the card 4px under the chip"),
    ("spacing", "view.notice.padding", "{space.card_padding} + {border.hairline}", "view/notices.rs a notice .p(CARD_PADDING) and its border_1"),
    ("spacing", "view.sheet.padding", "{worktree_confirmation.padding} + {border.hairline}", "view/worktree_removal.rs card .p(PADDING) and its border_1"),
    ("spacing", "view.sheet.button_pad", "{worktree_confirmation.gap} + {border.hairline}", "view/worktree_removal.rs Cancel and Delete .px(GAP) and their border_1"),
    ("spacing", "view.sheet.branch_pad", "{space.card_gap} + {border.hairline}", "view/worktree_removal.rs the branch box .px(CARD_GAP) and its border_1"),
    ("spacing", "view.clone.padding", "{space.panel_padding} + {border.hairline}", "view/clone_project.rs card .p(PANEL_PADDING) and its border_1"),
    ("spacing", "view.clone.field_pad", "{space.button_side} + {border.hairline}", "view/clone_project.rs a field, Browse and Clone .px(BUTTON_SIDE) and their border_1"),
    ("spacing", "view.clone.status_left", "{space.button_side} + {view.clone.rule}", "view/clone_project.rs the status block .p(BUTTON_SIDE) after its border_l_2"),
    ("spacing", "view.clone.footer_top", "{space.button_side} + {border.hairline}", "view/clone_project.rs the buttons' row .pt(BUTTON_SIDE) under its border_t_1"),
    ("spacing", "view.usage.padding", "{space.menu_padding} + {border.hairline}", "view/usage.rs the card .py(MENU_PADDING) and its border_1"),
    ("spacing", "view.update.button_pad", "{space.card_gap} + {border.hairline}", "view/release_notes.rs a button or category .px(CARD_GAP) and its border_1"),
    ("sizing", "view.field.caret", "18", "measured: gpui-component's caret in a focused single-line field at UI size"),
    ("sizing", "view.field.caret_w", "1.5", "measured: gpui-component's caret width"),
    ("spacing", "view.clone.side", "{space.button_side}", "space::BUTTON_SIDE as padding: view/clone_project.rs the status block and Cancel .p/.px(BUTTON_SIDE)"),
    ("sizing", "view.usage.head_mark", "{font.size.body}", "view/usage.rs a provider's head: harness_mark(.., size::BODY, ..)"),
    ("spacing", "view.titlebar.pill_pad", "{space.pill_padding} + {border.hairline}", "view/titlebar.rs the Update pill .px(PILL_PADDING) and its border_1"),
    ("spacing", "view.action_bar.item_pad", "{space.card_gap} + {border.hairline}", "view/settings/action_bar.rs an item's .p(CARD_GAP) and border_1; Add item and Restore defaults .px(CARD_GAP) and border_1"),
    ("spacing", "view.action_bar.chip_pad", "{space.chip_padding} + {border.hairline}", "view/settings/action_bar.rs the Usage sample .px(CHIP_PADDING) and its border_1"),
    # A Markdown preview is gpui-base's TextView (markdown/pane.rs): Guild sets only the
    # table and code block overflow, the table's ground and its bold head, so the rest is
    # the component's defaults, at Guild's 16px rem.
    ("fontSizes", "view.markdown.h1", "28", "gpui-base text/node.rs heading level 1 rems(2.) of px(14.), BOLD"),
    ("fontSizes", "view.markdown.h2", "21", "gpui-base text/node.rs heading level 2 rems(1.5) of px(14.), SEMIBOLD"),
    ("spacing", "view.markdown.heading_pad", "4.8", "gpui-base text/node.rs a heading's .pb(rems(0.3))"),
    ("spacing", "view.markdown.gap", "16", "gpui-base text/style.rs TextViewStyle paragraph_gap rems(1.), the default markdown/pane.rs keeps"),
    ("spacing", "view.markdown.code_pad", "12", "gpui-base text/node.rs CodeBlock .p_3()"),
    ("spacing", "view.markdown.actions_inset", "8", "gpui-base text/node.rs CodeBlock actions .absolute().top_2().right_2()"),
    ("borderRadius", "view.markdown.radius", "6", "gpui-base theme_tokens.rs RadiusTokens md px(6.): the code block and its actions .rounded(radius.md)"),
    ("fontSizes", "view.markdown.code", "13", "gpui-base theme_tokens.rs TypographyTokens mono_md text_style(13., 20.)"),
    ("number", "view.markdown.code_leading", str(round(20 / 13, 4)), "gpui-base theme_tokens.rs mono_md text_style(13., 20.): 20 / 13"),
    ("borderWidth", "view.markdown.quote_rule", "3", "gpui-base text/node.rs Blockquote .border_l_3() in border, surface.divider (editor.rs)"),
    ("spacing", "view.markdown.quote_pad", "16", "gpui-base text/node.rs Blockquote .px_4()"),
    ("spacing", "view.markdown.cell_px", "8", "gpui-base text/node.rs a table cell .px_2()"),
    ("spacing", "view.markdown.cell_py", "4", "gpui-base text/node.rs a table cell .py_1()"),
    ("spacing", "view.image.control_px", "{space.button_side}", "file_tab/image.rs Fit, 100%, − and + and Source .px(BUTTON_SIDE): BUTTON_SIDE as spacing, which Penpot pads with"),
    ("spacing", "view.markdown.table_rule", "{border.hairline}", "gpui-base text/node.rs the table's track and its cells .border_1() in border, surface.divider (editor.rs): a gap and an inset, since a frame shows them as spacing"),
    ("sizing", "view.markdown.checkbox", "14", "gpui-base text/node.rs a task's box .size(rems(0.875)), border_1 in foreground"),
    ("sizing", "view.markdown.check", "10", "gpui-base text/node.rs the checked box's svg .size(rems(0.625)) on foreground"),
    ("spacing", "view.markdown.checkbox_gap", "6", "gpui-base text/node.rs the box's .mr_1p5()"),
    ("color", "color.scrim", "#000000", "view/worktree_removal.rs gpui::black() under the sheet, at worktree_confirmation::DIM"),
    ("sizing", "view.icon_picker.row", "{view.icon_picker.width} - 2 * {border.hairline} - 2 * {view.popup.row_margin}", "view/workspaces.rs the No icon row: popup::row's .mx(radius::ROW) inside the 260 card and its border_1"),
    ("spacing", "view.popup.card_pad", "{space.menu_padding} + {border.hairline}", "view/popup.rs card .py(MENU_PADDING) and card_look's border_1"),
    ("sizing", "view.clone.inner", "{space.start_column} - 2 * {view.clone.padding}", "view/clone_project.rs the card w(START_COLUMN) inside its .p(PANEL_PADDING) and border_1"),
    ("opacity", "view.tree.ignored", "0.5", "view/tree.rs: an ignored row's icon, and a cut row, at .opacity(0.5)"),
    ("sizing", "view.changes.field", "99", "measured: root/changes.rs message .auto_grow(4, 12) at four rows, borders included"),
    ("sizing", "view.window.file_card_inner", "{view.window.file_work} - {view.dock.card} - {space.work_between_px} - {space.work_inset} - 2 * {border.hairline}", "the work card inside its border, right of the column and the default left dock"),
    ("spacing", "view.diff.number_inset", "{space.card_gap} / 2", "view/diff/gutter.rs numbers_cell .px(CARD_GAP / 2)"),
    ("spacing", "view.diff.tool_gap", "{space.tree_gap} / 2", "view/diff/surface.rs toolbar .gap(TREE_GAP / 2.)"),
    ("sizing", "view.diff.counter", "{space.head_button} * 2", "view/diff/surface.rs change count and file counter .w(HEAD_BUTTON * 2.)"),
    ("sizing", "view.diff.bar", "{space.mark_bar} * 2", "view/diff/surface.rs .pr(MARK_BAR * 2.) and the marks lane"),
    ("spacing", "view.diff.bar_inset", "{space.mark_bar} * 2", "view/diff/surface.rs .pr(MARK_BAR * 2.)"),
    ("sizing", "view.diff.body", "{view.window.work_inner} - {view.diff.bar}", "view/diff: the rows, left of the marks lane"),
    ("sizing", "view.titlebar.history_narrow", "{view.column.narrowest} - {space.traffic_lights} - {space.card_gap}", "view/titlebar.rs history group .min_w(column - traffic - gap) at the narrowest column"),
    ("sizing", "view.work.between", "{space.work_between_px}", "the gap between two cards, as a size (space.work_between_px is a spacing)"),
    ("sizing", "view.column.row_width_narrow", "{view.column.narrowest} - 2 * {column.list_inset}", "a column row at the narrowest column"),
    ("spacing", "view.editor.top", "9", "measured: gpui-component editor, the card strip to the first line"),
    ("borderRadius", "radius.full", "999", "GPUI rounded_full(): half the shorter side; Penpot clamps"),
    ("sizing", "view.titlebar.history_min", "{space.column} - {space.traffic_lights} - {space.card_gap}", "view/titlebar.rs history group .min_w(column - traffic - gap)"),
    ("spacing", "view.titlebar.branch_left", "({space.status_button} - {space.head_icon}) / 2 + {space.head_icon} * 4 / 24", "view/status.rs branch chip .pl(...); the code also subtracts the glyph's left bearing at run time"),
    ("sizing", "view.titlebar.open_in", "{space.status_button} * 2", "view/titlebar.rs Open In .w(STATUS_BUTTON * 2)"),
    ("spacing", "view.titlebar.traffic_inset", "{space.traffic_lights}", "view/titlebar.rs .pl(traffic) as a padding"),
    ("spacing", "view.status.bottom_inset", "{space.status_bottom}", "view/status.rs, column.rs .pb(STATUS_BOTTOM) as a padding"),
    ("spacing", "view.column.heading_right", "{column.list_inset} + {column.header_padding}", "view/column.rs Projects heading .pr"),
    ("spacing", "view.column.row_left_framed", "{view.column.row_left} + {border.hairline}", "worktree row padding plus its border_1"),
    ("spacing", "view.column.row_right_framed", "{view.column.row_right} + {border.hairline}", "worktree row padding plus its border_1"),
    ("spacing", "view.column.row_top_framed", "{view.column.row_top} + {border.hairline}", "worktree row padding plus its border_1"),
    ("spacing", "view.column.row_bottom_framed", "{view.column.row_bottom} + {border.hairline}", "worktree row padding plus its border_1"),
    ("sizing", "view.column.sort_icon", "{font.size.meta}", "view/column.rs project sort icon at size::META"),
    ("sizing", "view.column.row_width", "{space.column} - 2 * {column.list_inset}", "a row inside the list's px(LIST_INSET)"),
    ("sizing", "view.usage.mark", "{font.size.meta}", "view/usage.rs harness_mark at size::META"),
    ("spacing", "border.inset", "{border.hairline}", "the room a GPUI border_1 takes inside a box"),
    ("sizing", "view.hairline", "{border.hairline}", "a 1px rule drawn as a box"),
    ("sizing", "view.window.work_width", "{view.window.width} - {space.column} - {space.work_inset}", "root/render.rs work surface .mr(WORK_INSET)"),
    ("sizing", "view.window.work_inner", "{view.window.work_width} - 2 * {border.hairline}", "view/areas.rs card .p(px(1))"),
    ("sizing", "view.window.column_height", "{view.window.height} - {space.titlebar} - {space.status_bar}", "root/render.rs: the row between title bar and footer"),
    ("sizing", "view.window.status_width", "{view.window.width} - {space.column}", "root/render.rs footer: status bar beside the column foot"),
    ("sizing", "view.window.body_height", "{view.window.column_height} - 2 * {border.hairline} - {space.tabs}", "view/areas.rs body under the strip"),
    ("sizing", "view.terminal.cell_width", "{font.size.terminal} * 0.6", "a 0.6 em monospace cell; terminal/font.rs measures the face at run time"),
    ("sizing", "view.terminal.cell_height", "{font.size.terminal} * {font.leading.terminal}", "terminal/font.rs line at leading::TERMINAL"),
    ("color", "color.terminal.xterm_174", "#D78787", "xterm cube 174 (terminal/palette.rs indexed); program output, not theme"),
    ("color", "color.terminal.xterm_114", "#87D787", "xterm cube 114 (terminal/palette.rs indexed); program output, not theme"),
    ("sizing", "view.forge.icon", "{font.size.meta}", "view/forge.rs glyph icons::glyph(.., theme::size::META)"),
    ("sizing", "view.system.light", "14", "macOS traffic light diameter, measured from the window"),
    ("spacing", "view.system.light_gap", "9", "macOS traffic light spacing, measured from the window"),
    ("spacing", "view.system.light_left", "12", "macOS traffic light inset, measured from the window"),
]
for kind, name, value, src in DERIVED:
    add(kind, name, value, src)
add("fontWeights", "font.weight.regular", "400", "GPUI default weight")
add("fontWeights", "font.weight.bold", "700", "view/column.rs tile FontWeight::BOLD")

print(json.dumps({"order": order, "default": default, "palettes": palettes, "measures": measures}, indent=1))
