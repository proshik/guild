# terminal-file-links Specification

## Purpose
Defines which text in a terminal tab's output Guild treats as a link — a web
address or a file, with or without a line — when it is underlined, and what a
`cmd`-click on it opens, for every harness that runs in a terminal tab.

## Requirements

### Requirement: A printed path without a line is a link to an existing file

A run of path characters in a terminal tab's output that holds a `/` or an
extension (a `.` followed by a letter or digit) and at least one letter SHALL be a
file link even when no `:line` follows it. It SHALL be underlined under the
pointer with `cmd` held, and opened by a `cmd`-click, only when it resolves —
absolute as written, relative against the terminal's starting directory and then
the project's root — to a regular file. Trailing sentence punctuation
(`.`, `,`, `;`, `:`, `!`, `?`) SHALL NOT be part of the path. Text that does not
resolve to a regular file SHALL NOT be underlined, and a `cmd`-press on it SHALL
behave as an ordinary press — except on the link that is underlined at the
moment of the press: a file deleted after it was underlined is still the link
that was promised, the press is kept, and the window reports the file missing
(the existing M5.12 behaviour). A foreign-URI token SHALL start at a word start with any
`scheme://` (`[A-Za-z][A-Za-z0-9+.-]*://`), except `file://` followed by `/`,
or with one of the case-insensitive opaque schemes `mailto:`, `javascript:`,
`vbscript:`, `data:`, `tel:`, `urn:`, `about:`. It SHALL end at the first
whitespace, quote (`"`, `'`, backtick) or angle bracket (`<`, `>`). Nothing
inside that token SHALL be a file link or a separate embedded HTTP(S) link.
All other text SHALL use the ordinary path rules, including `name:digits`
locations.

#### Scenario: A Codex Markdown link to a report opens the report
- **WHEN** a Codex tab shows `Замеры и подробности добавлены в отчёт (docs/reports/2026-10-15-rendering.md).` and that file exists under the project
- **THEN** with `cmd` held only `docs/reports/2026-10-15-rendering.md` is underlined — not the parentheses and not the full stop
- **AND** a `cmd`-click opens that file in a tab

#### Scenario: Paths in prose, in backticks and at the end of a sentence
- **WHEN** the output reads `edit src/lib.rs and go`, `` see `docs/x.md` ``, `open Cargo.toml.` or `/abs/dir/x.md` and each file exists
- **THEN** the link covers exactly `src/lib.rs`, `docs/x.md`, `Cargo.toml` and `/abs/dir/x.md`

#### Scenario: Text that only looks like a path is not underlined
- **WHEN** the output holds `e.g.`, `v0.162.1`, `node.js`, or `docs/missing.md` and no such file exists, or a path that names a directory
- **THEN** nothing is underlined and a `cmd`-press there starts no navigation

#### Scenario: Words, times, ports and schemes stay non-links
- **WHEN** the output holds `12:30:45`, `localhost:8080`, `10.0.0.1:8080`, `ERROR:12`, `note:3`, `user@host:src/a.rs:3`, `ssh://host/src/main.rs:4`, `ftp://example.com/a.txt`, `ftp://example.com/download?file=src/lib.rs` or `mailto:someone@example.com`, even when `src/lib.rs` and `src/main.rs` exist
- **THEN** none of them, nor any part of them, is a file link

#### Scenario: A file deleted after it was underlined
- **WHEN** `docs/x.md` is underlined under the pointer, the file is then deleted, and the person `cmd`-clicks it
- **THEN** no tab opens and the notice says the file is missing

### Requirement: A path with a line keeps its line and column

A path followed by `:line` or `:line:column` (both at least 1) SHALL remain one link
covering the suffix and SHALL open at that line and column, as before this change.
An invalid suffix (`:0`, digits running into letters) SHALL NOT be part of the
link; the path before it SHALL be a link without a line.

#### Scenario: Compiler locations are unchanged
- **WHEN** the output reads `  --> src/main.rs:12:5` or `src/lib.rs:7: warning`
- **THEN** the link covers `src/main.rs:12:5` (line 12, column 5) or `src/lib.rs:7` (line 7, no column)

#### Scenario: An invalid line suffix leaves the bare path
- **WHEN** the output reads `src/lib.rs:0` and `src/lib.rs` exists
- **THEN** the link covers `src/lib.rs` and opens the file without a line

### Requirement: OSC 8 file hyperlinks open without a line

An OSC 8 hyperlink whose URI is `file://` followed by an absolute path SHALL lead to
that file whether or not it carries a `:line[:column]`, with its percent-encoding
decoded before the path is resolved. An OSC 8 hyperlink SHALL keep precedence over
the text it covers. Other schemes than `http`, `https` and `file` SHALL NOT be links.

#### Scenario: A percent-encoded trusted-file hyperlink
- **WHEN** a program writes an OSC 8 hyperlink to `file:///w/docs/%D0%BE%D1%82%D1%87%D1%91%D1%82%20a.md` over the text `отчёт`, and `/w/docs/отчёт a.md` exists
- **THEN** the whole text `отчёт` is one link and a `cmd`-click opens `/w/docs/отчёт a.md`

#### Scenario: A hyperlink with a line keeps it
- **WHEN** the OSC 8 URI is `file:///w/src/a.rs:3:2`
- **THEN** the link opens `/w/src/a.rs` at line 3, column 2

### Requirement: Where a file link opens

A `cmd`-click on a file link SHALL re-resolve the path at the click. A file inside
the project opened without a line SHALL be shown the way the Explorer opens it: an
existing tab for that file is brought forward with its caret unchanged, otherwise
a new tab opens at the start. With a line it SHALL open at that line and column.
A file outside the project SHALL produce the existing "outside the project"
notice naming its path, and a file gone since the pointer found it SHALL produce
the existing "missing" notice.

#### Scenario: An open file is brought forward without moving its caret
- **WHEN** `docs/x.md` is already open in a tab with its caret on line 40 and the person `cmd`-clicks `docs/x.md` in a terminal
- **THEN** that tab comes forward with the caret still on line 40 and no second tab opens

#### Scenario: A file outside the project
- **WHEN** the person `cmd`-clicks `/etc/hosts` printed in a terminal of the project
- **THEN** no tab opens and the notice names `/etc/hosts` as outside the project

#### Scenario: Every terminal harness gets the same links
- **WHEN** the same path text is printed in a shell, a Claude Code, a Codex or an OpenCode terminal tab
- **THEN** it is found, underlined and opened by the same rules
