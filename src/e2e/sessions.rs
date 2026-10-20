//! The session list: opening, renaming, picking several, moving to the Trash.
use super::harness::*;
use eframe::egui::{Key, Modifiers};
use serde_json::json;

#[test]
fn opening_a_session_shows_its_conversation_and_focuses_the_input() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100).reply("Alpha **reply** text").turn("second ask", "second answer"))
        .session(session("s-b", "beta prompt", 50))
        .launch();
    assert!(!s.sees("second answer"));
    s.click("alpha prompt");
    assert!(s.is_selected("alpha prompt"));
    assert!(s.sees("second ask"));
    assert!(s.sees("second answer"));
    assert!(s.sees("You"));
    assert!(s.sees("Claude"));
    s.type_prompt("typed straight away");
    assert_eq!(s.prompt(), "typed straight away", "the input has the focus once a session opens");
}

#[test]
fn clicking_anywhere_in_the_main_area_focuses_the_input() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100).reply("A reply to click on")).launch();
    s.click("alpha prompt");
    s.type_in("search", "x");
    s.press(Key::Escape);
    s.click("A reply to click on");
    s.type_prompt("y");
    assert_eq!(s.prompt(), "y");
}

#[test]
fn double_click_renames_a_session_and_enter_saves_it() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.double_click("alpha prompt");
    assert_eq!(s.field_text("rename"), "alpha prompt");
    s.type_in("rename", "My own name");
    s.press(Key::Enter);
    assert!(s.sees("My own name"));
    assert!(!s.has_field("rename"));
    assert_eq!(s.env.config_json("names.json")["s-a"], "My own name");
    let s = s.restart();
    assert!(s.sees("My own name"), "names are kept");
}

#[test]
fn escape_cancels_a_rename() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.double_click("alpha prompt");
    s.type_in("rename", "Not this");
    s.press(Key::Escape);
    assert!(!s.has_field("rename"));
    assert!(s.sees("alpha prompt"));
    assert!(!s.sees("Not this"));
}

#[test]
fn an_empty_name_brings_back_the_original_one() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .config("names.json", json!({"s-a": "Custom"}))
        .launch();
    assert!(s.sees("Custom"));
    s.double_click("Custom");
    s.press(Key::Backspace);
    s.press(Key::Enter);
    assert!(s.sees("alpha prompt"));
    assert!(!s.sees("Custom"));
}

#[test]
fn cmd_click_picks_several_sessions_and_escape_drops_the_picks() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .launch();
    s.click("gamma prompt");
    s.click_with("alpha prompt", Modifiers::COMMAND);
    assert!(s.is_selected("alpha prompt"));
    assert!(s.is_selected("gamma prompt"), "the open session joins the picks");
    assert!(!s.is_selected("beta prompt"));
    s.right_click("alpha prompt");
    assert!(s.sees("2 sessions"));
    assert!(s.sees("Move 2 sessions to Trash…"));
    s.press(Key::Escape);
    s.press(Key::Escape);
    assert!(!s.is_selected("alpha prompt"));
}

#[test]
fn shift_click_picks_a_range() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .launch();
    s.click("gamma prompt");
    s.click_with("alpha prompt", Modifiers::SHIFT);
    for t in ["alpha prompt", "beta prompt", "gamma prompt"] {
        assert!(s.is_selected(t), "{t} is in the range");
    }
}

#[test]
fn move_to_trash_asks_first_and_cancel_keeps_the_session() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.right_click("alpha prompt");
    s.click("Move to Trash…");
    assert!(s.sees("Move \"alpha prompt\" to the Trash?"));
    s.click("Cancel");
    assert!(!s.sees("to the Trash?"));
    assert!(s.sees("alpha prompt"));
    assert!(s.env.session_file("s-a").is_some());
}

#[test]
fn the_info_tab_shows_goal_model_and_context() {
    let mut s = setup().session(session("s-a", "alpha prompt to summarize", 100).model("claude-sonnet-4-5-20250929", 150_000)).launch();
    s.click("alpha prompt to summarize");
    s.click("Info");
    assert!(s.sees("First prompt: alpha prompt to summarize…"));
    assert!(s.sees("claude-sonnet-4-5-20250929"));
    assert!(s.sees("150k / 200k tokens (75%)"), "older models have a 200k window");
    assert!(s.sees("Usage limits"));
    assert!(s.sees("Enable"), "the status line isn't set up in a fresh home");
}

#[test]
fn context_window_is_one_million_for_current_models() {
    let mut s = setup().session(session("s-a", "alpha", 100).model("claude-opus-5-5", 250_000)).launch();
    s.click("alpha");
    s.click("Info");
    assert!(s.sees("250k / 1.0M tokens (25%)"));
}

#[test]
fn info_without_a_session_asks_to_select_one() {
    let mut s = setup().session(session("s-a", "alpha", 100)).launch();
    s.click("Info");
    assert!(s.sees("Select a session to see its info"));
}

