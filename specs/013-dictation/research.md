# Research: Dictation

Items marked **to verify** could only be settled by compiling against the real crates. Building
needs `cmake` (for whisper.cpp) and the crates resolved by cargo, and neither ran in the
planning session, so they are open until `cargo check` passes.

## 1. Audio capture

- **Decision**: `cpal` 0.16, default input device, stream format read from the device, frames
  averaged to mono `f32`, samples kept in memory until the stop click.
- **Rationale**: the only cross-platform capture crate that fits a single Rust binary. Its Linux
  backend (ALSA) needs system headers, so it sits under the macOS target for now.
- **Alternatives considered**: AVFoundation through `objc2` (no new crate, but a lot of unsafe
  Objective-C for one feature); shelling out to `ffmpeg` or `sox` (an external tool the user must
  install).
- **Open**: `cpal` 0.16 API (`default_input_config`, `StreamConfig` from `supported.config()`,
  `build_input_stream` with four arguments, `FromSample::from_sample_`). **to verify**.

## 2. Speech engine

- **Decision**: `whisper-rs` 0.15 with the `metal` feature, under the macOS target, running the
  multilingual `ggml-base.bin` model with language auto-detection.
- **Rationale**: this is what the user asked for. Whisper runs in-process, so no audio leaves the
  Mac (FR-004), and Metal gives acceleration on Apple Silicon.
- **Alternatives considered**: the `whisper-cli` binary from Homebrew's `whisper-cpp` formula.
  It avoids building whisper.cpp from source and so avoids cmake, but it adds a second install
  step and a temporary WAV file. Kept as the fallback if the in-process build keeps failing.
- **Open**: `whisper-rs` 0.15 API (`WhisperContext::new_with_params`, `set_language(None)`,
  `state.as_iter()`, `to_str_lossy()` return type). **to verify**.

## 3. Model loading

- **Decision**: load the model for each transcription, on the worker thread.
- **Rationale**: simplest, and the cost (about a second for the base model) is small next to the
  wait the user already accepts after clicking stop. It also avoids guessing whether the context
  is safe to share across threads.
- **Alternatives considered**: keep one loaded context in the app. Faster after the first use, but
  adds lifetime and thread-safety questions. Deferred until the delay is seen in use (constitution
  principle IV).

## 4. Model delivery

- **Decision**: the user downloads `ggml-base.bin` once (README command); the app checks that the
  file exists before recording and shows the URL and path if it doesn't.
- **Rationale**: keeps the app small and avoids a network feature that the spec doesn't ask for.
- **Alternatives considered**: in-app download with progress. More code and more UI, for a one-time
  step.

## 5. Inserting the transcript

- **Decision**: insert at the cursor. The cursor index is read from the prompt's stored egui
  `TextEditState` (keyed by `prompt-input`). After insertion the cursor is moved to the end of the
  inserted text and the input is re-focused. The string edit itself is a pure function,
  `dictation::insert_at`, so it can be unit-tested.
- **Rationale**: dictation is usually used mid-sentence; appending to the end would be surprising.
- **Alternatives considered**: always append to the end (simpler, but wrong mid-text).
- **Open**: `TextEditState::load`/`store` and `cursor.set_char_range` with egui 0.36. **to verify**.

## 6. Threading

- **Decision**: the `cpal` stream stays on the UI thread (owned by the `Dictation` state); the
  transcription runs on a `std::thread` that sends its result over an `mpsc` channel, polled once a
  frame like the refine request.
- **Rationale**: matches the existing `runner` and refine patterns; avoids assuming the stream can
  move between threads.

## 7. Microphone permission

- **Decision**: rely on macOS prompting the first time the input device is opened. No explicit
  permission request in the app.
- **Rationale**: minimal. The prompt is attributed to whatever app runs the binary (the terminal,
  for `cargo run`), which is worth stating in the README.
- **Alternatives considered**: an explicit `AVCaptureDevice` authorization request before
  recording. Deferred until the attribution actually confuses someone.
