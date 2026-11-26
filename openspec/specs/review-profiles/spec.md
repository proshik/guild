# Review Profiles

## Purpose

Describe current project profile customization and configuration persistence.

## Requirements

### Requirement: Originals and overrides

Built-in profiles SHALL remain immutable originals; edits SHALL create project overrides.

#### Scenario: Restore originals

- **WHEN** a user restores built-ins
- **THEN** overrides and hidden built-ins are reset within the stated scope while custom profiles and historical runs are preserved.

### Requirement: Portable import

Import SHALL validate the complete versioned document before mutation and SHALL copy conflicts under distinct identities.

#### Scenario: Invalid document

- **WHEN** one profile in an import is invalid
- **THEN** no profile from that import is applied.

### Requirement: Private runtime storage

Profile export SHALL exclude secrets and review history; local profiles SHALL reside in the main checkout .guild/review library.

#### Scenario: Export all

- **WHEN** a person exports their profile library
- **THEN** the portable document contains profiles rather than runtime transcripts or credentials.

### Requirement: Independent mappings

Implementation and review role mappings SHALL be separate, with null model and effort inheriting harness defaults.

#### Scenario: Review role differs

- **WHEN** a Codex builder has a Claude review role
- **THEN** the review resolves its own harness settings instead of using the builder’s.

### Requirement: Local project settings

Guild SHALL initialize missing engine and workflow settings in the main checkout's
.guild directory when opening the project. Settings SHALL remain local, shared
by its worktrees, and SHALL NOT be committed or overwritten on reopen.

#### Scenario: A project is reopened through a worktree

- **WHEN** the main checkout already has customized local settings
- **THEN** Guild reads that same configuration without creating a worktree copy or replacing the user's values.

#### Scenario: New project without OpenSpec

- **WHEN** Guild initializes missing settings for a new project
- **THEN** it uses plain-plan defaults without installing OpenSpec or enabling paid review, and leaves base/target selection to the project's actual rules.
