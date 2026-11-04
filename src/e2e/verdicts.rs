//! Tags for how a session's work ended: after a run ends, and for workspace sessions with
//! recent activity no verdict has read, the helper model says done, needs you or blocked.
use super::harness::*;
use serde_json::json;

fn work() -> Setup {
    setup().setting("tag_finished", json!(true))
}

fn verdict_calls(s: &Scope) -> usize {
    s.env.calls().iter().filter(|c| c.to_string().contains("Has this Claude Code session finished")).count()
}

#[test]
fn a_finished_run_is_tagged_done_until_there_is_newer_activity() {
    // Ten days old: too old for the sweep, so only the run gets it checked.
    let mut s = work().session(session("s-a", "alpha prompt", 10 * 24 * 3600)).workspaces(&[("Work", &["s-a"])]).launch();
    s.click_first("WORK");
    s.click("alpha prompt");
    s.settle();
    assert_eq!(verdict_calls(&s), 0, "nothing new to judge");
    s.send("ship it");
    s.wait_to_see("Echo: ship it");
    s.wait_to_see("✓ done");
    s.wait_until("the list has the tag", |s| s.list().contains("s-a  [done]"));
    assert_eq!(s.env.config_json("verdicts.json")["s-a"]["why"], "Fake work shipped");
    assert_eq!(verdict_calls(&s), 1);
    // A run that fails isn't judged, and its activity makes the old verdict stale (changes are
    // compared by the second: a second later).
    std::thread::sleep(std::time::Duration::from_millis(1100));
    s.send("FAIL now");
    s.wait_until("the tag goes", |s| !s.list().contains("[done]"));
    s.settle();
    assert_eq!(verdict_calls(&s), 1);
}

#[test]
fn recent_workspace_sessions_are_checked_on_their_own_and_old_ones_are_not() {
    let mut s = work()
        .session(session("s-new", "recent prompt", 300))
        .session(session("s-old", "old prompt", 10 * 24 * 3600))
        .session(session("s-out", "loose prompt", 300))
        .workspaces(&[("Work", &["s-new", "s-old"])])
        .launch();
    s.wait_until("the recent one is tagged", |s| s.list().contains("s-new  [done]"));
    s.settle();
    let list = s.list();
    assert!(!list.contains("s-old  [done]"), "too old to check: {list}");
    assert_eq!(verdict_calls(&s), 1, "only the recent workspace session");
}

#[test]
fn a_session_that_needs_you_says_so_on_its_orchestration_card() {
    let setup = work().session(session("s-a", "alpha prompt", 300)).workspaces(&[("Work", &["s-a"])]);
    // Before launch: the first look-over runs on the first frame.
    setup.env().reply("verdict", r#"{"state": "waiting", "why": "Asks whether to stop the test instance"}"#);
    let mut s = setup.launch();
    s.wait_until("tagged", |s| s.list().contains("s-a  [needs you]"));
    s.env.reply("orchestration", &json!({"progress": 50, "summary": "S", "sessions": [{"n": 1, "state": "working", "now": "watching CI", "next": ""}], "attention": [], "next": []}).to_string());
    s.click_first("WORK");
    s.click("Orchestration");
    s.wait_to_see("watching CI");
    assert!(s.sees("waiting"), "the verdict's state, not the report's stale working");
    assert!(!s.sees("working"));
}
