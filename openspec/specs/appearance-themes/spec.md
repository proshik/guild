# appearance-themes Specification

## Purpose
The window draws with one of two built-in themes, Light or Dark, chosen
directly or by following the macOS appearance, for the whole application and
every surface in it.

## Requirements

### Requirement: Two built-in themes

Guild SHALL offer exactly two built-in palettes, **Light** and **Dark**.
Without customization, every token of Light SHALL equal today's Cloud palette
and every token of Dark SHALL equal today's Midnight palette — surfaces, text,
accents, syntax inks with their italics, and diff grounds — bit for bit, with
one exception: Dark's `surface.raised` SHALL be `#313337` rather than
Midnight's `#27292d`, so that a raised fill shows on a `#26282c` card
(the person's decision of 2026-11-21), and what is derived from it — the
control hover over `raised` — follows it. The
Guild, old Light, Dusk, Ember, Graphite and Slate palettes SHALL no longer be
offered or drawn.

#### Scenario: Built-in Dark is today's Midnight

- **WHEN** Dark is in effect and the Dark theme has no customization
- **THEN** every token the window draws with equals the value Midnight has
  before this change, except `surface.raised`, which is `#313337`, and what
  is derived from it

#### Scenario: Built-in Light is today's Cloud

- **WHEN** Light is in effect and the Light theme has no customization
- **THEN** every token the window draws with equals the value Cloud has before
  this change

#### Scenario: Removed palettes are not offered

- **WHEN** the person opens Settings → Appearance
- **THEN** no control offers Guild, Dusk, Ember, Graphite, Slate, Midnight or
  Cloud by name; the only themes are Light and Dark

### Requirement: Theme mode

Settings → Appearance SHALL offer **Theme: System / Light / Dark**. Light and
Dark SHALL draw that theme regardless of the macOS appearance. System SHALL
draw Light while macOS is in its light appearance and Dark while it is in its
dark appearance, and SHALL follow a change of the macOS appearance while
Guild runs, without a restart. A new installation SHALL start on System. The
choice SHALL be one for the whole application, not per workspace or project.

#### Scenario: Fresh install follows the system

- **WHEN** Guild starts with no settings file and macOS is in dark appearance
- **THEN** Theme shows System and the window draws Dark

#### Scenario: System follows a live appearance change

- **WHEN** Theme is System and the person switches macOS from dark to light
  appearance while Guild is open
- **THEN** the main window, its terminals, editors, diffs and every open
  satellite window redraw in Light without a restart

#### Scenario: An explicit choice ignores the system

- **WHEN** Theme is Dark and macOS switches to light appearance
- **THEN** the window keeps drawing Dark

#### Scenario: One theme for every workspace

- **WHEN** the person switches to another workspace or project
- **THEN** the theme in effect does not change

### Requirement: Migration of the old theme setting

A settings file written before this change SHALL open with an explicit theme
choice derived from its old `theme` value: `cloud` and `light` SHALL become
Light; `guild`, `cide`, `darcula`, `ghostty`, `slate`, `islands`, any other
value, and a missing value SHALL become Dark. Migration SHALL NOT be applied to
a file that already holds a theme mode.

#### Scenario: Cloud opens Light

- **WHEN** Guild starts on a settings file whose `theme` is `cloud`
- **THEN** Theme shows Light and the window draws Light

#### Scenario: Slate opens Dark

- **WHEN** Guild starts on a settings file whose `theme` is `slate`
- **THEN** Theme shows Dark and the window draws Dark

#### Scenario: A file without a theme opens Dark

- **WHEN** Guild starts on an existing settings file that has no `theme` and
  no theme mode
- **THEN** Theme shows Dark, the palette an older build drew for that file

#### Scenario: A saved mode is not migrated again

- **WHEN** a settings file holds a theme mode of System and an old `theme` of
  `cloud`
- **THEN** Theme shows System

### Requirement: Settings file compatibility

Theme data SHALL never make the settings file unreadable as a whole. An
unknown theme mode SHALL be read as System; a malformed customization entry
SHALL be dropped on its own while the rest of the settings load. The old
`theme` value SHALL be kept in the file as it was read, so an older Guild
build reading the same file keeps drawing its own previous choice.

#### Scenario: Unknown mode from a newer build

- **WHEN** the settings file's theme mode is a value this build does not know
- **THEN** Guild opens on System with every other setting intact, and the file
  is not moved aside as unreadable

#### Scenario: Malformed customization

- **WHEN** a stored customization has a colour that is not a hex colour or an
  intensity out of range
- **THEN** that one entry is dropped or clamped, every other setting loads, and
  the file is not moved aside

#### Scenario: Older build beside the new one

- **WHEN** the new build saves a settings file migrated from `theme: "cloud"`
- **THEN** the file still holds `theme: "cloud"` byte for byte, and an older
  build opening it draws what it drew for that file before

### Requirement: Every surface follows the theme in effect

When the theme in effect changes — by the Theme control, by the system
appearance, by Save or Reset in the customizer, or by its live preview — the
main window chrome, terminals, file editors, diffs, Annotate, Markdown views,
component-drawn chrome, every open satellite window and every tooltip shown
afterwards SHALL draw with the new theme. Program output colours in a terminal
(the ANSI palette) and syntax inks SHALL stay those of the built-in theme of
the same kind.

#### Scenario: Open Push Commits window follows a change

- **WHEN** the Push Commits window is open and the person switches Theme from
  Dark to Light
- **THEN** the Push Commits window redraws in Light without being reopened

#### Scenario: Tooltip after a change

- **WHEN** the person changes the theme and then hovers a control with a
  tooltip
- **THEN** the tooltip is drawn with the new theme
