# Data Model: Dictation

Nothing is stored on disk. All state lives in memory on the app, in the `Dictation` enum.

## Dictation (state of the mic button)

| State | Holds | Mic button | Entered when |
|-------|-------|------------|--------------|
| `Idle` | nothing | microphone, enabled if a session is open | start, after a transcript is inserted, after an error, after a stop with no audio |
| `Recording` | the open capture stream and the samples collected so far | red stop symbol, always enabled | a click from `Idle` when the model exists and the microphone opens |
| `Transcribing` | receiving end of the worker thread's result | microphone, disabled, spinner beside it | a click from `Recording` |

### Transitions

- `Idle` → `Recording`: click, model file exists, microphone opened. On failure: stay `Idle`,
  set the error line.
- `Recording` → `Transcribing`: click. Stream is stopped, audio resampled to 16 kHz mono, worker
  thread started.
- `Recording` → `Idle` (no audio): click with zero samples. Nothing inserted, no error.
- `Transcribing` → `Idle`: result received. `Ok(text)` inserts through `insert_at`; `Err` sets the
  error line. A disconnected channel counts as an error.
- `Transcribing`: clicks are ignored.

## Recording (in memory)

- **Samples**: mono `f32`, at the device's native rate while recording, resampled to 16 000 Hz on
  stop.
- **Lifetime**: from the first click to the stop click; dropped after transcription.

## Transcript

- **Text**: trimmed output of the model, joined from its segments.
- **Insertion**: `insert_at(input, cursor, text) -> (new_input, new_cursor)`, where `cursor` and
  `new_cursor` are character indexes. A space is added before the text when the character before
  the cursor is not whitespace. Empty text returns the input unchanged.

## Validation rules (from the spec)

- Recording needs the model file to exist (FR-012).
- Recording needs an input device that opens (FR-013).
- Empty or whitespace-only transcripts insert nothing (FR-008, FR-014).
