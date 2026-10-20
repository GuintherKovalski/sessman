//! The prompt input: sending, live progress, the queue, stopping, history.
use super::harness::*;
use eframe::egui::{Key, Modifiers};
use serde_json::json;

fn one_session() -> Scope {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s
}

#[test]
fn enter_sends_the_prompt_to_its_session_and_shows_the_reply() {
    let mut s = one_session();
    s.send("please do it");
    assert_eq!(s.prompt(), "", "the input empties once sent");
    s.wait_to_see("Echo: please do it");
    assert_eq!(s.env.runs(), vec![("s-a".to_owned(), "please do it".to_owned())]);
    let call = &s.env.calls()[s.env.calls().len() - 1];
    let args: Vec<&str> = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(args.windows(2).any(|w| w == ["--resume", "s-a"]));
    assert!(args.contains(&"--include-partial-messages"));
    assert!(args.contains(&"--append-system-prompt"), "sessions are told about the scope commands");
    assert_eq!(call["cwd"], s.env.proj.display().to_string(), "it runs in the session's folder");
    assert_eq!(call["session"], "s-a");
}

#[test]
fn settings_add_the_model_and_extra_arguments() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .setting("model", json!("opus"))
        .setting("extra_args", json!("--permission-mode acceptEdits"))
        .launch();
    s.click("alpha prompt");
    s.send("go");
    s.wait_to_see("Echo: go");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--resume")).unwrap();
    let args: Vec<&str> = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert_eq!(&args[..3], ["-p", "--permission-mode", "acceptEdits"]);
    assert!(args.windows(2).any(|w| w == ["--model", "opus"]));
}

#[test]
fn an_empty_prompt_is_not_sent() {
    let mut s = one_session();
    s.send("   ");
    s.frames(10);
    assert!(s.env.runs().is_empty());
}

#[test]
fn progress_shows_while_claude_works() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("TOOL HOLD slow one");
    s.wait_to_see("Partial reply");
    assert!(s.sees("⚙ Bash…"));
    assert!(s.sees("⏹"), "the stop button shows while it works");
    s.env.release("hold");
    s.wait_to_see("Echo: TOOL HOLD slow one");
    s.wait_not_to_see("Partial reply");
    assert!(!s.sees("⚙ Bash…"));
}

#[test]
fn prompts_sent_while_claude_works_are_queued_and_sent_in_turn() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("HOLD first");
    s.wait_to_see("Partial reply");
    s.send("second");
    s.send("third");
    assert!(s.sees("queued 1 · second"));
    assert!(s.sees("queued 2 · third"));
    assert!(s.sees("alpha prompt  +2 queued"));
    s.env.release("hold");
    s.wait_to_see("Echo: third");
    let prompts: Vec<String> = s.env.runs().into_iter().map(|r| r.1).collect();
    assert_eq!(prompts, vec!["HOLD first", "second", "third"]);
    assert!(!s.sees("queued 1"));
}

#[test]
fn a_queued_prompt_can_be_removed_edited_or_pulled_back() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("HOLD first");
    s.wait_to_see("Partial reply");
    s.send("to remove");
    s.send("to edit");
    s.send("to pull back");
    // x removes the first.
    s.click_first("x");
    assert!(!s.sees("to remove"));
    // Clicking one puts it back in the input.
    s.click("queued 1 · to edit");
    assert_eq!(s.prompt(), "to edit");
    // Something typed takes its place in the queue then; Up in an empty input pulls back the newest.
    s.replace_in("prompt-input", "");
    s.press(Key::ArrowUp);
    assert_eq!(s.prompt(), "to pull back");
    s.env.release("hold");
    s.wait_for_runs();
    let prompts: Vec<String> = s.env.runs().into_iter().map(|r| r.1).collect();
    assert_eq!(prompts, vec!["HOLD first"], "nothing was left in the queue");
}

#[test]
fn ctrl_c_stops_claude_and_gives_the_prompt_back() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("HOLD long job");
    s.wait_to_see("Partial reply");
    s.send("queued behind it");
    s.press_with(Modifiers::CTRL, Key::C);
    s.wait_to_see("⏹ Interrupted");
    assert_eq!(s.prompt(), "HOLD long job", "the stopped prompt comes back to fix");
    assert!(s.sees("Partial reply"), "what it had written stays");
    assert!(s.sees("Queue paused: stopped with Ctrl+C"));
    s.env.release("hold");
    s.frames(20);
    assert_eq!(s.env.runs().len(), 1, "the queue doesn't go on by itself");
    s.click("Resume queue");
    s.wait_to_see("Echo: queued behind it");
}

#[test]
fn the_stop_button_stops_claude_too() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("HOLD long job");
    s.wait_to_see("Partial reply");
    s.click("⏹");
    s.wait_to_see("⏹ Interrupted");
    s.env.release("hold");
}

#[test]
fn a_failed_run_pauses_the_queue() {
    let mut s = one_session();
    s.env.hold("hold");
    s.send("HOLD FAIL doomed");
    s.wait_to_see("Partial reply");
    s.send("after the failure");
    s.env.release("hold");
    s.wait_to_see("Queue paused: the last run failed");
    s.frames(20);
    assert_eq!(s.env.runs().len(), 1);
}

#[test]
#[ignore = "known bug: end_run sets the error, then reload() clears it at once, so the reason never shows"]
fn a_failed_run_shows_its_reason() {
    let mut s = one_session();
    s.send("FAIL doomed");
    s.wait_to_see("fake failure");
}

#[test]
fn several_sessions_run_at_once() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .launch();
    s.env.hold("hold");
    s.click("alpha prompt");
    s.send("HOLD in alpha");
    s.wait_to_see("Partial reply");
    s.click("beta prompt");
    assert!(!s.sees("Partial reply"), "each session shows its own run");
    s.send("HOLD in beta");
    s.wait_to_see("Partial reply");
    s.env.release("hold");
    s.wait_to_see("Echo: HOLD in beta");
    // Both ran just now: once both files hold their reply, the list settles newest first.
    let env = s.env.clone();
    let done = |id: &str, text: &str| std::fs::read_to_string(env.session_file(id).unwrap()).unwrap().contains(text);
    s.wait_until("both runs are written", |_| done("s-a", "Echo: HOLD in alpha") && done("s-b", "Echo: HOLD in beta"));
    s.settle();
    s.click("alpha prompt");
    s.wait_to_see("Echo: HOLD in alpha");
}

#[test]
fn up_and_down_step_through_the_prompts_sent_before() {
    let mut s = setup().session(session("s-a", "first ever", 100).turn("second ever", "ok")).launch();
    s.click("first ever");
    s.type_prompt("draft");
    s.press(Key::ArrowUp);
    assert_eq!(s.prompt(), "second ever");
    s.press(Key::ArrowUp);
    assert_eq!(s.prompt(), "first ever");
    s.press(Key::ArrowDown);
    assert_eq!(s.prompt(), "second ever");
    s.press(Key::ArrowDown);
    assert_eq!(s.prompt(), "draft", "past the newest, what was being typed comes back");
}

#[test]
fn shift_enter_starts_a_new_line() {
    let mut s = one_session();
    s.type_prompt("line one");
    s.press_with(Modifiers::SHIFT, Key::Enter);
    s.type_prompt("line two");
    assert_eq!(s.prompt(), "line one\nline two");
    assert!(s.env.runs().is_empty());
    s.press(Key::Enter);
    s.wait_to_see("Echo: line one");
    assert_eq!(s.env.runs()[0].1, "line one\nline two");
}

#[test]
#[ignore = "known bug: with the setting on, the input gets return_key(None), so Enter adds no line (Settings says it should)"]
fn with_send_on_cmd_enter_enter_adds_a_line() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .setting("send_with_modifier", json!(true))
        .launch();
    s.click("alpha prompt");
    s.type_prompt("one");
    s.press(Key::Enter);
    s.type_prompt("two");
    assert_eq!(s.prompt(), "one\ntwo");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_to_see("Echo: one");
}

#[test]
fn the_input_says_what_it_is_for() {
    let mut s = one_session();
    assert!(s.sees("type a prompt · enter to send · cmd+enter refines · up/down for history"));
    s.env.hold("hold");
    s.send("HOLD x");
    s.wait_to_see("claude is working… enter queues a prompt · ctrl+c stops");
    s.env.release("hold");
}

#[test]
fn the_input_size_is_kept_across_restarts() {
    let mut s = one_session();
    s.click("Collapse the input to one line");
    assert!(s.sees("Expand the input"));
    assert_eq!(s.saved_state()["prompt_collapsed"], true);
    let s = s.restart();
    assert!(s.sees("Expand the input"));
}
