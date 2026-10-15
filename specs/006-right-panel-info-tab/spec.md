# Feature Specification: Right Panel Tabs with Session Info

**Feature Branch**: `006-right-panel-info-tab`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "On the right side, create tabs: the first one is Changes, the other one is Info. It should show context usage, model being used, a small description of this session's goal, and usage limits."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Info tab (Priority: P1)

The right panel has two tabs, Changes (the existing diffs) and Info. Info shows, for the open
session: the model, how full the context window is, a short description of the session's
goal, and the account's usage limits.

**Acceptance Scenarios**:

1. **Given** a session is open, **When** the user picks Info, **Then** they see the model of the latest reply and a bar with context used / context size.
2. **Given** a session with prompts, **Then** the goal shows the AI-written description if one was made, else the start of the first prompt and a "Summarize goal" button.
3. **Given** the user clicks "Summarize goal", **Then** a one- or two-sentence goal is generated, shown, and remembered.
4. **Given** sessman's status line is enabled and Claude Code has reported limits, **Then** the 5-hour and weekly limits show as bars with the time until reset.
5. **Given** it is not enabled, **Then** the section explains why and offers "Enable".
6. **Given** the user already has their own status line, **Then** sessman does not replace it and says so.

### Edge Cases

- No replies yet: context says so; model "unknown yet".
- A limit window whose reset time has passed is hidden (stale).
- API-key users get no limits from Claude Code; the section keeps its waiting message.

## Requirements *(mandatory)*

- **FR-001**: The right panel MUST have tabs Changes and Info.
- **FR-002**: Info MUST show model, context usage, goal, and usage limits.
- **FR-003**: Context size MUST come from Claude Code's own report when available, else from the model.
- **FR-004**: Goal summaries MUST be generated only on request and MUST NOT create a session.
- **FR-005**: Enabling limits MUST be an explicit user action and MUST NOT overwrite an existing status line.

## Success Criteria *(mandatory)*

- **SC-001**: The user can see context fullness and remaining limits without leaving the app.

## Assumptions

- Usage limits are only available to Pro/Max subscribers, through Claude Code's status line.
- Limits refresh when Claude Code refreshes its status line (after each reply in an interactive session).
