//! Ctrl+F / Cmd+F: find in the conversation or the open file.
use super::harness::*;
use eframe::egui::{Key, Modifiers};

fn conversation() -> Scope {
    let mut s = setup()
        .session(session("s-a", "first needle here", 100).reply("no match in this one").turn("another needle", "and a needle in the reply"))
        .launch();
    s.click("first needle here");
    s
}

#[test]
fn cmd_f_opens_the_find_bar_and_counts_matches() {
    let mut s = conversation();
    s.press_with(Modifiers::COMMAND, Key::F);
    assert!(s.has_field("find-input"));
    s.type_in("find-input", "needle");
    s.frames(3);
    assert!(s.sees("1 of 3"));
    s.press(Key::Enter);
    assert!(s.sees("2 of 3"));
    s.press_with(Modifiers::SHIFT, Key::Enter);
    assert!(s.sees("1 of 3"));
}

#[test]
fn ctrl_f_works_too_and_no_match_says_so() {
    let mut s = conversation();
    s.press_with(Modifiers::CTRL, Key::F);
    s.type_in("find-input", "haystack");
    s.frames(3);
    assert!(s.sees("No results"));
}

#[test]
fn escape_closes_the_find_bar() {
    let mut s = conversation();
    s.press_with(Modifiers::COMMAND, Key::F);
    s.type_in("find-input", "needle");
    s.press(Key::Escape);
    assert!(!s.has_field("find-input"));
}

#[test]
fn find_searches_the_open_file_tab() {
    let s = setup();
    let file = s.env().proj.join("code.rs");
    std::fs::write(&file, "let needle = 1;\nlet other = needle;\n").unwrap();
    let mut s = s.session(session("s-a", "alpha prompt", 100).edit(&file, "x", "needle")).launch();
    s.click("alpha prompt");
    s.double_click_first("code.rs");
    s.wait_to_see("📄 File");
    s.press_with(Modifiers::COMMAND, Key::F);
    s.type_in("find-input", "needle");
    s.frames(3);
    assert!(s.sees("1 of 2"), "the file's matches, not the conversation's");
}
