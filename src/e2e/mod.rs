//! App tests: the whole app, driven headless the way a user drives it, feature by feature.
//!
//! Each test starts the real app (`App::new`) on a throwaway home and config folder, with a
//! fake `claude` (examples/fake_claude.rs, built on first use), then clicks, types, presses
//! keys and sends `sessman` commands, and checks what the app shows and the files it writes.
//! They never read the app's fields, so the app's code can be reorganized freely: these
//! tests pass as long as it behaves the same. See harness.rs for how it works and what it
//! relies on.
//!
//! Not covered here (they need the real window or the system): the native folder picker,
//! moving files to the Trash, notifications, dragging and zooming the window by its title
//! bar, recording and transcribing dictation, the Whisper download, the lasso, reordering
//! orchestration cards by drag, and how things look (colors, stripes, the pinned "You").
mod harness;

mod changes;
mod commands;
mod conversation;
mod find;
mod info;
mod new_session;
mod orchestration_page;
mod overview;
mod prompt;
mod refine;
mod search;
mod sessions;
mod settings_page;
mod startup;
mod workspaces;
