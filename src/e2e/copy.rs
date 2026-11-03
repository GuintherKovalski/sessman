//! 📋 on each message copies all of it.
use super::harness::*;

#[test]
fn every_message_has_a_copy_button_that_copies_all_of_it() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100).reply("alpha reply\n\n[tool: Read]\nsecond **paragraph**")).launch();
    s.click("alpha prompt");
    s.click_first("📋");
    assert_eq!(s.copied.as_deref(), Some("alpha prompt"));
    assert!(s.sees("✔"), "it shows that it copied");
    s.click_last("📋");
    assert_eq!(s.copied.as_deref(), Some("alpha reply\n\nsecond **paragraph**"), "the Markdown source, without tool lines");
}
