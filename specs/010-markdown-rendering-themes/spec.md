# Feature Specification: Markdown Rendering, Styling, and Themes

**Feature Branch**: `010-markdown-rendering-themes`

**Created**: 2026-10-15

**Status**: Implemented

**Input**: User description: "Our focus now is on rendering, style, themes. I want to render tables in the main, render .md files when opened, etc."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Conversation rendered as Markdown, tables included (Priority: P1)

Claude's replies (and the user's prompts) are shown formatted instead of as raw text: headings,
bold / italic, inline code, bulleted and numbered lists, links, block quotes, horizontal rules,
fenced code blocks with syntax highlighting, and tables drawn as real tables with a header row,
column alignment, and borders.

**Why this priority**: Claude answers in Markdown; read raw, tables and code blocks are the
hardest parts to follow. This is the main ask.

**Independent Test**: Open a session where Claude replied with a table and a code block; both
render formatted.

**Acceptance Scenarios**:

1. **Given** a reply containing a Markdown table, **Then** it shows as a grid with a bold header row and cells aligned as the `:---:` markers say.
2. **Given** a reply with a fenced code block tagged with a language, **Then** it shows in a monospace box with syntax highlighting, and its text can be selected and copied.
3. **Given** headings, lists, bold, italic, inline code, quotes and links, **Then** each is visibly formatted; clicking a link opens it in the browser.
4. **Given** a table wider than the main area, **Then** it scrolls horizontally instead of squeezing or breaking the layout.
5. **Given** the pinned "You" prompt, **Then** it is rendered the same way.

---

### User Story 2 - .md files open rendered (Priority: P1)

When a Markdown file (`.md`, `.markdown`) is opened in a main-area file tab, it shows rendered,
with a toggle in the tab to switch to the raw source (the existing read-only view with line
numbers).

**Why this priority**: Specs, READMEs and plans Claude writes are Markdown; reading them rendered
is the point of opening them.

**Independent Test**: Double-click a changed `.md` file in the Changes file list; it opens
rendered; the toggle shows its source.

**Acceptance Scenarios**:

1. **Given** a `.md` file opened from the Changes list, **Then** it shows rendered with the same formatting as the conversation.
2. **Given** a rendered file, **When** the user clicks "Source", **Then** the raw text with line numbers shows; clicking "Rendered" goes back.
3. **Given** a non-Markdown file, **Then** nothing changes from today (source view, no toggle).

---

### User Story 3 - Themes: dark, light, follow system (Priority: P2)

A theme switch (Dark / Light / System) is always reachable. Every part of the app follows it:
panels, message cards, the prompt input, diff tints, code highlighting, workspace colors stay
readable. The choice is remembered between runs; "System" follows the macOS appearance.

**Why this priority**: Several colors are hard-coded for dark mode today, so a light theme
would break the prompt input and diffs; fixing that is part of "style".

**Independent Test**: Switch to Light; all text is readable, nothing stays dark-only; restart and
it is still Light.

**Acceptance Scenarios**:

1. **Given** the theme switch, **When** the user picks Light, **Then** the whole window turns light at once, including the prompt input, diff rows and code blocks.
2. **Given** a chosen theme, **When** the app restarts, **Then** the same theme is used.
3. **Given** System, **When** macOS switches between light and dark, **Then** the app follows.

---

### User Story 4 - Cleaner conversation styling (Priority: P2)

Messages are shown as distinct cards with consistent spacing: the user's prompts and Claude's
replies are easy to tell apart at a glance (role label and an accent color), and tool calls are
shown compactly and dimmed so they don't compete with the text.

**Acceptance Scenarios**:

1. **Given** a conversation, **Then** each message sits in its own card with an accent for its role, and consecutive messages are evenly spaced.
2. **Given** a tool call (`[tool: Name]`), **Then** it shows as a small dimmed line, not as a full message.

### Edge Cases

- Malformed Markdown (unclosed code fence, ragged table rows) still renders readably and never hides text; rows with missing cells show empty cells.
- Very long replies stay smooth to scroll (rendering must not noticeably slow scrolling in sessions with hundreds of messages).
- Images in Markdown are shown as their alt text / link, not loaded.
- Raw HTML in Markdown is shown as text, not interpreted.
- A `.md` file that is missing, binary or too large shows the same message as other files today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Conversation messages MUST be rendered as Markdown (CommonMark plus tables, strikethrough and task lists).
- **FR-002**: Tables MUST render as a grid with a header row, borders, per-column alignment, and horizontal scrolling when too wide.
- **FR-003**: Fenced code blocks MUST be syntax-highlighted when tagged with a known language, monospace otherwise, and selectable.
- **FR-004**: Links MUST open in the default browser when clicked.
- **FR-005**: Markdown files opened in a file tab MUST render by default, with a Rendered / Source toggle per tab.
- **FR-006**: The app MUST offer Dark, Light and System themes; the choice MUST persist in sessman's own config folder.
- **FR-007**: No color in the app may be readable in only one theme (prompt input, diff tints, accents, line numbers, card fills).
- **FR-008**: Messages MUST be shown as role-accented cards with consistent spacing; tool calls MUST be compact and dimmed.
- **FR-009**: The pinned "You" prompt MUST use the same rendering as the conversation.

## Success Criteria *(mandatory)*

- **SC-001**: A reply containing a table can be read row by row without mentally parsing pipes and dashes.
- **SC-002**: Opening a `.md` file shows it formatted in one double-click, with source one click away.
- **SC-003**: In each theme, every text element in the app is clearly readable against its background.
- **SC-004**: Scrolling a session of 300+ messages stays as smooth as before this change.

## Assumptions

- Markdown is shown read-only; sessman doesn't edit `.md` files.
- Themes are a three-way choice; no custom colors or font settings in this iteration (Minimal First).
- The theme switch lives in a small, always-visible spot (e.g. top of the left panel); exact placement is settled by using the app.
- Tool calls stay summarized as today (name only); showing their inputs/outputs is out of scope.
- Rendering in the right panel (Info answers, goal) may reuse the Markdown view, but isn't required.
