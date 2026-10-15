# Feature Specification: Workspaces and Pinned Prompt

**Feature Branch**: `002-workspaces-pinned-prompt`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "The 'You' in blue should always appear pinned on the top of the main. The bar on the left should have tabs on top: all sessions, +. If I click on plus, I can name a 'workspace'. I can drag and drop sessions there, or right click a session and have the option 'Move to', and when hovering or selecting it, all created tabs should appear in it."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pinned prompt (Priority: P1)

While reading a long reply, the prompt that started it stays visible at the top of the
conversation, so the user never loses track of what they asked.

**Independent Test**: Open a session with a long reply, scroll down; the "You" prompt of that turn stays at the top.

**Acceptance Scenarios**:

1. **Given** a session is open, **When** the user scrolls through a reply, **Then** the user prompt that the reply answers is shown pinned above the conversation.
2. **Given** the user scrolls into a later turn, **When** that turn's prompt reaches the top, **Then** the pinned prompt switches to it.
3. **Given** the pinned prompt is very long, **Then** it takes limited height and can be scrolled on its own.

---

### User Story 2 - Workspaces as tabs (Priority: P1)

The left bar has tabs: "All sessions", one tab per workspace, and "+". Clicking "+" asks for
a name and creates a workspace tab. Selecting a workspace tab lists only its sessions.

**Independent Test**: Click +, type "client-a", Enter; a "client-a" tab appears, empty.

**Acceptance Scenarios**:

1. **Given** the left bar, **When** the user clicks "+" and enters a name, **Then** a new tab with that name appears.
2. **Given** an empty name or Escape, **Then** no workspace is created.
3. **Given** workspaces exist, **When** the app restarts, **Then** they are still there with their sessions.

---

### User Story 3 - Put sessions into workspaces (Priority: P1)

The user can drag a session onto a workspace tab, or right-click it and choose
"Move to" → (list of all workspaces).

**Acceptance Scenarios**:

1. **Given** a workspace tab, **When** the user drags a session onto it, **Then** the session belongs to that workspace.
2. **Given** a session, **When** the user right-clicks and hovers "Move to", **Then** every workspace is listed; choosing one moves the session there.
3. **Given** a session in workspace A, **When** it is moved to B, **Then** it is in B and no longer in A.
4. **Given** a workspace tab is open, **When** the user right-clicks a session, **Then** they can remove it from that workspace.
5. **Given** a workspace tab, **When** the user right-clicks the tab, **Then** they can delete the workspace (sessions are not deleted).

### Edge Cases

- No workspaces yet: "Move to" says to create one with "+".
- A session in a workspace no longer exists on disk: it is simply not listed.
- Dropping a session on "All sessions" or outside a tab does nothing.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The conversation area MUST always show a "You" prompt pinned at its top: the last user prompt at or above the current scroll position (the first prompt when at the very top).
- **FR-002**: The left bar MUST show tabs "All sessions", each workspace, and "+".
- **FR-003**: "+" MUST let the user name and create a workspace.
- **FR-004**: Sessions MUST be movable to a workspace by drag-and-drop onto its tab and by a right-click "Move to" submenu listing all workspaces.
- **FR-005**: A session MUST belong to at most one workspace; "All sessions" always lists every session.
- **FR-006**: Workspaces MUST persist across restarts and MUST NOT be stored in Claude's files.
- **FR-007**: Users MUST be able to remove a session from a workspace and delete a workspace.

### Key Entities

- **Workspace**: name, ordered list of session ids.

## Success Criteria *(mandatory)*

- **SC-001**: Creating a workspace and moving a session into it takes under 10 seconds.
- **SC-002**: At any scroll position, the prompt being answered is visible without scrolling.

## Assumptions

- "Move" means a session lives in one workspace at a time.
- Workspaces are stored in the user's config folder as a small file.
- Renaming and reordering workspaces are out of scope for now.
