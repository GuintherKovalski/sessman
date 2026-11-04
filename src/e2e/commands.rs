//! `scope list|rename|move|new|ask`: the commands sessions (and terminals) send the app.
use super::harness::*;
use serde_json::json;

fn three_in_work() -> Scope {
    setup()
        .session(session("aaaaaaaa-1111", "alpha prompt", 300))
        .session(session("bbbbbbbb-2222", "beta prompt", 200))
        .session(session("cccccccc-3333", "gamma prompt", 100))
        .session(session("dddddddd-4444", "loose prompt", 50))
        .workspaces(&[("Work", &["aaaaaaaa-1111", "bbbbbbbb-2222", "cccccccc-3333"]), ("Empty", &[])])
        .launch()
}

#[test]
fn list_shows_each_workspace_in_bar_order() {
    let mut s = three_in_work();
    assert_eq!(
        s.list(),
        "Work (newest first)\n  1. gamma prompt  cccccccc-3333\n  2. beta prompt  bbbbbbbb-2222\n  3. alpha prompt  aaaaaaaa-1111\nEmpty (newest first)"
    );
    assert_eq!(s.command(json!({"do": "list", "workspace": "work"})).unwrap().lines().count(), 4, "names match in any case");
    assert!(s.command(json!({"do": "list", "workspace": "Nope"})).unwrap_err().contains("no scope is called \"Nope\""));
}

#[test]
fn list_without_workspaces_says_so() {
    let mut s = setup().session(session("s-a", "alpha", 10)).launch();
    assert_eq!(s.list(), "No scopes yet.");
}

#[test]
fn rename_finds_a_session_by_id_prefix_or_name() {
    let mut s = three_in_work();
    assert_eq!(s.command(json!({"do": "rename", "session": "aaaaaaaa", "name": "First"})).unwrap(), "Renamed \"alpha prompt\" to \"First\"");
    assert_eq!(s.command(json!({"do": "rename", "session": "first", "name": "Again"})).unwrap(), "Renamed \"First\" to \"Again\"");
    assert_eq!(s.command(json!({"do": "rename", "session": "aaaaaaaa-1111", "name": ""})).unwrap(), "Renamed \"Again\" to \"alpha prompt\"");
    assert!(s.sees("alpha prompt"));
    let err = s.command(json!({"do": "rename", "session": "nobody", "name": "x"})).unwrap_err();
    assert_eq!(err, "no session is called or has the id \"nobody\" (see scope list)");
}

#[test]
fn ambiguous_names_ask_for_the_id() {
    let mut s = setup()
        .session(session("aaaaaaaa-1111", "same name", 300))
        .session(session("bbbbbbbb-2222", "same name", 200))
        .launch();
    let err = s.command(json!({"do": "rename", "session": "same name", "name": "x"})).unwrap_err();
    assert!(err.starts_with("\"same name\" could be 2 sessions: "), "{err}");
    assert!(err.ends_with("; use the id"), "{err}");
}

#[test]
fn move_places_a_card_and_the_bar_stays_newest_first() {
    let mut s = three_in_work();
    let newest_first = "Work (newest first)\n  1. gamma prompt  cccccccc-3333\n  2. beta prompt  bbbbbbbb-2222\n  3. alpha prompt  aaaaaaaa-1111\nEmpty (newest first)";
    assert_eq!(s.command(json!({"do": "move", "session": "alpha prompt", "to": "top"})).unwrap(), "Moved \"alpha prompt\" to place 1 of 3 on the Work orchestration page");
    s.command(json!({"do": "move", "session": "alpha prompt", "to": "down"})).unwrap();
    s.command(json!({"do": "move", "session": "gamma prompt", "to": "bottom"})).unwrap();
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["sessions"], json!(["aaaaaaaa-1111", "bbbbbbbb-2222", "cccccccc-3333"]));
    s.command(json!({"do": "move", "session": "beta prompt", "to": "3"})).unwrap();
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["sessions"], json!(["aaaaaaaa-1111", "cccccccc-3333", "bbbbbbbb-2222"]));
    s.command(json!({"do": "move", "session": "beta prompt", "to": "up"})).unwrap();
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["sessions"], json!(["aaaaaaaa-1111", "bbbbbbbb-2222", "cccccccc-3333"]));
    assert_eq!(s.list(), newest_first, "the bar ignores the cards' order");
    let err = s.command(json!({"do": "move", "session": "loose prompt", "to": "top"})).unwrap_err();
    assert_eq!(err, "that session is in no scope, so it has no card to move");
    let err = s.command(json!({"do": "move", "session": "alpha prompt", "to": "sideways"})).unwrap_err();
    assert!(err.starts_with("move where? \"sideways\""), "{err}");
}

#[test]
fn the_bar_lists_newest_first_even_when_arranged() {
    let mut s = three_in_work();
    s.command(json!({"do": "move", "session": "alpha prompt", "to": "top"})).unwrap();
    s.click_first("WORK");
    assert!(s.shows_before("gamma prompt", "beta prompt"));
    assert!(s.shows_before("beta prompt", "alpha prompt"));
}

#[test]
fn new_starts_a_session_in_a_workspace_and_its_folder() {
    let mut s = three_in_work();
    let dir = s.env.folder("elsewhere");
    let reply = s.command(json!({"do": "new", "prompt": "build the thing", "workspace": "Empty", "folder": dir, "name": "Builder"})).unwrap();
    assert!(reply.starts_with(&format!("Started session \"Builder\" in Empty in {} (id ", dir.display())), "{reply}");
    let id = reply.trim_end_matches(')').rsplit("id ").next().unwrap().to_owned();
    s.wait_until("the new session is listed", |s| s.list().contains(&format!("1. Builder  {id}\n")) || s.list().ends_with(&format!("1. Builder  {id}")));
    let (run_id, prompt) = s.env.runs().pop().unwrap();
    assert_eq!((run_id.as_str(), prompt.as_str()), (id.as_str(), "build the thing"));
    let call = s.env.calls().into_iter().find(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--session-id")).unwrap();
    assert_eq!(call["cwd"], dir.display().to_string());
    assert_eq!(call["session"], id, "the session knows its own id (SCOPE_SESSION)");
    let file = s.env.session_file(&id).expect("its file was written");
    assert!(std::fs::read_to_string(file).unwrap().contains("Echo: build the thing"));
}

#[test]
fn new_defaults_to_the_callers_workspace_and_its_folder() {
    let mut s = three_in_work();
    let reply = s.command_from(json!({"do": "new", "prompt": "a sibling"}), Some("bbbbbbbb-2222"), None).unwrap();
    assert!(reply.contains(" in Work in "), "{reply}");
    assert!(reply.contains(&s.env.proj.display().to_string()), "the workspace's sessions' folder: {reply}");
}

#[test]
fn new_without_a_folder_to_go_by_says_so() {
    let mut s = setup().workspaces(&[("Empty", &[])]).launch();
    let err = s.command(json!({"do": "new", "prompt": "x", "workspace": "Empty"})).unwrap_err();
    assert_eq!(err, "which folder? the scope has none yet; use --folder");
    let err = s.command(json!({"do": "new", "prompt": "x", "folder": "/definitely/not/here"})).unwrap_err();
    assert_eq!(err, "/definitely/not/here is not a folder");
}

#[test]
fn a_new_session_lists_as_starting_until_it_is_indexed() {
    let mut s = three_in_work();
    s.env.hold("hold");
    let reply = s.command(json!({"do": "new", "prompt": "HOLD slow start", "workspace": "Empty", "folder": s.env.proj})).unwrap();
    let id = reply.trim_end_matches(')').rsplit("id ").next().unwrap().to_owned();
    let list = s.list();
    assert!(list.contains(&format!("1. HOLD slow start  {id}  [starting]")), "{list}");
    s.env.release("hold");
    s.wait_until("it is listed as a normal session", |s| !s.list().contains("[starting]"));
}

#[test]
fn ask_without_a_workspace_says_which() {
    let mut s = three_in_work();
    let err = s.command(json!({"do": "ask", "question": "how is it going?"})).unwrap_err();
    assert_eq!(err, "which scope? none is open; use --scope");
}

#[test]
fn ask_replies_with_the_orchestrators_answer_in_plain_names() {
    let mut s = three_in_work();
    let reply = s.command(json!({"do": "ask", "question": "how is it going?", "workspace": "Work"})).unwrap();
    assert_eq!(reply, "Fake answer about \"gamma prompt\"");
}

#[test]
fn commands_from_a_broken_request_get_an_error_reply() {
    let mut s = three_in_work();
    let inbox = s.env.config.join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    std::fs::write(inbox.join("0-broken.json"), "{nope").unwrap();
    s.wait_until("the broken command is answered", |s| s.env.config.join("inbox/0-broken.reply").exists());
    let reply = std::fs::read_to_string(inbox.join("0-broken.reply")).unwrap();
    assert!(reply.contains("could not read the command"), "{reply}");
}

#[test]
fn a_new_session_whose_first_run_failed_stays_listed_as_starting() {
    let mut s = three_in_work();
    let reply = s.command(json!({"do": "new", "prompt": "NOFILE never written", "workspace": "Empty", "folder": s.env.proj})).unwrap();
    let id = reply.trim_end_matches(')').rsplit("id ").next().unwrap().to_owned();
    s.wait_until("its run ended", |s| !s.list().contains("[working]") && s.env.runs().iter().any(|r| r.0 == id));
    s.settle();
    assert!(s.list().contains(&format!("1. NOFILE never written  {id}  [starting]")), "{}", s.list());
}
