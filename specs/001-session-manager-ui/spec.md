# Feature Specification: Claude Session Manager UI

**Feature Branch**: `001-session-manager-ui`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "An app to manage Claude sessions. A bar on the left with session names. Clicking one opens that session in the main space. Two main features: the input prompt always appears at the top of the main space, and pressing the up arrow there shows previous input prompts. Code diffs created in the session appear in a third bar on the right-most side. Bare minimum implementation, to be refined by playing with it."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Browse and open sessions (Priority: P1)

The user sees a list of their Claude sessions in a left sidebar, each labeled with a readable
name. Clicking one shows that session's conversation in the main area.

**Why this priority**: Without picking a session nothing else has context.

**Independent Test**: Launch the app with existing sessions on disk; click any entry and see its messages.

**Acceptance Scenarios**:

1. **Given** sessions exist, **When** the app starts, **Then** the sidebar lists them, most recent first.
2. **Given** the sidebar is shown, **When** the user clicks a session, **Then** the main area shows its user and assistant messages in order and the entry is highlighted.

---

### User Story 2 - Prompt bar with history (Priority: P1)

A prompt input is always visible at the top of the main area. When focused, pressing Up
recalls the previous prompt sent in the selected session; repeated Up goes further back,
Down goes forward and finally back to the empty draft. Pressing Enter sends the prompt to the
selected session, and the reply appears in the conversation.

**Why this priority**: Stated as main feature #1.

**Independent Test**: Open a session with past prompts, focus the input, press Up several times and see them in reverse order.

**Acceptance Scenarios**:

1. **Given** a session with prompts A, B, C, **When** the user presses Up three times, **Then** the input shows C, then B, then A.
2. **Given** the user is browsing history, **When** they press Down past the newest prompt, **Then** the input returns to what they were typing.
3. **Given** a typed prompt, **When** the user presses Enter, **Then** the prompt is sent to that session, the input clears, and the response appears when ready.
4. **Given** the conversation is long, **When** the user scrolls, **Then** the prompt bar stays at the top.

---

### User Story 3 - Diff panel (Priority: P2)

A right sidebar lists every code change made in the selected session (file edits and file
writes), showing the file path and removed/added lines.

**Why this priority**: Stated as main feature #2; useful only once a session is open.

**Independent Test**: Open a session in which files were edited; the right panel shows each change with removed lines and added lines distinguishable.

**Acceptance Scenarios**:

1. **Given** a session that edited files, **When** it is opened, **Then** the right panel lists each change in order with its file path.
2. **Given** a session with no edits, **When** it is opened, **Then** the right panel says there are no changes.

### Edge Cases

- No sessions on disk: sidebar shows an empty-state message.
- Corrupt or unknown lines in a session file: skipped silently; the rest still loads.
- Sending a prompt while a previous one is still running: input is disabled until it finishes.
- The `claude` command fails: the error text is shown in the conversation area.
- History at oldest entry: further Up does nothing.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST list existing Claude sessions in a left sidebar, most recently modified first.
- **FR-002**: Each session MUST display a name: its generated title if available, else its first prompt, else its id.
- **FR-003**: Clicking a session MUST show its conversation (user and assistant text) in the main area.
- **FR-004**: A prompt input MUST stay fixed at the top of the main area regardless of scrolling.
- **FR-005**: Up/Down in the prompt input MUST navigate the selected session's previous prompts.
- **FR-006**: Enter in the prompt input MUST send the prompt to the selected session and refresh the conversation when complete.
- **FR-007**: A right sidebar MUST list code changes (edits and file writes) from the selected session with file path and removed/added lines.
- **FR-008**: System MUST NOT modify Claude's session files directly.
- **FR-009**: Users MUST be able to manually refresh the session list and the open session.

### Key Entities

- **Session**: id, project directory, display name, last-modified time, ordered messages.
- **Message**: role (user/assistant), text.
- **Prompt history**: ordered list of user-typed prompts within a session.
- **Code change**: file path, removed lines, added lines, position in session.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With 200+ sessions on disk, the sidebar is usable within 2 seconds of launch.
- **SC-002**: Opening a session shows its messages within 1 second for sessions under 5 MB.
- **SC-003**: Recalling any of the last 10 prompts takes at most 10 key presses and no mouse use.
- **SC-004**: Every file edit in a session appears in the diff panel.

## Assumptions

- Sessions are the ones Claude Code stores locally for the current user (all projects).
- "Open the session" means view it and continue it by sending new prompts; a full interactive terminal inside the app is out of scope for v1.
- Prompt history is per session (not global); global history is a candidate for later.
- Diffs come from the edits recorded in the session, not from git.
- Desktop-only, single user, mouse + keyboard.
- No live streaming of replies in v1; the reply appears when complete.
