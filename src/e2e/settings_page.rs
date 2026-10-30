//! The Settings page and the settings it keeps.
use super::harness::*;
use eframe::egui::{Key, Modifiers};
use serde_json::json;

fn opened() -> Scope {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
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
    assert!(s.sees("Appearance"));
    assert!(s.sees("Keyboard shortcuts"));
}

#[test]
fn the_gear_button_opens_settings_too() {
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
fn main_goes_back_to_the_conversation_and_the_tab_closes() {
    let mut s = opened();
    s.click("Main");
    assert!(s.sees("Reply to alpha prompt"));
    assert!(!s.sees("Appearance"));
    s.click("⚙ Settings");
    assert!(s.sees("Appearance"));
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
