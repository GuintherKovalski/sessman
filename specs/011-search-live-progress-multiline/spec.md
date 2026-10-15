# Feature Specification: Session Search, Live Progress, Multi-line Prompt

**Feature Branch**: `011-search-live-progress-multiline`

**Created**: 2026-10-15

**Status**: Implemented

**Input**: User description: "Remove the button with a reload and sessions on top. Add a search input, that will look for matching the session names first, then the summarizations, and then the own content on the sessions. Put it in the bottom. Simply typing in the input will look for string match or string similarities, pressing enter will use a haiku/cheap model, and look across the last sessions. Also, I can't see anything happening until the last answer arrives, make it so that I can see the model progress before final answer arrives. In the input prompt, make it so that the input expands as I write more text, creating more lines. Pressing shift + enter should jump lines."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See Claude's progress while it works (Priority: P1)

After sending a prompt, the user's prompt shows in the conversation right away. While Claude
works, its reply appears as it is written, and each tool call shows up as it starts. Finished
steps (text, tool calls, file changes) stay in the conversation and the Changes tab while the
next step runs. The final answer is simply the last step; nothing jumps or reloads at the end.

**Why this priority**: Today nothing happens for minutes and then everything appears at once;
the user can't tell whether Claude is stuck or what it's doing.

**Independent Test**: Send a prompt that makes Claude read and edit files; text and
`⚙ Read`/`⚙ Edit` lines appear one by one before the run ends.

**Acceptance Scenarios**:

1. **Given** a prompt was sent, **Then** it appears in the conversation within a second.
2. **Given** Claude is writing a reply, **Then** the reply grows on screen as it is written, with an indicator that Claude is still working.
3. **Given** Claude starts a tool call, **Then** a tool line for it appears before the tool finishes.
4. **Given** Claude edited a file mid-run, **Then** the change shows in the Changes tab before the run ends.
5. **Given** the run fails, **Then** what was shown so far stays and the error appears.

---

### User Story 2 - Search sessions as you type (Priority: P1)

A search field at the bottom of the left panel filters the session list as the user types.
Results are ordered by where they matched: session names first, then goal summaries, then the
sessions' own content (prompts and replies). Close spellings still match names and goals
(e.g. "rendring" finds "rendering"). Each result shows where it matched; content matches show
a short snippet around the match. Clearing the field brings back the normal list.

**Why this priority**: With many sessions, scrolling to find one doesn't work; the header
space it replaces held only a title and a reload button.

**Independent Test**: Type a word that appears only inside one session's conversation; that
session is listed with a snippet.

**Acceptance Scenarios**:

1. **Given** sessions whose name, goal, or content contains the query, **Then** name matches are listed first, then goal matches, then content matches; a session appears once, at its best match.
2. **Given** a query with a small typo, **Then** sessions whose name or goal is a close match are found.
3. **Given** a content match, **Then** a snippet with the matching text is shown under the session name.
4. **Given** the search is active, **Then** clicking a result opens that session as usual, and the active workspace tab still limits which sessions are searched.
5. **Given** the user clears the field (or presses Escape), **Then** the normal list returns.

---

### User Story 3 - Ask a model to find a session (Priority: P2)

Pressing Enter in the search field asks a small, cheap model which of the most recent sessions
match what the user described (e.g. "the one where we fixed the login timeout"). While it
thinks, a spinner shows; then the matching sessions are listed, best first, each with a
one-line reason.

**Why this priority**: Typing finds words; describing finds intent. Valuable, but live search
covers most cases.

**Acceptance Scenarios**:

1. **Given** a query, **When** the user presses Enter, **Then** a spinner shows and, when done, the sessions the model picked are listed best first with a short reason each.
2. **Given** model results are shown, **When** the user edits the query, **Then** the list goes back to live results.
3. **Given** the model fails or finds nothing, **Then** a short message says so and live results remain available.

---

### User Story 4 - Prompt input grows; Shift+Enter for new lines (Priority: P2)

The prompt input starts as one line and grows as the text wraps or new lines are added, up
to a limit after which it scrolls. Shift+Enter inserts a new line; Enter sends.

**Acceptance Scenarios**:

1. **Given** a long prompt, **Then** the input grows to show it on several lines, up to about 10 lines, then scrolls.
2. **Given** the input is focused, **When** the user presses Shift+Enter, **Then** a new line is inserted; **When** they press Enter, **Then** the prompt is sent.
3. **Given** a one-line input, **Then** Up/Down still step through prompt history; in a multi-line input they move the cursor between lines.

---

### User Story 5 - No header; list refreshes itself (Priority: P3)

The "Sessions" heading and the reload button are gone from the top of the left panel; the
list picks up new and updated sessions by itself. The theme switch moves to the right panel's
tab bar.

**Acceptance Scenarios**:

1. **Given** a session started elsewhere (e.g. in a terminal), **Then** it appears in the list within about 10 seconds without any action.
2. **Given** the list refreshes, **Then** the selected session, scroll position and any search stay as they were.

### Edge Cases

- Many or very large session files: typing stays responsive; content search may lag briefly behind the newest changes, but never freezes the window.
- A search query of one character: only name and goal matches (content would match nearly everything).
- Enter on an empty search does nothing.
- The model returns sessions that don't exist or are malformed: those lines are ignored.
- A run that prints nothing for a long time still shows the working indicator.
- New sessions (first prompt) show progress the same way.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The left panel MUST NOT have the "Sessions" heading or reload button; the session list MUST refresh on its own at least every 10 seconds and after each run.
- **FR-002**: A search field MUST sit at the bottom of the left panel; typing MUST filter the visible sessions live.
- **FR-003**: Live results MUST be ranked by match source: name, then goal summary, then content; within a source, better matches first, then newer sessions.
- **FR-004**: Name and goal matching MUST tolerate small typos; content matching MAY require the words to appear as typed (case-insensitive).
- **FR-005**: Each result MUST show where it matched; content matches MUST show a snippet.
- **FR-006**: Enter in the search field MUST ask a small model to pick matching sessions among the 40 most recent ones (within the active workspace tab), showing a reason per result.
- **FR-007**: While a prompt runs, the conversation MUST show the user's prompt, Claude's reply text as it streams, and each tool call as it starts; completed steps and file changes MUST appear before the run ends.
- **FR-008**: The prompt input MUST grow with its content up to ~10 lines, then scroll; Shift+Enter MUST insert a new line; Enter MUST send.
- **FR-009**: Up/Down history MUST keep working for single-line input.
- **FR-010**: The theme switch MUST stay reachable after the header is removed.

## Success Criteria *(mandatory)*

- **SC-001**: After sending a prompt, the user sees something new within 2 seconds, and keeps seeing progress at least with every step Claude takes.
- **SC-002**: A session the user remembers by a word in its name is found by typing that word, with no other action, in under a second.
- **SC-003**: A session the user can only describe is found with one Enter press in most tries.
- **SC-004**: A 10-line prompt can be written and reviewed in full before sending.

## Assumptions

- "Summarizations" means the goal summaries from the Info tab (only sessions where the user clicked Summarize goal have one).
- Model search looks at the 40 most recent sessions (by last activity), using their names, goals and the start of their prompts; it costs a little usage, like Summarize goal.
- Searching respects the active workspace tab (searching "All sessions" covers everything).
- Live progress needs no change in what is stored: sessman still only reads Claude's files.
