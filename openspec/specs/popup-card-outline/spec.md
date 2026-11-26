# popup-card-outline Specification

## Purpose
Give every floating card Guild draws a visible outline derived from the active palette, measured against a reference popover where one can be measured, and never weaker than the palette's own divider.

## Requirements

### Requirement: Floating cards carry a visible outline

Every floating card Guild draws itself — context menus, pickers, the usage
card, release notes, Terminal Jobs, Clone Project and any other anchored or
overlaid card the inventory identifies — SHALL draw its one-pixel outer border in a
single theme-derived card outline colour. The colour SHALL be derived from the
active palette, SHALL be visibly distinct from `surface.card` in every palette
(its contrast against `surface.card` strictly above that palette's own
`surface.divider` contrast), and SHALL remain quieter than `text.faint`. Its
strength SHALL be taken from a measured reference popover outline, with the
measurement and its source recorded in the design system; when no reference
can be measured, the design system SHALL record why and the documented fallback
target (about 1.5:1 against `surface.card`) SHALL be used instead.

#### Scenario: Context menu in the dark default palette
- **WHEN** a person opens a worktree's context menu in the column
- **THEN** the card's outline is visible against both the card and the window behind it, and radius, shadow, rows and size are unchanged.

#### Scenario: Usage card
- **WHEN** the person opens the title bar's usage card
- **THEN** it carries the same outline colour as the context menu.

#### Scenario: Every palette
- **WHEN** any of the seven palettes is active
- **THEN** the outline's contrast against `surface.card` lies within the tested band — above that palette's divider contrast and below `text.faint`'s — lighter than the card on dark palettes and darker on light ones.

#### Scenario: No reference can be measured
- **WHEN** no reference application yields its popover outline colour
- **THEN** the design system records the attempt and the reason, and the outline uses the documented fallback target, still satisfying the per-palette band.

#### Scenario: Hairlines inside cards and window frames are untouched
- **WHEN** a menu shows a separator or footnote rule, the window draws its work card and area borders, or the title bar draws its usage and Terminal Jobs chips
- **THEN** those keep their current colours (`surface.divider`, `theme::settings::outline`) exactly as before.

#### Scenario: One definition
- **WHEN** the outline colour is changed in the theme
- **THEN** every floating card follows, because none spells its own outer border colour.
