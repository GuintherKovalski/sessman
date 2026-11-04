//! Workspace tabs: creating, filling, emptying and deleting them, and what each one opens on.
use super::harness::*;
use eframe::egui::{Key, Modifiers};
use serde_json::{Value, json};

fn members(s: &Scope, name: &str) -> Vec<String> {
    let ws = s.env.config_json("workspaces.json");
    let w = ws["list"].as_array().unwrap().iter().find(|w| w["name"] == name).unwrap_or_else(|| panic!("no workspace {name}: {ws}"));
    w["sessions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
}

fn names(s: &Scope) -> Vec<String> {
    let ws = s.env.config_json("workspaces.json");
    ws["list"].as_array().map(|l| l.iter().map(|w| w["name"].as_str().unwrap().to_owned()).collect()).unwrap_or_default()
}

#[test]
fn plus_creates_a_workspace_and_opens_it_empty() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("+");
    s.type_in("new-workspace", "Mine");
    s.press(Key::Enter);
    assert_eq!(names(&s), vec!["Mine"]);
    assert!(s.is_selected("MINE"));
    assert!(s.sees("Empty scope. Drag sessions onto its tab, or right-click a session and choose Move to."));
    assert!(s.sees("No sessions in this scope yet."), "an empty workspace opens on its orchestration page");
    assert!(!s.sees("alpha prompt"));
}

#[test]
fn every_workspace_gets_its_own_color() {
    let mut s = setup().workspaces(&[("One", &[]), ("Two", &[])]).launch();
    s.click("+");
    s.type_in("new-workspace", "Three");
    s.press(Key::Enter);
    let ws = s.env.config_json("workspaces.json");
    let colors: Vec<&Value> = ws["list"].as_array().unwrap().iter().map(|w| &w["color"]).collect();
    assert_eq!(colors.len(), 3);
    assert!(colors.iter().all(|c| c.is_array()), "colors are saved, also for workspaces from before colors: {ws}");
    assert!(colors[0] != colors[1] && colors[1] != colors[2] && colors[0] != colors[2]);
}

#[test]
fn move_to_from_the_context_menu_puts_a_session_in_a_workspace() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &[]), ("Play", &["s-a"])])
        .launch();
    s.right_click("alpha prompt");
    s.open_submenu("Move to");
    s.click_last("WORK");
    assert_eq!(members(&s, "Work"), vec!["s-a"]);
    assert!(members(&s, "Play").is_empty(), "a session is in one workspace at most");
    s.click_first("WORK");
    assert!(s.sees("alpha prompt"));
    assert!(!s.sees("beta prompt"), "a workspace tab lists only its sessions");
}

#[test]
fn dragging_a_session_onto_a_tab_moves_it_there() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &[])])
        .launch();
    // Dragging a highlighted row moves it (from any other row, a drag draws a box).
    s.click("alpha prompt");
    s.drag("alpha prompt", "WORK");
    assert_eq!(members(&s, "Work"), vec!["s-a"]);
}

fn work_open() -> Scope {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .workspaces(&[("Work", &["s-a", "s-b", "s-c"])])
        .launch();
    s.click_first("WORK");
    s
}

fn bar(s: &Scope) -> Value {
    s.env.config_json("workspaces.json")["list"][0]["bar"].clone()
}

#[test]
fn dragging_a_row_up_or_down_reorders_the_bar() {
    let mut s = work_open();
    assert!(s.shows_before("gamma prompt", "beta prompt"), "newest first until dragged");
    s.click("gamma prompt");
    // Dropped on a row's upper half: it goes above that row.
    s.drag("gamma prompt", "alpha prompt");
    assert_eq!(bar(&s), json!(["s-b", "s-c", "s-a"]));
    assert!(s.shows_before("beta prompt", "gamma prompt"));
    assert!(s.shows_before("gamma prompt", "alpha prompt"));
    assert!(s.list().starts_with("Work (arranged by hand)\n  1. beta prompt  s-b\n  2. gamma prompt  s-c\n  3. alpha prompt"), "{}", s.list());
    assert_eq!(members(&s, "Work"), vec!["s-a", "s-b", "s-c"], "the cards' order is apart");
    // The order survives a restart, and Sort newest first undoes it.
    let mut s = s.restart();
    s.click_first("WORK");
    assert!(s.shows_before("beta prompt", "gamma prompt"));
    s.right_click_first("alpha prompt");
    s.click("Sort newest first");
    assert_eq!(bar(&s), Value::Null);
    assert!(s.shows_before("gamma prompt", "beta prompt"));
}

#[test]
fn search_results_do_not_reorder() {
    let mut s = work_open();
    s.click("gamma prompt");
    s.type_in("search", "prompt");
    s.drag("gamma prompt", "alpha prompt");
    assert_eq!(bar(&s), Value::Null, "search results have no order of their own to change");
}

#[test]
fn dragging_a_tab_moves_it_among_the_tabs() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("One", &[]), ("Two", &["s-a"]), ("Three", &[])])
        .launch();
    s.click_first("TWO");
    s.drag("THREE", "ONE");
    assert_eq!(names(&s), ["Three", "One", "Two"]);
    assert!(s.is_selected("TWO"), "the open tab stays open");
    assert!(s.shows_before("THREE", "ONE"));
    assert_eq!(members(&s, "Two"), vec!["s-a"]);
}

#[test]
fn picked_sessions_move_together() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .workspaces(&[("Work", &[])])
        .launch();
    s.click_with("alpha prompt", Modifiers::COMMAND);
    s.click_with("beta prompt", Modifiers::COMMAND);
    s.right_click("beta prompt");
    s.open_submenu("Move to");
    s.click_last("WORK");
    let mut m = members(&s, "Work");
    m.sort();
    assert_eq!(m, vec!["s-a", "s-b"]);
}

#[test]
fn remove_from_workspace_takes_it_out_of_the_tab_only() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &["s-a"])])
        .launch();
    s.click_first("WORK");
    s.right_click("alpha prompt");
    s.click("Remove from scope");
    assert!(members(&s, "Work").is_empty());
    s.click("SCOPES");
    assert!(s.sees("alpha prompt"), "the session itself stays");
}

#[test]
fn deleting_a_workspace_asks_first_and_keeps_its_sessions() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &["s-a"]), ("Other", &[])])
        .launch();
    s.right_click_first("WORK");
    s.click("Delete scope");
    assert!(s.sees("Delete scope \"WORK\"?"));
    s.click("Cancel");
    assert_eq!(names(&s), vec!["Work", "Other"]);
    s.right_click_first("WORK");
    s.click("Delete scope");
    s.click("Delete");
    assert_eq!(names(&s), vec!["Other"]);
    assert!(s.sees("alpha prompt"));
    assert!(s.env.session_file("s-a").is_some());
}

#[test]
fn deleting_the_open_workspace_goes_back_to_all_sessions() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &["s-a"])])
        .setting("confirm_delete_workspace", json!(false))
        .launch();
    s.click_first("WORK");
    s.right_click_first("WORK");
    s.click("Delete scope");
    assert!(names(&s).is_empty(), "no question asked with the setting off");
    assert!(s.is_selected("SCOPES"));
}

#[test]
fn a_workspace_tab_reopens_the_session_last_open_in_it() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .workspaces(&[("Work", &["s-a", "s-b"])])
        .launch();
    s.click_first("WORK");
    assert!(s.is_selected("beta prompt"), "the first time, its newest session");
    s.click("alpha prompt");
    s.click("SCOPES");
    s.click("gamma prompt");
    s.click_first("WORK");
    assert!(s.is_selected("alpha prompt"));
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["last"], "s-a");
}

#[test]
fn all_sessions_clicked_again_goes_back_to_the_overview() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100).reply("alpha reply"))
        .workspaces(&[("Work", &["s-a"])])
        .launch();
    assert!(s.sees("SCOPES"), "All sessions opens on the overview");
    s.click("alpha prompt");
    assert!(s.sees("alpha reply"));
    s.click("SCOPES");
    assert!(!s.sees("alpha reply"));
    assert!(s.sees("1 scope · 1 session"));
}

#[test]
fn workspace_changes_made_by_another_scope_are_not_overwritten() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &[])])
        .launch();
    // Another scope adds a workspace behind this one's back.
    s.env.write_config("workspaces.json", &json!({"list": [{"name": "Work", "sessions": [], "color": [86, 156, 214]}, {"name": "Theirs", "sessions": ["s-b"], "color": [214, 122, 78]}]}));
    s.right_click("alpha prompt");
    s.open_submenu("Move to");
    s.click_last("WORK");
    assert_eq!(names(&s), vec!["Work", "Theirs"]);
    assert_eq!(members(&s, "Theirs"), vec!["s-b"]);
    assert_eq!(members(&s, "Work"), vec!["s-a"]);
}

#[test]
fn in_all_sessions_the_sessions_of_a_workspace_are_marked_by_its_color() {
    // The stripe itself is drawn; what can be checked is that grouped and loose sessions are
    // both listed and the overview counts the loose ones.
    let s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("Work", &["s-a"])])
        .launch();
    assert!(s.sees("alpha prompt") && s.sees("beta prompt"));
    assert!(s.sees("1 session not in a scope."));
}

#[test]
fn deleting_a_workspace_left_of_the_open_one_keeps_that_one_open() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .session(session("s-b", "beta prompt", 50))
        .workspaces(&[("First", &["s-a"]), ("Second", &["s-b"])])
        .setting("confirm_delete_workspace", json!(false))
        .launch();
    s.click_first("SECOND");
    s.right_click_first("FIRST");
    s.click("Delete scope");
    assert_eq!(names(&s), vec!["Second"]);
    assert!(s.is_selected("SECOND"), "the open tab stays on its workspace");
    assert!(s.sees("beta prompt"));
    assert!(!s.sees("alpha prompt"));
}

#[test]
fn a_workspace_is_renamed_from_its_tab_and_keeps_its_overview() {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 100))
        .workspaces(&[("Work", &["s-a"]), ("Other", &[])])
        .config("overview.json", json!({"Work": {"scope": "s", "sessions": {}, "tickets": [], "made": 1, "newest": 1}}))
        .launch();
    s.right_click_first("WORK");
    s.click("Rename scope");
    s.type_in("workspace-rename", "Job");
    s.press(Key::Escape);
    assert_eq!(names(&s), vec!["Work", "Other"], "Escape cancels");
    s.right_click_first("WORK");
    s.click("Rename scope");
    s.type_in("workspace-rename", "Job");
    s.press(Key::Enter);
    assert_eq!(names(&s), vec!["Job", "Other"]);
    assert_eq!(members(&s, "Job"), vec!["s-a"]);
    let overviews = s.env.config_json("overview.json");
    assert!(overviews.get("Job").is_some() && overviews.get("Work").is_none(), "the summary follows: {overviews}");
    s.right_click_first("JOB");
    s.click("Rename scope");
    s.type_in("workspace-rename", "other");
    s.press(Key::Enter);
    assert_eq!(names(&s), vec!["Job", "Other"], "a name already taken is refused");
}

/// Work holds alpha, beta and gamma, listed newest first; Work is open.
fn three_in_work() -> Scope {
    let mut s = setup()
        .session(session("s-a", "alpha prompt", 300))
        .session(session("s-b", "beta prompt", 200))
        .session(session("s-c", "gamma prompt", 100))
        .workspaces(&[("Work", &["s-a", "s-b", "s-c"])])
        .launch();
    s.click_first("WORK");
    s
}

#[test]
fn delete_on_a_clicked_session_takes_it_out_and_opens_the_one_above() {
    let mut s = three_in_work();
    s.click("beta prompt");
    s.press(Key::Delete);
    assert_eq!(members(&s, "Work"), vec!["s-a", "s-c"]);
    assert!(!s.sees("beta prompt"));
    assert!(s.is_selected("gamma prompt"), "the session above it opens:\n{}", s.screen());
    assert!(s.env.session_file("s-b").is_some(), "the session itself stays");
    // The bar keeps the keyboard: Backspace (the Mac's delete key) takes the next one out.
    s.press(Key::Backspace);
    assert_eq!(members(&s, "Work"), vec!["s-a"]);
    assert!(s.is_selected("alpha prompt"), "with nothing above, the one on top opens");
}

#[test]
fn deleting_the_top_session_opens_the_new_top() {
    let mut s = three_in_work();
    s.click("gamma prompt");
    s.press(Key::Backspace);
    assert_eq!(members(&s, "Work"), vec!["s-a", "s-b"]);
    assert!(s.is_selected("beta prompt"));
}

#[test]
fn up_and_down_move_through_the_bar_once_a_session_is_clicked() {
    let mut s = three_in_work();
    s.click("gamma prompt");
    s.press(Key::ArrowDown);
    assert!(s.is_selected("beta prompt"));
    s.press(Key::ArrowDown);
    s.press(Key::ArrowDown);
    assert!(s.is_selected("alpha prompt"), "it stops at the bottom");
    s.press(Key::ArrowUp);
    assert!(s.is_selected("beta prompt"));
    assert_eq!(s.prompt(), "");
}

#[test]
fn typing_after_clicking_a_session_goes_to_the_input_and_delete_stays_there() {
    let mut s = three_in_work();
    s.click("beta prompt");
    s.type_here("h");
    s.type_here("i");
    assert_eq!(s.prompt(), "hi");
    s.press(Key::Backspace);
    assert_eq!(s.prompt(), "h", "Backspace in the input edits it");
    assert_eq!(members(&s, "Work").len(), 3);
}

#[test]
fn delete_does_nothing_in_all_sessions() {
    let mut s = three_in_work();
    s.click("SCOPES");
    s.click("beta prompt");
    s.press(Key::Delete);
    assert_eq!(members(&s, "Work").len(), 3);
    assert!(s.sees("beta prompt"));
}
