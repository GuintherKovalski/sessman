# Feature Specification: New Session Button and Click-to-Focus Input

**Feature Branch**: `005-new-session-click-focus`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "If I click anywhere on main, it should select the prompt input. On top of All sessions in the left bar there should be a button to create a new session."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Click anywhere to type (Priority: P1)

Clicking anywhere in the main area puts the cursor in the prompt input, like clicking in a
terminal window.

**Acceptance Scenarios**:

1. **Given** a session is open, **When** the user clicks anywhere in the main area, **Then** the prompt input gets keyboard focus.
2. **Given** the user drags to select text in the conversation, **Then** focus does not jump (only plain clicks focus the input).
3. **Given** the user opens a session from the list, **Then** the prompt input is focused.

---

### User Story 2 - Create a new session (Priority: P1)

A "New session" button at the top of the left bar asks which folder the session should work
in. The main area then shows an empty new session; the first prompt starts it, and it appears
in the list, selected.

**Acceptance Scenarios**:

1. **Given** the left bar, **When** the user clicks "New session", **Then** a folder picker opens (starting at the current session's folder).
2. **Given** a folder is chosen, **Then** the main area shows "New session in <folder>" with the input focused.
3. **Given** the empty new session, **When** the user sends a prompt, **Then** Claude starts a new session in that folder, and when done it appears at the top of the list, selected, showing the conversation.
4. **Given** a workspace tab is open, **When** a new session is created, **Then** it is added to that workspace.
5. **Given** the user cancels the picker, **Then** nothing changes.

### Edge Cases

- Starting the session fails: the error shows above the input and the empty session stays so the user can retry.
- Clicking a session in the list while a new session is still empty discards the empty one.

## Requirements *(mandatory)*

- **FR-001**: A plain click anywhere in the main area MUST focus the prompt input.
- **FR-002**: Opening a session MUST focus the prompt input.
- **FR-003**: The left bar MUST have a "New session" button above the tabs.
- **FR-004**: Creating a session MUST ask for its working folder.
- **FR-005**: The first prompt MUST create the session in that folder; it then MUST appear in the list, selected.
- **FR-006**: A session created while a workspace tab is open MUST join that workspace.

## Success Criteria *(mandatory)*

- **SC-001**: Starting a new session takes one button click, one folder choice, and one prompt.

## Assumptions

- The folder picker is the standard macOS one.
- Nothing is created on disk until the first prompt is sent (Claude creates the session then).
