# editor-task-runs Specification

## Purpose
Let people launch tasks at their declarations and inspect independent runs in Guild's existing terminal panel without losing another task's output or process.

## Requirements

### Requirement: Source-aware launch actions
Guild SHALL show green launch actions at executable Just recipes, explicit literal Make targets, package.json scripts and Taskfile tasks, with a tooltip describing the task. Discovery SHALL NOT execute project commands or evaluate task templates. Unsupported or invalid declarations SHALL NOT produce misleading launch actions.

#### Scenario: Supported declarations
- **WHEN** a conventional supported task file is opened, including a nested task file
- **THEN** its valid task declarations offer launch actions on their own lines and the command chooser agrees with those actions.

#### Scenario: Changes and unsupported declarations
- **WHEN** a declaration is edited, moved, deleted, malformed or dynamically computed
- **THEN** actions reflect the current valid source and cannot launch a stale or invented task; inspecting the file executes no project code.

#### Scenario: Specific declaration click
- **WHEN** a person clicks the green triangle for one declaration with known inputs
- **THEN** that declaration launches immediately without asking to choose it again; missing required inputs or manager evidence use a compact task-specific input, and Option-click allows explicit argument configuration.

#### Scenario: Multiple targets on one line
- **WHEN** a Make rule declares several explicit runnable targets on one line
- **THEN** its launch action offers the target choice instead of guessing one or launching all.

### Requirement: Correct launch context and parameters
A launch SHALL use the declaring task file, owning working directory and project/worktree context, preserve names/paths/arguments as data and honor the package's manager. Required arguments SHALL be requested before launch; cancellation SHALL start nothing.

#### Scenario: Nested file and shell characters
- **WHEN** a task file, task name or argument contains spaces, quotes or shell metacharacters
- **THEN** the selected task runs in its own intended context and the input does not become an extra shell command.

#### Scenario: Package manager
- **WHEN** packageManager or an unambiguous lockfile identifies npm, pnpm, yarn or bun
- **THEN** its script uses that manager; absent evidence defaults to npm and conflicting evidence asks for a choice.

#### Scenario: Required arguments and file action
- **WHEN** a Just recipe needs arguments or the user chooses a file-level launch
- **THEN** required values are requested, Just/Make default launch is supported, package.json offers script selection, and Taskfile launches a declared default or offers task selection.

### Requirement: Saved source and honest errors
Guild SHALL save changed task declarations through existing editor guards before launching and SHALL re-resolve the task against the saved source. Failed saves, removed tasks and missing executables SHALL be explained without falsely reporting a successful run.

#### Scenario: Failed save or disappeared task
- **WHEN** a launch's source cannot be saved or its selected declaration disappears while preparing the launch
- **THEN** no task process starts and a readable error identifies the reason.

#### Scenario: Missing tool
- **WHEN** the task runner is unavailable
- **THEN** Run displays the attempted command and failed status with readable diagnostic output.

### Requirement: Independent run tabs
Guild SHALL retain separate selectable Run tabs per distinct task invocation with terminal output, elapsed time and honest running/finished/stopped/failed states. Rerun, Stop, Clear and keyboard shortcuts SHALL target the selected run; launching the same invocation SHALL rerun its tab.

#### Scenario: Parallel tasks
- **WHEN** a dev task is live and the user starts a different test task
- **THEN** both have distinct tabs and starting/stopping/clearing one does not alter the other's process or output.

#### Scenario: Rerun and close
- **WHEN** the selected invocation is rerun
- **THEN** its previous process tree stops before replacement and its captured command and directory are reused; closing a running tab offers Stop and close or Cancel.

#### Scenario: Switch, shutdown and restart
- **WHEN** the panel is hidden or the project selection changes
- **THEN** live runs continue in their owning projects; closing their project or quitting stops all owned process trees, and restarting never automatically reruns tasks or claims unavailable output was restored.

### Requirement: Aligned accessible editor controls
Task buttons SHALL remain aligned with their declaration through scroll, wrapping, zoom and resize and SHALL coexist with blame and line numbers without overlapping text. Launching SHALL reveal the selected Run output while preserving the editor's cursor and selection.

#### Scenario: Compact ordered gutter
- **WHEN** declarations or blame are displayed
- **THEN** content-sized annotation and number lanes precede the run controls, with the triangles to the right of the numbers and no fixed empty attribution band.

#### Scenario: Scrolled annotated editor
- **WHEN** blame is visible and the user scrolls a wrapped file horizontally/vertically, changes scale or resizes
- **THEN** launch hit areas follow their visible declarations, offscreen actions cannot be clicked and the annotation/number/control columns remain distinct.
