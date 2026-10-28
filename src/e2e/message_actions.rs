//! Copy and Delete on each message: deleting takes a message out of the session's context.
use super::harness::*;
use serde_json::{Value, json};

fn two_turns() -> Setup {
    setup().session(session("s-a", "alpha prompt", 100).reply("alpha reply").turn("beta prompt", "beta reply"))
}

/// The texts of the session file's user and assistant entries, in order.
fn entries(s: &Scope) -> Vec<String> {
    let text = std::fs::read_to_string(s.env.session_file("s-a").unwrap()).unwrap();
    text.lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["uuid"].is_string())
        .map(|v| match &v["message"]["content"] {
            Value::String(t) => t.clone(),
            c => c[0]["text"].as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

#[test]
fn delete_asks_first_then_takes_the_message_out_of_the_session_file() {
    let mut s = two_turns().launch();
    s.click("alpha prompt");
    assert!(s.all_enabled("📋") && s.all_enabled("🗑"), "every message has both");
    s.click_last("🗑");
    assert!(s.sees("Delete this message from the context?"));
    assert!(s.sees("beta reply"), "the modal shows which message");
    s.click("Cancel");
    assert_eq!(entries(&s), ["alpha prompt", "alpha reply", "beta prompt", "beta reply"], "cancel changes nothing");
    s.click_last("🗑");
    s.click("Delete");
    assert!(!s.sees("beta reply"));
    assert!(s.sees("beta prompt"));
    assert_eq!(entries(&s), ["alpha prompt", "alpha reply", "beta prompt"]);
    let backups: Vec<_> = std::fs::read_dir(s.env.config.join("backups")).unwrap().flatten().collect();
    assert_eq!(backups.len(), 1, "the file as it was is kept");
    assert!(std::fs::read_to_string(backups[0].path()).unwrap().contains("beta reply"));
}

#[test]
fn deleting_the_first_prompt_relinks_the_rest() {
    let mut s = two_turns().setting("confirm_delete_message", json!(false)).launch();
    s.click("alpha prompt");
    s.click_first("🗑");
    assert!(!s.sees("Delete this message from the context?"), "the setting turns the question off");
    assert_eq!(entries(&s), ["alpha reply", "beta prompt", "beta reply"]);
    let text = std::fs::read_to_string(s.env.session_file("s-a").unwrap()).unwrap();
    let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(first["parentUuid"], Value::Null, "the reply now starts the chain");
}

#[test]
fn delete_waits_while_claude_works_in_the_session() {
    let mut s = two_turns().launch();
    s.click("alpha prompt");
    s.env.hold("hold");
    s.send("HOLD busy");
    s.wait_until("the run has started", |s| !s.all_enabled("🗑"));
    assert!(s.all_enabled("📋"), "copy still works");
    s.env.release("hold");
    s.wait_until("the run has ended", |s| s.all_enabled("🗑"));
}
