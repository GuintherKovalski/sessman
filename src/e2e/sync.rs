//! Several scopes on one config folder (two windows, say) show the same: what another one
//! saves (workspaces, names, settings) shows here by itself, and a change made here is made on
//! top of theirs. The other scope is played by writing its files.
use super::harness::*;
use eframe::egui::Key;
use serde_json::json;

#[test]
fn a_workspace_another_scope_adds_shows_up_by_itself() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &["s-a"])])
        .launch();
    assert!(!s.sees("THEIRS"));
    s.env.write_config("workspaces.json", &json!({"list": [
        {"name": "Work", "sessions": ["s-a"], "color": [86, 156, 214]},
        {"name": "Theirs", "sessions": ["s-b"], "color": [214, 122, 78]},
    ]}));
    s.wait_to_see("THEIRS");
    s.click_first("THEIRS");
    assert!(s.sees("beta prompt"));
    assert!(!s.sees("alpha prompt"), "with the sessions they put in it");
}

#[test]
fn a_session_another_scope_takes_out_leaves_the_list_here() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &["s-a", "s-b"])])
        .launch();
    s.click_first("WORK");
    assert!(s.sees("2 sessions"));
    s.env.write_config("workspaces.json", &json!({"list": [{"name": "Work", "sessions": ["s-a"], "color": [86, 156, 214]}]}));
    s.wait_to_see("1 session");
    assert!(s.is_selected("WORK"), "the open tab stays on its workspace");
    assert!(s.sees("alpha prompt"));
}

#[test]
fn a_rename_made_in_another_scope_shows_here() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.env.write_config("names.json", &json!({"s-a": "Named over there"}));
    s.wait_to_see("Named over there");
    assert!(!s.sees("alpha prompt"));
}

#[test]
fn a_rename_here_keeps_the_ones_another_scope_made() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .launch();
    // Written just before the rename: this scope may not have noticed it yet.
    s.env.write_config("names.json", &json!({"s-b": "Theirs"}));
    s.double_click("alpha prompt");
    s.type_in("rename", "Mine");
    s.press(Key::Enter);
    let names = s.env.config_json("names.json");
    assert_eq!(names["s-a"], "Mine");
    assert_eq!(names["s-b"], "Theirs", "{names}");
    s.wait_to_see("Theirs");
}

#[test]
fn settings_another_scope_saves_apply_here() {
    let proj = std::path::PathBuf::from("/tmp/x.rs");
    let mut s = setup().session(session("s-a", "alpha prompt", 100).edit(&proj, "a", "b")).launch();
    s.click("alpha prompt");
    assert!(s.sees("⚙ Edit"));
    let mut settings = s.env.config_json("settings.json");
    settings["show_tool_lines"] = json!(false);
    s.env.write_config("settings.json", &settings);
    s.wait_not_to_see("⚙ Edit");
}

#[test]
fn a_zoom_another_scope_saves_is_taken_not_undone() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.frames(3);
    let mut settings = s.env.config_json("settings.json");
    settings["zoom"] = json!(1.5);
    s.env.write_config("settings.json", &settings);
    s.wait_until("the zoom is taken", |s| s.zoom() > 1.4);
    s.frames(5);
    assert_eq!(s.env.config_json("settings.json")["zoom"], 1.5, "the old zoom isn't saved back over it");
}
