//! Selecting text by dragging past the edge of a view scrolls it on.
use super::harness::*;

#[test]
fn dragging_a_selection_above_the_conversation_scrolls_it_up() {
    let reply: Vec<String> = (0..80).map(|i| format!("paragraph number {i}")).collect();
    let mut s = setup().session(session("s-a", "alpha prompt", 100).reply(&reply.join("\n\n"))).launch();
    s.click("alpha prompt");
    s.settle();
    // From the last paragraph up past the top of the window, and hold there.
    s.select_from("paragraph number 79", eframe::egui::pos2(300.0, 2.0), 120);
    s.copy();
    let copied = s.copied.clone().unwrap_or_default();
    assert!(copied.contains("paragraph number 79"), "the selection keeps where it started: {copied:?}");
    assert!(copied.contains("paragraph number 5\n"), "and reaches far up: {copied:?}");
}
