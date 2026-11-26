# explorer-file-operations Specification

## Purpose
Make routine Explorer file management accessible from keyboard and pointer while preserving file content, editor state and clear operation outcomes.

## Requirements

### Requirement: Discoverable scoped actions
Guild SHALL offer Shift+F6 Rename, F6 Move, Cmd+Backspace Trash, file/directory creation and Cut/Copy/Paste while Explorer owns list focus. Menus SHALL display actual configured shortcuts and respect custom keymaps. Project roots and External Libraries SHALL NOT be mutation sources.

#### Scenario: List versus text focus
- **WHEN** a user invokes Rename or a file clipboard shortcut in Explorer
- **THEN** the selected project item or set is acted on
- **AND** the same gestures in a rename/search input retain normal text editing and never mutate selected files.

#### Scenario: Context target
- **WHEN** a user opens a menu on a selected group member or an unselected row
- **THEN** group-capable actions use the group in the first case and the actual row in the second; Rename requires exactly one writable item.

### Requirement: Comfortable rename editing
Guild SHALL initially select a regular file's name without the final extension and a directory's entire name. Dotfiles without another dot and extensionless names SHALL be fully selected. Normal caret movement, selection, clipboard, undo and IME SHALL work.

#### Scenario: Stem selection
- **WHEN** Rename starts for `report.test.ts`, `.gitignore`, `notes` or a directory `docs.v2`
- **THEN** the selections are respectively `report.test`, `.gitignore`, `notes` and `docs.v2`
- **AND** typing replaces that selection; subsequent renders do not reset selection or undo.

#### Scenario: Navigation and refusal
- **WHEN** the user presses Cmd/Option+Left/Right with or without Shift, or enters an invalid/taken name
- **THEN** text navigation works normally and refused names retain the input and focus; Enter applies a valid name and Escape cancels without a write.

### Requirement: Mixed Explorer selection
Explorer SHALL support selecting multiple project directories together with files by modifier clicks and keyboard range selection. This supersedes Explorer's file-only modifier-selection rule in the active mixed-tree-navigation change; other lists SHALL retain their existing behavior. Plain directory activation SHALL retain folding, while modifier selection SHALL NOT fold a directory. Refresh/folding SHALL retain selected existing path identities; a selected ancestor SHALL subsume descendants for operations.

#### Scenario: Select a mixed group through the UI
- **WHEN** a user selects a file, Cmd-clicks two directories and Shift-extends the selection, then refreshes or folds a directory
- **THEN** the selected existing files/directories remain the operation group and dragging a member preserves that group without accidental folding.

### Requirement: Unified safe transfers
Guild SHALL support Move and session-local file Cut/Copy/Paste for multiple project files/directories using the same destination and collision rules as drag/drop. Copy SHALL copy on-disk content. It SHALL NOT overwrite existing targets silently, traverse copied symlinks or move a directory into itself.

#### Scenario: Multi-selection and destination
- **WHEN** a file and directory set is moved/copied into a project directory, including a selected parent and its child
- **THEN** each top-level selected source is processed once, successful destinations become visible, and original files remain after Copy.

#### Scenario: Collision or partial failure
- **WHEN** an existing or newly appeared target conflicts, or a later item fails
- **THEN** Guild offers Skip, Keep Both or Cancel without silent replacement, preserves failed sources and reports which items completed.

#### Scenario: Invalid topology
- **WHEN** a transfer targets the source itself, a descendant, an external symlink destination, a project root source or a read-only library
- **THEN** Guild refuses/no-ops without data loss or writing outside the project.

#### Scenario: Clipboard lifetime
- **WHEN** Cut is cancelled, text replaces the clipboard payload, the source disappears or Guild restarts
- **THEN** no stale file operation is replayed; cut sources are not deleted until a successful move.

### Requirement: Direct drag and drop
Guild SHALL allow dragging the selected project files/directories onto destinations, show valid target feedback, unfold hovered directories and scroll near list edges.

#### Scenario: Move into a folded offscreen directory
- **WHEN** the user drags a selected group, scrolls at the edge, hovers a closed directory and drops
- **THEN** the directory opens, the destination is clear, and the group moves through the shared transfer path with collision handling.

#### Scenario: Cancelled or invalid drag
- **WHEN** Escape is pressed, the pointer releases outside the target, or the destination is invalid
- **THEN** files do not move, transient feedback and timers clear, and no accidental file-open or dock move occurs.

### Requirement: Editor continuity
Guild SHALL preserve open editor entities, unsaved contents, cursor/selection, undo history and pane layout across successful file and directory rename/move. Saves and navigation SHALL follow the new path in the originating project.

#### Scenario: Dirty nested editors
- **WHEN** a directory containing dirty files open in multiple panes is renamed or moved
- **THEN** all affected project editors retain their contents and state, subsequent save writes the new paths only, and unrelated/library panes remain unchanged.

#### Scenario: Failure or project switch
- **WHEN** a transfer fails or the user switches projects while it runs
- **THEN** each editor continues to refer to its actual surviving file and completion cannot remap a different project's paths.

### Requirement: Complete everyday navigation
Guild SHALL retain mixed file/directory arrow navigation and current-list speed search, provide keyboard creation with an evident parent, retain Copy Path/Copy Relative Path/Reveal in Finder, and confirm deletion into Trash for selected writable items.

#### Scenario: Creation and navigation
- **WHEN** the user creates under a selected directory, selected file or empty selection, then navigates with arrows or typed speed search
- **THEN** creation uses that directory, file's parent or project root respectively, and existing tree navigation/search remains functional.

#### Scenario: Path actions and Trash
- **WHEN** the user invokes path actions or confirms multi-item Delete
- **THEN** path actions use the menu's actual target, Delete names its items and uses Trash, and cancellation changes nothing.
