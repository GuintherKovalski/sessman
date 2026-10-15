# Feature Specification: Goal Checklist and /btw Side Questions

**Feature Branch**: `007-goal-checklist-btw`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "When I click Summarize goal, steer the prompt so that it creates and renders a small checklist of subgoals. Add also a /btw-like input to ask questions about the current session."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Goal with subgoal checklist (Priority: P1)

"Summarize goal" produces the session's goal plus 3 to 7 subgoals, each already ticked if the
conversation shows it was done. The user can tick and untick items; that is remembered.

**Acceptance Scenarios**:

1. **Given** a session, **When** the user clicks "Summarize goal", **Then** the Info tab shows the goal, "N of M done", and a checklist.
2. **Given** a subgoal Claude reported finishing, **Then** it starts ticked.
3. **Given** the checklist, **When** the user ticks an item, **Then** it stays ticked after restarting.
4. **Given** a goal saved before checklists existed, **Then** it still shows as plain text.

---

### User Story 2 - /btw side questions (Priority: P1)

Typing `/btw <question>` in the prompt input asks about the current session with its full
context, without adding anything to the session. The answer shows above the input until
dismissed.

**Acceptance Scenarios**:

1. **Given** an open session, **When** the user sends `/btw what did we decide about X?`, **Then** an answer box appears above the input with a spinner, then the answer.
2. **Given** a /btw answer, **Then** the session's conversation is unchanged and no new session appears.
3. **Given** the answer box, **When** the user clicks Dismiss or opens another session, **Then** it goes away.
4. **Given** `/btw` with no question, or a new session with no conversation yet, **Then** a short error explains why.

## Requirements *(mandatory)*

- **FR-001**: The summarizer MUST return a goal and a 3–7 item checklist with done state.
- **FR-002**: Checklist items MUST be toggleable and persisted.
- **FR-003**: `/btw <question>` MUST answer with the session's full context and MUST NOT modify the session or create a new one.
- **FR-004**: /btw answers MUST be shown above the input and be dismissible.

## Success Criteria *(mandatory)*

- **SC-001**: The user sees what is done and what is left in a session at a glance.
- **SC-002**: The user can ask about a session without polluting it.

## Assumptions

- /btw uses the session's own model and context, so it costs about as much as a normal prompt.
