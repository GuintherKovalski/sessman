//! The orchestration page's Processes tab: what the workspace's sessions are running.
use super::harness::*;

/// Sessions are found in `ps` by their `--resume <id>`.
const ID: &str = "5d0c3a3e-8a43-4f7e-9a59-0d2b8c1f7a10";

fn work() -> Scope {
    let mut s = setup()
        .session(session(ID, "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .workspaces(&[("Work", &[ID, "s-b"])])
        .launch();
    s.click_first("WORK");
    s
}

#[test]
fn with_nothing_running_the_tab_says_so() {
    let mut s = work();
    s.click("Orchestration");
    s.click("Processes");
    s.wait_to_see("Nothing is running in this scope's sessions.");
    s.click("Report");
    s.wait_to_see("Fake summary of ");
}

#[test]
fn a_working_session_shows_its_claude_and_bash_command_with_its_time_limit() {
    let mut s = work();
    s.click_first("alpha prompt");
    s.env.hold("hold");
    s.send("SHELL HOLD build it");
    s.wait_to_see("Partial reply");
    s.click("Orchestration");
    s.click("Processes");
    s.wait_to_see("Wait for the build");
    assert!(s.sees("sleep 30"), "the command Bash runs");
    assert!(s.sees("claude -p"), "the session's claude above it");
    assert!(s.sees("left of 1:00"), "how much of its time limit is left");
    assert!(s.sees("Processes · "), "the tab counts them");
    s.click_last("alpha prompt");
    assert!(s.is_selected("alpha prompt"), "the session's name opens it");
    s.env.release("hold");
    s.wait_to_see("Echo: SHELL HOLD build it");
    s.click("Orchestration");
    s.click("Processes");
    s.wait_to_see("Nothing is running in this scope's sessions.");
}
