# Guild — the design system (extracted from the mockups)

**Date:** 2026-08-25
**Status:** approved as the visual reference
**Mockups:** Penpot, since 2026-11-19 — the Guild — Library and Guild — Screens files, see `docs/penpot.md`

> Every colour, size and radius in a Penpot screen is a token, named after a token here and read off `theme/` by `scripts/penpot-tokens.py`, so a number changed here is changed once there and every screen follows.
**Implemented:** in the desktop on GPUI + gpui-component.

The document exists for one reason: a mockup is a picture, and native components need numbers. The numbers are here.

---

## 1. The five rules everything follows from

Derived from a review: the first draft of the mockups broke all five and looked like a blueprint rather than an application.

1. **The backdrop is neutral, the accents are warm.** A grey-black base with no colour bias. Brass and patina appear in spots — on what is active, not as an overall tone. A warm backdrop produces a sepia effect and ages the interface.
2. **A border marks state; a hairline separates the window's areas.** Only an active, selected or attention-demanding element has an outline. The areas of the window — the sidebar, the work, the docks, the title bar and the status bar — are separated by one point of `divider`; everything inside an area is separated by a step in the background. A border around every block turns the interface into a table, and one line between five areas is not one. **Rewritten in M12.4** rather than bent: the old rule said *not structure* and was kept by a step of four parts in a channel, which the eye does not see. **Amended in M12.6** (section 4.1): the areas are now separated by the step from the window's `surface.panel` to the work's card of `surface.app`, which has a one-point `divider` border. A hairline is left only where a side dock meets the work inside the card.
3. **The text contrast is spread wide.** A heading is almost white, a second line strongly muted. Two similar mid-greys are the most common cause of "murkiness".
4. **The type scale is wide.** From 11 px rubrics to a 26 px heading. A range of three or four points creates no hierarchy.
5. **Colour sparingly, but saturated.** Status indicators are small, bright, with a soft halo. Muting everything equally means highlighting nothing.

---

## 2. Colour tokens

**Two palettes since `two-themes-customize`: Dark and Light.** Sections
2.1 to 2.4 and 2.7 are **Dark**'s — which was
called Midnight — and section 2.6 is **Light**'s, which was called Cloud.
Guild's own palette, the first Light, Dusk, Ember, Graphite and Slate were
removed; a settings file that named one opens Dark (or Light for the two
light ones). A person tints either palette by the rule in section 2.9; the
values here are the reference every tint is computed from, and they do not
move when a person customizes.

**Dark's source:** an editor colour scheme's own files, not a screenshot — M8.10: its syntax values, by the attribute names in brackets below, and its greys, as `gray-10` … `gray-40`. A colour read off a capture is a blend of two colours and of the antialiasing between them; a file is the thing itself.

**Transcribed:** `surface.app` is the editor's own background and `gray-10`; `surface.panel` is `gray-20`, `surface.card` `gray-30`, `divider` and `surface.badge` `gray-40`; `surface.inset` is the caret row; the text ranks are `TEXT` and the theme's grey ramp; `text.gutter` is `LINE_NUMBERS_COLOR`; `accent.brass` is the warning stripe, `accent.patina` the hyperlink, `accent.green` `ADDED_LINES_COLOR`, `accent.rust` the error red, `accent.violet` the constant's magenta.

**Derived** by the steps above: `surface.backdrop`, `.sunken`, `.card.muted` and `.raised`, the three accent backdrops, and the diff pair — a scheme says what a changed *line* is, not what a row of ours is.

**Syntax**, and this is the whole of what makes a file look like the scheme's:
- keyword ← `DEFAULT_KEYWORD` — and a built-in type and a string's escape with it, which is why Go's `int64` is orange;
- type ← `DEFAULT_CLASS_REFERENCE`, which is plain text: **the scheme does not colour a type**;
- string ← `DEFAULT_STRING`, number ← `DEFAULT_NUMBER`;
- comment ← `DEFAULT_LINE_COMMENT`, **upright**; doc ← `DEFAULT_DOC_COMMENT`, italic;
- macro ← `DEFAULT_METADATA`, the olive an annotation is drawn in;
- function ← `DEFAULT_FUNCTION_DECLARATION`;
- constant ← `DEFAULT_CONSTANT`, italic;
- tag ← `XML_TAG_NAME`.

### 2.1 Surfaces

| Token | Value | Where |
|---|---|---|
| `surface.backdrop` | `#131416` | behind the application window |
| `surface.app` | `#191a1c` | the main canvas |
| `surface.panel` | `#212326` | the rail, the side panels, the title strip |
| `surface.sunken` | `#1d1e21` | sunken blocks: the agent's event feed |
| `surface.inset` | `#1f2024` | a nested block inside a panel |
| `surface.card` | `#26282c` | a task card |
| `surface.card.muted` | `#212326` | the card of a completed task |
| `surface.raised` | `#313337` | a highlighted row, a secondary button, the active tab |
| `surface.badge` | `#33353b` | a counter badge |
| `divider` | `#33353b` | a divider inside a panel, the work card's border since M12.6, and — M12.4 to M12.6 — the hairline between the window's areas |

### 2.2 Text

| Token | Value | Role |
|---|---|---|
| `text.primary` | `#bcbec4` | headings, values |
| `text.secondary` | `#9fa2a8` | captions, secondary matter |
| `text.tertiary` | `#8b8e94` | metadata |
| `text.muted` | `#73767c` | rubrics, inactive |
| `text.faint` | `#6f737a` | almost invisible |
| `text.gutter` | `#4b5059` | line numbers, timestamps |

The key proportion: the gap between `primary` and `secondary` must read at once. When in doubt, mute the secondary further.

**Dark keeps its source's own ramp**, which is narrower between `primary` and `secondary` than the retired Guild palette was; rule 3's 80-point luma gap was that palette's and is not asserted of Dark or Light, which must stay as their sources draw them. What every palette must hold is the order of the ranks, which `every_palette_keeps_its_text_ranks_in_order` checks.

### 2.3 Accents

| Token | Main | Backdrop | Meaning |
|---|---|---|---|
| `accent.brass` | `#c29e4a` | `#3e3726` | in progress, the primary action; its backdrop a warning's ground, a drop target, a lit toggle |
| `accent.patina` | `#548af7` | `#26334c` | in review, an agent's action, a link; its backdrop a selected or lit row |
| `accent.green` | `#549159` | — | done, the tests passed |
| `accent.rust` | `#f75464` | `#45262a` | remarks, errors |
| `accent.violet` | `#c77dbb` | — | code keywords, the agent thinking |
| `accent.highlight` | `#3e3726` | — | the ground of selected text and of a search match — equal to `accent.brass` backdrop in both built-in palettes, kept apart so Highlight (section 2.9) can tint it without recolouring warnings, drop targets and toggles |

The correspondence with the domain's statuses (section 6.1 of the P1 spec):

| Status | Colour |
|---|---|
| `todo` | `text.faint` — a grey dot |
| `in_progress` | `accent.brass` |
| `in_review` | `accent.patina` |
| `done` | `accent.green` |
| `blocked` / remarks | `accent.rust` |
| `cancelled` | `text.muted`, the card muted |

### 2.4 Syntax highlighting

| Element | Colour |
|---|---|
| keyword | `#cf8e6d` |
| type, constructor | `#bcbec4` |
| string literal | `#6aab73` |
| number | `#2aacb8` |
| comment | `#7a7e85` (upright) |
| doc comment | `#5f826b` (italic) |
| macro, attribute | `#b3ae60` |
| function name | `#56a8f5` |
| constant | `#c77dbb` (italic) |
| tag | `#d5b778` |
| type.reference, where a grammar asks | `#6fafbd` |
| namespace | `#afbf7e` |
| builtin | `#cc7832` |

**Thirteen roles since M8.10, and the last six are Dark's source's doing.** The
first three of them — a doc comment, a constant, a tag — every palette
needed. **The last three are a grammar's to ask for**: a type
*reference*, a namespace and a built-in, which exist because the source
colours a type by the *language*, not by the scheme. Its Go support draws
a type reference teal while Java and Kotlin draw one plain, so one mapping
cannot be both, and `highlight::PAINTS` is where a grammar says which it
means. In Light, which has another source, the three keep the colour the plain
roles had — `type`, `macro` and `keyword` — so a grammar that asks changes
nothing there.

**Ten roles as of the first half of M8.10, and the last three are Dark's source's doing.** Seven was
what every palette here needed; Dark's source scheme draws a doc comment, a
constant and a markup tag apart from the comment, the number and the type,
and a palette that cannot say so cannot be that scheme. In Light, which is
not, each of the three keeps the colour the seven-role mapping gave it. **Italic moved here
with them**: it was one rule in code — *a comment is italic* — and the
scheme's line comment is upright.

**How a grammar's captures land on these seven** — one table, and the whole of what a theme knows about syntax. Every tree-sitter grammar names its spans with captures like `@keyword`, `@type.builtin`, `@function.method`; a capture falls back along its dots, so `function.method` finds `function`, which is `tree-sitter-highlight`'s own rule.

| Capture, with everything under it | Role |
|---|---|
| `keyword`, `keyword.control` | keyword |
| `type`, `constructor`, `enum`, `variant` | type |
| `string`, `string.escape`, `string.special` | string |
| `number`, `boolean`, `constant` | number |
| `comment`, `comment.doc` | comment, italic |
| `attribute`, `preproc`, `function.special` (a macro invocation) | macro |
| `function`, `function.method`, `function.definition`, `label`, `title` | function |
| `tag` (a JSX element's name) | type |
| `property`, `variable`, `variable.parameter`, `lifetime`, `link_uri` | `text.primary` — no colour of their own |
| `punctuation`, `operator` | `text.secondary` |
| `emphasis`, `emphasis.strong` | `text.primary`, italic and bold as they say |

**The table is one table — M8.1.1.** It lives in `highlight::CAPTURES`, the editor component's theme is built from it, and the spans the diff and Annotate are drawn with read it. Before that pass there were two, and they had drifted apart by six names; the live one was not the tested one.

**Some of what a grammar names gets renamed on the way in**, because the component's own vocabulary has forty names and no room for these. A capture without a dot gets no fallback either, so no theme could reach them. `highlight::RENAMED` is the list — **fourteen rows since M8.8.6**, which added four of them:

| What a grammar says | What it is registered as | Where |
|---|---|---|
| `escape` | `string.escape` | rust, go, json — every escape in every string was grey until M8.1.1 |
| `module`, `namespace` | `type` | elixir's `MyApp.Repo`, proto's packages |
| `parameter` | `variable.parameter` | kotlin |
| `character` | `string` | kotlin — a character literal is a string of one |
| `float` | `number` | kotlin |
| `conditional`, `repeat`, `include`, `exception` | `keyword` | make, whose query is nvim-treesitter-flavoured |
| `field` | `property` | nvim-treesitter's word for a record's member — sql's and lua's `a.b` |
| `method` | `function.method` | lua's `obj:method()`; the prefix rule takes `@method.call` with it |
| `storageclass` | `keyword` | sql's `primary key`, `unique`, `default` |
| `delimiter` | `punctuation.delimiter` | c's comma and semicolon |

**And what stays grey is a list, not an oversight.** `embedded` (the wrapper around a region handed to another grammar), `spell`, `error`, `text.*` (make's diagnostic ranks), and every capture whose name begins with an underscore — `@__name__`, `@_class` — which is the convention for a node a `#match?` predicate names rather than a colour. A test walks every registered query against the table and this list, so a capture on neither is a colour nobody decided about.

**Light** colours the same roles in section 2.6.

The diff's two backgrounds and two markers are section 2.7's.

### 2.5 System

The macOS window traffic lights: `#f2665a`, `#f5bd4f`, `#61c554`. In a native application the system draws them — there is no need to reproduce them; the values are given for the mockups only.

---

### 2.6 Light

Sections 2.1 to 2.4 are Dark's. Light is the other palette, one table.

**Every value here was read off a file, never written from memory.** That is
this repository's rule, and it has already cost it once — a migration test
that passed on invented data.

#### Light

Approved 2026-10-03 as Cloud: a cool light palette with blue-grey surfaces and ink, a light editor appearance and light icon variants.

| Token | Value |
|---|---|
| `surface.backdrop` | `#dbe3ec` |
| `surface.app` | `#f8fafc` |
| `surface.panel` | `#e7edf3` |
| `surface.sunken` | `#f0f4f8` |
| `surface.inset` | `#e2eaf2` |
| `surface.card` | `#ffffff` |
| `surface.card.muted` | `#f0f4f8` |
| `surface.raised` | `#dbe3ec` |
| `surface.badge` | `#e2eaf2` |
| `divider` | `#cbd5e1` |
| `text.primary` | `#172337` |
| `text.secondary` | `#33465d` |
| `text.tertiary` | `#52657c` |
| `text.muted` | `#63768b` |
| `text.faint` | `#98a8ba` |
| `text.gutter` | `#aebdcd` |
| `accent.brass` | `#805900` |
| `accent.brass.backdrop` | `#e7e0ce` |
| `accent.highlight` | `#e7e0ce` |
| `accent.patina` | `#286493` |
| `accent.patina.backdrop` | `#d7e8f5` |
| `accent.green` | `#2f7a3d` |
| `accent.rust` | `#c4452a` |
| `accent.rust.backdrop` | `#f6ebe9` |
| `accent.violet` | `#7a3fa8` |
| `syntax.keyword` | `#7a3fa8` |
| `syntax.type` | `#0f6c7c` |
| `syntax.string` | `#805900` |
| `syntax.number` | `#2f7a3d` |
| `syntax.comment` | `#63768b` |
| `syntax.macro` | `#286493` |
| `syntax.function` | `#286493` |
| `syntax.doc` | `#63768b` |
| `syntax.constant` | `#2f7a3d` |
| `syntax.tag` | `#0f6c7c` |
| `syntax.type.reference` | `#0f6c7c` |
| `syntax.namespace` | `#286493` |
| `syntax.builtin` | `#7a3fa8` |
| `diff.added` | `#e3ece5` |
| `diff.removed` | `#f5e7e7` |

---

### 2.7 Diff

Two backgrounds and two markers, and nothing else: a diff is code with a tint behind the lines that changed, not a third palette.

| Token | Value | Where |
|---|---|---|
| `diff.added` | `#242f27` | behind an added line, and behind the `+` |
| `diff.removed` | `#382226` | behind a removed line, and behind the `−` |

The markers are `accent.green` for `+` and `accent.rust` for `−`; the line numbers on both sides are `text.gutter`; a hunk's `@@` row is `text.faint`.

**The status marks in the file tree** (section 6.4) are letters in the accents that already mean those things: `M` in `accent.brass` — in progress — `A` in `accent.green`, `D` in `accent.rust`, and `?` for an untracked file in `text.faint`, the quiet one.

**Light** carries `diff.added` and `diff.removed` in its table in section 2.6. **The floor:** a background must differ from its `surface.app` by at least 16 summed over the three channels, or a changed line is not visibly changed.

### 2.8 Brands

A brand's mark belongs to its owner, and so does its colour. It is not part of either palette and it is the same in both. The value was measured on 2026-09-16 (M12.6) off a tab strip that draws Claude's mark; Anthropic's brand orange `#d97757`, used for one commit before that, was a different orange. A theme's accent says what a thing is doing. A brand's colour says whose it is. Claude's mark drawn in a dark palette's `#ffff00` brass is not Claude's mark, and that is exactly how the tab strip drew it until 2026-09-16.

| Token | Value | Where |
|---|---|---|
| `brand.claude` | `#ce825f` | Claude's mark on a tab that holds Claude Code (section 6.5) in a terminal |

Harness marks stay at full brand colour while their agents work; a separate activity ring rotates (section 6.5). `runs_colour` identifies the harness wherever a kind is coloured without the mark; Claude uses the theme's brass accent. All Claude sessions run in terminals.


### 2.9 Customization — `two-themes-customize`

A person tints either palette from Settings → Appearance → Customize theme.
**Only the choices are stored** — per palette, for each of Background,
Accent and Highlight, a colour and an intensity from 0 to 100 — and the
tokens are computed from the tables above every time (`theme/custom.rs`,
`theme/tint.rs`), so a fix to a built-in palette reaches every customized
one. Intensity 0, or no choice, is the built-in palette exactly.

The arithmetic is OKLab. A colour outside sRGB is brought in by shrinking
its chroma at the same lightness and hue, never by clipping channels.

| Tab | Tokens | Rule |
|---|---|---|
| Background | the ten `surface.*`; the six `text.*` | lightness kept; (a, b) towards the seed's hue at chroma ≤ 0.045 for surfaces, ≤ 0.020 at half the intensity for text |
| Accent | `accent.patina`; `accent.patina.backdrop` while Highlight has no colour | patina moves towards the seed in L, a and b; the backdrop takes the new accent's hue at chroma ≤ 0.06, lightness kept |
| Highlight | `accent.patina.backdrop`, `accent.highlight` | lightness kept; (a, b) towards the seed at chroma ≤ 0.08 |

**Guards, measured on the rounded colour.** Background: text ranks
primary to muted on `app`, `panel`, `sunken`, `card` and `raised` keep at
least the built-in palette's lowest contrast among those pairs. Accent: on
`app` and `card` it keeps the built-in accent's lowest contrast, by moving
its lightness towards white (Dark) or black (Light) and reducing chroma
only as far as sRGB requires. Selection grounds: primary text on them keeps
WCAG's 4.5, or the built-in contrast where that is lower. A choice that
would break a guard is taken at the strongest intensity that does not, so
a strong colour can look weaker than its slider says — on purpose.

**Never customized:** syntax inks, diff grounds, `brass`, `rust`, `green`,
`violet` and their backdrops, a terminal's sixteen program colours, brand
marks, and every size, radius and font. Everything drawn *from* the tinted
tokens follows: the column's selection, control hovers, card outlines,
settings outlines, a terminal's ground, selection and matches.

**Presets**, after a *Default* swatch that clears the tab: Blue `#3b82f6`,
Teal `#14b8a6`, Green `#22c55e`, Amber `#f59e0b`, Orange `#f97316`, Rose
`#f43f5e`, Violet `#8b5cf6`, Graphite `#71717a`. A first colour starts at
intensity 40 (Background), 100 (Accent) or 70 (Highlight).
---

## 3. Typography

**Families, since M12.11:** the **system face** for the interface, **Lilex** (400, 700, and both italics) for code and **Lilex Nerd Font** for the terminal — the same under every palette: a palette is colours. **Lilex Duo** (400, 700) ships as a second face for code, marked *Beta*. **Inter** (400, 500, 600, and a true 400 italic) and **JetBrains Mono** (400, 700, and both italics), the defaults from M5.9 to M12.11, still ship and are rows of the list. All are in `crates/desktop/assets/fonts`, registered before the window opens, OFL 1.1 with every licence beside the files.

What follows is why Inter replaced Plex in M5.9, kept as the record of that decision.

**It was IBM Plex Sans and IBM Plex Mono, and the swap is measured rather than tasted.** Inter is a common IDE interface face, and the mock this window is drawn against uses it. The number that decides it is x-height: Inter's is 0.5459em against Plex Sans's 0.5160em — **six per cent more ink at the same nominal size** — which is why Plex reads thin in a panel as narrow as a file tree, and why the obvious fix of raising the size is the wrong one: matching Inter's x-height would need 13.76px and overshoot the mock's cap height by five per cent. The family was the bug.

**One thing it changes silently:** Plex's digits are tabular by default and Inter's are proportional. Anywhere a column of figures must not jitter — a log's dates, a diff's line numbers — the face has to be told, or the column dances as the digits change.

**The terminal and code are two settings.** A `powerlevel10k` prompt is drawn with Nerd Font glyphs and is a row of boxes without them, so the terminal's face is Lilex Nerd Font; a file has no such glyphs, so code's is plain Lilex. Lilex Duo is offered to code alone: it has no Nerd glyphs, and its wide letters take a cell and a half, which a grid cannot draw.

| Role | Size | Weight | Tracking |
|---|---|---|---|
| Section heading (desktop) | 20 | 600 | −0.02em |
| Banner heading | 15 | 600 | −0.01em |
| Card heading | 13.5 | 500 | −0.005em |
| Body text | 13 | 400–450 | 0 |
| Interface text, code, tree | 12.5 | 400–500 | 0 |
| Metadata | 11.5 | 400 | 0 |
| Rubric (UPPERCASE) | 11 | 600 | +0.07em |

Line height: 1.45 for text, **1.70 for code** (checked — 1.78 does not fit into the editor's 24 lines), **16/12 in the terminal** — a 12-point face on 16-point lines, taken on 2026-09-24 because a terminal drawn that way fits more lines.

---

## 4. Shape and depth

The terminal content inset is **5 points vertically and 13 horizontally** (`theme::terminal::PADDING_Y` / `PADDING_X`). Grid fit, painting, pointer coordinates, selection edges and scrollbar share this rectangle. At 16-point leading this recovers one complete row compared with the former 13-point inset on every side; font metrics and program-emitted blank rows are unchanged. Area-card borders and split-pane hairlines remain outside this inset.

**Corner radii:** 6 — tree and list rows · 7–9 — cards, buttons, badges · 11 — the application window · 20+ — pills.

The project column's resize edge follows the cursor-only rule of every other
handle. Its **9-point target is centered on the visible boundary**, not wholly
inside the column, and keeps its full height; its width is
`space::COLUMN_RESIZE_REACH`, in logical points. Since 2026-11-26 (user
correction) it **draws nothing at rest, on hover or during a drag** — the resize
cursor is the only sign; from 2026-10-23 until then a 1-point line lit under the
hand. The pinned grip sits above the work surface and below deferred popups,
ends before the footer, and follows the same boundary in the folded peek.

### 4.1 The work is a card — M12.6

Since M12.6, in every palette, **the work is a card on the window's colour**, a layout measured on 2026-09-16.

- **The window's colour.** The title strip, the column, the status bar and the ground between the cards are `surface.panel`.
- **The top card, one per area — M12.8, 2026-09-29.** The same card, once for each of a seat's areas, the areas 6 apart on `surface.panel` — the gap is the border between them, and its grip over it draws nothing, at rest or under the hand: the resize cursor is the only sign, and a double click puts the split back at a half (6.36). **The side docks are no longer inside it**: each is a card of its own in the bottom dock's shape, 6 from the nearest area, so a window with no split at all is one area card with a dock card at either side — a change a person sees without dividing anything. ~~The top card holds the side docks, the tab strip and what a tab shows.~~ It holds the tab strip and what a tab shows (2026-09-29; before M12.8 it held the side docks too). The rest of this bullet describes the card as it was measured.
- **The top card, as measured.** It holds the tab strip and what a tab shows. It is `surface.app` with a 1 border in `divider` and a radius of 14, and it touches the column on its left. It is inset 6 from the window's right. It ends 6 above the status bar, and 6 above the bottom dock's card — **every gap between the window's black areas is one width**, across as down (2026-09-18; it was 10 between the cards until the person asked for them all alike and the thinner).
- **A card draws its corners and its border over what it holds — M8.13.** GPUI clips a child to the card's *rectangle*, not to its rounded shape, so a child that grounds itself to the edge — a file, a terminal's cells, a diff's rows — painted the corner square, three times in one day. The card's border and a band of the window's colour outside the arc are drawn last, over everything, by `view::panels::card_corners`; the card keeps a point of padding where its border was. Checked on all eight corners of both cards.
- **The bottom dock is a card of its own** in the same shape, inset 6 on both sides. M12.2 made it run under the column, and it still does.
- **A side dock's grip hangs over its edge too — 2026-09-18.** It stood beside the panel, nine points wide with its line at the far edge, so every side dock carried nine empty points and what was centred in it sat left of centre; the person found it under the commit box. The line is the work's first point, the reach is shared by both sides, and the grip is **the last child of the row that holds the work**, so it is drawn after the work and occludes it — a press on it never reaches the editor — and before every popup. It was `deferred` for an hour, and its line then cut through Find in Files.
- **The grip between the two cards lies over the gap** and takes no room of its own. The hand still gets its 9, reaching a point and a half into each card over a gap of 6.
- **The status bar runs under both cards**, 28 high: its buttons (22) sit on its foot, 6 above the window's edge and 6 below the card above, and 6 in from the side — so the corner's button is as far from the bottom as from the side and centred between the card and the edge.
- **The column's two gestures live on its head**, before the button that hides it: *Open…* and *New Worktree…*, 20 square at radius 6, 2 apart. In the column's foot they floated above the bottom dock's card, and in the status bar they were far from the list they act on — the person moved them twice.
- **Thin gaps, by the person's word** (2026-09-16). The layout it was measured from stands its card 14 from the right and leaves 46 under it; the person found the strips between the areas ate the work, so every gap here is 6.
- **No hairline.** Nothing between the title strip, the column and the cards is a hairline: the step from `surface.panel` to `surface.app` is the separation. A dock's grip draws nothing at rest and **nothing under the hand either** — the resize cursor is the only sign, on every handle that sizes something (2026-09-18; it lit `accent.patina` until the person asked for the cursor alone). This retires rule 2's hairline between the window's areas (M12.4); inside a card, a side dock still meets the work at a hairline.

| Measure | Value |
|---|---|
| the card's radius | 14 |
| the card's inset from the window's right, and the bottom dock's from both sides | 6 |
| the gap under a card | 6 |
| the gap between the two cards | 6 |
| the status bar's height | 28 |
| the tab strip's height | 36 |

**Shadows and outlines:**

| Device | Value | Where |
|---|---|---|
| Window | `0 34px 90px rgba(0,0,0,.62)` + `0 0 0 1px rgba(255,255,255,.055)` | the application frame |
| Active card | `inset 0 0 0 1px rgba(<accent>, .22–.24)` | an active card |
| Accent stripe | `inset 2px 0 0 <accent>` | a comment thread, a line with an error, a remark in a list |
| Indicator halo | `0 0 0 3px rgba(<accent>, .14–.16)` | the status dot on the desktop |

The halo around the dot is the device that enlivens the indicators the most. Do not skip it.

**The mockup canvas backdrop:** `radial-gradient(120% 90% at 50% 0%, #1b1b21 0%, #0a0a0c 60%, #08080a 100%)`. Not needed in the application — it is a presentation background.

---

## 5. Sizes and spacing

| Element | Size |
|---|---|
| Window title strip | 34 |
| Title bar offset | the strip 34, 2 above its contents, the traffic lights' frame at 11 — its contents a point below the centre line, 1 |
| Status bar tool button | 24 (icon 16, radius 6, gap 2, groups 12 apart) |
| Menu | 32 row (card at least 180 wide, padding 5, text inset 14, icon 16 then 6, separator 6 above and below, inset 8, radius 8, text 13) |
| Picker row | 28, the branch popup's own 44 — on a menu's own card |
| Update branch sources | 300 wide, rows 32, its list up to 224 |
| File tree | 230 |
| Changed-files panel | 236 |
| Task context panel | 294–306 |
| Active sessions panel | 330 |
| Section header | 64 |
| Tab strip | 42 |
| Panel heading | 42 — the tab strip's height: the heading and the tabs are one line, and a panel starts where the strip starts |
| Agent event feed | 198 |
| Panel padding | 18 |
| Board canvas padding | 26 |
| Gap between board columns | 20 |
| Gap between cards | 8 |
| Card inner padding | 13–14 |
| Gaps inside a card | 10 |
| Indent step in the tree | 18 |
| Search Everywhere popup | 680 wide, its list up to 440 |
| Find in Files popup | 760 wide, its list up to 280 |
| A popup's preview | 220 high, under the list |
| Language server state | detail cut at 240, a bar 48 by 3, its list 360 wide |
| Update control | 24 high; 7 either side; card surface, divider outline, neutral interaction hover |
| Release notes | the card over Update 360 wide with at most 8 entries; What's new 460 wide, its list up to 440 |
| Title bar tools | the usage chip 18 high, 6 either side, a disc of 7; its card 360 wide, a window's rounded bar 5 high, one-line name/share/reset rows; Open In a split capsule 48×24 |
| Start screen | a column at most 560 wide; the default card 60 high with a disc of 32; a tile 84 high, 8 apart; a row 32 |
| Area and pane | at least 320 across and 160 down |
| Pane outline | radius 6, 2 in |
| Start screen, compact | Recent from 440 high, the foot from 520 |

**A border is three pixels wide and nine to the hand.** Every handle in
the window — a panel's border, a column's, the message block's — draws a
three-pixel line and takes nine: the line is what the eye needs, and nine
is what a hand needs to catch it. Three is a target a pointer crosses
between two of its own samples, and a point on an external screen is
physically smaller than on the laptop's, which is where the resize cursor
was first reported missing. The extra six come out of the regions beside
the border rather than out of the border, so nothing looks wider; the line
colours on `accent.patina` when the *reach* is under the pointer, not the
line.

**Those are the widths a panel opens at, not the ones it is held to.** A
panel is dragged, and since M8.3.4 **its ceiling is the window rather than
a number**: it may take everything but 120 pixels, which is enough that
the tab strip and a line of what it holds stay on screen, so the way back
is reachable. M5.7's fixed 560 and 340 read as generous until somebody
wanted the Log at the top of a tall screen — a number cannot know how tall
the screen is. The floors stay: 160 across, 90 down, because a panel
dragged to nothing looks exactly like one that failed to open.

---

## 6. Component anatomy

### 6.3 Session row
No container at all. A 7×7 dot with a halo on the left, two levels of text on the right: a 13.5/500 heading and an 11.5 `mono` line in `text.muted` (branch · time · cost). The stop button is a 15×15 icon in `text.faint`. Below it, when needed, a sunken `surface.inset` block with a radius of 7 and the feed of tool calls.

**In the session list since M5.10** a row says what its tab says, drawn by the same two functions: the kind's mark retains its harness identity, and after the mode's label the dot of section 6.5 — rust while blocked, green while a turn is finished and unread.

### 6.4 File tree row
Height 22 (square edge-to-edge grounds), the interface face at 12.5 — `mono` until decision 54. **The row and the indent step of 18 were measured on 2026-09-18 — M8.10.** They were 26 and 22; the measured row and indentation remain unchanged. The former outer tree inset belongs inside each row (`TREE_PADDING + TREE_INSET`), so icons and text keep their positions while selection, status and hover grounds reach both viewport edges. **The row's look — decision 54, 2026-11-21.** The selection is one solid fill with no outline and priority over the subtle Git tint: `accent.patina_bg` while the tree or its own menu has the keys, `color.state.selection_inactive` (primary ink at `alpha::SELECTION_INACTIVE`, 14%, over the ground) while they are elsewhere. An unselected row under the pointer takes `alpha::ROW_HOVER` (9%) of primary ink over what it shows, its Git tint included (`color.state.row_hover` over the bare ground); a selected row keeps its fill. The pointer is the arrow. The keyboard's cursor has no mark: the arrows and a speed-search match move the selection. The right button selects its row before the menu, or keeps a group the row is already in. A folder a drag will land in takes `accent.brass_bg`. From left to right: the disclosure arrow, 11×11 in `text.faint`, on a directory only; **the icon, 16×16**; the name. Git status is a full-row tint rather than a letter: brass for Modified, green for Added, rust for Deleted and primary ink for Untracked; a directory carries the strongest status below it. See the 2026-11-04 Git tree grounds section for tokens and shared-host coverage.

**A directory whose only child is a directory is one row with it — M8.10.** `com/example/inaction` reads as one row, its names joined by `/`. Guild folds everywhere, not only under a source root — it has none — so `crates/desktop/src` folds too. The row carries **two paths**: it is *called* by the deepest, where *New File…* and the other deeds aim, and it *folds* from the topmost, so that closing it takes the whole chain. **The selection follows the row when a fold renames it** — expanding `cfg` turns its row into `cfg/guild` and collapsing turns it back — and lands on the nearest drawn ancestor of anything folded away; without that the selection named no row and the next arrow started from the top.

**The first row is the project's root** — its directory's name, a folder icon, expanded — and everything else sits one level in; it folds like any directory (first use of M8.1). **Since M8.8.3 it is not the only row at that depth**: *External Libraries* sits after the project's rows, once a `f12` has landed in a dependency — section 6.27. And the project's row is the one row of the tree with **no** *Rename…* and no *Delete* in its menu: its path is the empty one, and both gestures would act on the directory the window has open. The panel's heading reads *Explorer*, and at its right end sits *Reveal Active File*: Lucide's crosshair at 14 in `text.secondary`, `text.faint` when the tab in front is not a file. It opens the way to the file in front, selects its row and focuses the tree.

**The icon — added 2026-09-02, M8.1; its source since M8.13.** The Material Icon Theme's (material-extensions/vscode-material-icon-theme, MIT), **the published extension pinned by version** — 1131 SVGs and the theme's own generated table of associations — so that a `.rs` looks like what people already know a `.rs` looks like. Until M8.13 it was a 191-file subset, and eleven of Guild's languages drew the generic file. A file's rule: a whole name beats an extension, the longest suffix wins, and **then the file's language**, which is how the theme itself reaches `.ts`, `.js`, `.yml`, `.php` and `.zshrc` — by the language's name, else by its canonical suffix; an unmatched name draws the generic file. The light variant where the set has one — fifty-two stems, none of them a folder.

**A directory says what it is for, not what it is called — M8.12.** Four icons, a rule read off the path: `src`, `src/main/<language>` and a module's own `src` are the source-root folder, `src/test/<language>` and `tests` the test folder, `resources` the resources folder, everything else the plain folder; each with its open variant when expanded.

**A Kotlin or Java file draws what it declares — M8.13.** The IntelliJ platform's node icons, under Apache-2.0 (`assets/nodes` and its `PROVENANCE`): class, abstract class, interface, enum, annotation and record, the `_dark` file on the five dark palettes. Kotlin's `data class` wears record and its `object` wears class. The declaration is the one named after the file, else the only one; a file of several, or of functions, keeps its language's icon — which is also what every such row wears until its file has been read.

Drawn in their own colours, not tinted; an ignored entry takes the icon at half opacity with its dimmed name. The set of our own that section 1 would have asked for is a month of drawing before a `.rs` reads as one, and that is the trade.

### 6.5 Tab
**Underlined, not filled — since M12.6.**
- **No tab has a background.** The one in front has its name in `text.primary` and a bar 2 high in `accent.patina` along its whole foot. The others are in `text.tertiary`, and `text.primary` under the pointer.
- **The strip's bottom boundary — 2026-10-18.** Each area has a continuous one-point `surface.divider` across the entire strip, including empty space and fixed actions. It is painted inside the existing 36-point height, behind the active underline, without changing control geometry. It meets Settings navigation without a gap; Settings retains `theme::settings::outline`, so the two boundaries may have different shades.
- **A tab's shape.** It runs the strip's height (36), stands in 12 at its sides, keeps 9 between its mark and its name at `body` 13, and is at most 270 wide, its name cut short with an ellipsis. Its cross shows only under the pointer, **laid over the tab's end** on the card's ground rather than kept in the row, where its width came off every name.
- **The add button follows the last tab.** First a rule 1 × 20 in `divider`, then Claude's mark at 14 with a `+` at 9 in `text.tertiary` over its top right, in a 22 square. A click opens a Claude Code session, as `cmd-t` does. Beside it, a chevron at 9 in a button 14 wide, tooltipped *New Tab…* while its menu is not up, opens **the harness menu** (6.17) — a `Choice` per kind of tab, Claude Code, OpenCode, Codex, then the terminal, each with its own mark and no current one, so the menu carries no check column. A harness not on the `PATH` is greyed and the keyboard steps over it; the terminal is never greyed, needing nothing beyond the shell the window already runs.
- **The unsaved-changes dot** is 5×5, to the right of the name.

**What the agent is doing — updated 2026-10-15.** Claude, Codex and OpenCode tabs and session rows use `view::activity::mark`, shared with worktrees: a 14-point neutral rotating ring while working, a 16-point rust question while waiting on permission, a question or an `openDiff` proposal, a 16-point rust exclamation for observed failure, and a plain 8-point green dot for a finished-unread answer. Idle has no indicator. Harness marks keep their full brand colour without brightness blinking. Visible working rings continue when the application is inactive; clipping and reduced motion follow section 6.29, independently of caret timing. Tabs preserve their loudest-pane status and close-hover behavior. A file tab shows its icon at 16 before the name, where a session tab shows its kind mark. A diff tab shows `±` in `text.tertiary` there — the two markers of a diff in one sign, quiet like the settings' cog (M8.2) — and is named after its file, or *Diff · N files* for the working tree.

**Release feedback — 2026-10-02.** An accepted tab drop settles from the
pointer's preview position into its final slot over **180 ms**, cubic ease-out
(`1 - (1 - t)^3`), with no bounce. The tab briefly takes `accent.patina` at
**16%** opacity, fading linearly to transparent over **360 ms**. The same
feedback confirms a drop on a tab, trailing strip space or an empty area,
including a position already reached during the live drag. Only the released
tab animates; its neighbours keep their final layout. A new drag interrupts
previous feedback. Starting a drag selects the dragged tab and focuses its
contents, even when it was in the background; selection happens at the drag
threshold, before crossing a neighbour. The active underline follows it
through the move and release. A right-click still leaves selection unchanged.
`theme::motion::{TAB_SETTLE, TAB_FEEDBACK, TAB_TINT}` carry these values.

### 6.7 Buttons
- **Primary:** background `accent.brass`, text `#141414` at weight 600, radius 8–9, vertical padding 9–11.
- **Secondary:** background `surface.raised`, text `text.secondary`.
- **The commit box's button — 2026-09-18.** The person found the full primary out of keeping with the window: a slab of brass in bold, the loudest thing in a panel of greys. It takes the head's quiet shape instead — 72×28 at least, radius 8 — with the label at regular weight: `accent.brass` on `accent.brass_bg` while a commit can be made, `text.faint` on nothing with the reason as its tooltip while it cannot.

| Commit button | 28 high, at least 72 wide, radius 14, 10 either side |
- **Confirming:** the patina backdrop as the background, text `accent.patina`.
- **Where one ink on brass does not hold — M5.16.** `#141414` on Light's brass (`#8a5a00`) reads 3.1 : 1, under the 4.5 text needs; a primary button drawn in Light needs another ink.
- **In a head — since M12.3.** 24 high at radius 6 with 10 either side, the label at `ui` 12.5 weight 500: `accent.brass` on `accent.brass_bg` while it can be pressed, `text.faint` on nothing while it cannot. A primary button's full height and fill belong to a place with room around it, like the commit box; in a 42-point head beside a line of text it was the heaviest thing in the window, and the person said so of the Push tab's.

### 6.8 Counter badge
Background `surface.badge`, radius 9, padding 1/6–7, size 11/500. The accent version uses the accent's backdrop and the accent itself for the text.

**A project tab's count takes the accent of its most urgent agent — M5.10.** `accent.rust_bg` and `accent.rust` while any session in the project is blocked on the person; `accent.green` on the plain `surface.badge` while any has finished a turn nobody has read, because green has no backdrop token and one badge is not worth a token in every palette; the neutral badge otherwise. Most urgent wins: blocked over finished over working.

---

### 6.9 Title bar

The Workspaces toggle follows the native traffic lights, using the greater of
82 physical points and scaled TRAFFIC_LIGHTS. In fullscreen it uses scaled
WORK_INSET. AppKit retains its native top-edge hover reveal, including all three
traffic-light buttons; Guild never hides their container. Retracting that native
bar leaves Guild's toggle and arrows usable. The toggle pins a peeking column.

The usage and release-notes cards share §6.17’s `card_outline` outer border;
the chips that open them keep their existing outline colours.

Height 34. **Its contents stand a point below the centre line — 2026-09-25, the person's optical centring**: measured exactly centred — 6 above and 6 below a 22 button — they read as high, between the window's light top edge and the darker card under the bar with its rounded corner; 2 above them and the traffic lights' 14-high frame at 11 put everything on one line at 18. **Since M12.10 the bar starts with the branch** — the project's popup trigger left it on the person's word, the branch beside it naming the project nearly always; switching stays with the column and Recent Projects. Until then: the project's name at `size.card_title` (13.5) and weight 600 — first in the bar by weight, not by size: at 16 it read as a heading over a row of labels rather than as one of them (found in the first use of M8.1). The branch beside it at `size.meta` in `text.tertiary`. At the right end, the three layout buttons — a rectangle with the edge filled that names its panel, kept together because three controls of one kind read as a group; they sit in the title bar because they govern the whole frame. Under the bar, the row of projects; under that, the panels and the tab column side by side.

**The Update control — revised 2026-10-08.** Immediately after history at the main panel's upper edge, and only while a nightly release is offered. It uses the shared titlebar anatomy: `STATUS_BUTTON` height, `PILL_PADDING`, divider border, `rounded_full`, card surface and neutral interaction hover; meta text at medium weight. `theme::update_ink` uses brass when it reaches 4.5:1 on both surfaces, otherwise the palette's primary text. The release-notes card and restart gesture remain unchanged.

Hovering it shows what the update brings: a card on a menu's look, *Update to <version>* over the entries grouped under a menu's headings, at most 8 and then *and N more* in a menu's note, 360 wide, a line wrapping rather than cut. After the restart, *What's new* — the person's mockup of 2026-09-25: Go to Line's dimmed backdrop and placement, a card 460 wide at radius 8; a head of *What's new in Guild* at `title` 16, weight 600, over *You were on <version> · N releases since* at `meta` in `text.faint`, a rule; **a block per release**, newest first — its name at `body` 13 weight 500 with its date at the right in `text.faint`, then its groups as above at `body` 13, 18 in from the card's edge — in a list that scrolls past 440; a rule, and a foot with *escape* in `text.faint` at the left and *Continue* at the right, 6.7's button in a head, brass on its ground. Esc, *Continue* or a press on the backdrop put it away; a press on the card does not, since its list scrolls.


**The usage chip and Open In — M12.10, decision 46.** Two functions in the bar's own shapes. At the right end, 12 before the buttons, **the usage chip**: the branch button's height and radius, 6 either side, the lowest share left of Claude Code's windows at `meta` in `text.tertiary` with tabular figures, and before it a disc of 7 **only under 50** — `accent.brass`, `accent.rust` under 20 — the Problems button's rule that nothing is drawn to say things are as they should be; the neutral interaction fill under the pointer and `surface.raised` while its card is up; on hover its share or unavailable dash brightens to `text.primary`, returning to `text.tertiary` or `text.faint` on leave; provider marks and level discs keep their colors; while its card is open, the chip stays above the fullscreen dismiss layer for hover and second-click toggle, with outside clicks intercepted rather than passed to panels; no chip without Claude Code's credentials. **Its card** keeps its 360-wide menu frame, enough for Guild’s translated reset labels. The head is the harness mark and name at `body` in `text.primary`. Every provider uses the same window shape: the short name in `text.secondary`, the remaining share in its level colour, and *resets in …* in `text.faint` on one line at `meta`; underneath, a fully rounded 5-high bar on `surface.raised`. Share and fill are `accent.green` at 50 or above, `accent.brass` from 20 to 49, `accent.rust` below 20. The chip’s quieter disc rules stay unchanged. An inset menu separator precedes *All providers*, a secondary row with a chevron that folds the other providers' heads and windows in their existing order. The current provider appears only above the fold; changing the focused session updates both sections together, and a file, shell or empty pane retains the existing remembered-provider fallback. A second separator precedes the footer: `refresh_detail` at `meta` in `text.faint` on the left, plain secondary *Refresh* on the right without an icon. Asking, rate-limited, failed, current network, session-derived and outdated states retain their previous words; only Refresh triggers the request. The card is capped at the viewport height minus two 8-point window margins. Its heads/windows/provider fold scroll as one content region; the footer and its preceding rule stay outside scrolling and never shrink. **Open In** is a 48×24 split capsule: the selected system application icon at 16 points and a separate chevron, in the existing outlined titlebar style. With no project the frame stays and the control rests as the history arrows do: its glyphs in `text.faint`, an application's own icon at the faint-mark opacity 0.35, no hover and no cursor. The primary click / Cmd+O opens the current project/worktree in the selected destination; the chevron / Cmd+Shift+O opens a menu of the engine (6.17). Both shared compact hints show effective current bindings and are suppressed while the menu is open. The typed selected destination persists across restart; Finder is the initial default and unavailable-app fallback. Open In offers external apps only; legacy Terminal resolves to configured external Terminal.app or Finder. Copy Path does not change selection. Keyboard Open In immediately selects the first enabled action, while mouse opening retains hover selection. Add Project and Go to File retain menu/custom-keymap access without replacement defaults. The menu contains *Open In* as its heading, Finder, the installed external applications of a fixed list with their own icons, Copy Path, and the folder as its footnote.
### 6.10 Diff row

The shared read-only diff uses `TREE_ROW`, the configured code face/size, `surface.app` for context, `diff.added`/`diff.removed` for unified changed rows and semantic green/rust markers. The split center uses the dedicated semantic table below for whole added/removed/modified ranges, including number gutters and empty alignment ranges. Syntax follows section 2.4. Intraline replacements use the side's accent at .24 opacity; selection uses patina at .35 beneath the glyphs. Diff number gutters measure the maximum source line-count digits across both sides of the full projection in the current code font, plus identical half-CARD_GAP padding on each edge. They share one cached width for rows, clips, tracks, divider minima and underlay, stable across scrolling/folding and refreshed on font/digit changes; widest-digit measurement also covers non-tabular fonts. `DIFF_NUMBERS` remains a legacy fallback outside this measured diff. Gutters use center number ink and stay fixed during horizontal scrolling. In split mode old numbers align right and new numbers left, with identical half-CARD_GAP padding toward the optional blame/connector; unified retains right alignment. In side-by-side view old numbers follow old code and precede optional blame; new numbers follow optional blame and precede new code. An explicit TREE_GAP * 2 click strip lies between each source marker and its numbers, symmetrically outside horizontal translation. Left or right click opens the existing checked Annotate with Git Blame menu; continuous outer separators frame these strips. Source markers remain next to their code. Unified rows retain their leading old/new number pair.

**Fixed chrome.** A toolbar on `surface.panel`, at least `DIFF_TOOLBAR` high with a bottom `surface.divider` border, uses constant-width counters (`HEAD_BUTTON * 2`) and compact icon controls. It may wrap when the viewport narrows; changes in file/count/loading state never resize its slots. All action/mode/file controls have localized Guild hover hints and accessibility names, including disabled navigation boundaries. Previous/next change uses arrow-up/down; previous/next file uses chevrons; workspace-source uses the existing pencil (`menu-rename`). The right-aligned mode pair uses existing `columns-2` and `tool-stop` (Lucide square), with selected state and labelled hints.

The clickable file counter opens a shared FilesTree picker (`popup::card`, `BRANCH_POPUP`, `PICKER_ROW`, capped `RECENT_FILES_LIST`) over an occluding backdrop. It includes the complete inspection order, folder disclosure, current-file accent/check, keyboard/pointer selection and scroll; Escape closes only the picker and returns focus to the viewer. Ordinary multi-file diffs use the same picker locally. Historical selection preserves its exact revision and order.

A distinct fixed `TREE_ROW` revision strip on `surface.sunken` has a bottom divider in either mode. Before/After labels and truncated paths use `text.secondary`; real compact SHA values are separate in `accent.patina` and the configured code face. Loading/Absent/Unavailable and non-hash revision names stay ordinary secondary text. Full exact revision/path identities remain in tooltips, including renamed/absent sides and combined first-before/last-after metadata. Pending pairs say Loading rather than Absent. HEAD/Index/Working tree/Current/Proposed keep their source meanings. File transitions reuse the same diff surface, retain mode/divider, clear prior text before the new identity is displayed and reject stale asynchronous answers.

**One vertical document.** Source rows, blank alignment cells, unchanged-context folds and change-block geometry share one virtual list. Expanding a fold reveals the complete omitted prefix, gap or suffix without changing the difference count. No-changes files retain their status and offer full context. Empty, loading, failed, binary and size-limited states retain their existing meanings. The right rail reserves two `MARK_BAR` lanes: full displayed-block ranges (green insertion, rust deletion, brass replacement; minimum `MARK` height) and the independent usable vertical scrollbar. Clicking a range reveals the same logical block used by previous/next change. Projection, folds and mode determine its range; legacy hunk headers do not.

**Two columns.** Side by side is the default. Each source has its own horizontal position and bottom scrollbar, with trackpad and Shift-wheel support. Headers, gutters and the connector lane do not translate. The lane is `DIFF_CONNECTOR` (24 px) wide. Cubic ribbons join the true old/new start/end extents, with horizontal end tangents and the same opaque semantic fill as the matching code/number range. A zero-length side is a boundary anchor, not a full invented source range. Empty aligned cells use the dedicated empty fill. Continuous one-point `surface.divider` rules at both code/number boundaries extend through the viewport below the source rows. The viewport owns one full-height `surface.app` gutter background matching plain number cells; per-row ribbons are transparent outside their changed range and do not own a contrasting strip. File headers and context/EOF/status rows own opaque row surfaces above this underlay, masking rules beneath labels. Shapes, palette, order and widths are unchanged by this layering correction. It also owns the drag gesture. The revision strip's focusable divider accepts arrow-key adjustment. Divider limits reserve gutter width and approximately four code characters where the viewport permits; resizing clamps the transient fraction and horizontal positions. Unified has one horizontal position and no connector lane. No new application minimum width or persistent layout setting is introduced.

**Optional authors.** The gutter context menu reuses the editor’s localized checked Annotate with Git Blame item independently on each side, hidden by default, with a discoverable right-click hint. Date/author labels retain FileTab’s inherited UI face at `META` and `CARD_GAP` padding; metadata uses the dedicated subdued blue-gray ink and explicit normal weight. Fixed order: old code → old click strip → old numbers → optional old blame → central connectors → optional new blame → new numbers → new click strip → new code; source markers sit immediately with code. Numbers keep the code face, half `CARD_GAP` padding and `surface.app` background. Annotation columns use chronological blue shading from the dedicated table, superseding the rejected neutral surfaces/white text. Existing Annotate timestamp ranks are computed from known timestamps across the file’s two retained sources; equal timestamps get equal blue. Shade 0/1/2 maps to newest/middle/old blue. Unknown/padding retains the base old-blue background without attribution. Ranks update only when source attribution changes, not while scrolling. No new calendar threshold or setting is introduced. Measure full-file unique rendered labels using the shared editor text-width helper; cache by source/status/font/size/language and keep a stable width while scrolling. `ANNOTATE_COLUMN` is a maximum, not a reserved column. Multi-file projections take the side’s maximum measured file width to retain common horizontal geometry. Shrink annotations before clipping code at narrow widths; full blame details stay in tooltips. Historical attribution queries the exact revision and repository path shown on that side, including renamed paths and combined commits. Loading/unavailable states are explicit; alignment padding is blank and uncommitted/proposed text has no invented author. Opt-in survives file stepping and mode switches without a persistent setting. Fixed gutters and horizontal scrollbar insets share the same source geometry.

**Split-center semantic tokens (2026-10-25 authoritative OFF/ON reference).** `theme/diff_center.rs` owns these scoped values; widgets contain no color literals. Dark uses the reference-compatible table; Light uses its light counterpart. Existing unified/legacy diff tints remain unchanged.

| Token | Dark | Light |
|---|---|---|
| added range | `#264536` | `#dcecdf` |
| removed range | `#454849` | `#d5d8da` |
| modified range | `#243746` | `#dce8f3` |
| empty aligned range | `#17191d` | `#ebedf0` |
| blame newest / shade 0 | `#355b95` | `#aec9ef` |
| blame middle / shade 1 | `#304d79` | `#c4d7f0` |
| blame oldest / shade 2 | `#2a3c59` | `#dbe5f3` |
| metadata ink | `#a7b5ce` | `#425675` |
| number ink | `#717884` | `#68788b` |

Real Rust-render evidence is produced by the opt-in `diff-center-render` example using the existing public GPUI HeadlessAppContext/Metal readback and actual commit blobs/blame. Its OFF/ON PNGs and pixel relationships are offscreen evidence, not native-window acceptance. An artboard alone is not rendering proof.

**Source selection.** A gesture belongs to one source, including across unchanged unified rows. Shaped text and Foundation's composed-character boundaries map the pointer to original UTF-8 ranges; CRLF stays indivisible. Copy includes original source newlines and intervening folded text, without gutter numbers, markers or alignment padding. Copy, Select All, keyboard extension, word/line selection and bounded drag scrolling are read-only. Source replacement invalidates stale selection; view/context changes retain valid source ranges. Explicit source controls and gutter double-click navigate to the workspace; text selection does not open a file. Proposal answers retain their existing IDE ownership and never appear for historical inspection.

### 6.11 Tool buttons in the status bar

The branch glyph ink keeps the unclamped/pinned Back glyph's inset from the
work-card corner, in both pinned and folded states. The footer stays at the card
edge and never follows the title-bar clamp after the traffic lights and toggle.
Measure the arrow's stroked SVG extent and the branch glyph's shaped-font bearing;
matching hitbox edges alone is insufficient. This holds for compact rows and UI scales.
Explorer without Git keeps the work-card inset.
Taken on 2026-09-03 in M8.3.1 in place of the rail M8.3 built from the mockups: the person asked for the buttons that open the tools in the fixed strip at the bottom, and the 52 pixels the rail took come back to the panel. The status bar was 26 high until M12.6 and is 28 since (section 4.1): its buttons sit on its foot, 6 from the card above, from the window's edge and from the side. Since M12.6 it has no hairline, and before that it was **on `surface.panel` with a 1-point `surface.divider` above it** — M12's task 12: it was `surface.sunken`, a third grey under a strip that does the same work as the panels beside it, and it read as something bolted onto the window rather than part of it. The frame is one surface now — the column, the docks, the tab strip and this strip — and a hairline is the only thing that divides them; what a tab draws inside itself keeps its own ground, because that is the work and not the frame. At its left, the column's button, then — before the path — the left dock's tools as buttons 22 square at radius 6, 4 apart. Beside the bottom dock's tools, **Push Commits' ↑** is a button of its own since M8.3.6, shown while a project is open. The tools' buttons: the tool's icon at 14 in `text.tertiary`, `text.primary` on `surface.raised` for the one whose dock is open on it, `surface.raised` under the pointer. At its right, before the sessions counter, the bottom dock's tools and then the right dock's, the two groups 12 apart. A dock with no tools has no group. The icons are Lucide's `folder`, `git-commit-horizontal`, `history` and `square-terminal` under the licence beside the chevrons. A button's tooltip names the tool and its stroke. Pressing the lit button closes its dock; `cmd-b`, `cmd-j` and `cmd-l` open and close the docks whatever they show. A button is also the handle a tool is dragged by, section 6.15. The groups are 12 apart and the buttons within a group 4, and the sixth icon is Lucide's `circle-alert` for Problems.

**The Problems button is the count, since M8.8.5** — decision 14, one control and not two. While the project has an error or a warning the button stops being square, takes the card's padding, and draws after its icon a disc of the log's size in `accent.rust` while anything is an error and `accent.brass` when the worst of it is a warning, then the number in `text.faint` at `eyebrow` 11: `12 · 3` for errors and warnings together, one figure when only one kind is there. **The number and its colour are two tokens deliberately** — merged into one they say a third thing that is false, which the window proved: a coloured *117* beside a coloured *58* reads as two counts of errors that disagree, when one was counting everything and the other errors and warnings. Nothing is drawn while the project is clean: a button that says *0* is a person being told that things are as they should be.

**The sessions counter says who is waiting, since M5.10**, in the same shape and for the same reason: after *sessions N*, a disc 7 across in `accent.rust` and the number of agents blocked on the person, then a disc in `accent.green` and the number of turns finished and unread, each number in `text.faint`. A pair at zero is not drawn.

**The facts about the file are at the right end, since M8.8.5** — request 14: the path stays at the left, and the file's own facts go to the right. Reading outwards from the right edge: the sessions counter, the right dock's tools, the bottom dock's, then `line:column`, the line endings, the language, and — to the **left** of all of it — the server's state. The order is not taste. `line:column` changes on every keystroke, so it sits in a fixed 36-wide column and never moves its neighbours; the server's state comes and goes, so it is put where it grows into the empty middle instead of pushing the fixed things about. The language is in `text.muted`, *Plain text* where nothing claims it, and it is here because it is the only way to notice a file taken for the *wrong* one — `lock` is read by four different grammars and `.ini` by TOML's. The endings are `LF` or `CRLF` as a **label and not a menu**: changing a file's separators is a write to that file, and nobody asked for one; the tooltip says so. `line:column` counts from one, as every editor a person has used does, and a click on it is `cmd-g` — the widget presses the key rather than owning a second route to the same popup; `cmd-l` is the right dock's. **None of the three is drawn when the tab in front is not a file**: a bar saying `1:1` over a terminal is answering a question nobody asked.

### 6.12 Commit panel
The Commit tool window, since M8.3.1, wherever its dock is. From the top: the tool's head; a toolbar 42 high with six icon buttons 24 square at radius 6 — refresh, the selected row's diff, tick all, untick all, collapse all, expand all — Lucide's `refresh-cw`, `file-diff`, `check-check`, `square`, `chevrons-down-up`, `chevrons-up-down` at 14 in `text.tertiary`, `surface.raised` under the pointer, each with a tooltip naming its action and stroke; then the list, which takes the height; then, under a 1-pixel `divider`, the counter line — *N modified* at `meta` 11 in `text.faint` — the message box, and the *Commit* button.

**The list** has two sections, *Changes* — every tracked file with a change, one row whatever git has staged of it — and *Unversioned files*, each a head row 26 high: a chevron 11 in `text.faint` that folds the section, a checkbox, the name in `text.secondary` at `ui` 12.5, the count in `text.faint`; an empty section is not drawn, and a clean tree is one line of `text.faint` saying so. Under a head, one row per file in section 6.4's shape one indent in, with a checkbox before the icon and, after the name, the directory it is in — or `← old/path` for a staged rename — in `text.faint`, `mono` 12.5, clipped at the right; the mark at the edge as in the tree. **The checkbox** is 14 square at radius 3: a 1-pixel hairline in `text.faint` when empty, `accent.brass` with Lucide's `check` at 10 in `surface.panel` when ticked. Ticked means "in the commit": a tracked change is ticked when it arrives, an unversioned file is not, a conflict never, and the ticks are the panel's own until the commit, which brings the index to them — decision 4 of M8.3.1. The head's box does the whole section. `space` on the selected row is its box, `enter` opens its diff, the arrows walk both sections' rows.

**The commit box** has no border of its own to drag, deliberately: it is the editor component in plain mode, four rows growing to twelve, on `surface.inset` inside a 1-pixel `divider` at radius 8, with *Commit message* as its placeholder; `cmd-enter` in it commits, `enter` is a newline. **`cmd-k` commits nothing** — like *Commit…*, it opens the panel if it is not up, never closes it, and puts the caret in this box (2026-09-18). Under it the *Commit* button — section 6.7's primary while a commit can be made, the secondary with the reason as its tooltip while it cannot: nothing ticked, or no message.

### 6.13 Git
Named *Git* since M8.3.4, with **two tabs inside it — *Log* and *Console* — drawn in the dock's own head after the tool's name**, not in a row of their own: two strips at 42 each cost a panel 84 pixels before a line of content, and one costs 42. Each tab 24 high at radius 6, padded 8, at `eyebrow` 11 — the chosen one on `surface.raised` in `text.primary`, the other in `text.faint`.

**The Log.** Under the head, a **toolbar** 32 high, **one row across the whole panel and following the columns under it**: *Branch or tag* over the branch column, behind the same divider, and the rest over the commits — the two fields on one line rather than at two heights — content, not chrome, and it sits under a head already: the field 220 wide on `surface.inset` at radius 6, its placeholder *Text or hash*, edited by the shared `desktop::text_field` component; then *Branch*, *User* and *Date*, each **its name and its value** — `Branch: All`, `Branch: HEAD` — the value in `text.primary`, or in `accent.brass` with a `✕` in place of the chevron once it is narrowed, because taking a narrowing off should not mean opening a list to find *All* — a control that shows only its own name hides its own state. **A dropdown is a menu of the engine, section 6.17's, since M12.7** — hanging under its own button, not under the strip, at whatever width its longest row asks for rather than the fixed 260 it used to carry on its own. The first row is always the one that takes the filter off, and a rule separates it from the things to filter by — it is not one of them. The footer is section 6.17's, held outside the scroll: *Press ⌫ to clear the filter*, and the key does. The head of the Branch menu is *All* — every ref, where the log starts and what the cross and the ⌫ return to — then the branches a person reaches for: *HEAD*, which is the head of the branch checked out and a narrowing like any other, the default branch, and under a *Recent* heading whatever was listed lately — while **the rest are behind *Local ▸***, a submenu opening beside the menu on hover or on a click: a repository with forty of them turns the menu into the list it was meant to open. *User* and *Date* stay flat, their lists being short by nature. In the branch list the branch `HEAD` is on carries **the same star the branch column gives it**: one mark, one meaning, wherever a branch is named.

**What a field empties, and what takes the keys.** A narrowing that matches nothing puts its note **in the column that field narrows** — *No commits match the filter* where the commits are, *No branches* where the branches are — and leaves the rest of the panel standing: emptying all three columns over a typo reads as the tool breaking rather than as a search finding nothing. The list itself **stays on screen while git is asked again**: it is the same log one narrowing behind, and clearing it per keystroke made the panel blink once a letter. And the whole panel, **toolbar included, is what holds the focus** — not the columns with the toolbar beside them: GPUI focuses a focusable element on mouse-down and lets the window's root focus itself for a press outside one, so a toolbar outside the focusable element loses the keyboard in the same event that clicked it.

**Wide — a panel of 700 or more:** three columns. At the left, 200 wide behind a 1-pixel `divider`, *Local* at `ui` 12.5 in `text.secondary` and the branches under it, one row 26 high each, one indent in, the current one starred in `accent.brass`; **`main` is the first of them wherever the alphabet would have put it**. In the middle, the commits, **one row a commit, 26 high**: the author at 160 in `text.secondary`, ending in an ellipsis; the ancestry graph, with `LOG_GRAPH_LANE` 14-point lanes, `LOG_GRAPH_STROKE` 1.5-point edges and `LOG_DOT` 7-point nodes (rings for merges); lane colors cycle through patina, violet, brass, green and rust. Only actual parent identities connect; omitted parents terminate as short stubs within the source row. Rows follow topological order. The graph takes at most 25% of a row, compressing lanes together when necessary. The author's width is capped at 22% (and at its 160-point maximum), and refs are capped at 25% as well as their existing maximum, leaving room for the summary in narrow columns. The summary gives way first, down to `LOG_SUMMARY_MIN` 40 — four characters and an ellipsis — and only then do the refs, while the author and graph stay aligned across rows; the summary in `text.primary` at `ui` 12.5, ending in an ellipsis; the refs as **one** chip in `accent.patina` on `accent.patina_bg` at `meta` 11, **180 wide at most** and ending in an ellipsis when the row is short of room — several names join with `&`, and a name too long for that gives up its leading path segments to `..` before it gives up its tail, because `fix/` says nothing that `e2e-web-qa-delegation` does not. The refs are the least of what a row says and the summary is the most; and the date at the row's end at its own width in the code face, never cut — the author, the summary and the chip give way instead — *21 minutes ago* under an hour, *Today 11:41*, *Yesterday 23:55*, `04.09.2026` past that. The opened row on `surface.raised`; the last row drawn asks for the next two hundred.

At the right, 320 wide behind a `divider`, the opened commit — **its files first, as a tree, and its message under them**, which is the right order: what a commit touched is the question it is opened with. A directory row carries its path in `text.secondary` with the count of everything beneath it at `meta` 11 in `text.faint`, and **a chain of directories with one child each is one row** — `crates/desktop/tests/all`, not four lines of a name. Root and directory rows have clickable disclosure arrows; the file toolbar uses the existing Expand All and Collapse All icons and localized hints. Collapsing the root hides all descendants; expanding it preserves directory folds. File selection, speed search and scrolling use visible rows. The state belongs to the project and resets when the opened commit changes. File rows retain section 6.4's indentation with square, edge-to-edge status and selection grounds; Git status is a subtle background, not a letter badge. The message block below is 140 high by default, 60 to 400, **its border a grip like the columns' but across** — three pixels seen, nine to the hand, `accent.patina` under the pointer, double-clicked to come back — and it ends with the footer: the short hash, the author with the address, and the date in full. *Select a commit to see it* in `text.faint` while none is.

**Narrow — a side dock:** the commits alone, and the opened commit under the list at most 320 high. The same ancestry graph follows the commits.

**The two outer borders are handles**, since M8.3.2, in the shape the panel borders have: invisible, the resize cursor under the pointer and nothing lit, dragged to size and double-clicked to come back to the widths above — and **nine to the hand**, which is the rule below. The branches column moves between 120 and 400, the details between 200 and 640, and the commits never below 240 — when the room runs out it is the branches that give way first, being a column of short names, and the details last. The widths are pixels in the project's state, as the panels' are.

### 6.14 Annotate tab
*Annotate*, as a tab of ours — decision 10 of M8.3 says why not a column in the editor. The head is the diff's: the icon, the path, and *annotated* in `text.faint` at the right. Then one row per line, the diff row's height, in the code face: a column 200 wide that says, on a run's first line, who wrote it and how long ago at `meta` 11 — `text.secondary` for the newest third of the file's commits, `text.tertiary` for the middle third, `text.faint` for the oldest, by rank among the file's commits rather than by days — and nothing on the run's other lines; a line not committed yet says so in `text.faint`. A hover on the column gives the hash and the summary; a click opens the commit in the Log. Then the line number in `text.gutter`, then the line, **coloured by section 2.4's roles since M8.1.1** — the text parsed is built from the same runs the tab draws, so the line drawn is the line parsed.

### 6.15 Drop zone
Drawn only while a tool is dragged from the status bar, M8.3.1: a strip 52 wide down the left edge and another down the right, the panel heading's 42 high along the bottom above the status bar, each on `surface.panel` with a 1-pixel `divider` on its inner side, `accent.brass_bg` while the tool is over it. What travels with the pointer is the button itself — the tool's icon at 14 on `surface.raised`, 22 square, radius 6. Letting go over a zone puts the tool in that dock and opens it; letting go anywhere else, the window included, leaves everything as it was.

**An open panel is a drop target too**, since M8.3.2: letting go anywhere over the Explorer, the Commit panel, the Log or Run puts the dragged tool in *that* dock, and the panel takes `accent.brass_bg` while the tool is over it. Fifty-two pixels at the window's edge is a small target for a hand and says nothing about where the tool will end up; the panel is where the person is already pointing. The edge zones stay for the docks that are closed, which have no panel to aim at.

### 6.16 Dock head with more than one tool
A dock holds any number of tools since M8.3.1 and shows one; until M8.3.2 its head named only that one, and the others were findable in the status bar or nowhere. So: a dock names the tool it is **showing**, whatever else it holds — a title at `eyebrow` 11, **in capitals and semibold, in `text.tertiary` since M12.4** — `EXPLORER`, `GIT`, and the Git tool's `LOG` and `CONSOLE` in the same face — 36 high since M12.6, the tab strip's height, and whatever control that tool puts at the right. **The tabs are gone, M12 task 14.** A dock holding several drew them all in that 42, the shown one filled and the rest greyed, so the head read *Explorer* beside a dimmed *Commit* — two names over a panel that can only ever show one thing. What switches between them is the status bar's buttons of section 6.11, which carry the same names and are the same gesture wherever a tool lives. **And the naming is the part that gives way when the panel is narrow**: it shrinks to nothing before a single control does, because a dock whose heading has lost the bar that hides it is a panel a person cannot put away.

**At every head's right, a pair** — since M8.3.5: a ⋮ that opens the panel's own menu and a bar that puts the panel away, each 24 square at radius 6 with its icon at the head's size in `text.secondary`, `surface.raised` under the pointer, and a tooltip naming it. They come **after** whatever the tool put there, so the Explorer's crosshair keeps its place and gains them. **A bar, not a cross:** a cross says *close this*, and a dock is not closed but hidden — its tools are still in it and its button in the status bar brings them back. The Commit panel takes the bar alone; its head is a toolbar already. **The head's name keeps its width** and the toolbar has what is left: buttons that do not fit go into a ⋮ that appears for them, and only a head narrower than the name ends it in an ellipsis.

The menu is section 6.17's — a menu of the engine since M12.7, its card sized to what it holds rather than the fixed 200 it once carried on its own — and it hangs **four pixels under the head**, section 6.17's anchor, kept inside the window. Four under the head and not under the button: the button is 22 in a head of 42, and a menu hung from the button floats clear of the panel it belongs to — measured on the screen at 12. When the head has folded a toolbar button or more, they come first, each keeping its own icon; then *Move to* over the two docks the tool is **not** in, a rule, and *Hide*. **It offers what Guild has:** a *View Mode* that floats, undocks and windows a tool would be offered elsewhere, and Guild's tools are docked and nothing else — a menu naming modes that do not exist is a menu that lies.

~~At the right edge of every head, a close button carrying the same cross a tab's does.~~ **Replaced by the pair above, M8.3.5**, and the cross is gone with it: a cross says *close this*, and a dock is hidden rather than closed. What the bar does is what the tool's own button in the status bar does when it is pressed a second time, and what `cmd-b`, `cmd-j` and `cmd-l` do.

### 6.17 Context menu

**Floating-card contour (2026-10-13).** `theme::card_outline(theme)` is the
one outer-border colour for Guild-owned floating cards; `--card-outline` is
its design token. Keep one pixel, existing radius, shadow and layout. Blend
`surface.card` toward `text.primary` in sRGB, solving the fraction for WCAG
relative-luminance contrast `max(1.5, contrast(card, divider) + 0.05)` using
24 binary-search steps. The tested ceiling is strictly below `text.faint`.
The direction follows the palette: lighter on dark, darker on light. All eight
current palettes are covered, including Islands. Internal rules other than the menu separators below, work/area/dock
frames and titlebar trigger chips retain their existing colours.

No external reference yielded a reliable menu ground and outline (2026-10-13),
so the contract fallback of about 1.5:1 is used; Light rises
above its own divider by 0.05. CSS blend fractions are derived snapshots of
this formula, not independent palette choices.

This contour also applies to the usage card, release notes, Terminal Jobs,
Clone Project, branch/project/agents/server menus, all picker cards, Go to Line,
the compact Review peek, the shared text/review/terminal overlay, Push Commits
menus (Remote, Tags, Target, Push, Help) and the terminal Find card. The Usages
balloon and transient notices retain semantic state edges; tooltips have no
card frame. Inline Review report cards retain divider borders.

| Palette | Rounded outline preview | Target contrast |
|---|---|---|
| slate | `#48494c` | 1.5000:1 |
| guild | `#39393c` | 1.5000:1 |
| light | `#d3d3d3` | 1.5000:1 |
| cide | `#353538` | 1.5000:1 |
| darcula | `#424242` | 1.5000:1 |
| ghostty | `#494c53` | 1.5000:1 |
| islands | `#414348` | 1.5000:1 |
| cloud | `#ced1d5` | 1.5347:1 |

The menu the right button opens, M8.3.2 — **drawn by Guild since M12.2**, measured off reference screenshots rather than from memory. It was `gpui-component`'s until then, and that component fixes its rows at 26 pixels and its text at 0.875 rem in its own code, where no theme reaches. **Since M12.7 it is every floating menu in the window's, not the right button's alone** — one engine, one card, one highlight, behind one `Root.context_menu` that holds at most one of them: the tab strip's add button and its `⋮` (6.5, 6.26), a panel head's `⋮` (6.16), the agents list, the title bar's project popup, the Log's filter menus (6.13) and Find in Files' history (6.34) all build their rows from it and open through the same door, `Root::open_menu`.

**The card** is `surface.card` with a 1-pixel `card_outline` at radius 8 and a shadow, at least 180 wide and as wide as its longest item, 5 of padding above its first row and below its last. **A row** is 32 high, its content starting 14 from the card's edge. From the left: **a check column, since M12.7** — 16 wide, a ✓ in `text.secondary` when the row is the menu's current item, and empty rather than absent on every other row of a menu that has one, so an *Item* or a *Submenu* row keeps its neighbours' words lined up even though it can never be checked; a menu with no current item at all carries no column and no gap for it. Then — in a menu whose items carry icons — a 16 icon, and the text 6 after it; otherwise the text at once, `text.primary` at `body` 13 in the interface face. At the row's right: a **detail**, since M12.7 — faint right-hand text in the slot a stroke would otherwise take, winning it when a row has both (none does yet) — the project popup's *path · N sessions*, the agents list's workspace and project; or, failing a detail, the stroke when the item has one, in `text.faint` at `meta`; a submenu's chevron in its place; and, **only on the row the pointer or the keyboard is on**, a trailing ✕ when the row carries one — the project popup's close — in `text.faint`, `accent.rust` under the pointer, its own click swallowed so the row underneath does not also fire, a cross inside a row running the row's handler too and second. The row under the pointer or under the keyboard takes `surface.raised`, one highlight at a time. **A disabled row**, since M12.7 — a harness not on the `PATH` — is `text.faint`, takes no pointer and is never lit: a row drawn, and not a place to stand. **A separator** is a 1-pixel `card_outline` with 6 above and 6 below, inset by the row highlight’s radius (8) on both sides. It reuses §6.17’s palette-derived contour, stronger than `surface.divider` and no stronger than the outer edge. Footnote rules and panel/work frames retain `surface.divider`. **A heading**, since M12.7, is 24 high, its word at `eyebrow` 11 in `text.faint` at the row's own inset, never lit — the agents list's kind, a panel ⋮'s *Move to*. **A footnote**, since M12.7, sits under a rule at the menu's foot at `eyebrow` 11 in `text.faint`, and it is **outside what scrolls**: the Log's *Press ⌫ to clear the filter* stays on screen for as long as the menu is up, rather than scrolling away the moment a long list makes it wanted. **An item that loses something** — Delete, Remove, Rollback, clearing terminal/Run history, and dropping a stash or deleting a branch — is `accent.rust`, as a *Remove Project* usually is.

**Worktree strokes (2026-10-13, amended 2026-10-25).** New Terminal retains
its binding. Run remains available without a default shortcut: the user assigned
⌘R to local Replace in editable files. Copy Worktree Path gains ⌘⇧C and Remove Worktree gains ⌘⌥⇧R,
explicitly approved after checking free strokes; Close Project keeps ⌘⌥⇧W.
Worktree rows print these only for the active root. Project/Open In Copy Path
prints ⌘⇧C only when its folder equals the active root. Remove follows the
same primary/busy/locked/dirty guards and confirmation as the menu. Background
and divided-tab Close still show no ⌘W.

**The anchor, since M12.7.** A menu hangs from what opened it. The right button's hangs at the **pointer**. A trigger that opens downward — the tab strip's three, a panel ⋮, the Log's filters, Find in Files' history, the project popup — hangs **below** it, the card's top-left at the trigger's bottom-left; a panel ⋮ hangs four pixels under its head and not under its own button, section 6.16. The status bar's counter opens the agents list rising **above** it, the card's bottom-left at the trigger's top-left. Every anchor keeps the card 8 inside the window, snapped there when the place it would hang puts it past an edge.

**The scrolling card, since M12.7.** A menu's card does not grow past the window: its rows scroll inside it once they do not fit, capped by default at the window's height less 8 either side, and a menu that asks for less — the Log's filters, at `BRANCH_LIST`, 320 — keeps the height its list always had rather than reaching for the whole window. A footnote is held outside that scroll, the one row that is always on screen once its menu is open.

A row menu carries no icons; a project's menu does. `up` and `down` move the highlight past the separators, the headings, the footnotes and the disabled rows; `right` or `enter` opens a submenu, scrolling it with the keyboard as the menu itself scrolls; `left` closes it, `enter` takes an item and `escape` leaves.

**What goes in it, per surface**, as M8.8 left it. A **tree row**: New ▸ (file, directory), open, Open In ▸ (Reveal in Finder, a terminal here), Find in Files scoped to it, Rename… and Delete, Git ▸ (show its diff, Annotate, Add to VCS), then copy path and copy full path — a directory folds or unfolds instead of opening and takes *Add to VCS* flat, and **the project's own row takes neither Rename… nor Delete**, section 6.4. A **row under External Libraries** takes four items and none of them writes, section 6.27. A **tab**: close, close others, close all, close those to the left and to the right where there are any, then the paths when a file stands behind it — and **nothing at all of the path half when the file is under a dependency's root**, whose path is relative to that root and would name a different file joined to the project's. A **change in the Commit panel**: its diff against `HEAD`, against the index and the index against the disk, then to be included in the commit or left out, then Rollback… or Add to VCS — the two point opposite ways over whether git has ever seen the file — then Delete, then the paths; a conflict is offered no tick and no Rollback, a row already marked `D` no Delete, a directory no diff. A **section head** of that panel: tick all, untick all, collapse, expand. A **commit** in the Log: copy the revision, the full revision, the message. A **branch**: list it — or, for the one already listed, go back to the current branch — and copy its name. A **terminal**, a tab's or the Run panel's: Copy when there is a selection, Paste, Select All; Find…; Clear Terminal — with their strokes on a tab's terminal, where the focus comes back after the menu, and none on the Run panel's, where it does not. M5.12.

**The order is what a person reads:** what this row *is*, then what can be copied out of it, then where it lives on the disk, with a separator between the groups. **An item that would do nothing is not drawn** — *close the others* on a project's only tab, *show its diff* on a directory git did not descend into, *Delete* on a file that is already gone.

**A menu item may write, since M8.8 — decision 30.** Until then nothing in any menu touched the repository or the disk, and that was decision 20 with decision 18 behind it. M8.8 spends it twice: the tree makes, renames and trashes files, and the Commit panel rolls a change back, deletes one and adds one to git. The rule is **retired rather than bent**, because one that holds for five menus and not for two is one a reader will lean on in the wrong place. What replaces it: the writes are still only what `git::Write` spells; where git can refuse, its refusal is the notice; **and where git will not argue, the window asks first, in a sheet that names what it is about** — which is why a rollback has a sheet and `git add` has none. A file is deleted by moving it to the Trash, never by `unlink`.

### 6.18 Branch popup
The branch in the title bar is a button since M8.3.3 — the same shape the project's name beside it has: the name in `text.tertiary` at `meta` 11 after git's own `⎇`, a chevron after it, `surface.raised` under the pointer, and a card that opens below on a click.

**The card** is 460 wide, on `surface.card` at radius 6 with a 1-pixel `card_outline` and a shadow, hanging from the title bar's underside at the branch's own left edge — a picker card, section 6.32, hanging. From the top: **two tabs** — *Branches* and *Stashes* — each 28 high at radius 6, the chosen one on `surface.raised` in `text.primary`, the other in `text.faint`; then **the field**, 34 high, its placeholder *Switch or type to create a branch…*, typed into by `desktop::edit` like every field of ours; then, under a hairline, **the list**, at most 320 high and scrolling past that, its rows 44 high at radius 6.

**A branch row** carries a 14 tick in `accent.brass` for the branch `HEAD` is on and git's `⎇` in `text.faint` for the others, then two lines: the name in `text.primary` at `ui` 12.5, and under it the tip's author, its age and its subject in `text.faint` at `meta` 11, separated by the `·` the session rows use and clipped at the right. The lit row takes `surface.raised` — section 6.32's one highlight. At its right, only on that row and never on the current branch, a bin at 14 in `text.faint`, `accent.rust` under the pointer.

**A stash row** is the same shape with the reference in place of the name and no tick. **The create row** appears at the top of the list only when nothing matches what was typed: *Create branch "…"* in `accent.brass`, at the same height as a branch row.

**Nothing in the card is a menu item**: it is a picker, and its keys, its pointer and its closing are section 6.32's. What each row does when taken is decision 21 in `docs/decisions.md`, not this document's.

**The Update branch source popup — 2026-11-17** is the same picker card hanging from the title bar's ↓, which opens it on a click and never updates by itself. It is **300 wide** (`UPDATE_SOURCES`): a source row carries one name, not the branch row's second line, and 300 holds `origin/` and a long branch name beside a tag. No tabs: the field, 34 high, *Search source branches…*, then under a hairline the list, at most **224** high (`UPDATE_SOURCES_LIST`, seven rows) and scrolling past that. **A row** is a menu's — 32 high (`MENU_ROW`), inset by `radius::ROW`, radius 8 — with the menu's 16 check column, the check in `text.secondary` on the source an update uses now; then the name in `text.primary` at `ui` 12.5, as a person reads it: `origin/main` for a remote-tracking branch, `main` for a local one, never `refs/`, `heads/` or `remotes/`; then, right-aligned in what the name leaves and cut first, `eyebrow` 11 in `text.faint`: *local only* for a local branch with no upstream, `→ origin/x` for one that is updated from its upstream. **The lit row** — under the pointer or lit by the arrows, one highlight for both — takes a menu's `control_hover` over `surface.card`, because `surface.raised` on the card was a step too small to see; on it the tag goes one step up, to `text.tertiary` (`popup::trailing_ink`), as the start screen's recent rows raise their trailing time, because the fill all but swallows faint. With nothing typed the repository's default source comes first, then the other `main` and `master` refs, remote-tracking before local, then git's order; the checked row is lit when the popup opens, and typing ranks the shown names by scattered letters. **The foot**, only when there is something to say, under a hairline at `meta` 11 in `text.faint`: auto fast-forward's skip line, and one line on the last manual update — *Last: already up to date* — with no hashes, path or recovery text. **Taking a row asks first**, in the platform's sheet, at the warning level: *Update feature from origin/main?* over *Fetches origin/main and merges it into feature.* — or *Merges main into feature.* for a local source — with *Update* and *Cancel*; only Update remembers the source.

### 6.19 Console
The Git tool's second tab: what the window told git, and what git said back. In the code face at `meta` 11, newest at the bottom — a console is read downwards and the place a person looks after pressing a button is the end. Each entry is a heading in `text.primary` — `13:14:21.049: [/path/to/project] git switch main`, the milliseconds being what tell two runs of one command apart — and under it what git printed: `text.secondary`, or `accent.rust` when git refused. *Nothing has been done to this repository yet* in `text.faint` until there is.

**Only the writes appear.** The window runs `git status` on every batch of file events and `git log` on every change under `.git`; a console carrying those is a console nobody reads.

### 6.20 Run panel
M8.4, a Run tool window in Guild's shape. Under the dock head, a **toolbar 32 high** — the Log's, padded 13 — holding, from the left: **Run**, **Stop** and **Clear** at 24 square, radius 6, their icons at 14 in `text.tertiary`, `surface.raised` under the pointer; then the **command** in the code face at `ui` 12.5 in `text.secondary` with a chevron after it, itself a button at 24 high, at most 420 wide and **the one control that gives way** when the panel is narrow — it truncates, and everything else keeps its size; then, at the right, **what became of the run**, at `meta` 11: *running · 12s* in `accent.brass`, *exit 0 · 1m 04s* in `accent.green`, *exit 101 · 8s* in `accent.rust`, *stopped · 4s* in `text.faint`, *could not start* in `accent.rust`. Under the toolbar, the terminal, filling what is left.

**The state is the point of the panel.** `just check 2>&1 | tail` exits 0 whatever happened inside it and this project has already closed three commits on a check that read as green — a panel that shows the output and not the code repeats that in a nicer font. It is why the state never clips and the command does.

**A button that would do nothing is greyed and says why in a tooltip** — *Nothing is running* on Stop, *Choose a command to run* where the command would be. Run is never greyed: with nothing chosen it opens the list instead, because a gesture that refuses silently is the shape of bug `cmd-k` shipped with.

**The list** hangs under the command button — a picker card, section 6.32, hanging, at section 6.18's 460 wide: a **field** 34 high in the code face, its placeholder *Run a command…*, with the picker's caret, over a list of rows 28 high at radius 6 (`PICKER_ROW`, M12.7 task 9 — the fonts list is the same), at most 320 tall and scrolling past that. A row is the line in the code face at `ui` 12.5 with, at its right, *recent* or *declared* at `eyebrow` 11 in `text.faint`; the lit row takes `surface.raised`, section 6.32's one highlight. The first row, when what has been typed is not already in the list, is that line in `accent.brass` and no label — the branch popup's create row in another hat. Nothing has been run yet says *Nothing has been run here yet* in `text.faint`, centred in the body.

### 6.21 What a language server says
M8.5, and most of it is drawn by `gpui-component` rather than by us — what is ours is what goes in.

**A diagnostic** is a wavy underline one pixel thick under the range the server named, in `accent.rust` for an error, `accent.brass` for a warning, `accent.patina` for information and `text.faint` for a hint. The four are set as `error`, `warning`, `info` and `hint` in the same JSON `editor::theme_value` builds for the syntax colours; without them the component uses its own red. A hover over one opens the component's diagnostic card with the server's message.

**A hover over anything else opens nothing — 2026-09-18.** The component opens its documentation card 150 ms after the pointer rests on a symbol, a page of it over the code being read, and the person did not want that. The documentation slot is left empty; the diagnostic card above is a different slot and stays.

**The count is on the Problems button in the status bar** — section 6.11 draws it, and section 6.24 is what it opens. **This is the gutter the roadmap asked for** — the component owns the editor's own gutter, which is decision 10 of M8.3 for the third time, so the number goes where a person is already looking. Until M8.8.5 it was a per-file count beside the language, in the bar's own words; it is the **project's** count now, and per-file counting is gone rather than kept beside it. The reason is what the control does: it opens a project-wide list, and a button reading *0* over a panel listing forty would be lying about what pressing it opens. Nothing is drawn when there is nothing wrong.

**The server's own state is at the right too**, in `text.faint`, and to the left of everything fixed: *server starting…* until it has answered, and otherwise whatever `$/progress` names — *Indexing*, *cargo check*. A server that is listening and quiet says nothing at all: a person does not need to be told that things are as they should be. Until the first diagnostics land this is the only thing that distinguishes *nothing is wrong* from *nobody has looked*, and on a cold start those are two minutes apart.

**Since M8.5.1 it is drawn as background tasks commonly are.** From the left:
- `gpui-component`'s spinner at 14, turning, in `text.faint`;
- the title of the task begun last;
- its message, or the last line of kotlin-lsp's Gradle output when it has none, cut at 240;
- when the server says how far, a bar 48 by 3 in `accent.patina` and the percentage;
- `+N` when other tasks are running.

A task is kept by its token, so the first of two tasks to end does not silence the other. jdtls's *Starting…* status is the title while no task has begun. **A language whose row names a server that is not running says so in `accent.rust`**: a disc 7 across and *gopls not found*, *… keeps stopping* or *… refused to start: why*. Saying nothing there made a missing server look like a ready one. The whole state is a button, `surface.raised` under the pointer. A click opens **the list of the project's servers**: a card 360 wide in section 6.32's shape, hanging up from the button's right edge. Each server has a head at `eyebrow` 11, *language · command* with its state at the right, and under the head one row per task: the title, its message at `meta` 11 in `text.faint`, and a bar 3 high the card's width when it says how far.

**A language with no server says nothing here, since M8.8.5** — request 3. *no server* was true of every Go, Kotlin and TypeScript file for ever, which makes it a fact about the language rather than about this moment, and the bar repeated it a hundred times a day. It belongs where a person asks for something a server would have answered: `f12` says it there, and the Problems panel says it as one of its three emptinesses.

**The hover card, the completion menu and the definition underline are the component's**, unchanged; what we give it is the answer. The jump itself is ours — `f12` — and a definition outside every open project is a notice with the path rather than a tab, because a file tab is a path within a project.

### 6.22 What an agent proposes
M8.6, and it is section 6.10's diff with two answers added — because a
proposal is a question and an ordinary diff is not.

**The tab** is named for its file with `· proposed` after it, where a
diff of a revision names the revision — *note.txt · proposed*. It wears
**the shared question mark**, as any tab waiting on the person: the
agent that opened it is blocked until this tab is answered, and it is the
only diff in the window with anybody waiting.

**The two answers** sit in the diff's toolbar at its right, before the
base: **Refuse** on `surface.raised` in `text.secondary`, then **Apply**
on `accent.brass_bg` in `accent.brass`, each 24 high at radius 6, padded
by the tree's gap, at `meta` 11. **Words, not a tick and a cross** — the
pair decides whether an agent's edit reaches the disk, and an icon is a
poor place to learn that. Their tooltips say who is waiting and what
each does: *The agent is waiting: this writes the file* and *The agent is
waiting: this writes nothing*.

**They are drawn nowhere else.** On a diff against `HEAD`, the index or a
commit there is nobody to answer, and a toolbar that carried them
greyed-out would say there is.

### 6.23 Search
M8.7, and the fifth tool. At the bottom by default, with Git and the
Run, because what it draws is a wide list of lines: a result row in a
300-pixel left dock is a path with no line beside it.

**The toolbar** is the Log's, at 32 high and padded 13, and it **wraps**
rather than clipping — the tool can be moved to a side dock, and a person
who turned *match case* on at the bottom must not lose the way to turn it
off. From the left: the **query**, 220 wide, `surface.inset` at radius 6,
in the interface face at `meta` 11, typed into by `desktop::edit` with no
caret and bordered `accent.brass` while the letters go to it; then the
three toggles at 24 square, **`Aa`, `W` and `.*`** — letters rather than
icons, because that is what every search field in this trade draws —
`accent.brass` on `accent.brass_bg` when on, `text.tertiary` otherwise,
each with the sentence in a tooltip; then the **file mask**, 120 wide,
because `*.rs` goes in it and not a sentence.

**At the right, what came back**, at `meta` 11 in `text.faint`: *128
matches in 21 files*, with *and more* after it when a limit stopped the
walk and *· searching…* while it is still going. A pattern that will not
compile replaces it in `accent.rust` — **one line**, the last of the
regex crate's four, with the whole diagram in the tooltip because the
caret is what says *where* — and the field that carries the mistake takes
a rust border, since with two fields and one message *unclosed group*
leaves a person looking at both.

**The results** are the Log's opened-commit tree with folding: a
directory row, the files under it, the lines under those, rows 26 high,
indented by 22 a level. A directory row is written with **the segments it
adds**, not the whole path, and a run of single-child directories is one
row. Each row carries how many hits are beneath it at `eyebrow` 11 in
`text.faint`. A hit's line number sits in a 36-wide column in
`text.gutter` and its text is drawn in **the code face** — a hit read in
the interface font is a line that does not look like the line it came
from — with the matches in `accent.brass`.

**Three emptinesses, three sentences.** *Type to search this project*
before anything is typed, *Nothing matches* when a real query found none,
and — for a pattern that will not compile — nothing in the body at all,
because the toolbar is already saying it. The controls stay outside
whatever the emptiness replaces.

**One engine, two holders** — M8.9. The walk behind this tool is the one
behind the Find in Files popup (6.34): `search::Walk`, the thread that
streams a file at a time, drops a replaced walk's answers and keeps the
old hits until the new walk has something to put in their place. Each
holds its own, and the two remember one query per project.

### 6.24 Problems
M8.8.5, and the sixth tool. **A tool and not a popup** — decision 14: a
tool costs one variant and reuses the dock, the status-bar button and the
persisted placement the other five already have, where a popup costs a
second occluding surface with its own drag, focus and dismissal rules,
each of which this window has got wrong at least twice.

**The rows are the search panel's**, in the same three kinds at the same
26 and the same 22 an indent: a directory written with the segments it
adds and a single-child chain folded into one row; a file in section
6.4's shape; and under it one row per diagnostic. A directory or a file
carries, at its right, **a disc in the worst severity beneath it and then
the count of everything beneath it** in `text.faint` at `eyebrow` 11 —
two tokens for the reason section 6.11 gives. A diagnostic row is the
disc, then `line:column` in `text.gutter` counting from one, then the
server's message in `text.secondary`, then — where the server names
itself — *rustc* or *clippy* in `text.faint`, which is what tells two
servers apart when they disagree about one file. **The message is one
line**: rust-analyzer sends paragraphs, the first line is the sentence
and the rest is the explanation, and the explanation is one hover away in
the editor's own card. A click on a diagnostic opens its file at its
line. The four severity colours are section 6.21's, so the panel and the
squiggles agree without anybody learning a second alphabet.

**Three emptinesses, three sentences**, and they are not the same
sentence: *No language server has looked here* for a project no server
serves, *Still looking…* while one is starting, and *Nothing is wrong
here* when a server has answered and had nothing to say. On a cold start
the first two are two minutes apart from the third, and a single note
would call all three good news. When a cap stops the list, a line under
it says *and more* — the cap is 2,000, which is about seventeen
simultaneous renames of a widely used name and high enough that no
ordinary state reaches it, which is exactly why the line that says it was
reached has to exist before anything does.

**No toolbar and no keyboard, deliberately.** Nothing is typed at this
panel and nothing is filtered, so there is no control to outlive the note
that replaces the list — and the next person to add a filter inherits
that rule from section 6.13. The gesture is a click; a highlight to move
would want `pick::step`, `pick::settle` and a focus handle, and that is a
pass of its own.
### 6.25 Recent Files
M8.8.2, and the third popup of ours after Recent Projects and the branch
list. A floating picker card, section 6.32: **it moves by its head and
remembers where it was left** (M12.3), and **it does not resize** — its
size is this section's, because it is a switch made in under a second and
a popup a person has to size first is not one.

**The card** is 520 wide and 420 tall, clamped to the current window,
and hangs 120 below the window's top, centred horizontally until it is moved.
The height is definite so its scrolling list can fill the space below the head;
file rows and variable-height location excerpts retain their natural height.

**The head — user correction 2026-10-26.** Recent Files is a selector,
with a named header and the existing *Show edited only* checkbox. It has no
query, editable field or synthetic caret. Cmd+E retains its edited-only toggle.
The footer explains Backspace closing; arrows/Enter choose and Escape dismisses. Printable keys
are consumed by the modal rather than reaching the editor.

**A file row** is `TREE_ROW`, 22 high at radius 6: the existing file icon,
name in `UI` 12.5 and muted directory in `META` 11. The lit row takes
`surface.raised`, as every picker's does (user decision 2026-11-21; it was
`accent.patina_bg`); the pointer moves that one highlight.
The project's MRU records each actual open/reopen immediately, deduplicates paths
and retains the newest first. Backspace forgets an open file only after its
ordinary guarded close completes; cancelling preserves the entry.

Empty states distinguish no opened files from no edited files.

**The same card is Recent Locations** on `cmd-shift-e` — one popup with
two faces, because the highlight, the arrows and Enter mean
the same thing in both and a second copy of that is a second chance for
them to disagree. Its head retains its real query field and loses the *⌫ closes* hint, since a place is
not a thing that can be closed. A row is the path and `:line` in `meta`
11 — `text.secondary` and `text.faint` — over **five lines of code in the
code face** at `meta` 11, their numbers in a 36-wide column in
`text.gutter`: the place's own line in `text.primary` and the four around
it in `text.muted`, because they are context and should say so. The
excerpt slides rather than padding when the place is near either end of
the file.

**And the Go to Line field** is the third thing this card is: 240 wide,
one line, with the shared `card_outline` outer border (§6.17),
with `line:column` as its placeholder — the placeholder is the syntax.

### 6.26 The tab strip's two buttons
M8.8.1, requests 2 and 5. A chevron sits beside the last visible tab
when the tabs do not fit — *Show Hidden Tabs* — and a `⋮` beside it whose
own name is *Recent Files, Tab Actions, and More*. Both are here.

**They sit after the scrolling box, not inside it.** A control that
scrolls away with the thing it controls is reachable exactly when it is
not needed. Each is the status bar's button in another place: 22 square
at radius 6, the icon in `text.secondary`, `surface.raised` under the
pointer, 4 apart, with the card's inner gap before the pair. The chevron
is drawn **only while something is hidden** and the `⋮` always.

**The chevron lists the tabs that cannot be seen**, not every tab. The
`ScrollHandle` gives the box's own edges and each tab's, in one space, so
*hidden* is a comparison rather than a guess — with half a pixel of
slack, or the tab that fits exactly would be called hidden for ever and
the chevron would never go away. **A tab clipped by an edge counts as
hidden**: half a name is not a tab a hand can pick out, and the tab under
the button itself is the first one a rule counting only the wholly
invisible would miss. Picking a row brings that tab to the front, and the
strip scrolls to it by itself.

**The `⋮`'s items are actions, not deeds aimed at a row — and since
M12.7 it is not the only menu of ours that is.** Everywhere else a menu
holds deeds aimed at the row under the pointer, because a deed reaches
the thing the pointer or the click named and an action reaches the thing
in front — a background tab's *Close* must close *that* tab, decision
20. This menu hangs off the **strip** rather than a row: *Recent Files*,
then a rule, *Reopen Closed Tab*, *Close All Tabs*, *Close Other Tabs*,
then the area's group. *Settings* is not here: it opens from the title bar's
gear and `⌘,`. Every one of them means the same thing however it is reached, so
every one of them shows its own stroke — which is the whole reason for
this button rather than a longer tab menu.

**A panel ⋮'s folded toolbar buttons (6.16) and the title bar's project
popup's *Open…* and *New Project* are actions for the same reason —
ruled on in M12.7.** A folded button is the toolbar's own command
reaching the panel in front, exactly as it would from the toolbar
itself, not a deed about the row it happens to be drawn beside; *Open…*
and *New Project* are what the app menu's File section already
dispatches, drawn here rather than given a duplicate of their own. Both
show their real stroke for the reason every action here does.

### 6.27 The Explorer's file menu, and External Libraries
M8.8.3, requests 16 and 10.

**The menu** is section 6.17's, and it goes from four items to a
full set. A **file's**, in order, with a rule between each group: *New ▸* —
*File…* and *Directory…*, made within the row under the pointer, which
for a file row means its directory, the only rule that is
not surprising; then *Open* and *Open In ▸* — *Reveal in Finder* and **a
terminal tab at that directory**, since we own the terminal and can open
a tab rather than hand a path to an external terminal; then *Find in Files…* scoped to that row; then *Rename…*
and *Delete*, the destructive one **inside** its group rather than alone
at the foot; then *Git ▸* — *Show Diff*, *Annotate*, *Add to VCS*, second
doors onto what M8.2 and M8.3 built, aimed at the row under the pointer
rather than at the file in front; then *Copy Path* and *Copy Full Path*.

Those two are **two of the usual eight**. The other six — content root,
source root, repository root, and the two URLs — are project-model words,
and we have no project model to say them with. A **directory's** menu is
the same one with *Collapse*/*Expand* where *Open* was, and *Add to VCS*
flat rather than in a *Git ▸* of one line: a submenu holding a single
item is a press bought for nothing.

**Explorer shortcuts — user decision, 2026-10-10.** Shift+F6 renames the
file, F6 moves it, Cmd+Backspace asks to send it to Trash, Cmd+N creates a
file and Cmd+Option+N creates a directory. They act only with Explorer list
focus; name/search fields keep native text editing. Configured strokes are
shown in menus. This explicitly supersedes M8.8.3's decision to leave Rename
and Delete unbound: the user chose these familiar file-management gestures;
symbol/reference refactoring is outside their contract. Rename initially selects
the stem before the last non-leading dot; dotfiles, extensionless files and
directories select their whole name. Selection happens once per operation.

**`F4` opens the selected file — *Jump to Source*, 2026-09-18.**
It is Enter's gesture on a file, the keyboard going with it, but a stroke
of the keymap's rather than one of the tree's own keys, so a person can
move it; on a directory it does nothing. *Open* in the menu shows `F4`
**only on the selected row**. Since decision 54 the right button selects
its row before the menu opens, so that is the row the menu is about —
unless the row is one of several selected, which stay selected with it
as their cursor.

**External Libraries** is one row after the project's — after the
content roots — with the tree's directory icon
and the fold chevron. Nothing of ours walks into it: *Expand All* and the
tree's own following are the project's alone. **Since M8.14 it lists what the
project's build systems resolved** — the JDKs first as
`< 21 >` with the JDK icon, then every library with the library
icon and the label its build system's convention gives it: `Gradle: group:module:version`,
`Maven: …`, `serde 1.0.228`, `dario.cat/mergo@v1.0.2`, `@types/node@22.5.0`,
`rake (13.2.1)`, `jason 1.4.4` — and after them the roots a `f12` reached
that no build system listed, by **the name a person knows them by**, the
directory's own. While a build system is being asked the head reads
*External Libraries …*. A row unfolds as its directory, or — a jar, a
JDK — as its sources extracted on a thread, the row saying `…` meanwhile;
a jar with no sources and a dependency nothing downloaded say so in a
notice. The rows under a root are that library's own tree.
A file opened from there is a tab like any other, and the status bar
writes its path under that root's name for the same reason: `src/lib.rs`
alone would read as this project's.

**A library row's menu writes nothing.** The rest of the tree's menu
makes, renames and deletes; a crate unpacked under `~/.cargo/registry` is
not a person's own file, and offering to rename one would offer to break
every build on the machine. What is left is what a hand actually wants
there: *Open* — or *Collapse*/*Expand* for a directory — then *Copy Full
Path* and *Reveal in Finder*. The **full** path only: the short one is
relative to a root that is not the project, so it would paste as a lie.

### 6.28 The Push tab
M8.8.4, request 17. A *Push Commits* window, `⇧⌘K`, **as a tab** —
decision 15's point: Guild has one window, and `Body` is where a new kind
of thing to look at is named.

**The head** is one line above a `divider`, padded by the tree's padding:
the local branch in `text.primary`, an arrow in `text.faint`, the remotes
as chips, a colon, then the branch it lands on, and at the right the
button. The rule about which parts may be changed is kept by drawing
it — the local branch is plain because it cannot be changed, and the
remote is the one thing that can. *Set upstream* sits after the
destination at `meta` 11 in `text.faint`: what this push does besides
sending the commits, said before it happens rather than found in the
Console afterwards.

**The remote is a row of chips, not a dropdown.** Two remotes is already
unusual and three is rare, so the whole choice fits on the head's own
line — and a chip needs none of `deferred`, the backdrop that must
`occlude`, the keys that must be caught where every key passes, or the
release that can go missing. Every one of those is a trap this repository
has paid for. A chip is the row's radius, padded by the tree's gap; the
chosen one takes `surface.raised` in `text.primary` and the others
`text.faint`.

**The button** is section 6.7's primary — `accent.brass`, semibold, the
card's radius — and `surface.raised` in `text.faint` when it is dead. It
is dead for two different reasons and explains neither, because something
else already does: with nothing waiting the empty list says so, and with
a push out the label reads *Pushing…*. What it must never be is pressable
while a push is going, which is two pushes for one press.

**Under the head**, the commits in **the Log's own row** — section 6.13's
26 high, the author, the disc or the merge's ring, the summary, the date
— so that a commit reads as the same thing in both places; and beside
them, at the Log's own details width, the opened commit's files through
the same `files_tree` that collapses a chain of single-child directories
into one row. The width is not draggable here: the tab has the window
rather than a dock, so there is nothing to trade it against.

**Four emptinesses, and only one of them is silent.** *Nothing to push*
is the empty list itself — there is no string for it. The other three are spelled, because a blank
head says nothing at all: *This repository has no remote*, *HEAD is
detached: there is no branch to push*, and the three the Log and the
Changes panel already use for a git that will not run.

### 6.29 The column

**M12.** Down the left of the window, from under the title bar to the status bar — beside the docks and the bottom one, because it is where a person changes *what* they are working on rather than *how* they are looking at it. It replaces the row of projects M5.1 put under the title bar: a row was right while a project was a directory and there were three of them, and a project is a repository with several worktrees now.

**232 wide by default**, `surface.panel` — what every panel of the window is drawn on. **Its right border is dragged**, between 180 and 420: nine taken and nothing drawn — the resize cursor says it — and a double click puts the 232 back — the panels' own grip since section 6.11, and the width is the window's setting rather than a project's. Below 180 a worktree's row is its own name cut short, which is what the column is for; above 420 it is taking room from the work it exists to reach. **`surface.inset` was written here first and fails in the window**: it is `#0b0b0d` against the canvas's `#0d0d0f`, two parts in each channel, which is no separation at all — seen on the screen rather than reasoned about. **Hidden, it takes nothing at all** — and the pointer within 6 points of the window's left edge brings it back, drawn **over** the work with a 1-point `surface.divider` down its right side, until the pointer leaves its width. The head retains workspace identity; it has no hide/pin button. The title-bar toggle pins a peeking column and folds a pinned one. **A folded rail of 48 was written here first and was taken out in M12's task 10**: it kept each project's initial on a tile with a dot per worktree, and the person's answer was that a column folded to a strip is still a side panel. Who is waiting survives it — section 6.5's dot is in the status bar and on the Dock icon since M5.10, which is where a person who has hidden the column is already looking.

**Drawn this way since M12.4** — the person chose it, and the numbers are the approved mockup's. The column is called **Workspaces** in the interface; inside the code it is still `column`.

**The head** retains the 30-high workspace identity and the 24-high *Projects*
line, with no Open, Get from Version Control or hide/pin buttons. **The list** is
inset 6 from the column's edges, with 12 points between projects and no line.

**A project's row** is 28 high, padded 10: a **tile** 18 square at radius 5 holding the repository's first letter in capitals at 10, bold, in `surface.app`, on one of `accent.brass`, `accent.patina`, `accent.violet`, `accent.rust` and `accent.green` — the repository's own, chosen by FNV-1a of its name so it is the same every time; 8 after it the name at `body` 13 in `text.primary`; at the right the `⋯` and the `+`. **The group's own dot is gone**: every row under it wears its own.

**A project's row is a button that folds it** (M12.5 task 12):
- **The pointer.** The row shows the hand and takes `surface.raised` at radius 8 under the pointer.
- **The chevron.** It sits after the name: `chevron-down` at 11 in `text.muted`, drawn only under the pointer. When the project is folded, it is `chevron-right` and always drawn.
- **Folded.** The worktree rows are not drawn, and the row wears its scoped primary activity, using the same ring/question/error/dot as its worktrees.
- **Where the fold is kept.** By the repository in the desk, one answer for every worktree and every workspace.
- **Unfolding.** Whatever brings a root forward unfolds its project, except a restart.

**A worktree's row** is two lines, padded 4 above and below, at radius 4 (`radius::WORKTREE_ROW`). The row in front wears the column's selection — `accent.patina` at `alpha::COLUMN_SELECTED` (5%) over the panel, the fill its tab in front wears too — with its 1-point `text.gutter` outline carrying the selection; the hover is the column's louder neutral hover. In a fixed 14×18 leading slot, its scoped activity: a thin 14-point neutral-gray rotating ring (`text.secondary`) for working, a centered 16-point vector rust question for waiting, a 16-point rust exclamation for observed failure, an 8-point green dot for an unread finished answer, or an empty idle slot; 2 after it git's `⎇` at `meta` 11 in `text.faint`; 6 after that the name at `body` 13, `text.primary` on the row in front and `text.secondary` otherwise, the main worktree's star after it, and **at the right the lines its branch added and removed** — `+36` in `accent.green`, `−298` in `accent.rust`, in the code face at `meta` 11, a half that is zero not drawn. Under the name, its age at `meta` 11 in `text.faint` and compact independent secondary counts. Counts above nine use `9+`; Git facts above 999 use `999+`. Both content lines have 20-point line boxes scaled by the interface size; neither wraps. Exact activity counts remain in the tooltip and disclosure. **The branch line is gone**: a row is called after its branch since M12.2.

**The row in front** is `surface.raised` with a one-point outline in `text.gutter`, and **it is the only row with a background** — every project's row in front used to be lit, which is half of why the projects ran together. Another row takes `surface.raised` under the pointer. An unopened worktree retains full row opacity and enabled action contrast; opening it is its first click.

**Linked worktrees can be reordered by dragging their rows within one project and workspace.** The primary checkout stays first and cannot be dragged. The upper and lower halves of a row mean before and after; a valid insertion paints a one-point hairline in `accent.patina` along that edge, including below the last row. Invalid and no-op targets draw nothing. The drag preview reuses the project's raised card and branch name. Dragging preserves the current project, tab and input focus; an unopened checkout remains unopened. The workspace retains path order across refresh and restart, ignores missing saved paths and appends newly discovered linked checkouts. Reordering waits for the repository's initial Git list. An order containing an unsupported non-UTF8 path is refused with a warning, retaining the prior order. Escape cancels the gesture before panel/editor bindings run, preserving open panels, editor cursors and focus.

**The lines** are: `git merge-base HEAD <the main worktree's branch>` in the worktree, then `git diff --numstat -z` from it — the branch since it left, with what is not committed. For the main worktree the merge-base is its own `HEAD`, so its lines are what it has not committed. An untracked file is not counted, a binary file counts nothing. They are asked with the worktree list, every three seconds while the window has the focus, two commands a drawn row — **an unopened row is not free any more**, which M12 had promised. ~~Not `+N −M`: the status map is what git says about paths, not about lines.~~ **Retired in M12.4**: that was true of the map, and the lines come from a command of their own.

**At the foot**, *Open…* (Cmd+O) and *Get from Version Control…* use
STATUS_BUTTON at the left WORK_INSET, STATUS_BUTTON_GAP apart. Workspace marks and + stay centered inside the card above the footer line,
independently of those buttons.
One to five marks compress only as necessary at minimum width without scrolling;
legacy counts above five scroll only inside the marks viewport.
Pinned and peeking columns share these two rows.

**M12.5 introduced the workspace switcher; workspaces-sidebar-controls restores
it to the bottom row inside the column card, above the window footer line.** Its buttons
use STATUS_BUTTON (22 square at the default scale), STATUS_BUTTON_GAP (4) and
STATUS_BUTTON radius (6), with WORK_INSET around the row. The column toggle is
in the title bar immediately after the native traffic lights. Folding hides the
marks; peeking shows the same single copy. A workspace with
no icon is an 8-point dot; otherwise its 16-point Lucide mark is used. The
selected button is `surface.raised`. The default identity is one centered mark:
rust dot for waiting/observed failure, rotating thin 14-point gray ring (`text.secondary`) for working,
green dot for unread answers and muted dot for idle. It has no corner badge.
Custom icon settings remain intact. Names and independent exact counts are in
tooltip/accessibility text. The final `+` opens the name
field. The workspace's own line is 30 high above *Projects*, inset 16, with an
optional 16-point mark, 8 to its name at `body` 13. A double click renames it;
the switcher's right button offers Rename, Change Icon and Delete.

**The list and its two-line head are one carousel page.** A precise horizontal
gesture over the pages above the marks moves the current and adjacent pages with the
fingers. Ten pixels decide intent; horizontal must be at least twice vertical.
A third of the 232-point page or a 0.6 px/ms flick commits one workspace. A
missing neighbour moves a third as far and returns. Landing is 180 ms; 650 ms
of momentum is swallowed, and 250 ms of quiet stands in for the `Ended` event
AppKit sometimes omits. Wheel lines and vertical/diagonal gestures remain the
project list's.

### 6.30 Find in a terminal
M5.12. `cmd-f` over a terminal — a tab's, or the Run panel's — opens a
bar **over the grid at its top right**, 8 in from the right edge and 8
down, rather than above it: a bar that took a row would resize the pty,
and a program would redraw for a person who only wanted to look for a
word.

**The card** is `surface.card` with a 1-pixel `card_outline` at
`radius.card` (14), padded `card.padding` (13), with `card.gap` (8) between its rows and 4
between its controls. The close button stays at the top right in its own
column, separated from the controls by `card.gap`; the controls **wrap**
when the terminal is narrower than their remaining space — section 6.23's
rule, because Run can live in a 300-pixel dock. From the left: the
**query**, 220 wide and 24 high on `surface.inset` at `radius.row` (8), in the interface face at `meta` 11,
*Find in the output* as its placeholder in `text.faint`, typed into by
`desktop::text_field` with a native selection and caret, and **bordered `accent.brass` only while
its keys go to it** — a bar left open while Recent Files, Go to Line or a
menu holds the keyboard draws no border, because one field takes the
letters at a time. Then **`Aa`** and **`.*`**, section 6.23's toggles at
24 square. Then **older** and **newer**, `chevron-up` and `chevron-down`
at `head.icon` (16) in `text.tertiary` on 24-square buttons,
`surface.raised` under the pointer. Then **what was not found**, at `meta` 11 in `text.faint`:
*Nothing matches*, or — for a pattern that will not compile — its one
line in `accent.rust`, with the query bordered rust. **A match found says
so by its highlight and no count**: counting walks the whole history,
10 ms a keystroke in a release build whatever it finds — measured by
M5.12's task 0. The **close button** uses the standard close SVG at
`head.icon` (16) on a 24-square button in `text.tertiary`.

**Under the controls, only in the alternate screen**, a line at `meta` 11
in `text.faint`: *This program draws on the screen alone: only what is on
it is searched.* It wraps within `notes.card` (360), bounded by the
card's available content width. `less`, `vim` and a full-screen program
keep no history in the terminal, and a bar that found nothing above the
screen without saying why would read as broken.

**A match** is painted on the cell's background, `accent.highlight` under
the cell's own text colour, and **the current one** `accent.brass` under
`surface.app` text. A selection wins over a match; the current match wins
over both, because it is where *next* was just pressed. Nothing else about
the cell changes: its colours are the program's.

**The first match is the nearest above the bottom of what is shown**, and
`enter`, the older button and `cmd-g` go further back; `shift-enter`, the
newer button and `cmd-shift-g` come down. A match off screen is scrolled
into view; one already on screen is not scrolled to. **Three
emptinesses**: nothing typed — no highlight and no word; nothing found —
*Nothing matches*; a pattern that will not compile — its line in rust.
`escape` or the cross close the bar, take every highlight with it, and
give the keys back to the terminal; `cmd-f` in an open bar selects its
query.

### 6.31 A link in a terminal
M5.12. A terminal's output is full of places — a URL, a hyperlink a
program wrote with OSC 8, a `path:line:column` from a compiler — and **a
link is drawn only while `cmd` is held with the pointer on it**: a
1-pixel underline in the cell's own text colour under every cell of the
link, across each row it wraps over, and the pointing hand. Nothing is
underlined at rest: output that looked like a page of links all day would
be read as one, and its colours are the program's.

**Only what leads somewhere is underlined** — a URL whose scheme is
`http` or `https`, an OSC 8 hyperlink with one of those, or a path with a
line to a file that exists, resolved against the directory the terminal
started in and then against the project's root. A bare `src/lib.rs` in
prose is not a link. **`cmd`-click** opens a URL in the browser and a file
inside the project in its tab at the line and column. **A file outside
the project is a notice** — *Outside this project: {path}* — section
6.21's rule for `f12`, because a file tab is a path within a project; a
file gone since the pointer found it is *No such file: {path}*. A click
without `cmd` is a selection, as it always was.

### 6.32 The picker card
M8.9, decision 1 of its plan. **Every list a popup holds is one card**:
Recent Projects, Recent Files, the run list, the branch popup, the font
list, and the two popups M8.9 adds. Each popup says what its head and its
rows are; everything written here is the card's, and a section that
describes a popup does not repeat it.

**The card** is a menu's own look since M12.7 task 9 (6.17) — `surface.card`
at radius 6 with a 1-pixel `card_outline` border and a shadow — clipping
what it holds, so a picker's card cannot drift from a menu's card look
without both changing together. It is either
**floating** — over the middle of the window until it is moved, and as
far below the window's top as its popup puts it (64 for Recent Projects,
which has no section of its own; 120 for Recent Files, section 6.25),
dragged by its head, and either resized from its edges and remembered with
its size (Recent Projects) or kept at the width and the greatest height its
section gives and only moved, so that only where it is is remembered
(Recent Files) — or **hanging** from the button that opened it, at that
button's bottom left and kept 8 inside the window. A floating card sits
over `surface.backdrop` dimmed to .32; a hanging card over a layer nothing
can see. **Either one closes behind the card**, and not on the same
moment: a floating card's backdrop closes on a click — the press and the
release both on it — and a hanging card's layer on the press itself, the
way a menu closes.

**The head** is the popup's own strip, and on a floating card it is the
handle: an open hand under the pointer, a closed one while held. **The
field** in it types at the end: what has been typed, then a 1.5-by-15
caret in `accent.brass`, then — only while nothing has been — the
placeholder in `text.faint`, 6 after the caret. The caret blinks on the
window's one phase and is painted in the head's own ground when it is off,
never taken away. A query put there by the window rather than typed is
**selected whole**: the first letter replaces it and any rub empties it.

**The rows** are the popup's, inset from the card as a menu's rows are since
M12.7 task 9: `MENU_PADDING` above the first and below the last and no gap
between, each row `mx`'d in by the card's own radius and `px`'d the rest of
the way, so its text lands `MENU_INSET` from the card's edge. **A one-line
row** is `PICKER_ROW`, 28 high — Recent Projects, Recent Files, the run
list, the fonts, Search Everywhere and Find in Files' hits all draw at it —
and the branch popup keeps its own `BRANCH_ROW`, 44; Recent Locations, whose
rows wrap to several lines, keeps its own height too. **Recent Projects**
marks the project already open with a check in the same column a menu's
check owns, not a dot of its own. A **lit row** takes `surface.raised`;
there is exactly one, and **no hover tint** — the pointer moves that one
highlight, on a move rather than on hover. **A heading** is a title over a
section, never lit, its own inset aligned with a row's. In a long list —
thousands of rows, a list of rows of one height — it is drawn at the height
of the rows around it, because that list gives every row one height; in a
short list, which scrolls as a `div`, it keeps its own height, as the branch
popup's *Local* in `eyebrow` 11 always has. **The scrollbar** is a
`SCROLLBAR`-wide pill in `text.faint`, 2 in from the right, spanning the
list's own area.

**The edges** a resizable card is taken by are 5 wide along the left,
right and bottom and 14 square at the two bottom corners, with the
platform's resize pointers.

**The seven rules, once:**

1. **A list with rows keeps one lit**, or Enter answers nothing — on
   opening, after every letter, and when the list shrinks under the lit
   row. A letter lights the first row of the new list; a popup's own
   opening row (Recent Files' second row) is applied once.
2. **The pointer moves the one highlight.**
3. **The card occludes** what is behind it, and carries the handlers for
   the drag it is part of.
   Active floating-card gestures listen in window capture, before occluding
   children and text inputs can stop bubble events. The release coordinate is
   applied before saving placement once; no per-move filesystem work is added.
4. **A drag with no button held is over**, because a release outside the
   window never arrives.
5. **The keyboard scrolls the list**: the arrows wrap from either end, a
   page is the rows in view less one, `home` and `end` go to the ends.
6. **The popup takes the focus and gives it back** — to what held it
   before, or to what taking a row opened.
7. **A hanging card outranks its place in the tree.**

### 6.33 Search Everywhere
M8.9. A floating picker card (6.32), **680 wide with its list up to 440**
(section 5) the first time it opens; a hand resizes it from its edges and
it is remembered with its size, as Recent Projects is. One popup behind
its gestures — a double Shift opens it on *All*, the Go to File menu
item or custom binding on *Files*, `cmd-alt-o` on *Symbols*, `cmd-shift-a` on *Actions* — and a
tab's stroke pressed while it is up switches to that tab, or, on its own
tab, turns on *Include non-project items*.

**The head** is two strips. The first, `CARD_GAP` in all round, holds the
**five tabs** — *All*, *Files*, *Symbols*, *Actions*, *Text* — each 28 high
(`BRANCH_TAB`) at radius 6 with `CARD_PADDING` either side, the chosen one on
`surface.raised` in `text.primary` and the others in `text.faint`, 4 apart:
section 6.18's tabs, five of them. Tab and Shift-Tab walk them and wrap. At
the strip's right, **the checkbox** *Include non-project items* — section
6.12's 14-square box, its words at `meta` 11.5, in `text.primary` while
ticked and `text.faint` while not. Before it, **the preview switch** in the same
shape, *Preview*, remembered across restarts; while it is on, the code
around the lit row of *Files*, *Symbols*, *Text* or *All* stands under the
list, `PREVIEW` 220 high over a hairline — section 6.34's preview. The strip
wraps when a hand has made the card too narrow for both switches. A press
on a tab or on a switch is not the start of a drag of the head. The second strip is **the field**, 34
high (`BRANCH_FIELD`), `CARD_PADDING` in, over a hairline in
`surface.divider`, its placeholder *Type to search*.

**A row** is 28 high (`PICKER_ROW`) at radius 6, inset from the card as
every picker's row is (6.32); the letters the query matched are in
`accent.brass`, and every
row is one line:

- *Actions*: the label at `ui` 12.5 in `text.primary`, and the stroke bound
  to it now, printed, at the right at `meta` 11.5 in `text.faint`. A row
  the focus behind the popup does not handle — *Copy* over an editor — is
  drawn in `text.faint` and takes no Enter.
- *Files*: the file's icon from the tree's table, its name at `ui` 12.5,
  and the directory it is in at `meta` 11.5 in `text.faint`; a file an
  ignore file names is dimmed as the tree dims it, icon at half strength
  and name in `text.faint`.
- *Symbols*: a one-letter badge for the kind, 12 wide in `text.faint` —
  `f` function, `m` method, `C` class, `S` struct, `I` interface, `E` enum,
  `M` module, `!` macro, `K` constant, `T` type, `P` property, `·` anything
  else — the name at `ui` 12.5, what it is in beside it in `text.faint`,
  and `file:line` at the right at `meta` 11.5 in `text.faint`.
- *Text*: section 6.34's hit row, at this card's height — the line in the
  code face with its matches in brass, and the file and the line at the
  right in `text.faint`.

**All** is *Files*, then *Symbols*, then *Actions*, each under a
**heading** — the tab's word at `eyebrow` 11 in `text.faint`, at the row's
height because the list is one `uniform_list` — and each at most a handful
of rows; a section that holds more ends in a **More row**, *12 more* at
`meta` 11.5 in `text.faint`, and Enter on it goes to that tab. A section
with no rows has no heading.

**Nothing to show** says which emptiness it is, at `body` 13 in
`text.faint`, `CARD_PADDING` in: *Indexing…* while an index has not landed
its first walk, *Type to search* on a tab that asks nothing until something
is typed — *Symbols*, *Text*, and *All* beyond the recent files — the Search
tool's *searching…* while a walk has found nothing yet, a pattern's refusal
in the regex crate's own words, and *Nothing matches* otherwise. **An empty
*Files* or *All* is the recent files**, in `cmd-e`'s order.

### 6.34 Find in Files
M8.9. **A floating picker card (6.32)** that a person moves by its head and
resizes from its edges, opening at 760 wide with its list up to 280 high
(section 5) and remembering where it was left. It is `cmd-shift-f`; the
Search tool is `cmd-5`, and **the two share one engine** (6.23).

**The head** is two lines on `surface.sunken`. The first, at `meta` 11:
*Find in Files* in `text.secondary`, then what the search came to in
`text.faint` — *6 matches in 5 files*, *and more* when a limit stopped the
walk, *· searching…* while it goes, and a pattern's refusal as the last
line of the regex crate's words — and at the right **the pin** and **the
cross**, each a `HEAD_BUTTON` of 24. The pin is lit `accent.brass` on
`accent.brass_bg` while it is in; its tooltip says what a click will do,
*Keep the window open* or *Close the window after opening a result*. The
second line is **the query**, then the three toggles `Aa`, `W` and `.*`
drawn exactly as the tool draws them and turned also by `alt-c`, `alt-w`
and `alt-x`, then **the mask's checkbox** (6.12's, 14 square) and **the
mask**, 120 wide. Both fields are 24 high on `surface.inset` at radius 6,
the query at `body` 13 and the mask at `meta` 11; the field the letters go
to is bordered `accent.brass` and **only it carries the caret** — Tab moves
the letters between them. A mask that is unticked is drawn in `text.faint`
and kept: it is not applied, and it is still there to tick. A field that
carries a refusal is bordered `accent.rust`. A button in the head acts on
the press, so pressing it is not also taking hold of the card.

**What was searched before.** Each field ends in a chevron, `text.faint`
and `accent.brass` while open, that lists the queries or the masks used
before, newest first, in the menu card (6.17) under a title in `meta`
`text.faint` — *Recent searches*, *Recent masks*. A row puts its text in
the field. `alt-up` and `alt-down` walk the same list from the field and
come back to what was being typed. A query is remembered when it is used —
Enter, or *Open in Find Window* — never per keystroke: twenty queries, ten
masks.

**Where it looks** is the head's third line, at `meta` 11: two chips 24
high at radius 6, *In Project* and *Directory*, the one in force
`accent.brass` on `accent.brass_bg` and the other `text.secondary` on
`surface.inset`; beside *Directory* the directory, relative to the
project, in `text.faint`; and `…`, a `HEAD_BUTTON`, for a folder from the
platform's chooser. *Directory* takes the Explorer's selected row — a
directory itself, a file its directory — and opens the chooser when
nothing but the project is selected. A folder outside the project is
refused with a notice naming it, and the scope is left as it was. The
scope is not remembered. *Find in Files…* on a tree row opens the card
with *Directory* already on that row.

**A hit row** is a row of a long list (decision 4 of M8.9), 28 high:
the line in **the code face** at `meta` 11 in `text.secondary` with its
matches in `accent.brass` — from the hit's own ranges, never searched
again — and at the right, in `text.faint`, the file's name and the line.
The lit row is `surface.raised`, and there is no hover tint. Enter or a
click opens the file at the match, with the match selected.

**The preview** sits under the hits while there are hits, 220 high on
`surface.sunken` behind a hairline: eleven lines of the lit row's file
around its line, **as the editor draws code** — the code face at the code
size and the code leading, `leading::CODE` — **coloured by the grammar
the file's tab would use** and painted whole, so a block comment opened
above the window is still a comment. The line numbers are right-aligned in
a 40-wide column (`DIFF_NUMBERS`) in `text.gutter`; the hit's line is
`surface.raised`, and its match is `accent.brass` over whatever colour it
crosses — **only while that line is still the hit's own text**, so a file
changed under the popup marks nothing rather than the wrong words. It reads
an open tab's buffer before the disk, and a file past the tab's highlight
limit is shown plain with its match still marked. It follows the lit row.

**The foot** is one line behind a hairline: at its right *Open in Find
Window* with `⌘↩` after it in `text.faint`, a 24-high button in
`text.secondary`. It, or `cmd-enter` with or without a lit row, hands the
question — the query, the toggles, the mask as applied and the scope — to
the Search tool (6.23), which searches again with the keyboard in its
field; the card closes. The hits are not handed over: the tool walks.

**The pin.** An unpinned card has the dimmed backdrop and closes when a
hit is opened. **A pinned card has no backdrop** and stays: a click in
the editor behind it lands in the editor, and Escape belongs to whatever
holds the focus — the editor's, or the program's in a terminal — and
closes the card only while the card holds it. `cmd-shift-f` gives a card
that lost the focus the focus back; the cross closes it from anywhere.

**Three emptinesses**, as the tool has them: *Type to search this
project*, *Nothing matches*, and nothing in the body while a walk runs or
a pattern is refused, because the head is already saying it.

### 6.35 The start screen — M14.5

What a worktree shows with no tab open and no Settings page:
an empty workspace's functions in Guild's shapes (decision 48). The project-less window keeps M5.11's hints — there is no
folder to read a history for and nothing to start in it.

**A column 560 wide**, centred in the work's card (4.1), the whole of it
centred vertically in the room under the tab strip and clipped by the
card rather than wrapped, as the hints were. From the top, 24 between
the parts:

| Part | What it is |
|---|---|
| **Heading** | The worktree's folder name at `section` 20, weight 600, `text.primary`; under it, centred, `⎇ <branch> · choose what to start` at `meta` 11.5 in `text.tertiary` — one line of text, centred as one, not a glyph placed beside a computed string. 18 under it, the card |
| **The default card** | 60 high, `surface.card`, radius 8, 13 in at its sides. A disc 32 in `surface.raised` with the harness's mark at 20; 10 after it its name at `card_title` 13.5/500 `text.primary`, and under it *Default* at `meta` in `text.tertiary`; `↵` at the right in `text.faint`. **Its outline is the lit state**: `accent.brass` at .38, one point — the one outline on the screen, rule 2 |
| **All tools** | A heading in a menu heading's voice (6.17): `eyebrow` 11, weight 600, `text.faint`, then 10 after it a 1-point `divider` rule to the column's edge, in a row 24 high; 8 under it what it heads |
| **Tiles** | One per kind of tab, in the harness menu's order (6.5), 8 apart, 84 high, `surface.card` at radius 8, no border, 10 in at the sides and never narrower than 58 — the mark's 22 with the 18 of air above it at either side, where the key sits clear of it. The mark at 22, 18 from the top; the name at `ui` 12.5 in `text.secondary` 10 under it; the key at `eyebrow` in the code face, `text.faint`, 10 from the top-right corner. Under the pointer `surface.raised` and the name in `text.primary`. **A harness not on the `PATH`** (6.5): the mark greyed, the name in `text.faint`, *not on PATH* under it at `eyebrow`; it takes no pointer, no click, and its key does nothing. The terminal is never greyed |
| **Recent** | The heading as above, its rule stopping 10 before **the chips** at the right: the Git dock's tabs (6.13) — 24 high, radius 6, padded 8, 4 apart, `eyebrow` 11 weight 500; the chosen one on `surface.raised` in `text.primary` with its count in `text.tertiary`, the others and their counts `text.faint`; the count 10 after the name. *All* first, then a chip per harness **that has at least one session**; no chips at all when only one harness has any. No hover tint: a second raised chip would read as a second choice |
| **Rows** | A menu row's measure (6.17): 32 high, radius 6, drawn 8 wider than the column on each side and inset 8, so the mark stands on the headings' line. The harness mark at 14, the title 10 after it at `body` 13 in `text.secondary`, cut with an ellipsis, one line; the age at the right at `meta` in `text.faint`, in `Ui::ago`'s words. **The lit row** is `surface.raised`, its title `text.primary`, and *Resume ↵* at `meta` in `text.tertiary` in place of its age. A scrolled list draws the picker's thumb (6.32) |
| **Foot** | A 1-point `divider` rule, then 10 under it *Settings* at `meta` in `text.faint`, with the cog at 13, 6 before its word — it opens Settings at *Sessions*, where the default tab is chosen |

**The list is as long as the history, at most 50 rows**, and scrolls
inside the room left between the tiles and the foot; the heading, the
card, the tiles and the foot do not move. **One highlight on the whole
screen** (6.32's rules 1 and 2): on the default card when the screen
appears, moved by the arrows and by the pointer — on a move, never a
hover tint in `surface.raised` — and never two. A tile under the pointer
is not the highlight: tiles are reached by their keys and a click, and
`enter` never takes one.

**No sessions at all** — the heading *Recent* stays, and under it one
line at `body` in `text.faint`: *No sessions in this worktree yet.*
**Still reading** — the rows' room stays empty, and so does it for the
moment between a worktree switch and the new folder's first answer:
the previous folder's rows never stand under this one's name. Nothing
flickers in and out; a read takes tens of milliseconds and a spinner
would be seen more often than the list.

| Measure | Value | Token |
|---|---|---|
| the column | 560 | `START_COLUMN` |
| between the parts | 24 | `START_GAP` |
| the default card, its disc, its mark | 60, 32, 20 | `START_CARD`, `START_DISC`, `START_MARK` |
| a tile, its mark | 84, 22 | `START_TILE`, `START_TILE_MARK` |
| a row, its mark | 32, 14 | `MENU_ROW`, `STATUS_ICON` |
| the foot's cog | 13 | `START_COG` |

### 6.36 Areas and panes — M12.8

The mockups are the person's A1 and P3 (https://claude.ai/artifact/VWcFLoLgjyiZGrH8Vw9thW).

**Four levels, and what each keeps.** A *seat* divides into *areas*; an area has a row of tabs and one in front; a *tab* divides into *panes*; a pane shows one body or nothing. The last area stays, a tab has at least one pane, a tab is in exactly one area at every moment, and an area or a pane is at least 320 across and 160 down (`AREA_MIN_WIDTH`, `AREA_MIN_HEIGHT`) — a split that would give the new side less is refused with a notice, not made narrow. The room is measured from the work's card, not from the window.

**Areas (A1).** Each area is 4.1's card, 6 apart (`WORK_BETWEEN`). The focused area's front tab is underlined 2 in `accent.patina`; every other area's front tab keeps its name in `text.secondary` and its underline in `text.gutter`. **One outline, on the innermost place with the keys** (the person's, 2026-09-30): when the seat has more than one area, the focused area's card edge is `accent.patina` at .38 (`LIT_OUTLINE`) if its front tab is undivided, and the focused pane's outline, the same patina, if it is divided — never both; the underline alone did not say where the keys were. Every other area, a lone area and a divided front tab's area edge keep `divider`.

**Panes (P3).** Panes are divided by a 1 hairline in `divider`, its grip the same 9 over it and drawing nothing. The focused pane is outlined 1 in `accent.patina` at .38 (`LIT_OUTLINE`), radius 6 (`radius::PANE`), 2 in from the pane's edge (`PANE_INSET`) — only when the tab has two or more panes. A pane's edge is where the hairline beside it ends: the line belongs to neither pane, so each outline is 2 from it. A divided tab shows the split glyph, Lucide's `columns-2`, after its name in the strip.

**An empty area or pane is 6.35's start screen**, and it is compact: the default card always; the tiles and the heading while the place has the height for them, the heading going first; *Recent* only from 440 high (`START_RECENT_FROM`), the foot only from 520 (`START_FOOT_FROM`), and neither without the rest. **A part goes rather than being cut** — amended by M12.8's second window check, when a pane 213 high cut its tiles in half: the heading, the card and the tiles need about 320 and a place may be 160. The heights are the parts' own numbers added up (`start::fits`), with the tiles in two rows where the place is under 408 across — and there they are drawn as two rows of two, the rows the height was counted for (M12.8's third window check). The column's 560 is a maximum; the foot's hint wraps rather than running under *Settings*. Only the focused place's copy takes the keys.

**The `⋮` at the strip's end**, after the three that close: *Split Right* `⌃⌘⇧→`, *Split Down* `⌃⌘⇧↓`, *Split Tab Right* `⌃⌘⌥→`, *Split Tab Down* `⌃⌘⌥↓`, *Close Area* — the last disabled with *the last area stays* in its right slot when there is one area. **The tab's menu**, after its closings: *Move to Area ▸*, then *Split Tab Right* and *Split Tab Down*. *Move to Area* lists the other areas by where they are — *Left Area*, *Right Area*, *Area Above*, *Area Below* — a rule, then *New Area to the Right* and *New Area Below*. An area has no name of its own.

**Strokes** (as the window prints them): `⌃⌘` and an arrow moves the focus to the neighbouring pane or area, over one geometry, and does nothing at the window's edge; `⌃⌘⇧→` and `⌃⌘⇧↓` split the area; `⌃⌘⌥→` and `⌃⌘⌥↓` split the pane; `⌃⌘⇧W` closes the area; `⌘W` closes the focused pane when its tab has more than one, else the tab.

| Measure | Value | Token |
|---|---|---|
| an area or a pane, at least | 320 across, 160 down | `AREA_MIN_WIDTH`, `AREA_MIN_HEIGHT` |
| the gap between areas | 6 | `WORK_BETWEEN` |
| the focused pane's outline | 1, radius 6, 2 in | `radius::PANE`, `PANE_INSET` |
| the compact start screen | Recent from 440, the foot from 520 | `START_RECENT_FROM`, `START_FOOT_FROM` |

### 6.31 Push Commits — M8.3.6

A Push Commits dialog, as a window of its own. It opens 1000 × 620, centred, and then wherever it was left. It has the system's title bar.

**Updated on 2026-10-03.** Keep the existing branch, commit inspection,
file tree and push behavior. All surfaces, selection, typography and
controls follow the selected Guild palette. The earlier dialog-only
colour overrides are retired; changing the application theme also changes
Push Commits. The continuous background is `surface.app`, hover is
`surface.raised`, selection is `accent.patina_bg`, links use `accent.patina`, checked
controls use `accent.brass`, borders are `surface.divider`, and text uses
the normal `text` roles.

**Interface scaling, 2026-10-02.** Push and the separate diff window use
the same rem conversion as the main window: 16px per rem at the base
13px interface font, scaled by the chosen size. Their frames and
tooltips use the selected UI family. A size or family change refreshes
already-open windows. Push's rows and controls scale with their text;
hairlines, pointer grips and window coordinates stay in pixels. Below
900 logical pixels of width (viewport width divided by the UI scale),
the footer becomes two 48-point rows, with Cancel and Push right-aligned
on the second. Both rows remain visible at the maximum 20px UI size in
the default 1000-wide window. The artboard's `interfaceSize` previews
13, 16 and 20; its system title bar is illustrative.

- **Left side, on `surface.app`.**
  - **The branch row**, 28 high: `branch → remote : target` at `body` 13. One repository needs no repository name or checkbox here. Remote and target are links: choose a listed remote, or edit the target with Guild's shared line-editing rules. A new upstream has a green *New* label at `meta`.
  - **The commits**, one row each: 26 high, indented 34, the summary at `body` 13 in `text.primary`. Selection uses `accent.patina_bg`; hover uses `surface.card`. The branch is selected on opening. A plain click selects one commit, Command/Control-click toggles a commit, Shift-click spans from the anchor. Selection changes inspection, never what Push sends.
  - Selected commits combine changes: a file's first before and last after, following renames and cancelling an added-then-deleted file. Hashes and paths remain immutable while inspected; a skipped commit does not contribute files or a later blob for an otherwise untouched selected file.
- **The grip between the sides** is 9 wide with a 1 line in `divider`, and the resize cursor under the hand — nothing lights. The left side's share is dragged and remembered.
- **Right side, on the same `surface.app`.**
  - **A toolbar** 30 high: *Show Diff*, a rule, *Diff Preview*, *Edit Source*, a rule, *Show Details*; *Expand All* and *Collapse All* at the right. File actions are dim without a selected file. Preview uses the existing diff view inside the dialog; Show Diff opens the separate window. Edit Source opens the working file in the main window.
  - **The files tree.** Its root is the repository and file count. Rows are 24 high and indented 12 + 18 a level. A directory row has a chevron, its folder icon, its name, and *N files* in `text.faint`; single-child directory chains collapse. A file row has its icon and its name in the UI face at `body` 13, coloured by its mark. Selected files use `accent.patina_bg`. A click selects a file, and a double click opens its diff. An empty list says *No commits selected*, centred.
  - **The grip** between the tree and the message.
  - **The details**, shown only for exactly one selected commit while Show Details is on, in the UI face at `ui` 12.5: the summary in bold, the body, then the author, date and sha. Hidden details leave the whole height to the tree.
- **A refusal** is a band 48 high on `accent.rust_bg`, with two lines of git's verdict in `accent.rust`, above the foot.
- **The foot**, 48 high with a `divider` above:
  - centred Help, a Guild checkbox, *Push tags:*, and a select 26 high opening a menu with *All* and *Current Branch*;
  - a Guild checkbox and *Run Git hooks*, on by default; off passes `--no-verify` for this push only;
  - *Cancel*, outlined;
  - *Push*, 28 high on `accent.brass_bg` with `accent.brass` text, then a 28-wide chevron; disabled text on the dialog background while nothing can be pushed. Its menu contains normal Push only; Force Push remains excluded.
  - Return pushes. A push that lands closes the window.

### 6.32 The diff window — M8.3.6

A commit's file in a window of its own, titled `name (folder)`. The body is the diff tab of section 6.10. Its toolbar gains, after ↑ ↓:
- a rule, then the pencil (*Edit Source*), which opens the file in the main window at its first change;
- a rule, then ←, *i / N files* in `accent.patina`, and →. Each arrow is `text.gutter` at its end.

`ctrl-shift-←` and `→` step between the files, and `f7` between the changes.

## 7. Notes for the GPUI implementation

Written down so as not to work them out again later. **Every point needs checking against reality** — GPUI's API is pre-1.0 and changes between versions.

- **Fonts.** The families are embedded in the binary and registered at runtime. **Done in M5.9 (2026-09-05):** Inter and JetBrains Mono, eight faces, 2.7 MB, registered before the window opens — `desktop::fonts`. What is *chosen* is still a setting, and a person's own family wins over both; the terminal's preference for an installed Nerd Font is unchanged and is the reason the two are resolved differently. **What it was until then, kept because it is what the numbers were measured against:** the interface in the system sans, and code — the terminal and the editor alike — in a face chosen from what is installed, a Nerd Font Mono preferred because shell prompts are drawn with its glyphs (M5.1). Both are still settings, as they have been since M5.5, and M5.9 turned the setting from a button that cycled into a list where every row is drawn in the face it names.
- **Inset outlines.** The "active card" device is built on an `inset box-shadow`. If GPUI does not support an inner shadow, we draw the outline as a separate layer or as a one-pixel border in `rgba(accent, .24)` — visually equivalent.
- **Indicator halo.** Also an outer shadow with zero offset. The alternative without shadows: two nested circles — an outer 13×13 of a semi-transparent accent, an inner 7×7 of the solid one.
- **Syntax highlighting.** `gpui-component` carries a code editor component with LSP support — the theme colours are given by the tokens from section 2.4; we do not write our own highlighting. **Half true since M8.1.1, and the half matters:** the component paints a file in a tab and can reach nothing else, because it has no diff view. So the diff, Annotate and a commit's files are painted by `desktop::syntax`, ours, from the same grammars, the same queries and the same table — two engines that agree by construction rather than by care, and the rule for a capture inside another is copied from the component's own rather than invented. The component would stop painting anything the day it gains a seam that takes ready-made spans; that is also the only way a grammar from outside could ever paint a tab, because a wasm-loaded language parses only on a parser with a wasm store and the component makes its own without one. **Checked on 2026-09-02, task 0 of M8.1:** it holds, on four terms. Its `Root` must be the window's root view and ours its child; it binds `tab` in that root and the terminal unbinds it with `NoAction`; the editor takes the mono face, its size and the 1.70 leading on the `Input` element, not from its theme; and its grammar feature — twenty-eight grammars, 31 MB — stays off, with six grammars registered by hand for 14 MB. The seven colours of section 2.4 reach its highlighter through a theme built from them.
- **Virtual lists.** The file tree and the diff have to be virtual: `gpui-component` has tables with virtual scrolling over hundreds of thousands of rows, and the tree is built on the same mechanism. **Read against what was done, M8.1:** the tree is GPUI's own `uniform_list` — rows of one height, only the visible ones built — and the editor carries its own scrolling; the component's table was not needed for either. **M8.2:** the diff is the same `uniform_list`, every row 26 high — a file's header, an `@@` row, a line — drawn by us and not by the editor component, which has no diff view; a ten-thousand-line diff costs its visible rows. **M8.3:** side by side is the same list, each row two halves; the log and the annotate tab are the same list again. **M8.3.1:** a tool's drag is GPUI's own `on_drag` with the button as the preview, and the drop zones are drawn only while the drag lives — a drop listener is registered per frame against that frame's hitbox, so a zone that exists at release is a target.
- **The window frame.** The traffic lights, the window's rounded corners and its shadow come from the system. In the mockups they are drawn purely for presentation — we do not reproduce them in the application.
- **The gradient backdrop** from the mockups is not needed in the application.

---

## 8. What in the mockups is a placeholder

So that nobody takes it for a requirement:

- **The interface language.** The mockups are in Russian; that was chosen to make them easy to judge. The question is now settled the other way: the application ships in English with i18n built in, and the language is chosen in the settings.
- **The content is illustrative.** The amounts and the token counters are plausible but invented.
- **The mockups are static.** They have no hover, focus, loading, empty-list or error states — those are designed during implementation.


## Settings redesign — 2026-10-03

Since `two-themes-customize` the Theme row is System / Light / Dark; a file's old `theme` value (`cloud`, `islands`, `slate`…) is kept as it was read and only migrated: `cloud` and `light` open Light, the rest Dark.

Navigation: General, Appearance, Terminal, Sessions, Projects, Editor, Keymap. Terminal typography belongs to Terminal; local logs belong to General. Search spans translated titles, descriptions, grouping, persisted keys and shortcut names. Values save immediately through the existing setters. No provider accounts or new worktree settings are introduced.

Native layout tokens (`theme::settings`, logical points at default scale): navigation 180, content maximum 820, padding 24, group gap 28, control 32, switch 38, swatch 48, label minimum 170, number width 36. Below 600 points of actual settings width navigation wraps above content; rows and font presets wrap. Theme swatches use four columns above 780 points and two below. These native compact adjustments preserve access in split areas. Existing typography and radii remain unchanged. Focus uses the palette's patina border; switches and menus have keyboard tab stops. Menus reuse Guild's focus and edge placement engine.

### File selection and current tool shortcuts — 2026-10-03

Commit uses Cmd+K (the Latin physical key through the platform Command layout);
Run (formerly Output) uses Cmd+4; Find uses Cmd+3; Git Log uses Cmd+9. Explicit keymap overrides win. Terminal font buttons and
shortcuts change the saved size by 0.5 points, within the existing limits.

Commit, the files of a Git Log commit (including file history), and Push
files share path-based selection: plain click selects one, Command-click
toggles, Shift-click selects the visible range from the anchor. Shift with
Up/Down or Home/End extends; Command-A selects visible rows. Command with
Up/Down or Home/End moves the selection exactly as the plain keys do, and
Command- or Control-Space toggles nothing — decision 54: no cursor walks
apart from the selection. Copy copies selected paths. Refresh and folding remove
paths that are no longer visible. Modifier selection does not open a diff.
Commit's Space includes/excludes the selected non-conflicting files, and
Enter opens their diffs. Log Enter opens selected diffs. Push's file list
has its own focus; Enter there opens the focused diff, never pushes.
Context menus, rollback, delete and Add to VCS retain their named row as
target; no destructive bulk actions are added.

Project and External Libraries file rows share selection keyed by source
and relative path, so namesakes in separate roots stay distinct. Selected
rows have section 6.4's fill; since decision 54 the file cursor has no
outline of its own. Ordinary Up/Down and Home/End visit visible rows in the current
source tree; selection modifiers keep their file-only range semantics. Copy uses
full paths across roots; Enter opens selected files through their own
origins. Directories keep folding and explicit single-row context actions.
Completing a project name selects its project row and clears any library cursor.
Closing the name input by success or Escape returns focus to Explorer; opening
an editor after successful creation or rename takes focus normally. Refused
names keep the input focused for correction.
Find and Search select FILE groups while individual matches keep location
navigation. Ordinary arrows can leave a selected file group for a match or
directory and return to a visibly selected file; Enter then folds that group.
Explicit file selection (file clicks, modified selection keys or Select All)
keeps Enter opening selected files (Find follows the first usage link per file,
including external source URIs). Modifier file clicks select without opening/folding. Search results have separate focus from query input.
Recent Files/locations and the Find in Files popup remain location choosers.
Folding prunes invisible file identities; a deselected cursor is never an
implicit file action target. User-specified context-menu rows remain explicit
single targets. No destructive bulk action is introduced.


### Shared text field editing — 2026-10-07 correction

All Guild-owned single-line fields use InputState through desktop::text_field.
Keep the existing inset surfaces, row radii and field borders; the text uses the
UI scale, and the caret and selection are the component's, not drawn characters.
Fields support standard macOS selection, navigation, clipboard and undo/redo,
plus Guild's word/line deletion. A pointer selects text without dragging the
surrounding picker or sending input to a terminal. File buffers and multiline
commit/review fields keep their established component modes.

**2026-10-26 hint correction:** Empty editable fields show their placeholder in
`text.muted` while unfocused and hide it while focused. This is presentation
only: a real value, suggested path, history entry or task default is never
cleared. The same rule covers the Commit textarea; multiline query fields with
no hint remain empty. Accessible names remain available while hints are hidden.

### Focused list speed search (2026-10-09)

Explorer, Commit files, Push commits/files and Git Log's selected-commit files
show a transient TextField immediately above their scrolling list when typing
begins in that list. Its input is `HEAD_TAB` high, with `TREE_PADDING` at the
sides, `radius.ROW`, an inset surface and a one-pixel patina border; the border
uses rust for a nonempty query with no matches. Matching original text spans
use `accent.brass_bg` and `text.primary`. The current match is the list's
selection — decision 54, replacing the patina outline it had — and inclusion
ticks stay as they were.
Push commit search uses the existing inspection selection. Rows retain their
positions; folded descendants stay hidden. Escape and leaving the list dismiss
the field. Existing Git Log filters and picker fields keep their own behavior.

### Terminal light-theme semantics (2026-10-08)

The default terminal foreground/background follow `text.primary`/`surface.app`.
Explicit ANSI white/bright-white stay light: Light reuses the retired Slate's
`text.secondary`/`text.primary` (`#b4b4b9`, `#fafafc`), preserving readable text on an application's
explicit dark background. Light ANSI blue uses the current `accent.patina`.
Truecolour and OSC overrides remain the program's own values. Shell prompt
context/status/directory segments use the default background in new light
Agnoster sessions, with the path in ANSI blue; Guild passes supported variables
at launch, without editing shell files. Claude launched by Guild receives the
automatic application theme in its per-launch settings. These are application
launch hints, not a filter rewriting arbitrary terminal output.

Terminal Jobs uses the existing terminal SVG in a `STATUS_BUTTON` square, with
a tooltip naming the list. Its circular frame uses `surface.card`, a
`surface.divider` border and `surface.raised` hover, matching Open In and Settings.
Terminal overlay close controls use the component Close SVG in the same square,
centered independently of text metrics. Terminal Jobs has no close icon.

The title bar omits the legacy native Review launcher. Native task/history
surfaces remain available for existing tasks and project settings.

The gear opens Guild Settings directly for mouse, Enter and Space activation,
matching Cmd+Comma. The legacy project review editor is no longer in settings
navigation; saved Review tabs restore as Settings. Native archives and profiles
remain unchanged. This supersedes the earlier settings-menu entry.

### Markdown selection overlay — 2026-10-08

Preview uses `accent.brass` at `alpha.MARKDOWN_SELECTION = 0.25`
(`--alpha-markdown-selection: 25%`). The rich-text component paints selection
over glyphs, so an opaque background would hide the selected text. Source
editor selection keeps its existing background.

### Panel corner controls — 2026-10-08

This supersedes the placement in sections 6.9 and 6.11. The outer areas
surface supplies one horizontal geometry for its cards and frame controls,
independent of focused area or pane. Back/Forward start at the whole work
surface left edge (the workspace column boundary), above the left dock when
open and above the areas when closed. Guild Update follows navigation,
retaining release notes and restart behavior. The current branch begins at
the left edge of the open left dock in the footer, or the main areas edge when the dock is closed, followed by the Update branch button (hint: *Update branch…*), with no source
label beside it — 2026-11-17. A click on it, or Enter, Space or an arrow while it has
the focus, opens the Update branch source popup under it (section 6.18); it never
updates by itself. Both branch lists open upward, confined
to the viewport. Source selection uses the shared TextField and picker and
never switches branches.

The path label and footer Push button are removed. File > Push Commits and
its shortcut open the existing dialog. File facts, tool buttons and session
controls remain. Frame controls use existing META, TITLE_CHIP, ROW, CARD_GAP
and STATUS_BUTTON tokens. The main surface keeps its existing width when
hidden: history x=max(workspace column boundary, traffic-light inset + STATUS_BUTTON + CARD_GAP), with WORK_INSET replacing traffic lights in fullscreen. Windowed inset is max(TRAFFIC_LIGHTS_MIN, scaled TRAFFIC_LIGHTS): at least 82 physical points, because the native buttons do not shrink with the interface. The Workspaces toggle sits at that inset, using the rail-column icon, STATUS_BUTTON size/radius and surface.raised while pinned; its clicks never reach the titlebar zoom gesture. Guild Update
follows history when clamped. Branch glyph x=work-card left edge + unclamped Back glyph inset, including when folded; its hitbox begins at the card edge. Normal pinned/dock
layouts follow the leftmost work card edge exactly; these explicit clamps apply only
when controls would collide. Dock widths retain their existing behavior.

Guild Update uses STATUS_BUTTON height, PILL_PADDING, a full rounded outline,
surface.card, a divider border, brass text and surface.raised hover, matching
titlebar control anatomy while keeping the update affordance recognizable.
The release-notes tooltip and click handler are unchanged. FileMenu includes
Push Commits with Cmd+Shift+K.

Update results use existing notices with an Open Guild Terminal button,
addressed to the originating worktree. Conflict guidance names manual
resolution (git add and git commit) and git merge --abort. Success records
old/new HEAD and distinguishes merge-commit revert from fast-forward commit
inspection/reverts. The latest per-worktree receipt persists in settings and
is summarised in one line under the Update branch source popup after the notice
fades — the outcome only; hashes and recovery steps stay in the notice and Console.

All tools except Explorer (including Commit, Git, Run, Terminal, Problems and Find) are centered on the whole work surface
excluding workspaces independently of the branch labels. The footer always uses
one row: the branch label shares the budget beside centered tools, icon gaps
shrink, and the geometric window minimum prevents controls from colliding.

Bottom tool widths and right-side facts are measured independently of placement.
Changed measurements defer a Root notification until after prepaint so the next
frame applies the minimum, gaps and label budgets. Wider badges/facts raise the
geometric window minimum; no branch or tool row is added. Closed side docks
retain independently normalized sizes; only two open docks share a width budget.
Recovery diagnostics are limited to a short excerpt; full output stays in Console.
Structured saved receipts are formatted in the current language. A no-op/refusal
does not replace the most recent merge, fast-forward or conflict recovery record.


### Work surface and dock terminals (2026-10-08)

The pinned workspace column and work surface flex above the full-window footer.
The bottom dock stays beside the column, beneath upper cards and above the footer.
The footer remains one STATUS_BAR (28) line at the window bottom when docks open,
close or resize, with WORK_INSET under its buttons. Only Open and Get from Version Control occupy the column-owned slot on that footer
baseline. Workspace marks and + stay centered inside the card above the footer,
below scrolling project pages. Folding hides both rows; peeking shows one copy. The toggle is in the titlebar after the traffic lights (WORK_INSET in fullscreen). Saved counts above five remain reachable through
horizontal overflow, keyboard traversal and the existing column carousel; peeking
never draws a duplicate switcher and folding reclaims the column width.
New workspace creation is limited to five at submission, shared by UI and CLI;
+ is hidden at capacity and returns below five. Loading never truncates workspaces.
Explorer appears once after the branch/update group with CARD_GAP (8),
retaining its assigned dock. Without
a Git branch, Explorer begins at WORK_INSET with no branch/update placeholder. Commit
and all other tools stay centered over the whole work surface independently of
saved dock assignments. The pinned column owns its footer slot; a peeking column occludes the folded footer beneath the same slot. When folded, branch/update and Explorer start at the leftmost work card inset. One to five marks fit the minimum pinned column width without scrolling
through the supported UI scale range; legacy counts above five scroll.
Icon gaps shrink instead of adding compact rows.
The dock grip occupies WORK_GAP above the bottom card, up to nine physical pixels,
clamped clear of header controls at small scales. Measured footer height is excluded
from dock resize bounds and oversized restored docks leave upper work visible.
Update branch retains the existing arrow-down icon.

Terminal is a separate movable tool, initially in the bottom dock. Its shared
panel header holds the draggable title, numbered shell tabs with close buttons,
and a plus immediately after the tabs; overflowing tabs scroll horizontally.
The selected tab uses surface.raised and text.primary. The body is the existing
terminal view, including its selection, paste, find and context menu. Each
project/worktree owns its active tab and live shells. Hiding, moving or changing
projects does not restart shells; closing a tab releases only its PTY. Closing
the last tab leaves an empty panel with the plus available. Restarting Guild
does not restore shell processes. See TerminalWorkspace and TerminalPanel
artboards.

The 30-point workspace identity row retains identity and rename, with Projects
on the following 24-point line. Open and Clone live on the column-owned footer
baseline. There is no head Hide/Pin button; the title-bar toggle pins a peek.

When titlebar controls do not fit beside the work-surface anchor, only the
empty alignment space before navigation shrinks, down to the Workspaces toggle plus CARD_GAP. Existing buttons keep their natural widths and Settings stays reachable.

### Terminal Jobs recovery — 2026-10-08

Its card carries §6.17’s `card_outline`; the opening button stays unchanged.

Terminal Jobs uses popup::card_look, ROW radius, BODY/META typography and
MENU_INSET/MENU_PADDING. TERMINAL_JOBS_WIDTH is 400 px and
TERMINAL_JOBS_HEIGHT is 420 px; both shrink to the actual viewport. A fixed
header contains the title, shared scope menu and a short opening-context line above
scrolling groups. This session is the default for a focused conversation; file/empty
panes default to This worktree, and a window without a root to All launches.
This project spans roots sharing the repository identity. Unavailable scopes are
disabled with an explanation; empty areas offer explicit widening. Each opening
resets the scope, and each scope change resets the rows' scroll position.
Ongoing (starting/running/cancelling) precedes Completed; each group sorts original
launch times descending, unknown last, with the stable job-ID tie-breaker.
Broader rows show distinguishing original worktree/session context and All launches
adds the repository. Last observed branch is explicitly an asynchronous cache
observation, never freshly verified historical HEAD. It does not filter history.
The title and broader context expose bounded full-path details, including distinct
literal program cwd. Unknown legacy context remains explicit; today's labels and
resume bindings never reinterpret it. The nested shared scope menu outranks the
list and returns focus to it; no menu or dismissal key reaches a terminal behind it. Each row has a truncated
title beside process status, a calendar date/time (including seconds), and small
wrapping ghost actions. Unknown legacy launch times remain explicitly unknown.
Running status uses patina; text, surfaces and borders follow the selected palette.

Outside clicks, Escape and the trigger dismiss; other popups and project/tab
switching replace the list. A backdrop outranks terminal overlays and the list
outranks its backdrop. Escape returns prior focus; showing output gives focus
to that output. No list close icon is drawn.

Show output and Delete saved output describe their effects. Explicit deletion
leaves a visible Saved output deleted row; restart without a buffer says Output
unavailable in this Guild session. A completed terminal without text explains
that a program may have cleared its screen. No transcript is synthesized.

Terminal output starts at the existing compact host size (650 × 400 px), clamped
to its pane. Explicit producer percentages set initial geometry, and subsequent
user movement/resizing remains authoritative. Completed output shows process exit
status, never a review verdict. Canonical artboard sources are updated locally.

User correction, 2026-10-08: launches on the current local day show only the time (including seconds); earlier days show the date and time. Unknown legacy times remain unknown. Local midnight, rather than UTC midnight, determines the day.


### Bounded notifications — 2026-10-10

Notices stay at the bottom right, `space::NOTICE_INSET` (16) from the viewport edges.
Their width is at most `space::NOTICE` (320), reduced by the current viewport.
The available height, after both insets and `space::CARD_GAP` (8) between cards,
is shared by the visible one to three cards. Each is naturally sized up to that
share; integer rounding must not push their combined stack past the top inset.
Padding remains `space::CARD_PADDING` (13), with the existing card radius and
level colors. A minimum-zero text area wraps even unbroken Unicode text and
scrolls vertically. Copy and Close stay outside that scroller, as does an
optional explicit Open Guild Terminal action. Copy preserves the full original
message. Only Close dismisses: clicking or scrolling text must not dismiss it,
move keyboard focus, or reach the surface behind it. Information pauses its
fade under the whole card, including actions.

Footer labels share the left half-budget after reserving the centered tools,
fixed branch glyph and buttons. Entering/leaving Update
never changes the footer height. Centered tools never wrap: their gaps shrink
from STATUS_BUTTON_GAP to zero before resizing stops at the native minimum.
StatusBar's branchLabel previews a long truncated label. A restored oversized
bottom dock starts mouse and keyboard resizing from its displayed height. The
height limit reserves both WORK_GAP spaces, borders and the measured footer,
rounded up to preserve at least 120 pt of upper work at supported UI scales.

Refreshed branch labels retain the same footer parent and baseline.

### Settings hierarchy and softer shared corners — 2026-10-13

The user explicitly requested stronger structural separation and softer corners
throughout Guild. The current shared radius::ROW is 8 and radius::CARD is 14;
this supersedes historical row/card values above wherever those shared tokens
are used. WORK remains 14, pane outlines/status controls/checkboxes retain their
own measurements. Settings navigation blends surface.panel and surface.sunken equally, with a right outline
(or bottom outline when stacked). Section icons use already vendored Lucide
assets with explicit tint. Group headings use META/semibold in text.tertiary,
rather than competing with page titles. Settings boundaries mix surface.divider
with text.faint at 50 percent; underlying editor/palette dividers are unchanged.
The directory picker belongs to the existing field row, with the future-only
hint alongside its label, instead of a separate empty action row.

Settings frame correction (2026-10-13): navigation is continuous from the top
of its host. The full-width Settings header is removed; the existing exit
action is an unframed Back row above search, retaining its host close handler.
The selected section title and scope remain in content, without duplication.
The compact host stacks the same navigation above content.

Settings surface refinement (2026-10-13): group backgrounds mix sunken with
35% panel rather than using the brighter generic card. Group outlines retain
divider/faint 50%; internal rules use 20% faint. Inactive Settings value buttons
have transparent grounds and visible outlines; hover/focus/selected states stay
intact. Inactive navigation labels use tertiary text, selected labels patina.
This is limited to Settings; other screens retain their existing surfaces.

Settings background correction (2026-10-13): the user's final direction is
one continuous sunken background for navigation and content. This supersedes
the earlier panel/sunken blend for navigation. The outline between regions
remains, including the bottom boundary when the compact host stacks them.

User refinement (2026-10-13): usage/Open In/terminal-job titlebar controls use
the same visible outline as Settings. The titlebar Settings gear is removed;
File adds the existing OpenSettings action, with the Guild application-menu
entry and Cmd+, retained. Group captions in Settings are uppercase, including
Russian, while page titles and navigation labels retain their normal case.

Terminal Jobs discoverability (2026-10-13): its outlined titlebar trigger stays
visible when no receipts exist. Opening it reuses the existing scoped empty
state and does not start a job or subprocess. Existing recovery/scope actions
and ownership guards remain unchanged.
Worktree activity (2026-10-12): workspace → repositories → worktrees → sessions.
Each scoped session counts once, including hidden tabs and panes. Primary
priority is waiting > observed failure > working > unread > idle; busy removal
checks any waiting/working session across every workspace independently.
The leading activity control opens the existing native menu with harness marks,
titles and waiting reasons. Normal row clicks still select the worktree. Enter
or Space opens disclosure; arrows and Enter choose; Escape/outside dismiss and
restore focus. Selection resolves the current exact session tab/pane again.
Green means an unread finished answer, not verified task success. Workspace
selection never bulk clears hidden answers; genuinely visible panes retain Seen.
Working rotation uses native GPUI canvas with an alpha-tapered vector, mounted
only on visible working indicators, including when Guild is inactive. Brand marks
and caret timing are independent.
Local artboards are updated.

### Activity sizing correction — 2026-10-12

The new activity ring is 14 points, question/failure vectors 16 points and dot
8 points: a two-point increase over the first implementation. `theme::activity`
reuses the existing 14-point heading and 16-point icon sizes at base scale; these
activity marks stay fixed in points across interface text sizes. The worktree
slot remains 14×18; workspace buttons use STATUS_BUTTON (22×22 at the default scale). Vectors stay centered with
at most one point of symmetric overflow in the slot, clear of the branch icon.
Harness/project identity icons keep their existing sizes and colours. Working
rings use `text.secondary` in every palette, including worktrees, default
workspace faces and disclosure's status slot beside the unchanged harness mark.
Native component rotation is linear and keeps constant neutral colour; caret
and brand timing remain separate. A frame sequence is required as evidence of
movement; one screenshot is only appearance evidence.

### Activity rotation refinement (2026-10-12)

An accepted video reference supersedes the uniform 0.8-second arc: use
1.2-second clockwise linear rotation, secondary gray theme tint, a smooth alpha
trailing arc and denser rounded head. Ring/glyph/dot remain 14/16/8 points in the
fixed leading slot. All three surfaces share the vector. Native frame requests
occur only after actual content clipping establishes visibility; scrolled-off
worktree and disclosure rows schedule none. Reduced motion keeps a static mark;
shared artboards and the accepted prototype disable the animation, preserving
the same static vector, neutral tint and fixed geometry.
Mouse activation preserves the prior input owner before default mouse-down focus;
keyboard activation returns to its trigger on dismissal.

### Terminal Jobs card refinement — 2026-10-13

The usage card is the presentation reference: BODY title at left, META status at
right on the same baseline, then secondary META time and labelled origin context.
Rows use MENU_ICON_GAP spacing, MENU_INSET horizontal padding, MENU_ICON_GAP top
and CARD_GAP bottom padding. Shared popup separators use ROW insets, MENU_RULE
vertical margins and card_outline. Rows with no available actions add no empty
button band. The fixed header and bounded scrolling remain unchanged.

Unavailable output no longer adds an explanation to the history row; its receipt
and independently available Open run remain. Empty and deleted output still have
explicit explanations. This worktree omits the row's worktree only when both
original and selected paths are known and equal and the header identifies the
selected root. Wider scopes and differing/unknown saved paths retain labelled
worktree/session context; legacy receipts retain their existing scope-specific
fallback strings. Full-path tooltips and their original text remain accessible.
This supersedes the restart-message presentation described above, without changing output state,
process ownership, recovery or keyboard behavior. Local artboards are updated.

### Visible background activity (2026-10-13)

Working rings continue rotating while the window is visible and another
application has focus. The default workspace faces remain visible summaries;
worktree rows and folded repository heads retain their shown-workspace guard.
A disabled session-menu action still displays live rotating work: disabled means
navigation is unavailable, not that the status is hidden. Actual paint clipping
and reduced motion bound frame requests. Idle, waiting, failed and finished marks
request no continuous frames. The inactive terminal caret remains steady under
its existing policy; reactivation does not introduce another animation clock.

Activity animation energy budget (2026-10-15, section 6.29): the 1.2-second ring rotation requests at most 20 decorative redraws per second per window, coalesced across visible marks. A recently repainted mark reuses event-driven frames instead of adding another redraw. Inactive visible windows retain motion; clipping, occlusion and reduced motion stop recurring demand. Input, terminal output and status changes retain their immediate notifications.

### Single footer baseline and window minimum — 2026-10-17

`desktop::frame` derives the main content minimum; the column owns its footer
Open/Get from Version Control buttons on the branch/tools baseline. Marks and +
stay centered inside the card above that line. The peek slot occludes
the folded footer and receives clicks without reaching the work beneath.
At UI13 the base minimum is **900 × 500 pt**. Derived width is 180 column + 7×22 tool
squares + 2×194 side budgets + 6 outer inset. Each side budget reserves two
22-point buttons, three 14-point glyph slots, four 6-point paddings, four
4-point gaps, three 8-point gaps and two 22-point label budgets. A second width
floor reserves two 160-point side docks, borders/gaps and 120-point work.
Height is 34 title + 28 footer + 2×6 dock gaps + 2 borders + 90 bottom dock +
120 work. Height uses **max(500 physical points, derived height)** at every scale;
the 286-point derived UI13 height overlapped Commit content and clipped the start
screen. Base minima at UI10/16/20 are **900×500 / 900×500 / 1024×500 pt**.
Width uses **max(900 physical points, derived width)**: the 728-point derived
UI13 minimum clipped the centre start screen with both side docks open.
Saved dimensions below either physical floor are clamped on restore too.
Wider actual badges/right metadata raise the measured minimum. Measurements are
normalized to UI scale so changing font size does not retain stale dimensions.
A pinned column is reduced toward 180 before consuming the footer budget.
Window restoration clamps saved dimensions and origin to the minimum and display;
column/dock restoration respects their model minima and the remaining work area.
Native contentMinSize is refreshed as scale/content changes; the ordinary native
fullscreen traffic-light reveal is unchanged.

### Control tooltip trial — 2026-10-23

Compact status/dock targets are 24 points with 2-point gaps: the 26-point pitch
is unchanged. Toolbar/head and status control glyphs are 16 points; heading
targets remain 24. Activity rings remain 14. The footer remains 28 points, using
a 4-point bottom inset instead of 6 to accommodate the larger targets. Panel
heights and user-set widths do not change. These trial numbers supersede the
older 22/14/4 control descriptions above.

Control tooltips are one line, 28 points high, BODY (13) text, radius ROW (6),
a 1-point card_outline and surface.raised background. Horizontal inset is 8,
label/shortcut gap 12, width follows content. No empty shortcut cell. Action
hints read the current keymap; unbound actions and plain labels invent nothing.
Rich informational cards keep their layouts. The supplied recording shows
initial pointer arrival near 0.733 seconds and the first tooltip fade near
0.967 at 30 fps, supporting an approximately 250 ms initial delay; this is
measurement evidence, not an exact source constant. Adjacent transition timing
needs live comparison and is not claimed identical. Local design only.

The final trial preserves the original width budget: footer group gap before
Explorer is 5 points, source-choice horizontal padding 3. These replace the
three 8-point label-air slots, absorbing the larger target/icon budget. Supported
window minima and user-saved column/panel widths are unchanged, including UI20's
1024-point minimum and a 230-point column in a 1100-point viewport. Multi-line
regex errors, branch reason/recovery and run information keep their formatting
through details tooltips. Dock Hide shows its current existing toggle binding.


### Open In Apps in General Settings

Use existing Settings cards, CARD_GAP/CARD_PADDING, BODY/META labels and CONTROL
fields. Builtin rows have real app icon/name/filename, enabled switch and compact
up/down controls. Custom rows use TextField for the display name, a chooser and
Remove; Add app reveals the new row. The chooser expands under that row with a
CONTROL-height TextField and BRANCH_LIST-height virtualized application list of
BRANCH_ROW rows. Search results carry actual app icon/name/path. Configuration
persists; enabled installed rows determine picker order. One canonical-path icon
cache supplies Settings, results, menu and titlebar. Missing icons alone use the
existing visible external-app glyph. No frame/panel size changes or new palette.


### Worktree hover actions — 2026-10-23

Same-scale comparison of the original and 23:49 result recordings corrects the
initial estimates: 20-point targets (`space::WORKTREE_ACTION`), 12-point glyph
canvases (`space::WORKTREE_GLYPH`) and 4-point action corners
(`radius::WORKTREE_ACTION`) on the worktree's first line. Worktree hover rows
use their own 4-point corners (`radius::WORKTREE_ROW`), without changing other lists. Pointer entry
reveals Copy path, Pin/Unpin (except main/master) and
Delete (linked worktrees only), replacing diff numbers. Delete uses accent.rust
and a scoped rust-over-row hover blend (`alpha::WORKTREE_DESTRUCTIVE_HOVER`, 36%); ordinary actions use secondary text and bounded neutral primary-ink blend (`alpha::WORKTREE_HOVER`, 16%) over the row
hover. Enabled unopened rows retain full opacity. Hints use the existing Kit tooltip
overlay: 500 ms first entry, fast adjacent switching, centered above the target
with viewport clamping, a 4-point gap and a 16-point hint line
(`space::WORKTREE_HINT_LINE`, 28-point total card at UI13). Copy/Delete show the current Guild action binding;
Pin has no invented shortcut. Existing row height, typography and activity geometry remain.
Pins appear once in a separate workspace-local Pinned section above Projects;
unpin returns the row to its repository's saved ordering. Main/master
cannot be pinned.


### Worktree deletion confirmation — 2026-10-23

The approved recording's centered card is 400 points wide, padding 20, context
height 40, footer buttons 34, section gaps 16 (`theme::worktree_confirmation`).
CARD_TITLE/BODY type, CARD/ROW radii and palette card/outline/rust colours remain
shared. The viewport is dimmed at 50%; the card is bounded to the window.
Cancel and Delete are pill buttons; Delete uses rust on app ink. Cancel initially
owns keyboard focus. Tab/Shift+Tab cycle inside; Space toggles the checkbox,
Escape cancels, background input is occluded. Delete branch too is checked by
default for an eligible branch; main/master and detached/unavailable contexts
keep the branch and disable the checkbox. The descriptor includes ignored files
to retain prior consent. GUI keyboard/menu/hover share this card and recheck
protection after confirmation. CLI worktree removal continues to keep branches.

Native geometry verification: 400×188 points at UI13, with 18-point text lines; the same rem-scaled card stays centered at UI10/13/16/20. Real screen comparison remains UNVERIFIED under the known capture/input permissions gap.

Worktree row composition correction (2026-10-24): actions occupy their own
nonshrinking flex allocation after the ellipsis label, replacing the resting diff
counts. They share the row ground. Copy/Pin hover uses the existing neutral
primary ink at scoped `alpha::WORKTREE_HOVER` (16%) over `surface.raised` inside
the button bounds, clearing on leave; no dark panel plate.
Delete uses a scoped 36% rust-over-row blend for a soft red ground with
contrast comparable to neutral controls; shared `accent.rust_bg` is unchanged. This applies equally in Pinned
and Projects, including main/master's smaller action set.

### Local document Find and Replace (2026-10-25)

The editor owns its source; Guild draws the search bar below the tab strip. Both
aligned bordered fields have maximum width 440 (`LOCAL_SEARCH_FIELD`,
`--space-local-search-field`), with the existing head-button height, row radius,
card gaps and panel surface. History/search is on the query's left; multiline,
case, whole-word and regex are inside its right edge. Count, previous/next,
selection filter and More follow it; Close stays at the far edge. Replace expands
a second aligned field with literal preserve-case and Replace/Replace All/Exclude.
The source pane retains its selection, scroll and Undo; a search query never
becomes source text. Query errors use the existing rust border/text roles.

Find in Files puts its enabled mask in the title row, then a full-width bordered
query with its toggles, an optional replacement row, actual Project/Directory
scopes, results, preview and footer actions. Capped or running searches cannot
replace; the footer explains the result limit rather than silently replacing a
partial result set. Search Everywhere retains its five categories and visibility
checkboxes, a bordered query, compact filename/directory rows, preview and a
keyboard footer. The artboard sources are prepared locally.


### Neutral interaction fill and first focus — 2026-10-26

Ordinary row/button hover blends `text.primary` at the existing
`alpha::WORKTREE_HOVER` (16%) over the actual control ground. The shared
`theme::control_hover` role works across all eight palettes without changing
static palette surfaces. Semantic selected/destructive/disabled treatments keep
their own roles. Open In split halves retain `STATUS_BUTTON` radius inside the
capsule. Update ink is checked against its interaction fill as well as card and
raised grounds. This is an interaction distinction, not a WCAG background claim.

An intended first autofocus or proxy handoff uses the component's public focus
operation, starting its real blinking caret. Merely mounting a passive optional
field does not focus it: its muted hint is immediately visible. Focus hides the
hint without changing the value, selection or Undo history. Recent Files is the
selection-only exception to query-picker anatomy; Recent Locations retains input.

The optional Display name created by Settings → General → Open In → Add app
is passive from its first mount: show the muted hint immediately, then hide it
and show the real blinking caret only when clicked. Creating the row does not
redirect typing from another control. Picker searches and explicit rename
autofocus retain their existing intent. A checked Recent Files checkbox colors
the SVG check itself with the intended `surface.app` ink on patina. Cancelled
or refused Backspace close requests end with the close flow and cannot later
forget an independently closed recent file.

Titlebar Terminal Jobs and Usage have named, rebindable actions: defaults
Ctrl+Cmd+J and Ctrl+Cmd+U. Their compact tooltips use the effective keymap
binding; Usage retains its provider prefix. Keyboard toggles call the same
methods as the existing buttons and preserve their focus behavior. Waiting
(Cmd+Shift+U) and Untick (Cmd+Alt+U) are unchanged.


### Frame navigation and session navigator — 2026-10-29

Supersedes panel corner controls' frame placement, corrected after the native
trial on 2026-10-30: Workspaces, Back and Forward form a fixed cluster after
native traffic lights. Branch and branch update follow the content boundary,
clamped after those controls. The latest screenshot correction places Sessions in
the very bottom global footer/status row: right-aligned inside the workspace-width
left segment, at the same y center as folder-plus/down-arrow. Preserve those
actions and the colored workspace selectors/+ above the footer; no separate
Sessions content row. Compact truncation prevents collision at narrow widths.
Sessions is absent whenever column_folded is true, including hover-peek. Its popup opens upward from the footer,
tracks its trigger while resizing, and closes when the column hides. Both branch pickers now open downward. Guild Update
retains its release notes and restart behavior. Footer tools follow persisted
dock order: left, centered bottom, right; Explorer has no special placement.
File/server facts retain their right-side behavior.

The column-footer disclosure is always labeled Sessions (Сессии in Russian).
One leading aggregate status indicator precedes the constant label, without
counts or a chevron: Waiting/Failed use a rust dot, unread Done a green dot,
Working a brass dot, and inactivity a filled gray check-circle. Existing attention
urgency takes priority over working; counts remain only in popup groups and the
semantic Queue group keeps its name. The upward
card uses the existing BRANCH_POPUP width, BRANCH_LIST scroll cap, menu card, ROW
radius and shared spacing/type tokens. The complete card fits above its measured
footer trigger; the fixed group/keyboard footer stays reachable while rows shrink
and scroll within the remaining space, including UI20 at 500-point height. Rows show the project's tile with harness
badge, title, branch, status dot and real observed relative time; no conversation
preview. The current session carries a check. Running lists open Working agents
in tab/pane order. Queue lists Waiting, Failed and unread Done by existing
attention urgency and wait order. Timeline lists all open agents by latest
observed transition; unknown time stays an em dash after restart. Viewing an
unread turn clears attention without changing execution age. Group buttons show
counts; the footer documents local arrows, Enter and Escape. Cmd+Shift+U remains
the existing attention navigator. No closed-session resume is introduced.

Shared sources: frame/TitleBar, frame/StatusBar, workspace/SessionNavigator.


### Unified update card — 2026-11-01

Settings → Updates uses one existing Settings card with CARD radius, CARD_PADDING
rows and settings::rule dividers. Current version precedes channel and update
status; labels/details stay left and the current action stays right, wrapping
within the existing narrow host. Check, Download, Install and restart, or Cancel
are mutually exclusive primary actions. Explicit reinstall/replacement remains
in additional settings. Recovery choices retain their existing authority and
stay visible when blocked. Release notes and additional settings are independent
inline disclosures, initially collapsed. Note categories/counts come from the
actual Keep a Changelog groups; no empty/invented categories. Native buttons use
BODY, CONTROL height, ROW radius and palette focus/hover roles. Source, local
directory, provenance and history remain accessible in additional settings.
The concise titlebar notes and expanded What's new remain shared. Restart copy
explains existing unsaved-file checks, stopped sessions and retained conversations
without promising completion of interrupted work. No scheduling or new runtime
resumption behavior. Local artboard sources are prepared.

### Shared release notes — 2026-11-01

Settings, Update and What's New share wrapping inset category segments with real
Keep a Changelog counts, UI labels/META counts, and BODY entry sentences at TEXT
leading. No repeated category headings or invented per-entry titles. Each entry
starts beside a HEAD_BUTTON square tile at ROW radius, surface.inset and a muted
existing category SVG at ICON size; CARD_INNER_GAP separates tile and sentence,
CARD_PADDING separates entries. Genuine continuation lines remain verbatim.

The titlebar Update card uses NOTES_CARD width and a stable WHATS_NEW_LIST total
height, clamped to available viewport space. Header and its current Download or
Install and restart action stay outside the scrolling body. What's New is
centered over the existing dim backdrop, WHATS_NEW wide with a fixed bounded
WHATS_NEW_LIST body and a separate Continue footer. Short categories do not shrink
or move either card. Its filtered entries retain each missed release's version
and date. The native toolkit owns buttons, scrollbars and modal focus trapping;
accent.patina marks focus and the existing brass/brass_bg pair marks actions.
Hover transfer retains the same interactive card; departure dismisses it. The
Settings notes also use the bounded scroller, with zero extra list inset inside
the existing padded section, so category swaps keep the lower disclosure in
place. The Settings card and independent SVG disclosures retain their overall composition.
No new theme values, assets or dependencies. Local design publication is excluded.

User correction to shared release notes: show only Added, Changed and Fixed.
Deprecated/Removed contribute their unchanged entries and counts to Changed;
Security contributes to Fixed. This is a shared display mapping, not a source
changelog/schema change. Only nonempty buckets appear. At normal width the three
segments have balanced equal shares; wrapping is a constrained-layout fallback.
All release provenance and verbatim entry text remain.


### Interface scale corrections (2026-11-04)

File-type icons use `space::ICON` (16), and disclosure arrows use
`space::CHEVRON` (11). Both follow the interface rem scale. Explicit caller
sizes retain precedence; the activity rail and pinned sidebar geometry remain
physical pixels, including its repository disclosure. Search result rows use
`space::SEARCH_ROW` (26), shared popup group headings use `space::MENU_HEADING`
(24), and failed-server indicators use `space::DISC` (7), all rem-based.
These tokens preserve the existing dimensions at the default UI size of 13.
The terminal settings sample uses the selected terminal font size in physical
pixels and `leading::TERMINAL`; only its surrounding UI follows the interface
scale. Changing UI size must not suggest a different terminal font size.


### Git tree grounds and caret occurrences — 2026-11-04

Explorer (including aggregated directory status), Commit and Git Log/History file
rows use the existing status accents as full-width backgrounds instead of M/A/D/?
letters. Push files use the same grounds instead of coloured names. Modified uses
brass, added green, deleted rust, untracked faint ink: 12% over the host surface
(`alpha::GIT_ROW`, `--alpha-git-row`). File text retains its normal role. Selection
uses section 6.4's fill with priority over status, and no outline (decision 54).
Every host's hover is 6.4's: `alpha::ROW_HOVER` of primary ink over the tint.
All file-tree status, selection and hover grounds are square
and extend to both viewport edges. Explorer (including external/library roots,
sources and declaration rows), Changes and commit file trees in Log/history and
Push share this rule with Search/Find source-result trees. Indentation belongs
inside the row, not in an inset around its background. Controls, cards, menus,
the project/worktree column and word occurrence backgrounds keep their own roles.
Trees without a Git status presentation (Search, Find Usages and external
libraries) draw no tint, and take 6.4's selection and hover all the same.

An ordinary caret inside a Unicode identifier highlights its case-sensitive,
whole-identifier textual mentions throughout the file, including itself and in
read-only external/library sources. These keep focus, caret placement, selection
and Copy through the component's public readonly mode; typing, paste and deletion
remain disabled. Highlighting does not enable editing.
Identifiers use Unicode XID_Start/XID_Continue plus underscore; no normalization
or LSP symbol resolution is implied. Whitespace, punctuation, EOF and nonempty
selection at any caret clear it, including a secondary cursor's selection.
Editing, reload and caret movement refresh the ranges. The
component's independent range Fill uses Guild `text.primary` at 12%
(`alpha::IDENTIFIER_OCCURRENCE`, `--alpha-occurrence`), refreshed on theme changes,
below Find and selection, without changing glyph colours,
diagnostics, caret or layout. The File artboard illustrates the repeated `hash`.

Occurrence processing shares `file_tab::HIGHLIGHT_LIMIT`: buffers above
2 MiB of UTF-8 text receive no occurrence backgrounds and are not flattened for
this feature. Within that byte limit, at most 4096 whole matches of the current
identifier, including itself, are installed. The 4097th match clears the entire
set rather than displaying partial results; it is detected before allocating
another range or any decoration. Smaller/reloaded buffers and a different
identifier within budget re-enable the same behavior. There are no new settings,
notices or limits on reading/editing the file itself.


The same occurrence role is used on immutable source surfaces. DiffTab owns the
active identifier and matches it independently against each original before/after
snapshot, with the same per-snapshot byte/count limits. Unified context retains
both original line mappings; selection on the old side uses its mapping there.
Only visible source cells paint their intersecting ranges, under intraline diff
marks and user selection. Context expansion, scrolling and view mode changes
retain source coordinates; replacing the revision or file invalidates them.
All DiffTab hosts share this route: workspace comparisons, IDE proposals,
commit/file-history inspection windows and the embedded Push preview.

Restored standalone Annotate uses the shown blame lines as one bounded snapshot.
Popup and Find/Usages previews use the excerpt they actually show; result snippets
in Search/Find/Usages and Search Everywhere use their displayed source line.
They share source hit testing through public StyledText layout, leave focus and
row activation with their hosts, and clear backgrounds during dragging. Their
existing search foregrounds, syntax styles and navigation gestures stay intact.
Problems displays diagnostic messages rather than source snippets, so source
occurrences belong to the file it opens. Markdown Source remains a FileTab;
Markdown prose and terminal/output are outside source occurrence matching.

Accepted Markdown scope: preserve cross-block rich Preview selection and Copy.
Occurrence highlighting applies only in Source mode (including the Source pane
of Split). Preview prose, inline code, fenced code and indented code retain their
existing component rendering and selection behavior. This is a user-approved
scope decision, not an outstanding API blocker.

### Main source editor Git line markers — 2026-11-04

The ordinary project FileTab Source editor also compares its current buffer,
including unsaved text, with HEAD. A dedicated gutter lane follows the existing
number/run cells: `GIT_MARKER_LANE` 6px, `GIT_MARKER_STRIPE` 3px, and
`GIT_MARKER_DELETED` 3px high. Added lines use `accent.green`, paired replacement
lines `accent.brass`; excess old lines use a short full-lane `accent.rust` mark
at the current deletion boundary. BOF and an empty current document anchor at
the first row; EOF anchors before the terminal empty row or at the last row's
bottom when there is no trailing newline. There are no line backgrounds.
The following accepted hunk popup extends this lane's interaction; it does not
add staging, commits or preferences.

Saving does not clear the difference from HEAD. Commit/HEAD/branch changes,
reload, filesystem changes and relocation invalidate the baseline; stale
baseline/path/text generations cannot restore old marks. HEAD resolves to an
immutable object and is size checked before reading. New/untracked eligible
Git sources have an empty base; Git failures, ignored untracked paths, binary,
non-UTF-8 and oversized sources produce no false all-added markers. Ordinary
text Source modes, including Markdown, and main FileTab inline blame share this
lane. External read-only sources, diff/commit/history/Push/proposal viewers,
standalone Annotate, rich Markdown Preview and source excerpts/results do not.

Each side is limited to 2 MiB and 20,000 retained-editor logical LF rows; the
complete result is limited to 4096 markers. Line comparison has a 50ms deadline;
overflow/overrun skips the entire set without a warning or partial result.
Large Rope buffers are rejected before flattening, and supported flattening,
Git reads and comparison run on the background executor. One FileTab-owned job
and one coalesced latest intent replace per-file threads/polling. Closed views
cancel queued work; already executing reads retain the existing Git gateway's
synchronous child ownership, and weak-owner publication is revoked.

CRLF is normalized like FileTab loading; trailing newline and blank lines remain
part of line tokens. Bare CR stays within the actual retained editor's LF row,
rather than inventing a different source-row coordinate system. Painting reads
current public range/input geometry after the editor paints, clips wrapped
and partially scrolled rows, and suppresses hidden folded rows. No copied
component padding or row-times-font-height positioning is used. Number, Run,
Annotate and word-occurrence interaction roles remain separate.


### Source Git hunk popup — 2026-11-05

Clicking a visible Git stripe opens one anchored card for its contiguous change
block from the same immutable HEAD-to-buffer comparison. A paired replacement
and its excess deletion share one block. Previous/up and Next/down stay within
that file and are disabled at its ends; navigation keeps the card open and uses
the existing public centered reveal. Rollback Lines is the only mutation action. It is an icon-only curved revert
arrow (`ui/tool-revert.svg`), the same HEAD_BUTTON square and hover as up/down.
Its localized tooltip uses RollbackLines (Russian: «Откатить изменение»), with no
shortcut.
There is no commit field/action, stage, Copy/Show Diff button, preference or new
shortcut. Added blocks have only the toolbar, because no original text exists.
Modified/Deleted show normalized original HEAD code in the existing readonly
Editor, including syntax highlighting, selection/Copy and shared identifier
occurrences. Its parser work uses the component's background ownership; no LSP
provider is installed. Closing the card releases that editor and its occurrence
owner. Escape (also from code selection) and an outside press close the card
and restore Source focus without letting ordinary typing reach the editor behind.

The card reuses popup card/outline/shadow, ROW radius, MENU_PADDING/CARD_GAP,
HEAD_BUTTON/BUTTON_SIDE and RUN_COMMAND width. Short blocks size to code;
PREVIEW bounds the code height and its own Editor scrolls long/wide code. Width,
height and anchor are clamped to the source viewport/window. No palette or
spacing token is introduced. Disabled arrows use text.faint rather than an active hover state.

Rollback is one atomic public EditorState::replace of a UTF-8 selection range,
using normalized HEAD text, preserving previous history and one-step Undo/Redo.
The comparison also retains exact current raw LF-line byte boundaries: pasted
CRLF may remain in the editor Rope even though comparison tokens are normalized.
Rollback replaces that raw range only; CRLF/Unicode bytes outside it remain
unchanged, and Undo restores the exact original raw bytes.
It changes only this block in the live buffer, keeps the caret near the change,
and closes the card. The existing FileTab save/autosave path remains responsible
for disk; no Git/index/HEAD write or confirmation sheet is involved. All previous
comparison limits remain: 2 MiB per side, 20,000 LF rows, 4096 markers, 50ms diff
deadline; block ranges and marker links are prepared by that same worker.

Buffer/HEAD/path/relocation/editability and Source-presentation changes revoke
old card callbacks. The source Rope identity and exact comparison must still
match before mutation. Marker hitboxes require the retained gutter frame's
text, scroll, folding, viewport, scale and font identity; numbers/Run/Annotate
retain their original targets. The marker has its own left-click target and
never opens Annotate. Main project Source only, including Markdown and SVG;
all previously excluded viewers remain excluded. Rich Markdown Preview is
unchanged under the accepted Source-only decision.
