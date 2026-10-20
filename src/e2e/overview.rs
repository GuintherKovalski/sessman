//! The workspaces overview in All sessions.
use super::harness::*;
use serde_json::json;

fn two_workspaces() -> Sessman {
    setup()
        .session(session("s-a", "alpha prompt about ABC-123", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "loose prompt", 100))
        .workspaces(&[("Work", &["s-a"]), ("Play", &["s-b"]), ("Idle", &[])])
        .launch()
}

#[test]
fn each_workspace_gets_a_card_with_its_summary() {
    let mut s = two_workspaces();
    assert!(s.sees("3 workspaces · 2 sessions"));
    s.wait_to_see("Fake purpose");
    assert!(s.sees("Fake now"));
    assert!(s.sees("Fake heading"));
    assert!(s.sees("No sessions yet. Drag some onto its tab."), "an empty workspace's card");
    assert!(s.sees("1 session not in a workspace."));
    s.wait_until("both are summarized", |s| s.env.config_json("overview.json")["Play"]["purpose"] == "Fake purpose" && s.env.config_json("overview.json")["Work"]["purpose"] == "Fake purpose");
    assert!(s.sees("Summarized"));
}

#[test]
fn summaries_are_kept_and_not_asked_again_without_news() {
    let s = two_workspaces();
    let mut s = s;
    s.wait_until("summarized", |s| s.env.config_json("overview.json")["Play"].is_object() && s.env.config_json("overview.json")["Work"].is_object());
    let asked = s.env.calls().len();
    let mut s = s.restart();
    s.frames(20);
    assert!(s.sees("Fake purpose"));
    assert_eq!(s.env.calls().len(), asked, "no new questions on a restart");
}

#[test]
fn refresh_asks_again() {
    let mut s = two_workspaces();
    s.wait_until("summarized", |s| s.env.config_json("overview.json")["Play"].is_object() && s.env.config_json("overview.json")["Work"].is_object());
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
    s.env().reply("overview", r#"{"purpose": "P", "now": "N", "heading": "H", "tickets": [{"key": "ABC-123", "note": "the epic"}, {"key": "ZZZ-9", "note": "made up"}]}"#);
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
fn clicking_a_card_name_opens_its_workspace() {
    let mut s = two_workspaces();
    s.click_last("Play");
    assert!(s.is_selected("Play"));
    assert!(s.sees("beta prompt"));
    assert!(!s.sees("alpha prompt about ABC-123"));
}
