# Feature Specification: Workspace Colors and Terminal-Style Input

**Feature Branch**: `004-workspace-colors-terminal-input`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "Create a unique color for each workspace. In the tab All sessions, each session should have a highlight with the color of its workspace. In the main, the input place should be the bottom of main, and it should look like a regular terminal input."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Workspace colors (Priority: P1)

Every workspace has its own color. Its tab shows that color, and in "All sessions" each session
that belongs to a workspace is highlighted in that workspace's color, so the user can see the
grouping at a glance.

**Independent Test**: Create two workspaces, move one session into each; the tabs have different colors and the two rows in "All sessions" carry the matching colors.

**Acceptance Scenarios**:

1. **Given** workspaces exist, **Then** no two of them share a color.
2. **Given** a session in workspace A, **When** viewing "All sessions", **Then** its row is highlighted in A's color.
3. **Given** a session in no workspace, **Then** its row has no highlight.
4. **Given** the app restarts, **Then** each workspace keeps its color.
5. **Given** a workspace is deleted and a new one created, **Then** the new one gets a color no remaining workspace uses.

---

### User Story 2 - Terminal-style input at the bottom (Priority: P1)

The prompt input sits at the bottom of the main area and looks like a terminal input line:
dark background, monospace text, a prompt symbol, no buttons. Up/Down history and Enter to send
work as before.

**Acceptance Scenarios**:

1. **Given** a session is open, **Then** the input is at the bottom of the main area, below the conversation.
2. **Given** the input, **Then** it is styled like a terminal line (prompt symbol, monospace, dark background).
3. **Given** focus in the input, **When** Up/Down/Enter are pressed, **Then** history and sending behave as in feature 001.

## Requirements *(mandatory)*

- **FR-001**: Each workspace MUST have a color distinct from all other existing workspaces, persisted with the workspace.
- **FR-002**: Workspace tabs MUST show their color.
- **FR-003**: In "All sessions", a session in a workspace MUST be highlighted with that workspace's color.
- **FR-004**: The prompt input MUST be at the bottom of the main area and styled like a terminal input line.
- **FR-005**: This supersedes spec 001 FR-004 ("prompt input at the top").

## Success Criteria *(mandatory)*

- **SC-001**: The user can tell which workspace a session belongs to from "All sessions" without opening tabs.

## Assumptions

- Existing workspaces without a color get one assigned on load.
- The pinned "You" box stays at the top of the conversation.
