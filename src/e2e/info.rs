//! The Info tab's goal: summarizing it, ticking its checklist, asking about it.
use super::harness::*;
use eframe::egui::Key;
use serde_json::json;

fn info_of_one() -> Scope {
    let mut s = setup().session(session("s-a", "alpha prompt", 100).reply("alpha reply")).launch();
    s.click("alpha prompt");
    s.click("Info");
    s
}

#[test]
fn summarize_goal_writes_the_goal_and_its_checklist() {
    let mut s = info_of_one();
    s.click("Summarize goal");
    s.wait_to_see("Ship the fake feature");
    assert!(s.sees("1 of 2 done"));
    assert!(s.is_selected("Write the parser"), "ticked where the conversation shows it done");
    assert!(!s.is_selected("Add the tests"));
    assert!(s.sees("Summarize again"));
    let goals = s.env.config_json("goals.json");
    assert_eq!(goals["s-a"], "GOAL: Ship the fake feature\n- [x] Write the parser\n- [ ] Add the tests");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("overall goal"))).unwrap();
    let request = call["args"].as_array().unwrap().last().unwrap().as_str().unwrap().to_owned();
    assert!(request.contains("USER: alpha prompt\nCLAUDE: alpha reply"), "it reads a digest of the conversation");
}

#[test]
fn ticking_a_subgoal_saves_it() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .config("goals.json", json!({"s-a": "GOAL: Do it\n- [ ] first\n- [ ] second"}))
        .launch();
    s.click("alpha prompt");
    s.click("Info");
    assert!(s.sees("0 of 2 done"));
    s.click("second");
    assert!(s.sees("1 of 2 done"));
    assert_eq!(s.env.config_json("goals.json")["s-a"], "GOAL: Do it\n- [ ] first\n- [x] second");
}

#[test]
fn a_question_about_the_goal_is_answered_without_touching_the_session() {
    let mut s = info_of_one();
    s.type_in("goal-question-input", "what is left?");
    s.press(Key::Enter);
    s.wait_to_see("Fake side answer");
    assert!(s.sees("what is left?"));
    assert_eq!(s.field_text("goal-question-input"), "");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--fork-session")).unwrap();
    let args: Vec<&str> = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(args.windows(2).any(|w| w == ["--resume", "s-a"]));
    assert!(args.contains(&"--no-session-persistence"));
    assert!(args.last().unwrap().ends_with("Question: what is left?"));
    assert!(s.env.runs().is_empty());
    s.click("×");
    assert!(!s.sees("Fake side answer"));
}

#[test]
fn a_session_without_a_goal_can_ask_for_one() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.click("Info");
    assert!(s.sees("No goal summarized yet"));
    assert!(!s.sees("First prompt: alpha prompt…"), "the first prompt isn't shown as a goal");
    assert!(s.sees("Summarize goal"));
}
