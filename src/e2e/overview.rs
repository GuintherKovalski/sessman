//! The workspaces overview in All sessions: each workspace's scope, then all of its sessions.
use super::harness::*;
use serde_json::json;

fn two_workspaces() -> Scope {
    setup()
        .session(session("s-a", "alpha prompt about ABC-123", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "loose prompt", 100))
        .workspaces(&[("Work", &["s-a"]), ("Play", &["s-b"]), ("Idle", &[])])
        .launch()
}

fn summarized(s: &Scope, names: &[&str]) -> bool {
    let o = s.env.config_json("overview.json");
    names.iter().all(|n| o[n]["scope"] == "Fake scope")
}

#[test]
fn each_workspace_shows_its_scope_and_what_each_session_does_and_needs() {
    let mut s = two_workspaces();
    assert!(s.sees("3 scopes · 2 sessions"));
    s.wait_to_see("Fake scope");
    s.wait_to_see("Fake doing");
    assert!(s.sees("⚠ Fake decision"), "what it needs from the developer");
    assert!(s.sees("last interaction 5 min ago"));
    assert!(s.sees("No sessions yet. Drag some onto its tab."), "an empty workspace's card");
    assert!(s.sees("1 session not in a scope."));
    assert!(!s.sees("Finish session"), "nothing is old enough");
    s.wait_until("both are summarized", |s| summarized(s, &["Work", "Play"]));
    let o = s.env.config_json("overview.json");
    assert_eq!(o["Work"]["sessions"]["s-a"]["needs"], "Fake decision", "kept by session: {o}");
    assert!(s.sees("Summarized"));
}

#[test]
fn every_session_is_listed_newest_first() {
    let mut s = setup()
        .session(session("s-old", "old prompt", 900))
        .session(session("s-new", "new prompt", 100))
        .session(session("s-mid", "mid prompt", 500))
        .workspaces(&[("Work", &["s-old", "s-new", "s-mid"])])
        .launch();
    s.frames(10);
    assert!(s.sees("3 sessions"));
    assert!(s.shows_before("last interaction 1 min ago", "last interaction 8 min ago"));
    assert!(s.shows_before("last interaction 8 min ago", "last interaction 15 min ago"));
}

#[test]
fn summaries_are_kept_and_not_asked_again_without_news() {
    let mut s = two_workspaces();
    s.wait_until("summarized", |s| summarized(s, &["Work", "Play"]));
    let asked = s.env.calls().len();
    let mut s = s.restart();
    s.frames(20);
    assert!(s.sees("Fake scope"));
    assert!(s.sees("Fake doing"));
    assert_eq!(s.env.calls().len(), asked, "no new questions on a restart");
}

#[test]
fn refresh_asks_again() {
    let mut s = two_workspaces();
    s.wait_until("summarized", |s| summarized(s, &["Work", "Play"]));
    s.settle();
    let asked = s.env.calls().len();
    s.click("↻ Refresh all");
    s.wait_until("both asked again", |s| s.env.calls().len() >= asked + 2);
}

#[test]
fn jira_keys_seen_in_the_sessions_become_tickets() {
    let s = setup()
        .session(session("s-a", "work on ABC-123 today", 300))
        .workspaces(&[("Work", &["s-a"])])
        .setting("jira_url", json!("https://acme.atlassian.net"));
    s.env().reply("overview", r#"{"scope": "P", "sessions": [], "tickets": [{"key": "ABC-123", "note": "the epic"}, {"key": "ZZZ-9", "note": "made up"}]}"#);
    let mut s = s.launch();
    s.wait_to_see("the epic");
    assert!(!s.sees("ZZZ-9"), "keys not in the sessions are dropped");
    assert!(!s.sees("made up"));
    let call = s.env.calls().into_iter().find(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("overview card"))).unwrap();
    assert!(call["args"].as_array().unwrap().last().unwrap().as_str().unwrap().contains("Jira keys mentioned: ABC-123."));
}

#[test]
fn a_failed_summary_shows_why() {
    let s = setup().session(session("s-a", "alpha", 100)).workspaces(&[("Work", &["s-a"])]);
    s.env().reply("overview", "no json here");
    let mut s = s.launch();
    s.wait_to_see("Could not summarize: unexpected reply: no json here");
}

#[test]
fn the_workspace_with_the_latest_idle_session_comes_first() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 900))
        .session(session("s-b", "beta prompt", 100))
        .workspaces(&[("Work", &["s-a"]), ("Play", &["s-b"])])
        .launch();
    s.frames(10);
    assert!(s.shows_before("beta prompt", "alpha prompt"));
    assert!(s.shows_before("last interaction 1 min ago", "last interaction 15 min ago"), "Play's card, though its tab is second");
}

#[test]
fn clicking_a_card_name_opens_its_workspace() {
    let mut s = two_workspaces();
    s.click_last("Play");
    assert!(s.is_selected("Play"));
    assert!(s.sees("beta prompt"));
    assert!(!s.sees("alpha prompt about ABC-123"));
}

#[test]
fn clicking_a_session_opens_it_in_its_workspace() {
    let mut s = two_workspaces();
    s.wait_to_see("Fake doing");
    s.click("last interaction 3 min ago");
    assert!(s.is_selected("Play"), "its workspace's tab");
    assert!(s.sees("Reply to beta prompt"), "the session itself");
    assert!(!s.sees("alpha prompt about ABC-123"));
}

#[test]
fn a_session_idle_too_long_is_red_and_can_be_finished() {
    let mut s = setup()
        .session(session("s-old", "old prompt", 30 * 3600))
        .session(session("s-new", "new prompt", 100))
        .workspaces(&[("Work", &["s-old", "s-new"])])
        .launch();
    s.frames(10);
    assert!(s.sees("last interaction 1 d ago"));
    s.click("Finish session");
    s.wait_until("it left the workspace", |s| s.env.config_json("workspaces.json").to_string().contains("s-new") && !s.env.config_json("workspaces.json").to_string().contains("s-old"));
    assert!(s.sees("1 session not in a scope."), "the session itself is kept");
    assert!(!s.sees("Finish session"));
    assert!(!s.is_selected("Work"), "finishing doesn't open anything");
}

#[test]
fn the_idle_hours_come_from_settings() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 3 * 3600))
        .workspaces(&[("Work", &["s-a"])])
        .setting("stale_after_hours", json!(2))
        .launch();
    s.frames(10);
    assert!(s.sees("Finish session"));
}

/// Runs are found by their `--resume <id>`, which has to look like a real session id.
const ID: &str = "7a1d2c3e-8a43-4f7e-9a59-0d2b8c1f7a10";

#[test]
fn a_session_whose_run_finished_unseen_is_highlighted_until_opened() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session(ID, "long task", 600))
        .workspaces(&[("Work", &["s-a", ID])])
        .launch();
    s.frames(10);
    assert!(!s.sees("✓ Finished"), "nothing ran since scope started");
    s.env.hold("hold");
    s.env.run_outside(ID, "HOLD keep going");
    s.wait_to_see("working");
    assert!(!s.sees("✓ Finished"), "not while it works");
    s.env.release("hold");
    s.wait_to_see("✓ Finished");
    assert!(s.sees("1 finished"));
    assert!(s.shows_before("long task", "alpha prompt"), "it moved up as the newest, nothing else");
    s.click("long task");
    s.wait_to_see("Echo: HOLD keep going");
    s.click("Scopes");
    s.frames(10);
    assert!(!s.sees("✓ Finished"), "seen now");
    let mut s = s.restart();
    s.frames(10);
    assert!(!s.sees("✓ Finished"), "and still seen after a restart");
}
