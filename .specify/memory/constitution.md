<!--
Sync Impact Report
- Version: 1.0.0 → 1.1.0: Principle II allows the edits the user asks for (Trash, delete a message), with a backup
- Version: (template) → 1.0.0
- Principles defined: I. Minimal First, II. Read Claude's Data, Don't Own It, III. Single Binary, IV. Learn By Using
- Templates: plan/spec/tasks templates compatible, no changes needed
- Follow-up TODOs: none
-->
# sessman Constitution

## Core Principles

### I. Minimal First
Build the smallest thing that can be played with. No feature lands before someone needs it
while using the app. Prefer deleting code to configuring it. YAGNI applies to abstractions,
settings, and error handling beyond "show the error and keep running".

### II. Read Claude's Data, Don't Own It
Claude Code already stores sessions on disk. sessman reads those files and drives the
`claude` CLI; it MUST NOT keep a parallel copy of session state or write to Claude's files,
except for an edit the user asks for explicitly (moving a session to the Trash, deleting a
message from its context), which leaves a way back (the Trash, or a backup of the file).

### III. Single Binary
One Rust crate, one binary, `cargo run` to start. Dependencies are allowed only when they
remove more code than they add.

### IV. Learn By Using
Each iteration ends with something runnable. Decisions that can only be settled by using the
app (layout, colors, refresh rates, keybindings) are deferred and recorded as open questions,
not guessed at length up front.

## Constraints

- Personal tool, macOS first; no packaging, auth, or telemetry.
- Tests only for pure logic (parsing, diffing, history navigation), not for UI.

## Governance

This constitution guides scope decisions in `/speckit-plan`. Amend it freely as usage teaches
what matters; bump the version when a principle changes.

**Version**: 1.1.0 | **Ratified**: 2026-10-15 | **Last Amended**: 2026-10-28
