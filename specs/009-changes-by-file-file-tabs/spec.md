# Feature Specification: Changes by File, File List, and Read-Only File Tabs

**Feature Branch**: `009-changes-by-file-file-tabs`

**Created**: 2026-10-15

**Status**: Draft

**Input**: User description: "In the right bar, split changes by file, and use some rendering to show it with a better style. At the bottom of this tab, create a list with files; if I click a file, show its changes on top; if I double-click a file, open it in the main area (create a tab Main, highlighted, and a tab with the file name next to it). Files are not editable, we can only see them."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Changes grouped by file, styled (Priority: P1)

The Changes tab shows one card per changed file: file name, folder, and +added/-removed
counts, followed by its edits as diff hunks with syntax highlighting, tinted rows for added and
removed lines, and unchanged stretches collapsed.

**Acceptance Scenarios**:

1. **Given** a session that edited a file several times, **Then** all its edits appear in one card for that file.
2. **Given** a card, **Then** added lines are green-tinted with `+`, removed red-tinted with `-`, context dimmed, gaps shown as `⋮`.

---

### User Story 2 - File list filters the changes (Priority: P1)

At the bottom of the Changes tab, a list of changed files with their counts. Clicking a file
shows only its changes above; clicking it again (or "Show all") shows all.

**Acceptance Scenarios**:

1. **Given** the file list, **When** the user clicks a file, **Then** only that file's card is shown and the file is highlighted.
2. **Given** a filtered view, **When** the user clicks the file again or "Show all", **Then** all cards show.

---

### User Story 3 - Open files read-only in main tabs (Priority: P1)

Double-clicking a file (in the list or on its card header) opens it in the main area. A tab bar
appears above the main area with "Main" (the conversation) and one tab per open file; the active
tab is highlighted. Files are read-only, with line numbers and syntax highlighting.

**Acceptance Scenarios**:

1. **Given** the file list, **When** the user double-clicks a file, **Then** a tab with its name opens and is active, showing the file's current contents.
2. **Given** file tabs, **When** the user clicks "Main", **Then** the conversation shows again.
3. **Given** a file tab, **When** the user clicks its `x`, **Then** it closes.
4. **Given** an open file, **Then** it cannot be edited (text can be selected and copied).
5. **Given** Claude changes the file, **When** the session reloads, **Then** the tab shows the new contents.

### Edge Cases

- Deleted, binary, or very large (>2 MB) files show a message instead of contents.
- Only changes made with Claude's Edit / MultiEdit / Write tools are known; edits done through shell commands don't appear.

## Requirements *(mandatory)*

- **FR-001**: Changes MUST be grouped by file in order of first change, with per-file counts.
- **FR-002**: Diff lines MUST be syntax-highlighted and tinted by kind; context MUST be limited to 3 lines around changes.
- **FR-003**: A file list MUST sit at the bottom of the Changes tab; click filters, double-click opens.
- **FR-004**: The main area MUST show tabs "Main" + open files when any file is open; the active tab MUST be highlighted.
- **FR-005**: File tabs MUST be read-only.

## Success Criteria *(mandatory)*

- **SC-001**: The user can go from "what changed in this file" to reading the whole file in one double-click.
