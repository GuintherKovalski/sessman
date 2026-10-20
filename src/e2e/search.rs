//! Search at the bottom of the session list: live matches, and Enter to ask Haiku.
use super::harness::*;
use eframe::egui::Key;
use serde_json::json;

fn three() -> Scope {
    setup()
        .session(session("s-a", "Markdown rendering and theming", 300))
        .session(session("s-b", "Fix the login flow", 200).reply("We talked about the quantum flux capacitor here"))
        .session(session("s-c", "Unrelated chores", 100))
        .config("goals.json", json!({"s-c": "GOAL: Speed up the indexer\n- [ ] profile it"}))
        .launch()
}

#[test]
fn typing_filters_by_name_first() {
    let mut s = three();
    s.type_in("search", "markdown");
    assert!(s.sees("Markdown rendering and theming"));
    assert!(!s.sees("Fix the login flow"));
    assert!(!s.sees("Unrelated chores"));
}

#[test]
fn close_spellings_still_match_names() {
    let mut s = three();
    s.type_in("search", "rendring");
    assert!(s.sees("Markdown rendering and theming"));
}

#[test]
fn goals_and_conversation_text_match_too_with_a_note() {
    let mut s = three();
    s.type_in("search", "indexer");
    s.wait_to_see("Unrelated chores\nmatched in its goal");
    s.replace_in("search", "capacitor");
    s.wait_to_see("Fix the login flow\n");
    assert!(s.sees("flux capacitor"), "a snippet of the conversation shows under the name");
    assert!(!s.sees("Unrelated chores"));
}

#[test]
fn no_match_suggests_asking_haiku() {
    let mut s = three();
    s.type_in("search", "zzzzqqq");
    assert!(s.sees("No matches. Press Enter to ask Haiku."));
}

#[test]
fn enter_asks_haiku_and_lists_its_picks_with_reasons() {
    let mut s = three();
    s.env.reply("search", "2: about logging in\n1: about markdown");
    s.type_in("search", "the auth work");
    s.press(Key::Enter);
    s.wait_to_see("Haiku's picks, best first. Edit the query for live results.");
    // The catalog is newest first: 1 = Unrelated chores, 2 = Fix the login flow.
    assert!(s.sees("Fix the login flow\nabout logging in"));
    assert!(s.sees("Unrelated chores\nabout markdown"));
    assert!(s.shows_before("Fix the login flow", "Unrelated chores"));
    assert!(!s.sees("Markdown rendering and theming"));
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("Looking for: the auth work"))).unwrap();
    assert!(call["args"].as_array().unwrap().windows(2).any(|w| w[0] == "--model" && w[1] == "haiku"));
}

#[test]
fn haiku_finding_nothing_says_so() {
    let mut s = three();
    s.env.reply("search", "NONE");
    s.type_in("search", "nothing like it");
    s.press(Key::Enter);
    s.wait_to_see("Haiku found no matching session. Edit the query for live results.");
}

#[test]
fn escape_clears_the_search() {
    let mut s = three();
    s.type_in("search", "markdown");
    s.press(Key::Escape);
    assert_eq!(s.field_text("search"), "");
    assert!(s.sees("Fix the login flow"));
}

#[test]
fn search_stays_within_the_open_workspace() {
    let mut s = setup()
        .session(session("s-a", "alpha markdown", 200))
        .session(session("s-b", "beta markdown", 100))
        .workspaces(&[("Work", &["s-a"])])
        .launch();
    s.click_first("Work");
    s.type_in("search", "markdown");
    assert!(s.sees("alpha markdown"));
    assert!(!s.sees("beta markdown"));
}
