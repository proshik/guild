# theme-customizer Specification

## Purpose
A person tints the built-in Light and Dark themes — their background, accent
and highlight — and keeps the result, while the built-in palettes stay the
reference every customization is computed from.

## Requirements

### Requirement: Opening the customizer

Settings → Appearance SHALL show a **Customize theme** button under Theme.
Pressing it SHALL open the customizer with a Light / Dark switch naming the
theme being edited; it SHALL open on the theme in effect. While the
customizer is open, the window SHALL draw the theme being edited, including
its unsaved changes, and SHALL return to the theme in effect when the
customizer closes.

#### Scenario: Opens on the theme in effect

- **WHEN** Theme is System, macOS is dark and the person presses Customize theme
- **THEN** the customizer opens with Dark selected in its switch

#### Scenario: Editing the other theme previews it

- **WHEN** Dark is in effect and the person switches the customizer to Light
- **THEN** the window draws Light, with Light's unsaved changes, until the
  customizer closes or is switched back

### Requirement: Background, Accent and Highlight tabs

The customizer SHALL have three tabs, **Background**, **Accent** and
**Highlight**. Each tab SHALL offer a colour field (a swatch, a hex value the
person can type, and a way to pick by hand), a row of preset swatches whose
first swatch returns that tab to the built-in look, an eyedropper, and an
**Intensity** slider from 0 to 100. Choosing a colour or an intensity SHALL
change only the tab it was made in, for the theme being edited.

#### Scenario: Preset swatch

- **WHEN** the person selects a preset swatch in the Accent tab of Dark
- **THEN** Dark's accent takes that colour at the tab's intensity, and its
  Background and Highlight do not change

#### Scenario: Default swatch

- **WHEN** the person selects the first, default swatch in a tab
- **THEN** that tab returns to the built-in palette's look

#### Scenario: Invalid hex

- **WHEN** the person types a value that is not a hex colour and confirms
- **THEN** the field shows the value as invalid, the theme does not change, and
  leaving the field restores the current colour

#### Scenario: Zero intensity

- **WHEN** a tab's Intensity is 0
- **THEN** every token that tab drives equals the built-in value exactly

### Requirement: Background tint

Background SHALL shift every surface token towards the chosen hue by the
intensity while keeping each surface's lightness, so the lightness order of
the surfaces — backdrop, app, panel, card and the others — is unchanged. Text
tokens SHALL take a slighter tint of the same hue. The contrast of text on
every surface SHALL NOT fall below the built-in theme's minimum for the same
ranks and surfaces; when a choice would, Guild SHALL use the strongest
intensity that keeps it.

#### Scenario: Surfaces keep their order

- **WHEN** Dark's Background is tinted with any colour at any intensity
- **THEN** each surface keeps the perceptual lightness it has in built-in Dark
  and their order from darkest to lightest is unchanged

#### Scenario: Contrast guard

- **WHEN** Light's Background is tinted with a saturated colour at Intensity 100
- **THEN** primary, secondary, tertiary and muted text on every surface keep a
  contrast at or above built-in Light's minimum for those pairs

### Requirement: Accent

Accent SHALL replace the theme's interactive accent (`accent.patina`) and what
the window derives from it: links, the selected and focused outlines drawn in
it, the column's selection fill and the selected settings card. Intensity SHALL
move the accent from the built-in colour (0) to the chosen colour (100).
While Highlight has no colour of its own, selected-row grounds SHALL follow the
new accent. The accent's contrast on the app and card surfaces, measured on
the final sRGB colour, SHALL NOT fall below the built-in accent's minimum on
them; when it would, Guild SHALL keep the accent's hue, move its lightness
towards the contrast end, and reduce its chroma only as far as needed to stay
inside sRGB. Every derived colour of every tab SHALL be a valid sRGB colour,
brought into gamut by reducing chroma at the same lightness and hue, never by
clipping channels.

#### Scenario: Accent at full intensity

- **WHEN** Dark's Accent is set to a colour at Intensity 100 and its contrast
  passes
- **THEN** links and the column selection are drawn in that colour

#### Scenario: Saturated blue on Dark

- **WHEN** Dark's Accent is `#0000ff` at Intensity 100
- **THEN** the accent is a valid sRGB blue-hued colour whose contrast on Dark's
  card and app surfaces is at or above built-in Dark's accent minimum

#### Scenario: Saturated yellow on Light

- **WHEN** Light's Accent is `#ffff00` at Intensity 100
- **THEN** the accent is a valid sRGB yellow-hued colour whose contrast on
  Light's card and app surfaces is at or above built-in Light's accent minimum

#### Scenario: Selected rows follow the accent

- **WHEN** Accent is changed and Highlight has no colour
- **THEN** a selected file-tree row's ground takes the accent's hue and keeps
  its built-in lightness

### Requirement: Highlight

Highlight SHALL tint the selection and highlight grounds — the selected rows
drawn with `accent.patina_bg` (a selected file-tree row among them), text
selection in editors, and search-match grounds — towards the chosen colour by
the intensity, keeping their lightness. A picker's lit row — Go to File,
Search Everywhere, Recent Files and the other popups' lists — is
`surface.raised`, which Background tints and Highlight does not. Primary text
on those grounds SHALL keep a contrast of at least WCAG's 4.5:1, or the
built-in theme's contrast on them where that is lower; when a choice would
not, Guild SHALL use the strongest intensity that does.

#### Scenario: Found text

- **WHEN** Light's Highlight is set and the person searches in a file
- **THEN** the matches' ground takes the highlight's hue and the matched text
  stays legible

### Requirement: Live preview, Save and closing

Every change in the customizer SHALL preview live in the window. **Save
<Light|Dark> Theme** SHALL persist the edited theme's choices and keep the
customizer open. Closing the customizer — its close control, Escape from
wherever the focus is on the Settings page, or the card no longer being shown
(another section, a search that hides it, leaving the Settings page) — SHALL
discard unsaved changes of both themes and draw the saved theme in effect. A
card found by the Settings search SHALL stay open while it is shown. Only the person's choices SHALL be stored:
for each theme and tab, a colour and an intensity. Tokens SHALL always be
computed from the built-in palette, so a later fix to a built-in palette
reaches customized themes.

#### Scenario: Save persists across restart

- **WHEN** the person tints Dark's Background, presses Save Dark Theme and
  restarts Guild with Dark in effect
- **THEN** the window draws the tinted Dark

#### Scenario: Slider release reaches every surface

- **WHEN** the person drags an Intensity slider and releases it without moving
  further, or presses Save with the draft just previewed
- **THEN** file, diff and Annotate tabs draw the previewed theme, not the one
  before the drag

#### Scenario: Unsaved preview reverts

- **WHEN** the person changes Light's Accent without saving and closes the
  customizer
- **THEN** the window draws the saved Light accent and nothing is written to
  the settings file

### Requirement: Reset

**Reset** SHALL return the edited theme's three tabs to the built-in palette in
the customizer; after Save the theme is the built-in palette exactly.

#### Scenario: Reset to built-in

- **WHEN** Dark is customized, the person presses Reset and then Save Dark Theme
- **THEN** every Dark token equals built-in Dark exactly and the settings file
  holds no Dark customization

### Requirement: Copy from the other theme

The customizer SHALL offer **Copy from Light** while Dark is edited and **Copy
from Dark** while Light is edited. Copying SHALL take the other theme's saved
choices — colour and intensity for each tab — into the edited theme's unsaved
changes, applied over the edited theme's own built-in palette.

#### Scenario: Copy keeps lightness

- **WHEN** Light has a saved blue Background and the person presses Copy from
  Light while editing Dark
- **THEN** Dark takes a blue background tint while its surfaces keep Dark's
  lightness

### Requirement: Eyedropper

Each tab's eyedropper SHALL open the macOS screen colour sampler. A sampled
colour SHALL become that tab's colour; cancelling the sampler SHALL change
nothing.

#### Scenario: Cancelled sampling

- **WHEN** the person opens the eyedropper and presses Escape
- **THEN** the tab's colour is unchanged

### Requirement: What customization never changes

Customization SHALL NOT change syntax inks, the terminal's ANSI program
colours, brand marks, diff grounds, the warning, error and success accents
(`brass`, `rust`, `green` and their grounds), or any size, radius or font.

#### Scenario: Syntax unchanged

- **WHEN** every tab of Dark is customized
- **THEN** a Rust file's keyword, string and comment inks equal built-in Dark's
