# worktree-row-order Specification

## Purpose
Allow people to arrange linked worktrees within each project by priority and keep that arrangement across normal desktop use and restart.

## Requirements

### Requirement: Native worktree reordering

Guild SHALL allow linked worktree rows to be dragged before or after other linked rows within the same project and workspace. It SHALL display the insertion location during a valid drag and keep the primary checkout first.

#### Scenario: Move upward or downward
- **WHEN** a person drags a linked worktree to an insertion location within its project, including after the final row
- **THEN** the row moves there and every other row retains its relative order
- **AND** the primary checkout remains first.

#### Scenario: Invalid or cancelled gesture
- **WHEN** a drag targets another project or workspace, moves the primary checkout, targets itself without changing position, loses a source or target to refresh, or is cancelled
- **THEN** no ordering is changed and no worktree is opened, created or removed.

#### Scenario: Escape with a focused panel or editor
- **WHEN** a person presses Escape during a worktree drag with the Sessions panel open or multiple editor cursors active
- **THEN** the native drag ends before any bound panel or editor action runs
- **AND** panel state, editor cursors, active tab and input focus are preserved, and subsequent release does not reorder rows.

#### Scenario: Navigation is preserved
- **WHEN** a worktree is reordered
- **THEN** the active project, tabs, sessions and focus remain unchanged
- **AND** ordinary row clicks, context menus and project-header reordering remain available.

### Requirement: Durable worktree order

Guild SHALL retain the chosen order per project and workspace across refresh and restart, without requiring an unopened worktree to be opened. Missing saved paths SHALL NOT create rows; newly discovered linked worktrees SHALL follow the retained linked rows in their existing discovery order.

#### Scenario: Refresh and restart
- **WHEN** a person reorders opened and unopened worktrees and Git refreshes or Guild restarts
- **THEN** the saved order is restored and unopened worktrees remain unopened.

#### Scenario: Changed worktree membership
- **WHEN** a saved worktree disappears and a new worktree appears
- **THEN** the missing path produces no extra row, the remaining saved rows retain relative order, and the new linked row is appended after them.

#### Scenario: Old state and separate workspaces
- **WHEN** Guild reads state saved before this capability or shows the same repository in another workspace
- **THEN** absent ordering uses the previous default behavior and an order set in one workspace does not change another workspace's order.

#### Scenario: Initial Git list is pending
- **WHEN** the desktop restores opened roots and saved unopened paths but the repository's initial worktree list has not arrived
- **THEN** row reordering is refused without replacing the saved order
- **AND** list arrival restores the relative order of the saved unopened rows.

#### Scenario: Unsupported path serialization
- **WHEN** the full candidate order includes an unopened Git-listed path that cannot be serialized as a UTF-8 path
- **THEN** the candidate is refused with a warning before Desk is changed
- **AND** the previous order is retained, no unopened root is opened, and subsequent workspace saves remain possible.
