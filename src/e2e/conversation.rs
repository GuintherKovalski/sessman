//! How a conversation reads: Markdown, links, stops, usage limits.
use super::harness::*;
use serde_json::json;

#[test]
fn replies_render_as_markdown_with_links() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100).reply("# Heading\n\nSee https://example.com/page and **bold** text.\n\n| a | b |\n|---|---|\n| 1 | 2 |"))
        .launch();
    s.click("alpha prompt");
    assert!(s.sees("Heading"));
    assert!(!s.sees("# Heading"));
    assert!(s.sees("https://example.com/page"), "bare URLs become links");
    assert!(s.sees("bold"));
    assert!(!s.sees("**bold**"));
    assert!(!s.sees("|---|"), "tables are drawn as tables");
}

#[test]
fn a_stop_saved_by_claude_shows_as_interrupted() {
    let s = setup();
    let dir = s.env().projects().join("-stop");
    std::fs::create_dir_all(&dir).unwrap();
    let lines = [
        json!({"type": "user", "cwd": "/tmp", "message": {"content": "write a story"}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "Once upon"}]}}),
        json!({"type": "user", "message": {"content": "[Request interrupted by user]"}}),
    ];
    std::fs::write(dir.join("s-stop.jsonl"), lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
    let mut s = s.launch();
    s.click("write a story");
    assert!(s.sees("⏹ Interrupted"));
    assert!(s.sees("Once upon"));
}

#[test]
fn usage_limits_show_once_the_status_line_is_enabled() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.click("Info");
    s.click("Enable");
    let claude_settings: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(s.env.home.join(".claude/settings.json")).unwrap()).unwrap();
    assert!(claude_settings["statusLine"]["command"].as_str().unwrap().ends_with("statusline"));
    s.wait_to_see("Enabled. Limits appear after your next reply");
    let status = s.env.config.join("status");
    std::fs::create_dir_all(&status).unwrap();
    let reset = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3 * 3600;
    std::fs::write(status.join("latest.json"), json!({"rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": reset}, "seven_day": {"used_percentage": 41.0}}}).to_string()).unwrap();
    std::fs::write(status.join("s-a.json"), json!({"context_window": {"context_window_size": 400_000}}).to_string()).unwrap();
    s.wait_to_see("Session limit");
    assert!(s.sees("24% used · 76% left · resets in 2h 59m") || s.sees("24% used · 76% left · resets in 3h 0m"));
    assert!(s.sees("Weekly"));
    assert!(s.sees("41% used · 59% left"));
    assert!(s.sees("50k / 400k tokens (12%)"), "the exact window from the status line");
    // Limits that have all reset since the status line saw them say so instead of vanishing.
    std::fs::write(status.join("latest.json"), json!({"rate_limits": {"seven_day": {"used_percentage": 11, "resets_at": 1000}}}).to_string()).unwrap();
    s.wait_to_see("Reset since they were read");
    assert!(s.sees("Not known yet"));
}

#[test]
fn someone_elses_status_line_is_left_alone() {
    let s = setup().session(session("s-a", "alpha prompt", 100));
    std::fs::create_dir_all(s.env().home.join(".claude")).unwrap();
    std::fs::write(s.env().home.join(".claude/settings.json"), r#"{"statusLine": {"type": "command", "command": "my-line"}}"#).unwrap();
    let mut s = s.launch();
    s.click("alpha prompt");
    s.click("Info");
    assert!(s.sees("yours is already set to `my-line`"));
    assert!(!s.sees("Enable"));
}

#[test]
fn the_mic_offers_to_download_the_speech_model_first() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.click("🎤");
    assert!(s.sees("Download the speech model?"));
    s.click("Not now");
    assert!(!s.sees("Download the speech model?"));
}

/// A session whose replies are each taller than the window: "line k of rN", 40 lines each.
fn long_turns() -> Scope {
    let reply = |n: usize| (1..=40).map(|k| format!("line {k} of r{n}")).collect::<Vec<_>>().join("\n\n");
    let mut f = session("s-long", "prompt one", 100).reply(&reply(1));
    for n in 2..=4 {
        f = f.turn(&format!("prompt {n}"), &reply(n));
    }
    let mut s = setup().session(f).launch();
    s.click("prompt one");
    s.settle();
    s
}

/// Whether `label` shows near the top of the conversation (below the tabs and the pinned prompt).
fn at_top(s: &Scope, label: &str) -> bool {
    s.sees(label) && (20.0..260.0).contains(&s.top_of(label))
}

#[test]
fn arrows_move_through_messages_once_the_conversation_is_clicked() {
    let mut s = long_turns();
    assert!(s.sees("line 40 of r4"), "it opens at the bottom");
    s.click("line 40 of r4");
    assert_eq!(s.prompt(), "", "the click didn't go to the input");
    s.press(eframe::egui::Key::ArrowUp);
    s.wait_until("prompt 4 is at the top", |s| at_top(s, "prompt 4"));
    s.press(eframe::egui::Key::ArrowUp);
    s.wait_until("r3 starts at the top", |s| at_top(s, "line 1 of r3"));
    s.press(eframe::egui::Key::ArrowUp);
    s.wait_until("prompt 3 is at the top", |s| at_top(s, "prompt 3"));
    s.press(eframe::egui::Key::ArrowDown);
    s.press(eframe::egui::Key::ArrowDown);
    s.wait_until("r4 starts at the top", |s| at_top(s, "line 1 of r4"));
}

#[test]
fn typing_with_the_conversation_clicked_goes_to_the_input() {
    let mut s = long_turns();
    s.click("line 40 of r4");
    s.type_here("h");
    s.type_here("i");
    assert_eq!(s.prompt(), "hi", "the first key moved the keyboard to the input, the next typed there");
}
