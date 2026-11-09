//! The Settings page and the settings it keeps.
use super::harness::*;
use eframe::egui::{Key, Modifiers};
use serde_json::json;

fn opened() -> Scope {
    opened_with(setup())
}

fn opened_with(setup: Setup) -> Scope {
    let mut s = setup.session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.press_with(Modifiers::COMMAND, Key::Comma);
    s
}

#[test]
fn cmd_comma_opens_settings_as_a_main_area_tab() {
    let s = opened();
    assert!(s.sees("Settings"));
    assert!(s.sees("⚙ Settings"), "it has a tab");
    assert!(s.sees(&format!("Saved to {}", s.env.config.join("settings.json").display())));
    assert!(s.sees("APPEARANCE"), "groups are headed by their title, like scopes");
    assert!(s.sees("AI HELPERS AND HARNESSES"));
    assert!(s.sees("KEYBOARD SHORTCUTS"));
}

#[test]
fn the_gear_in_the_title_bar_opens_settings_too() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("⚙");
    assert!(s.sees("⚙ Settings"));
}

#[test]
fn a_change_applies_and_saves_at_once_and_resets() {
    let mut s = opened();
    s.click("☀ Light");
    assert_eq!(s.env.config_json("settings.json")["theme"], "light");
    s.click_first("↺"); // Theme is the first setting; the claude command (the fake one) differs too
    assert_eq!(s.env.config_json("settings.json")["theme"], "system", "↺ puts the default back");
}

#[test]
fn the_model_for_prompts_is_chosen_and_goes_to_the_next_prompt() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "model for prompts");
    s.click("Fable");
    assert_eq!(s.env.config_json("settings.json")["model"], "fable");
    s.click("Opus");
    assert_eq!(s.env.config_json("settings.json")["model"], "opus");
    s.click("Main");
    s.send("go");
    s.wait_to_see("Echo: go");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--resume")).unwrap();
    let args: Vec<&str> = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(args.windows(2).any(|w| w == ["--model", "opus"]));
}

#[test]
fn the_search_field_filters_the_settings() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "jira");
    assert!(s.sees("Jira URL"));
    assert!(!s.sees("Monospace font size"));
}

#[test]
fn a_search_with_no_match_says_so() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "nothing like this");
    assert!(s.sees("No setting matches."));
    assert!(!s.sees("ABOUT"));
}

#[test]
fn main_goes_back_to_the_conversation_and_the_tab_closes() {
    let mut s = opened();
    s.click("Main");
    assert!(s.sees("Reply to alpha prompt"));
    assert!(!s.sees("APPEARANCE"));
    s.click("⚙ Settings");
    assert!(s.sees("APPEARANCE"));
    s.middle_click("⚙ Settings");
    assert!(!s.sees("⚙ Settings"));
    assert!(s.sees("Reply to alpha prompt"));
}

#[test]
fn zoom_from_cmd_plus_is_kept() {
    let mut s = opened();
    s.press_with(Modifiers::COMMAND, Key::Plus);
    s.frames(3);
    let zoom = s.env.config_json("settings.json")["zoom"].as_f64().unwrap();
    assert!(zoom > 1.0, "zoom {zoom}");
}

#[test]
fn tool_lines_show_or_hide_by_setting() {
    let proj = std::path::PathBuf::from("/tmp/x.rs");
    let mut s = setup().session(session("s-a", "alpha prompt", 100).edit(&proj, "a", "b")).launch();
    s.click("alpha prompt");
    assert!(s.sees("⚙ Edit"));
    drop(s);
    let mut s = setup().session(session("s-a", "alpha prompt", 100).edit(&proj, "a", "b")).setting("show_tool_lines", json!(false)).launch();
    s.click("alpha prompt");
    assert!(!s.sees("⚙ Edit"));
}

#[test]
fn a_group_is_found_by_its_title_and_hides_when_nothing_in_it_matches() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "conversation and files");
    assert!(s.sees("CONVERSATION AND FILES"));
    assert!(s.sees("Tab width"), "every row of the group, by the group's title");
    assert!(!s.sees("APPEARANCE"), "groups with no match are not drawn");
}

#[test]
fn each_harness_says_where_it_runs_who_answers_and_shows_its_whole_prompt() {
    let mut s = opened_with(setup().setting("helper_model", json!("sonnet")));
    s.type_in_hint("🔍 Search settings", "search bar");
    assert!(s.sees("🔍 Search bar"));
    assert!(s.sees("Bottom of the session list: press Enter in the search field"));
    assert!(s.sees("sonnet (the helper model)"), "the model that answers, as set");
    assert!(!s.sees("◆ Orchestrator"));
    assert!(!s.sees("Looking for: <what you typed>"), "the prompt is folded away");
    s.click("Full prompt");
    assert!(s.sees("Looking for: <what you typed>"));
    assert!(s.sees(crate::search::DEFAULT_HARNESS));
}

#[test]
fn an_edited_harness_shows_in_its_prompt_and_resets() {
    let mut s = opened_with(setup().setting("question_harness", json!("Answer in one line.")));
    s.type_in_hint("🔍 Search settings", "ask about this goal");
    assert!(s.sees("💬 Ask about this goal"));
    assert!(s.sees("edited"));
    s.click("Full prompt");
    assert!(s.sees("Answer in one line.\nThe session's goal and subgoal checklist:"));
    s.click("↺");
    assert_eq!(s.env.config_json("settings.json")["question_harness"], crate::goal::QUESTION_HARNESS);
    assert!(!s.sees("edited"));
}

#[test]
fn the_orchestrator_card_holds_its_model_and_both_prompts() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "orchestrator");
    assert!(s.sees("Orchestration model"));
    assert!(s.sees("Update the report every (minutes)"));
    s.click("Full prompt: a question");
    assert!(s.sees("QUESTION: <your question>"));
    s.click("Full prompt: a report update");
    assert!(s.sees("Your PREVIOUS REPORT is below"));
}

#[test]
fn the_built_in_helpers_show_their_fixed_prompts() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "built-in");
    assert!(s.sees("Names a new session"));
    assert!(s.sees("Every session you run"));
    s.click("Tags a finished session");
    assert!(s.sees("Has this Claude Code session finished its work?"));
}

#[test]
fn zoom_has_its_own_setting() {
    let mut s = opened();
    s.type_in_hint("🔍 Search settings", "zoom");
    s.click_last("+");
    let zoom = s.env.config_json("settings.json")["zoom"].as_f64().unwrap();
    assert!((zoom - 1.1).abs() < 0.01, "zoom {zoom}");
    assert!(s.sees("110%"));
}
