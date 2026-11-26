# Overlay Surfaces

## Purpose

Preserve the implemented reusable overlay ownership and producer separation contract.

## Requirements

### Requirement: Session ownership

Generic overlays SHALL be addressed by stable IDs owned by their calling session.

#### Scenario: Another caller uses an ID

- **WHEN** a different session requests an existing owner’s overlay
- **THEN** it cannot take ownership of that surface.

### Requirement: Literal content

Generic overlay text SHALL be displayed as literal content without execution.

#### Scenario: Tool text supplied

- **WHEN** a producer sends text containing commands or HTML
- **THEN** Guild displays the content without running or rendering it as executable markup.

### Requirement: Producer independence

Hide, show and close SHALL operate on presentation rather than producer lifetime.

#### Scenario: Build log closed

- **WHEN** a user closes a generic build-log overlay
- **THEN** Guild does not terminate the build process.

### Requirement: One geometry-preserving terminal dismissal control
The native terminal overlay SHALL offer a single cross that hides its view while preserving process lifetime, retained output and geometry. Its localized tooltip SHALL explain that work continues. Reopening SHALL restore the same view and position/size. Explicit process cancellation and CLI presentation-close semantics SHALL remain unchanged; text and native-review surfaces SHALL retain their existing dismissal rules.

#### Scenario: Hide and reopen a resized live terminal
- **WHEN** a person resizes and moves a running terminal overlay, presses its cross and reopens its output
- **THEN** the same program remains running with retained content, its view returns at the same geometry and keyboard focus recovers correctly.

#### Scenario: Close archive presentation without stopping its viewer
- **WHEN** a person presses the cross over a read-only archive viewer
- **THEN** it hides while the viewer continues, and the separate Close viewer action remains the explicit stop control.
