//! ✨ Refine: the helper model rewrites the prompt, maybe after asking questions.
use super::harness::*;
use eframe::egui::{Key, Modifiers};

fn typed(text: &str) -> Scope {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.type_prompt(text);
    s
}

#[test]
fn cmd_enter_replaces_the_input_with_the_refined_prompt() {
    let mut s = typed("make it faster pls");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_until("the refined prompt is in the input", |s| s.prompt() == "Refined: make it faster pls");
    assert!(s.sees("✨ Refined"));
    assert!(s.env.runs().is_empty(), "a refined prompt is never sent by itself");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("You refine prompts"))).unwrap();
    let request = call["args"].as_array().unwrap().last().unwrap().as_str().unwrap().to_owned();
    assert!(request.contains("PROMPT:\nmake it faster pls"));
    assert!(request.contains("- alpha prompt"), "the session's recent prompts are context");
}

#[test]
fn the_refine_button_does_the_same() {
    let mut s = typed("tidy this");
    s.click("✨");
    s.wait_until("refined", |s| s.prompt() == "Refined: tidy this");
}

#[test]
fn undo_refine_brings_the_original_back() {
    let mut s = typed("original words");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_until("refined", |s| s.prompt().starts_with("Refined"));
    s.click("Undo refine");
    assert_eq!(s.prompt(), "original words");
    assert!(!s.sees("✨ Refined"));
}

#[test]
fn cmd_z_right_after_a_refine_undoes_it() {
    let mut s = typed("original words");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_until("refined", |s| s.prompt().starts_with("Refined"));
    s.press_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(s.prompt(), "original words");
}

#[test]
fn questions_come_first_when_the_prompt_is_ambiguous() {
    let mut s = typed("fix it");
    s.env.reply("refine", r#"{"questions": [{"question": "Which part is broken?", "options": ["The login", "The search"]}]}"#);
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_to_see("A few questions first. Pick an answer, write your own, or skip.");
    assert!(s.sees("Which part is broken?"));
    s.click("The search");
    std::fs::remove_file(s.env.fake.join("refine")).unwrap();
    s.click("✨ Refine with these answers");
    s.wait_until("refined", |s| s.prompt() == "Refined: fix it");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("You refine prompts"))).unwrap();
    let request = call["args"].as_array().unwrap().last().unwrap().as_str().unwrap().to_owned();
    assert!(request.contains("- Which part is broken? → The search"));
    assert!(request.contains("Do not ask questions."));
}

#[test]
fn skip_all_refines_with_what_is_known() {
    let mut s = typed("fix it");
    s.env.reply("refine", r#"{"questions": [{"question": "Which part?", "options": ["A", "B"]}]}"#);
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_to_see("Which part?");
    std::fs::remove_file(s.env.fake.join("refine")).unwrap();
    s.click("Skip all");
    s.wait_until("refined", |s| s.prompt() == "Refined: fix it");
}

#[test]
fn escape_cancels_a_refine_in_progress() {
    let mut s = typed("slow refine");
    s.env.hold("refine.hold");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_to_see("Refining the prompt… (Esc to cancel)");
    s.press(Key::Escape);
    assert!(!s.sees("Refining the prompt"));
    s.env.release("refine.hold");
    s.frames(30);
    assert_eq!(s.prompt(), "slow refine", "the late answer is dropped");
}

#[test]
fn a_reply_it_cannot_read_shows_an_error() {
    let mut s = typed("whatever");
    s.env.reply("refine", "I would rather not");
    s.press_with(Modifiers::COMMAND, Key::Enter);
    s.wait_to_see("Refine failed: unexpected reply: I would rather not");
    s.click("Dismiss");
    assert!(!s.sees("Refine failed"));
    assert_eq!(s.prompt(), "whatever");
}


#[test]
fn cmd_r_refines_even_with_the_input_unfocused() {
    let mut s = typed("tidy this");
    s.press(Key::Escape);
    s.press_with(Modifiers::COMMAND, Key::R);
    s.wait_until("refined", |s| s.prompt() == "Refined: tidy this");
}

#[test]
fn cmd_r_with_an_empty_input_does_nothing() {
    let mut s = typed("");
    s.press_with(Modifiers::COMMAND, Key::R);
    s.frames(5);
    assert!(s.env.calls().iter().all(|c| !c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("You refine prompts"))));
}

#[test]
fn hovering_the_refine_button_explains_it_and_shows_its_shortcut() {
    let mut s = typed("");
    s.hover("✨");
    assert!(s.sees("Refine: a helper model rewrites the prompt clearly, asking questions if it is ambiguous"));
    assert!(s.sees("Type a prompt first"), "disabled, it says why");
    let c = crate::platform::CMD;
    assert!(s.sees(&format!("{c}+R, or {c}+Enter in the input")));
}
