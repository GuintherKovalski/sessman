# Tasks: Dictation

**Input**: `specs/013-dictation/plan.md`, `spec.md`, `research.md`, `data-model.md`, `quickstart.md`

Status key: **[done]** written, **[verify]** written but not yet compiled or run (see the build
blocker in plan.md), **[todo]** not started.

## Phase 1: Setup

- [done] T001 Add `cpal` 0.16 and `whisper-rs` 0.15 (feature `metal`) under `cfg(target_os = "macos")` in `Cargo.toml`
- [done] T002 Register `mod dictation;` in `src/main.rs`
- [verify] T003 Install cmake (`brew install cmake`) and confirm `cargo check` resolves the new crates

## Phase 2: Core logic (pure, tested)

- [done] T004 `dictation::insert_at(input, cursor, text) -> (String, usize)` in `src/dictation.rs`, with a space before the text when needed and character-based cursor
- [done] T005 Unit tests for `insert_at`: empty input, space rules, mid-text, trimming and empty text, multi-byte characters, cursor past end (`src/dictation.rs`, `mod tests`)
- [done] T006 `dictation::resample` (linear, to 16 kHz) with its unit test (macOS only, `src/dictation.rs`)
- [verify] T007 Run `cargo test dictation` and confirm all six tests pass

## Phase 3: Capture and transcription (macOS)

- [verify] T008 `Recorder::start`/`finish` with cpal: default input, mono averaging, sample formats F32/I16/U16 (`src/dictation.rs`)
- [verify] T009 `transcribe` with whisper-rs: model load, greedy decode, language auto-detect, segment text (`src/dictation.rs`)
- [done] T010 Non-macOS stub `Recorder` and `transcribe` returning a clear error

## Phase 4: UI (US1, US2)

- [done] T011 `Dictation` enum field and initialization in `src/app.rs`
- [done] T012 `toggle_dictation`: model check, start, stop with transcription thread (`src/app.rs`)
- [done] T013 `poll_dictation`: receive result, insert or show error, handle disconnect
- [done] T014 `insert_dictated` uses `dictation::insert_at` and moves the cursor after the text
- [done] T015 Mic button left of ✨ with idle/recording/transcribing states, tooltips, spinner, and width adjustment in `prompt_bar`
- [verify] T016 Check 🎤 renders in egui's default font (else draw the icon)

## Phase 5: Failures (US3)

- [done] T017 Missing model: error with URL and path, recording not started (FR-012)
- [done] T018 Microphone open failure: error line (FR-013)
- [done] T019 Empty recording inserts nothing (FR-014): `finish` returns empty; `insert_at` leaves input unchanged
- [done] T020 Transcription failure and disconnect: error line, input unchanged (FR-015)

## Phase 6: Platform (US4)

- [done] T021 Button hidden when `dictation::AVAILABLE` is false (FR-017)
- [verify] T022 `cargo check` on Linux confirms the non-macOS stub compiles

## Phase 7: Docs and verification

- [done] T023 README: feature line, cmake and model setup, microphone note
- [todo] T024 Manual checks from quickstart.md on macOS (record/insert, failures, states)
- [todo] T025 Accuracy and latency checks for SC-001 and SC-002 with fixed test sentences
