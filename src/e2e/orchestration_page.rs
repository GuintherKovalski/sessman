//! A workspace's orchestration page: the report, the session cards, questions.
use super::harness::*;
use eframe::egui::{Id, Key};

fn work() -> Scope {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .workspaces(&[("Work", &["s-a", "s-b"])])
        .launch();
    s.click_first("Work");
    s
}

fn on_page() -> Scope {
    let mut s = work();
    s.click("Orchestration");
    s.wait_to_see("Fake summary of ");
    s
}

#[test]
fn the_workspace_bubble_opens_the_page_which_writes_its_first_report() {
    let mut s = on_page();
    assert!(s.sees("42%"));
    assert!(s.sees("NEEDS YOU"));
    assert!(s.sees("Fake next step"));
    assert!(s.sees("waiting"), "the first session's state chip");
    assert!(s.sees("fake doing"));
    assert!(s.sees("Next: fake step"));
    // [[1]] reads as the newest session's name.
    assert!(s.sees("beta prompt"));
    let saved = s.env.config_json("orchestration.json");
    assert_eq!(saved["Work"]["report"]["progress"], 42);
    s.settle();
    assert!(s.sees("Orchestration\n2 sessions · 42% · 1 needs you"), "the bubble sums it up");
}

#[test]
fn the_tab_reopens_on_the_page() {
    let mut s = on_page();
    s.click("Scopes");
    s.click_first("Work");
    assert!(s.sees("Fake summary of "));
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["last"], "@orchestration");
}

#[test]
fn a_session_card_opens_its_session() {
    let mut s = on_page();
    s.click_last("alpha prompt");
    assert!(s.is_selected("alpha prompt"));
    assert!(s.sees("Reply to alpha prompt"));
}

#[test]
fn update_and_clear_ask_the_orchestrator_again() {
    let mut s = on_page();
    s.settle();
    let asked = s.env.calls().len();
    s.click("↻ Update");
    s.wait_until("asked again", |s| s.env.calls().len() > asked);
    s.settle();
    let asked = s.env.calls().len();
    s.click("Clear & re-summarize");
    s.wait_until("asked again", |s| s.env.calls().len() > asked);
    s.settle();
    assert!(s.sees("Updated"));
}

#[test]
fn questions_are_answered_on_the_page_and_kept() {
    let mut s = on_page();
    s.type_in_id(Id::new(("orchestration-question", "Work")), "how far along?");
    s.press(Key::Enter);
    s.wait_to_see("Fake answer about");
    assert!(s.sees("how far along?"));
    assert_eq!(s.env.config_json("orchestration.json")["Work"]["log"][0]["question"], "how far along?");
    s.click_last("×");
    assert!(!s.sees("how far along?"));
}

#[test]
fn the_orchestrator_can_act_when_asked() {
    let mut s = on_page();
    s.env.reply("ask", "Renaming it.\n```actions\n[{\"do\": \"rename\", \"session\": 2, \"name\": \"Renamed by the orchestrator\"}]\n```");
    s.type_in_id(Id::new(("orchestration-question", "Work")), "rename the old one");
    s.press(Key::Enter);
    s.wait_to_see("Renaming it.");
    assert!(s.sees("✓ Renamed \"alpha prompt\" to \"Renamed by the orchestrator\""));
    assert_eq!(s.env.config_json("names.json")["s-a"], "Renamed by the orchestrator");
}

#[test]
fn a_report_it_cannot_read_shows_why() {
    let mut s = work();
    s.env.reply("orchestration", "not a report");
    s.click("Orchestration");
    s.wait_to_see("Could not update: unexpected reply: not a report");
}

#[test]
fn an_empty_workspace_page_says_how_to_fill_it() {
    let mut s = setup().workspaces(&[("Empty", &[])]).launch();
    s.click_first("Empty");
    assert!(s.sees("No sessions in this scope yet."));
}
