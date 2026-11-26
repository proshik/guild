# popup-menus Specification

## Purpose
Make menus and the usage card read clearly: clear inset separators, destructive rows in red, strokes shown only where they act on the row's target, and usage levels in green, yellow and red.

## Requirements

### Requirement: Menu separators read clearly

A separator between groups of a menu SHALL be drawn inset from the card's
edges at the row highlight's inset, with more clear space above and below
than the previous 4pt, in a colour visibly lighter than `surface.divider` on
dark palettes (darker on light ones) and no stronger than the card outline.

#### Scenario: Worktree context menu
- **WHEN** a person opens a worktree's context menu
- **THEN** its separators are inset from both card edges, clearly visible, and spaced with clear room above and below.

#### Scenario: Other hairlines untouched
- **WHEN** the window draws footnote rules, panel dividers or the work card
- **THEN** they keep their previous colour and geometry.

### Requirement: Dangerous menu items are red

Every menu row of Guild's own whose action is irreversible or destroys data
SHALL be drawn as a losing row (`accent.rust`), and the audit of all menus
SHALL be recorded.

#### Scenario: Audit
- **WHEN** any menu with a delete, remove, rollback, discard or drop action is opened
- **THEN** that row is red, and non-destructive rows are not.

### Requirement: Menus show strokes, and frequent actions gain free ones

A menu row SHALL show a stroke only when that stroke would perform the same
action on the same target as the row (the existing `stroke_shown` /
`stroke_for` rule in `view/menu/mod.rs`), and SHALL show it whenever that
holds. Existing bindings SHALL NOT change. New bindings SHALL be added only on free
strokes, after the person has approved the proposed list, and a destructive
action reached by a stroke SHALL still ask for confirmation.

#### Scenario: Existing bindings preserved
- **WHEN** the keymap is compared before and after
- **THEN** every previous binding is unchanged and every new stroke is unique.

#### Scenario: Stroke would act on a different target
- **WHEN** a person opens the menu of a background tab, or of a tab split into several panes
- **THEN** Close shows no ⌘W, because the stroke would close the current tab or only the focused pane.

#### Scenario: Remove worktree by stroke
- **WHEN** the approved list includes Remove Worktree and the person presses its stroke on a worktree
- **THEN** the same confirmation as the menu item opens, and the menu row shows the stroke.

### Requirement: The usage card's layout and level colours

The usage card SHALL show a head with the harness mark and name at body size;
for each window one line with its name, `N% left` and `Resets in …`, then a
thick fully rounded bar; a rule; *All providers* with a chevron; and a footer
with the existing refresh status (`refresh_detail`: `Updated …` for current
network data, *from a session*, *outdated*, or the failure) at the left and
*Refresh* at the right. The bar and the
percentage SHALL be green at or above 50% left, yellow below 50% and red
below 20%.

#### Scenario: Plenty left
- **WHEN** 86% of the session window is left
- **THEN** the percentage and bar are green, and `Resets in …` stands on the same line.

#### Scenario: Running low
- **WHEN** 35% and then 10% is left
- **THEN** the colour is yellow and then red.

#### Scenario: Refresh
- **WHEN** the person clicks *Refresh* in the footer
- **THEN** usage is re-read as before and `Updated …` reflects the new time, or the failure is shown as before.

#### Scenario: Data from a session or outdated
- **WHEN** the shown shares come from a session rather than the network, or a window's reset has passed so the share is outdated, with no error
- **THEN** the footer's left side says so exactly as `refresh_detail` does today, never a plain `Updated …`.
