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
    s.wait_to_see("Session (5-hour)");
    assert!(s.sees("24% used · 76% left · resets in 2h 59m") || s.sees("24% used · 76% left · resets in 3h 0m"));
    assert!(s.sees("Weekly"));
    assert!(s.sees("41% used · 59% left"));
    assert!(s.sees("50k / 400k tokens (12%)"), "the exact window from the status line");
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
