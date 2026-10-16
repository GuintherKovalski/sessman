# Feature Specification: Dictation

**Feature Branch**: `main` (no branch created)

**Created**: 2026-10-16

**Status**: Draft

**Input**: User description: "Dictation: a microphone button in the prompt bar, left of the refine (✨) button. Click starts recording and the button changes into a red stop symbol; clicking again stops. The audio is transcribed locally with OpenAI Whisper on macOS; Linux later. The transcribed text is inserted into the prompt input at the cursor."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Dictate a prompt by voice (Priority: P1)

The user wants to speak a prompt instead of typing it. They click the microphone button next to
the refine button, speak, click the same button again, and the spoken words appear in the prompt
input where the cursor is, ready to edit or send.

**Why this priority**: This is the whole feature. Nothing else is useful without a working
record-stop-insert loop.

**Independent Test**: Open a session, click the microphone, say "fix the failing login test",
click again, and confirm the words appear in the input at the cursor.

**Acceptance Scenarios**:

1. **Given** a session is open and the prompt input is empty, **When** the user clicks the
   microphone button, **Then** the button immediately shows a red stop symbol and recording starts.
2. **Given** recording is in progress, **When** the user clicks the red stop symbol, **Then**
   recording ends, the button shows the microphone again while the speech is transcribed, and the
   transcribed text is inserted into the input.
3. **Given** the input contains "Please ", with the cursor after the space, **When** the user
   dictates "refactor the parser", **Then** the input reads "Please refactor the parser" and the
   cursor sits after the inserted text.
4. **Given** the cursor is in the middle of existing text with no space before it, **When** a
   dictation is inserted, **Then** a single space separates the inserted text from the words
   before it.
5. **Given** a dictation has been inserted, **When** the user presses Enter (or Cmd+Enter, per
   settings), **Then** the prompt is sent with the dictated text, exactly as if it had been typed.

---

### User Story 2 - Understand the microphone button's state (Priority: P2)

The user needs to know at a glance whether the app is listening, transcribing, or idle, and what
the button will do on the next click.

**Why this priority**: Recording a microphone without a clear signal is a privacy and trust
problem, so the state must be obvious. It is secondary to making dictation work at all.

**Independent Test**: Click the microphone and watch the button; hover it in each state and read
the tooltip.

**Acceptance Scenarios**:

1. **Given** the app is idle, **When** the user hovers the microphone, **Then** the tooltip reads
   "Dictate: click to start, click again to stop".
2. **Given** recording is in progress, **When** the user hovers the stop symbol, **Then** the
   tooltip reads "Stop dictation", and the symbol is red.
3. **Given** the speech is being transcribed, **When** the user looks at the prompt bar, **Then** a
   spinner shows next to the microphone, the microphone cannot be clicked, and its tooltip reads
   "Transcribing…".
4. **Given** no session is open, **When** the user hovers the microphone, **Then** it is disabled
   and its tooltip reads "Select a session first".

---

### User Story 3 - Clear messages when dictation cannot run (Priority: P2)

When something prevents dictation (missing model, no microphone, no permission, failed
transcription), the user is told what to do, and the prompt input is never left broken.

**Why this priority**: These are the most likely first-run problems, and a silent failure would
look like a broken button.

**Independent Test**: Remove the model file, click the microphone, and read the message. Repeat
with microphone permission denied.

**Acceptance Scenarios**:

1. **Given** the Whisper model file is missing, **When** the user clicks the microphone, **Then**
   recording does not start and the error line above the input shows where to download the model
   and where to put it.
2. **Given** no microphone is available, **When** the user clicks the microphone, **Then**
   recording does not start and the error line says no microphone was found.
3. **Given** microphone access is denied by macOS, **When** the user clicks the microphone,
   **Then** recording does not start and the error line says the microphone could not be opened.
4. **Given** the user clicks the stop symbol right away, **When** nothing was captured, **Then**
   nothing is inserted and the button returns to the microphone without an error.
5. **Given** transcription fails, **When** the result arrives, **Then** the error line shows the
   reason, the input is unchanged, and the button returns to the microphone so the user can try
   again.
6. **Given** the speech is silent or unintelligible, **When** transcription finishes with no
   text, **Then** nothing is inserted and the input is unchanged.

---

### User Story 4 - Dictation is hidden where it is not supported (Priority: P3)

On platforms where dictation is not built yet (anything other than macOS), the microphone button
does not appear, and the rest of the prompt bar looks and behaves exactly as before.

**Why this priority**: Keeps the other platforms clean until Linux support arrives.

**Independent Test**: Run the app on a non-macOS build and confirm no microphone appears next to
the refine button.

**Acceptance Scenarios**:

1. **Given** a non-macOS build, **When** the prompt bar is shown, **Then** there is no microphone
   button and the refine button keeps its previous position.

---

### Edge Cases

- **Switching sessions while recording**: recording continues; when it stops, the text goes into
  the shared prompt input, the same as typing would.
- **Clicking send while recording**: the prompt sent is whatever was typed so far; the
  microphone keeps recording until it is stopped, and its text goes into the input afterwards.
- **Starting a second recording during transcription**: not possible; the button is disabled
  until transcription ends.
- **Very long recording** (several minutes): it is transcribed in full and inserted in one go;
  the spinner shows the wait.
- **Model file present but damaged or of the wrong kind**: the error line shows the transcription
  failure; the app does not crash.
- **Refine running at the same time**: dictation still works; the refined text replaces the input
  only as refine already does.
- **Non-English speech**: the language is detected automatically; the text is inserted in the
  language spoken.
- **Microphone unplugged during recording**: recording stops with an error and any audio already
  captured is not inserted.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The prompt bar MUST show a microphone button immediately to the left of the refine
  (✨) button, on macOS.
- **FR-002**: A click on the microphone MUST start recording from the default microphone, with no
  extra dialog.
- **FR-003**: While recording, the button MUST show a stop symbol in a color different from the
  idle microphone (red), and a click on it MUST stop recording.
- **FR-004**: Transcription MUST run entirely on the user's Mac, with no audio or text sent to any
  online service.
- **FR-005**: Transcription MUST use OpenAI's Whisper model, with Metal acceleration on macOS.
- **FR-006**: The language MUST be detected automatically, so English and Portuguese both work.
- **FR-007**: Once transcription finishes, the text MUST be inserted into the prompt input at the
  cursor position, with a space added before it when the preceding character is not whitespace,
  and the cursor MUST be placed after the inserted text.
- **FR-008**: Leading and trailing whitespace MUST be removed from the transcript before
  insertion; an empty transcript MUST insert nothing.
- **FR-009**: The inserted text MUST be editable and sendable like typed text; it MUST NOT be sent
  automatically.
- **FR-010**: While transcribing, the microphone MUST be disabled and a spinner MUST be shown.
- **FR-011**: The app MUST NOT record audio unless the user has clicked the microphone and has not
  yet clicked it again to stop.
- **FR-012**: When the Whisper model file is missing, the app MUST NOT start recording and MUST
  show, in the error line, the model's download address and the path where to put it.
- **FR-013**: When no microphone is available or it cannot be opened, the app MUST NOT start
  recording and MUST show an error line that says so.
- **FR-014**: Recordings with no captured audio MUST insert nothing and MUST NOT show an error.
- **FR-015**: A transcription failure MUST show an error line with the reason, leave the input
  unchanged, and return the button to idle.
- **FR-016**: The microphone MUST be enabled while recording even if no session is open, so the
  user can always stop it; while idle, it MUST be disabled when no session is open.
- **FR-017**: On platforms other than macOS, the microphone button MUST NOT be shown and the rest
  of the prompt bar MUST be unchanged.
- **FR-018**: The microphone button's tooltips MUST match the states in User Story 2.

### Key Entities *(include if feature involves data)*

- **Recording**: one session of microphone capture, from the first click to the second. It has
  no stored form; it lasts only in memory until it is transcribed.
- **Transcript**: the text produced from one recording. It is inserted into the prompt input and
  is not kept separately.
- **Whisper model**: the speech-recognition file the user installs once. It is not created or
  changed by the app.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can dictate a 10-second sentence and see it in the input in under 5 seconds
  after clicking stop, on a Mac with an Apple Silicon chip.
- **SC-002**: At least 90% of short clear English and Portuguese sentences are transcribed with
  every word correct, in a test set recorded in a quiet room.
- **SC-003**: In every failure case listed in the acceptance scenarios, the user sees a
  message that names the cause, and the prompt input is never lost or changed by the failure.
- **SC-004**: No audio or text leaves the Mac during dictation, verifiable by checking that the
  app makes no network request while recording or transcribing.
- **SC-005**: The microphone's state (idle, recording, transcribing) is identifiable from its
  appearance alone, in both light and dark mode, without reading the tooltip.
- **SC-006**: On non-macOS builds, the prompt bar is identical to its state before this feature.

## Assumptions

- The user installs the Whisper model file once, following the README steps; the app does not
  download it. The default model is the multilingual "base" size, around 150 MB.
- The app uses the Mac's default microphone; choosing another input device is out of scope.
- Dictation is available only to people using the Mac app; there is no keyboard shortcut to
  start or stop it in this version.
- Recording length is not capped; the user stops it manually.
- Each transcription is independent: the app does not keep the audio or the transcript after
  inserting the text.
- Linux and Windows support are out of scope for this feature and will come later.
