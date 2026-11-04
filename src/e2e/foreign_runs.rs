//! Runs scope did not start, such as the ones still going from before a restart: scope
//! finds them, shows them as working and follows them to the end.
use super::harness::*;
use eframe::egui::{Key, Modifiers};

/// Runs are found by their `--resume <id>`, which has to look like a real session id. Each
/// test has its own: tests run at once, and one would find the other's run by its id.
const ID: &str = "5d0c3a3e-8a43-4f7e-9a59-0d2b8c1f7a10";
const ID2: &str = "5d0c3a3e-8a43-4f7e-9a59-0d2b8c1f7a11";

fn open_session(id: &str) -> Scope {
    let mut s = setup().session(session(id, "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s
}

#[test]
fn a_run_left_from_before_a_restart_shows_as_working_until_its_reply_arrives() {
    let mut s = open_session(ID);
    s.env.hold("hold");
    s.env.run_outside(ID, "HOLD left running");
    s.wait_to_see("⏹");
    s.wait_to_see("HOLD left running");
    s.send("after it");
    assert!(s.sees("queued 1 · after it"), "a prompt waits for it, as for scope's own runs");
    s.env.release("hold");
    s.wait_to_see("Echo: HOLD left running");
    s.wait_to_see("Echo: after it");
    assert_eq!(s.env.runs().iter().map(|r| r.1.as_str()).collect::<Vec<_>>(), ["HOLD left running", "after it"]);
}

#[test]
fn ctrl_c_stops_a_run_scope_did_not_start() {
    let mut s = open_session(ID2);
    s.env.hold("hold");
    s.env.run_outside(ID2, "HOLD left running");
    s.wait_to_see("⏹");
    s.press_with(Modifiers::CTRL, Key::C);
    s.wait_to_see("⏹ Interrupted");
    let gone = || !std::process::Command::new("pgrep").args(["-f", ID2]).status().unwrap().success();
    s.wait_until("its process ends", |_| gone());
    s.env.release("hold");
    s.frames(20);
    assert!(!s.sees("Echo: HOLD left running"), "it ended before replying");
}
