//! + New session: an empty session whose first prompt creates it.
use super::harness::*;
use serde_json::json;

/// A workspace whose new sessions go to `folder` without asking (the native folder picker
/// can't be driven from a test, so these start where the workspace already has a folder).
fn workspace_with_folder() -> Scope {
    let s = setup().session(session("s-a", "alpha prompt", 100));
    let folder = s.env().folder("newproj");
    let mut s = s
        .workspaces_json(json!({"list": [{"name": "Work", "sessions": ["s-a"], "folder": folder}]}))
        .launch();
    s.click_first("Work");
    s
}

#[test]
fn new_session_opens_an_empty_session_in_the_workspace_folder() {
    let mut s = workspace_with_folder();
    let folder = s.env.root.join("newproj");
    s.click("+  New session");
    assert!(s.sees(&format!("New session in {}", folder.display())));
    assert!(s.sees("Type your first prompt below."));
    assert!(s.sees("Model") && s.sees("Effort") && s.sees("Permissions") && s.sees("MCP servers"), "what it will start with");
    assert!(s.env.runs().is_empty(), "nothing exists until the first prompt");
}

#[test]
fn the_first_prompt_creates_the_session_in_the_workspace() {
    let mut s = workspace_with_folder();
    let folder = s.env.root.join("newproj");
    s.click("+  New session");
    s.env.hold("hold");
    s.send("HOLD start the new thing");
    s.wait_to_see("Partial reply");
    assert!(s.sees("HOLD start the new thing"), "listed (as starting) at once");
    let (id, prompt) = s.env.runs().pop().unwrap();
    assert_eq!(prompt, "HOLD start the new thing");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--session-id")).unwrap();
    assert_eq!(call["cwd"], folder.display().to_string());
    assert!(s.list().contains(&format!("{id}  [starting]")), "it joins the open workspace");
    s.env.release("hold");
    s.wait_until("it is a normal session", |s| !s.list().contains("[starting]"));
    s.wait_to_see("Echo: HOLD start the new thing");
    s.settle();
    assert!(s.is_selected("HOLD start the new thing"), "it stays open and selected");
    assert_eq!(s.env.config_json("workspaces.json")["list"][0]["last"], id.as_str());
}

#[test]
fn haiku_names_the_session_after_its_first_answer() {
    let mut s = workspace_with_folder();
    s.env.reply("name", "\"Start new thing.\"");
    s.click("+  New session");
    s.send("please start the new thing for me");
    s.wait_to_see("Start new thing");
    let (id, _) = s.env.runs().pop().unwrap();
    assert_eq!(s.env.config_json("names.json")[id.as_str()], "Start new thing");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("Name this coding session"))).unwrap();
    let args = call["args"].as_array().unwrap();
    assert!(args.windows(2).any(|w| w[0] == "--model" && w[1] == "haiku"));
    assert!(args.last().unwrap().as_str().unwrap().contains("Echo: please start the new thing for me"), "it sees the answer");
}

#[test]
fn a_starting_session_can_be_left_and_opened_again() {
    let mut s = workspace_with_folder();
    s.click("+  New session");
    s.env.hold("hold");
    s.send("HOLD slow start");
    s.wait_to_see("Partial reply");
    s.click("alpha prompt");
    assert!(!s.sees("Partial reply"));
    s.click("HOLD slow start");
    s.wait_to_see("Partial reply");
    s.env.release("hold");
    s.wait_to_see("Echo: HOLD slow start");
}

