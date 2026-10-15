# Feature Specification: Rename Sessions

**Feature Branch**: `003-rename-sessions`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "If I double click a session name, I want to rename it (like renaming a folder). If I press Enter, save it."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Rename in place (Priority: P1)

Double-clicking a session's name turns it into an editable field with the current name
selected, like renaming a folder. Enter saves the new name; it is shown everywhere the session
appears and survives restarts.

**Independent Test**: Double-click a session, type "auth refactor", Enter; the row shows "auth refactor", also after restarting.

**Acceptance Scenarios**:

1. **Given** the session list, **When** the user double-clicks a name, **Then** the name becomes editable with all text selected.
2. **Given** the name is being edited, **When** the user presses Enter, **Then** the new name is saved and shown.
3. **Given** the name is being edited, **When** the user presses Escape, **Then** the edit is cancelled and the old name stays.
4. **Given** the name is being edited, **When** the user clicks elsewhere, **Then** the new name is saved (as with folders).
5. **Given** a renamed session, **When** the user saves an empty name, **Then** the original name comes back.

### Edge Cases

- Renaming does not change which workspace the session is in.
- The renamed title is used in every tab and while dragging.

## Requirements *(mandatory)*

- **FR-001**: Double-click on a session name MUST start an inline rename with the text selected.
- **FR-002**: Enter and clicking away MUST save; Escape MUST cancel.
- **FR-003**: Custom names MUST persist across restarts and MUST NOT be written to Claude's files.
- **FR-004**: Saving an empty name MUST restore the default name.

## Success Criteria *(mandatory)*

- **SC-001**: Renaming a session takes one double-click, typing, and Enter.

## Assumptions

- Names are sessman-only: Claude Code itself keeps its own title.
