# Quickstart: Dictation

How to check the feature works end to end. Run it on a Mac.

## Prerequisites

1. Build tools: `brew install cmake` and Xcode command-line tools (`xcode-select --install`).
2. The model (once; see README):

   ```sh
   mkdir -p ~/.config/sessman/models
   curl -L -o ~/.config/sessman/models/ggml-base.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
   ```

## Automated checks

```sh
cargo check
cargo test
```

`cargo test` covers the pure logic: `resample` (length and identity) and `insert_at` (spacing,
cursor placement, empty text). See `plan.md` for the list.

## Manual checks

Run `cargo run --release`, open a session, and go through the spec's acceptance scenarios:

| Check | Steps | Expected (spec) |
|-------|-------|-----------------|
| Record and insert (US1) | Click 🎤, say a sentence, click ⏹ | Text appears at the cursor after a short spinner (SC-001) |
| Mid-text insert (US1-3, 4) | Type `Please `, put the cursor at the end, dictate | `Please <dictated>`, cursor after it |
| Tooltips and states (US2) | Hover each state; disable with no session | Tooltips and colors per FR-018 |
| Model missing (US3-1) | Move the model file away, click 🎤 | Error line shows URL and path; no recording |
| Mic denied (US3-3) | Deny microphone access for the terminal in System Settings, click 🎤 | Error line; no recording |
| Silent recording (US3-4, 6) | Click 🎤 and ⏹ at once; or stay silent | Nothing inserted; no error for the first, no text for the second |
| Non-macOS (US4) | Build on another OS, or check the code path | No mic button |

## Recording the result

Record in the spec's `checklists/` or in `tasks.md` which checks passed. SC-002 (transcription
accuracy) needs a fixed set of recorded sentences, which is not yet set up.
