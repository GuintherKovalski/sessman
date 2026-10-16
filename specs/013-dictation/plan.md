# Implementation Plan: Dictation

**Branch**: `main` (no branch created) | **Date**: 2026-10-16 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/013-dictation/spec.md`

## Summary

A 🎤 button left of ✨ in the prompt bar records the default microphone on click and, on the
second click, transcribes the audio locally with Whisper (whisper.cpp through `whisper-rs`, Metal
on macOS). The transcript is inserted at the prompt's cursor. The code is drafted; this plan
records the design, the file-level changes, and how it will be verified.

## Technical Context

**Language/Version**: Rust, edition 2024 (existing crate)

**Primary Dependencies**: eframe/egui 0.36 (existing); `cpal` 0.16 for capture and `whisper-rs`
0.15 with `metal` for transcription, both under `cfg(target_os = "macos")`

**Storage**: none. The model file lives at `~/.config/sessman/models/ggml-base.bin`; nothing else
is written.

**Testing**: `cargo test` for pure logic only (`dictation::resample`, `dictation::insert_at`)

**Target Platform**: macOS (Apple Silicon and Intel). Other platforms build without the feature.

**Project Type**: desktop app (single binary)

**Performance Goals**: transcript inserted within 5 s of stopping a 10-second recording on Apple
Silicon (SC-001)

**Constraints**: offline (FR-004); the UI thread never waits on transcription; the crate builds on
Linux without ALSA headers

**Scale/Scope**: one mic button, one recording at a time, recordings of minutes at most

**Open build blockers** (not design questions, but they gate verification):
- `cmake` is not installed, so `whisper-rs-sys` cannot build. Fix: `brew install cmake`.
- `cargo` and `brew` commands were gated by the approval step during planning, so the crate
  versions and API calls are not yet checked against the real crates (see research.md, items 1
  and 2, and item 5 for egui).

## Constitution Check

*GATE: passed before Phase 0; re-checked after Phase 1.*

- **I. Minimal First**: PASS. The feature was asked for directly. It adds one module, one
  button, and no settings, shortcuts, or download code.
- **II. Read Claude's Data, Don't Own It**: PASS. Dictation touches only the input box; it writes
  nothing to Claude's files or to sessman's state.
- **III. Single Binary**: PASS with one justified dependency group (see Complexity Tracking).
  Still one crate and one binary, started with `cargo run`.
- **IV. Learn By Using**: PASS. The open questions (model size, cold-load delay, microphone
  attribution, whether 🎤 renders in egui's font) are listed in research.md and get settled by using
  the app, not up front.
- **Constraints, tests for pure logic only**: PASS. The two pure functions are tested; the button
  and the threading are verified by hand (quickstart.md).

## Phase 0: Research

Output: [research.md](research.md). Decisions: `cpal` capture with linear resampling; `whisper-rs`
in-process, with `whisper-cli` as the fallback; model loaded per transcription; model downloaded by
the user; insertion at the cursor; the stream kept on the UI thread; microphone permission left to
macOS. No NEEDS CLARIFICATION remains in the spec. Open API questions are marked **to verify**.

## Phase 1: Design

Output: [data-model.md](data-model.md) (the three-state `Dictation` machine and the insertion
rule), [quickstart.md](quickstart.md) (manual and automated checks). No `contracts/` directory:
the feature exposes no interface beyond the button, whose states are already in the spec and the
data model, and the project has no external interface.

### File-level design

| File | Change | Why |
|------|--------|-----|
| `src/dictation.rs` | New. `AVAILABLE`, `model_path`, `MODEL_URL`; on macOS `Recorder` (cpal capture, mono, `finish` resamples to 16 kHz), `transcribe` (whisper-rs), `resample`, `insert_at`; stubs on other platforms | Keeps all dictation logic in one place and keeps pure functions testable |
| `src/app.rs` | `Dictation` enum field; `toggle_dictation`, `poll_dictation`; mic button in `prompt_bar` after the refine button; button-width adjustment | The UI and state hooks sit where refine's already do |
| `src/main.rs` | `mod dictation;` | |
| `Cargo.toml` | `cpal` and `whisper-rs` under `cfg(target_os = "macos")` | Keeps other platforms' builds free of native dependencies |
| `README.md` | Dictation feature line and setup steps | The model download is a manual step |

### Change to the drafted code

`insert_dictated` currently does its string edit inline in `app.rs`. The plan moves the edit into a
pure `dictation::insert_at(input: &str, cursor: usize, text: &str) -> (String, usize)` (character
indexes in and out). `app.rs` then reads the cursor, calls it, and stores the new cursor. This is
the only refactor, and it is needed to test the insertion rule under the constitution.

### Verification strategy

- **Unit tests** (`cargo test`):
  - `resample`: 48 kHz to 16 kHz gives the right length; equal rates return the input.
  - `insert_at`: inserting into an empty input; inserting after a non-space character adds one
    space; inserting after a space adds none; inserting in the middle; whitespace-only text leaves
    the input unchanged; multi-byte characters (e.g. `ç`, `✨`) give the right character cursor.
- **Build**: `cargo check` and `cargo build --release` on macOS once cmake is installed; `cargo check`
  on Linux to confirm the non-macOS stub compiles.
- **Manual** (quickstart.md): the record/insert path, each failure message (model missing,
  microphone denied, silent recording), the three button states and their tooltips, and the
  mic-free layout on non-macOS.
- **Accuracy** (SC-002): deferred until a fixed set of recorded sentences exists.

## Project Structure

### Documentation (this feature)

```text
specs/013-dictation/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── checklists/
│   └── requirements.md
└── tasks.md             # from /speckit-tasks, not created here
```

### Source Code (repository root)

```text
src/
├── app.rs          # Dictation state, toggle/poll/insert, mic button
├── dictation.rs    # NEW: capture, transcription, resample, insert_at, stubs
└── main.rs         # mod dictation;

Cargo.toml          # cpal, whisper-rs (macOS target)
README.md           # setup and feature line
```

**Structure Decision**: a single new module beside the existing ones (`refine.rs`, `runner.rs`),
with the platform-specific code behind `cfg` inside it, so `app.rs` has no platform checks other
than `dictation::AVAILABLE`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Two native dependencies (`cpal`, `whisper-rs`) under Constitution III ("allowed only when they remove more code than they add") | Capture and Whisper inference cannot reasonably be written by hand; the feature is not possible without them | Writing audio capture on AVFoundation by hand is much more unsafe code than the crate. Running `whisper-cli` as an external program is kept as the fallback because it trades the native build for an install step and temp files |
