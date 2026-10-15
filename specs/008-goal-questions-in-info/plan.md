# Implementation Plan: Goal First, with Its Own Question Field

**Branch**: `008-goal-questions-in-info` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

Reorder the Info sections. Move the side-question answer box from above the main input into
the Info tab, below a new single-line field (`goal_question`). On Enter, the question is
wrapped with the saved goal + checklist ("answer briefly; do not change any files") and sent
with the existing `runner::btw` (forked, non-persisted resume). Remove the `/btw` branch from
`send` and its hint.

Constitution: unchanged (PASS on all four).
