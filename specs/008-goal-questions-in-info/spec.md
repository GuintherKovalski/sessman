# Feature Specification: Goal First, with Its Own Question Field

**Feature Branch**: `008-goal-questions-in-info`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "Move goal to the top, and at the bottom of the items add the text input to ask questions about the current goal. I don't want to use the regular input for that."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Ask about the goal from the Info tab (Priority: P1)

Goal is the first section of the Info tab. Under its checklist there is a field to ask
questions about the goal; the answer appears right below it. The main prompt input is only for
prompts again (no `/btw`).

**Acceptance Scenarios**:

1. **Given** the Info tab, **Then** Goal is shown first, then Model, Context, Usage limits.
2. **Given** a session with a conversation, **When** the user types a question under the checklist and presses Enter, **Then** the answer appears below the field, and the session is unchanged.
3. **Given** an answer is showing, **When** the user clicks Dismiss or opens another session, **Then** it goes away.
4. **Given** the main input, **When** the user types `/btw …`, **Then** it is sent as a normal prompt (the feature moved).

## Requirements *(mandatory)*

- **FR-001**: Goal MUST be the first Info section.
- **FR-002**: A question field MUST follow the goal checklist; answers MUST use the session's context and MUST NOT modify the session.
- **FR-003**: The question sent MUST include the current goal and checklist, so answers stay on the goal.
- **FR-004**: Supersedes spec 007 US2: `/btw` is removed from the main input.

## Success Criteria *(mandatory)*

- **SC-001**: The user can ask about progress on the goal without touching the main input.
