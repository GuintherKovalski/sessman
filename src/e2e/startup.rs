//! Starting the app: what it lists, and what it brings back after a restart.
use super::harness::*;
use eframe::egui::Key;
use serde_json::json;

#[test]
fn lists_every_session_newest_first_by_ai_title_or_first_prompt() {
    let s = setup()
        .session(session("s-old", "old prompt", 300).titled("Old one, titled by Claude"))
        .session(session("s-mid", "middle prompt\nwith a second line", 200))
        .session(session("s-new", "newest prompt", 100))
        .launch();
    assert!(s.sees("Old one, titled by Claude"));
    assert!(!s.sees("old prompt"), "the ai-title replaces the first prompt");
    assert!(s.sees("middle prompt"));
    assert!(!s.sees("with a second line"), "only the first line of a prompt names a session");
    assert!(s.shows_before("newest prompt", "middle prompt"));
    assert!(s.shows_before("middle prompt", "Old one, titled by Claude"));
}

#[test]
fn an_empty_home_says_there_are_no_sessions_or_workspaces() {
    let s = setup().launch();
    assert!(s.sees("No sessions found in ~/.claude/projects"));
    assert!(s.sees("No workspaces yet."));
}

#[test]
fn files_without_a_conversation_are_not_listed() {
    let s = setup().session(session("s-real", "a real prompt", 10)).launch();
    let dir = s.env.projects().join("-tmp-empty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("s-empty.jsonl"), "{\"type\":\"summary\"}\n").unwrap();
    let s = s.restart();
    assert!(s.sees("a real prompt"));
    assert!(!s.sees("s-empty"));
}

#[test]
fn sessions_started_elsewhere_show_up_by_themselves() {
    let mut s = setup().session(session("s-first", "first prompt", 100)).launch();
    session("s-later", "made in a terminal", 0).write_to(&s.env);
    s.wait_to_see("made in a terminal");
}

#[test]
fn a_restart_brings_back_the_tab_session_draft_and_right_panel() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &["s-a", "s-b"])])
        .launch();
    s.click_first("Work");
    s.click("alpha prompt");
    s.type_prompt("half-written thought");
    s.click("Info");
    let mut s = s.restart();
    s.frames(3);
    assert!(s.is_selected("Work"), "the workspace tab is open again");
    assert!(s.is_selected("alpha prompt"), "and so is the session");
    assert_eq!(s.prompt(), "half-written thought");
    assert!(s.is_selected("Info"));
    assert!(s.sees("claude-test-model"), "the Info tab shows");
}

#[test]
fn drafts_are_kept_per_session_across_a_restart() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .launch();
    s.click("alpha prompt");
    s.type_prompt("draft for alpha");
    s.click("beta prompt");
    assert_eq!(s.prompt(), "");
    s.type_prompt("draft for beta");
    let mut s = s.restart();
    assert_eq!(s.prompt(), "draft for beta");
    s.click("alpha prompt");
    assert_eq!(s.prompt(), "draft for alpha");
}

#[test]
fn a_broken_state_file_is_moved_aside_and_the_app_starts_fresh() {
    let s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    std::fs::write(s.env.config.join("state.json"), "{ not json").unwrap();
    let s = s.restart_without_saving();
    assert!(s.env.config.join("state.json.bad").exists());
    assert!(s.sees("alpha prompt"));
    assert!(s.sees("Workspaces") || s.sees("No workspaces yet."), "it opens on the overview");
}

#[test]
fn state_of_sessions_that_are_gone_is_dropped() {
    let s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .config("state.json", json!({"open": "s-gone", "views": {"s-gone": {"draft": "lost"}}, "queues": {"s-gone": {"items": ["x"]}}}))
        .launch();
    assert!(s.sees("alpha prompt"));
    assert!(!s.sees("lost"));
}

#[test]
fn with_restore_off_the_app_starts_on_the_overview() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &["s-a"])])
        .setting("restore_state", json!(false))
        .launch();
    s.click_first("Work");
    s.click("alpha prompt");
    let s = s.restart();
    assert!(s.is_selected("All sessions"));
    assert!(s.sees("Workspaces"));
}

#[test]
fn queued_prompts_come_back_paused_after_a_restart() {
    let s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .config("state.json", json!({"open": "s-a", "queues": {"s-a": {"items": ["queued before quitting"]}}}))
        .launch();
    assert!(s.sees("alpha prompt  +1 queued"));
    assert!(s.sees("queued 1 · queued before quitting"));
    assert!(s.sees("Queue paused: restored after a restart"));
    assert!(s.env.runs().is_empty(), "nothing is sent unasked");
}

#[test]
fn the_window_layout_settings_and_search_are_restored() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .launch();
    s.type_in("search", "alpha");
    s.press(Key::Tab);
    let s = s.restart();
    assert_eq!(s.field_text("search"), "alpha");
    assert!(s.sees("alpha prompt"));
    assert!(!s.sees("beta prompt"), "the list is filtered again");
}
