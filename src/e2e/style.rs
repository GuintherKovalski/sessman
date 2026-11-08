//! `scope style`: Claude (or anyone at a terminal) changes how scope looks, and it shows at once.
use super::harness::*;
use eframe::egui::{Color32, Key, Modifiers, TextStyle};
use serde_json::json;

fn dark() -> Scope {
    setup().session(session("s-a", "alpha prompt", 100)).setting("theme", json!("dark")).launch()
}

#[test]
fn a_preset_set_by_command_applies_at_once_and_is_saved() {
    let mut s = dark();
    assert_eq!(s.style().visuals.panel_fill, Color32::from_gray(27), "classic: egui's own grey");
    let reply = s.command(json!({"do": "style", "args": ["midnight"]})).unwrap();
    assert!(reply.starts_with("Set preset = midnight."), "{reply}");
    s.frames(2);
    assert_eq!(s.style().visuals.panel_fill, Color32::from_rgb(0x0f, 0x14, 0x20));
    assert_eq!(s.env.config_json("settings.json")["style"]["preset"], "midnight");
}

#[test]
fn keys_change_colors_corners_spacing_and_text() {
    let mut s = dark();
    s.command(json!({"do": "style", "args": ["set", "background", "#202830", "radius=8", "spacing", "1.5", "text_size", "16"]})).unwrap();
    s.frames(2);
    let st = s.style();
    assert_eq!(st.visuals.panel_fill, Color32::from_rgb(0x20, 0x28, 0x30));
    assert_eq!(st.visuals.widgets.inactive.corner_radius.nw, 8);
    assert_eq!(st.spacing.item_spacing.x, 12.0);
    assert_eq!(st.text_styles[&TextStyle::Body].size, 16.0);
    let shown = s.command(json!({"do": "style"})).unwrap();
    assert!(shown.contains("background   #202830* /"), "{shown}");
    s.command(json!({"do": "style", "args": ["reset"]})).unwrap();
    s.frames(2);
    assert_eq!(s.style().visuals.panel_fill, Color32::from_gray(27));
}

#[test]
fn a_bad_value_is_refused_and_changes_nothing() {
    let mut s = dark();
    let err = s.command(json!({"do": "style", "args": ["set", "radius", "4", "accent", "teal"]})).unwrap_err();
    assert!(err.contains("not a color"), "{err}");
    assert_eq!(s.env.config_json("settings.json")["style"]["radius"], 2.0);
}

#[test]
fn the_settings_file_edited_by_hand_applies_live() {
    let mut s = dark();
    let mut v = s.env.config_json("settings.json");
    v["style"] = json!({"preset": "ember"});
    s.env.write_config("settings.json", &v);
    s.wait_until("the ember background shows", |s| s.style().visuals.panel_fill == Color32::from_rgb(0x1d, 0x17, 0x14));
}

#[test]
fn the_settings_page_picks_a_preset() {
    let mut s = dark();
    s.click("alpha prompt");
    s.press_with(Modifiers::COMMAND, Key::Comma);
    s.click("Forest");
    assert_eq!(s.env.config_json("settings.json")["style"]["preset"], "forest");
    s.frames(2);
    assert_eq!(s.style().visuals.panel_fill, Color32::from_rgb(0x14, 0x1c, 0x17));
}
