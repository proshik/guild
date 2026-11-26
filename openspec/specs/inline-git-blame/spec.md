# inline-git-blame Specification

## Purpose
Keep Git attribution beside the current editable file and make each committed annotation a reliable route to its exact historical commit and changed file.

## Requirements

### Requirement: Annotation in the current editor
Annotate with Git Blame SHALL toggle annotations in the current project file without creating a new tab or replacing its editor. It SHALL preserve cursor, selection, undo history and scroll. The gutter menu SHALL reflect the target file's toggle state and use an aligned compact row without unexplained leading padding.

#### Scenario: Toggle twice
- **WHEN** a person enables and then disables annotations from the current file's gutter menu
- **THEN** annotations appear and disappear in that file, no annotation tab is added, editing state remains intact and the menu check follows the actual state.

#### Scenario: Non-project file
- **WHEN** a dependency or external file is displayed
- **THEN** Guild does not annotate an unrelated same-named project file.

### Requirement: Compact accurate attribution
Committed attribution SHALL show a compact numeric date and compact author with complete metadata on hover. Annotations SHALL match the currently displayed lines, including unsaved edits, and SHALL remain aligned with wrapped/scrolled rows. Uncommitted lines SHALL be visibly distinguished and not offer commit navigation.

#### Scenario: Compact label and tooltip
- **WHEN** a committed annotation is displayed or hovered
- **THEN** its label uses DD.MM.YYYY and compact author with bounded width, and its tooltip includes full author/date, hash and message.

#### Scenario: Unsaved edits and stale response
- **WHEN** the buffer changes while blame is being read or after blame arrives
- **THEN** unchanged lines retain correct attribution, inserted/changed lines are uncommitted and no stale answer can attach a previous line's hash to different text.

#### Scenario: Refresh, disposal and restart
- **WHEN** the file is saved, renamed, externally changed or HEAD moves
- **THEN** enabled attribution refreshes without resetting editor state; closed/disposed files reject late responses and restart never displays obsolete cached blame as current.

### Requirement: Exact historical commit and file navigation
Clicking committed attribution SHALL reveal Git Log, select the exact commit and select/reveal the relevant historical file in that commit's changed-files list. The transition SHALL work when the commit is outside loaded pages or hidden by existing filters and SHALL leave the origin editor position intact. It SHALL NOT mutate Git.

#### Scenario: Commit outside the current log
- **WHEN** an annotation's commit is not loaded or a filter excludes it
- **THEN** Guild resolves and displays that exact commit, loads its changes and selects its relevant file, rather than merely opening the Git tool.

#### Scenario: Renamed historical path
- **WHEN** the current file has been renamed since the blamed change
- **THEN** navigation uses the historical filename recorded for that annotation and selects the corresponding changed-file entry, including its rename paths.

#### Scenario: Unavailable commit or changed file
- **WHEN** the commit or file cannot be resolved, or the line is uncommitted
- **THEN** Guild gives a readable unavailable state where applicable and never selects an unrelated commit or file.
