//! The Templates tab: a scope's prompt templates, skills and commands, shown in the main
//! area; Create template, where Claude writes one (and can be pointed at a session).
use super::harness::*;
use eframe::egui::Key;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// A scope whose root (its sessions' folder) has a template, a skill and a command, opened
/// on the Templates tab.
fn templates_tab() -> (Scope, PathBuf) {
    let s = setup().session(session("s-a", "alpha prompt", 100).reply("alpha reply"));
    let root = s.env().proj.clone();
    write(&root.join(".scope/templates/release-notes.md"), "---\nname: Release notes\ndescription: Writes the release notes\n---\nWrite notes for version {{version}}.\n");
    write(&root.join(".claude/skills/deploy/SKILL.md"), "---\nname: deploy\ndescription: Ships the app\n---\nRun the deploy steps.\n");
    write(&root.join(".claude/commands/review.md"), "Review the diff carefully\n");
    let mut s = s.workspaces_json(json!({"list": [{"name": "Work", "sessions": ["s-a"], "folder": root}]})).launch();
    s.click_first("WORK");
    s.click("Templates");
    (s, root)
}

/// The arguments of the last composer run (the one with `--tools`), and its folder.
fn composer_call(s: &Scope) -> (Vec<String>, String) {
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--tools")).expect("a composer run");
    let args = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_owned()).collect();
    (args, call["cwd"].as_str().unwrap().to_owned())
}

fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).map(|i| args[i + 1].as_str())
}

#[test]
fn the_tab_lists_the_scopes_templates_skills_and_commands() {
    let (s, _) = templates_tab();
    for text in ["Prompt templates · 1", "Release notes", "Writes the release notes", "Skills · 1", "deploy", "Ships the app", "Commands · 1", "review", "Review the diff carefully"] {
        assert!(s.sees(text), "{text}:\n{}", s.screen());
    }
}

#[test]
fn clicking_one_shows_it_in_the_main_area() {
    let (mut s, root) = templates_tab();
    s.click("Release notes");
    s.wait_to_see("Write notes for version {{version}}.");
    assert!(s.sees("📝 Release notes"), "in a tab of its own");
    assert!(s.sees(&root.join(".scope/templates/release-notes.md").display().to_string()));
    s.click("Source");
    assert!(s.sees("name: Release notes"), "the source has its front matter");
    s.click("Main");
    assert!(s.sees("alpha reply"), "Main is the conversation again");
    s.click("📝 Release notes");
    s.click("deploy");
    s.wait_to_see("Run the deploy steps.");
}

#[test]
fn use_in_prompt_puts_a_template_in_the_input_and_a_skill_by_its_name() {
    let (mut s, _) = templates_tab();
    s.click("Release notes");
    s.click("Use in prompt");
    assert_eq!(s.prompt(), "Write notes for version {{version}}.");
    s.click("deploy");
    s.click("Use in prompt (/name)");
    assert_eq!(s.prompt(), "/deploy ");
}

#[test]
fn create_template_has_claude_write_it_only_in_the_templates_folder() {
    let (mut s, root) = templates_tab();
    s.click("✨  Create template");
    assert!(s.sees("New prompt template"));
    s.type_in("template-input", "TEMPLATE notes for a release");
    s.press(Key::Enter);
    s.wait_to_see("Do the fake thing to {{target}}");
    assert!(s.sees("Echo: TEMPLATE notes for a release"), "Claude's reply shows under it");
    assert!(s.sees("✨ Fake template"), "the tab is named after it");
    let dir = root.join(".scope/templates");
    assert!(dir.join("fake-template.md").is_file());
    let (args, cwd) = composer_call(&s);
    assert_eq!(cwd, root.display().to_string(), "it runs in the scope's root");
    assert_eq!(after(&args, "--tools"), Some("Read,Grep,Glob,Write,Edit"));
    assert_eq!(after(&args, "--permission-mode"), Some("dontAsk"));
    let allowed = after(&args, "--allowedTools").unwrap();
    assert!(allowed.contains(&format!("Edit(/{}/**)", dir.display())) && allowed.contains(&format!("Edit(/{})", root.join(".gitignore").display())), "{allowed}");
    let system = after(&args, "--append-system-prompt").unwrap();
    assert!(system.contains("add the line `/.scope/templates/`") && !system.contains("scope list"), "its own instructions, not scope's commands");
    let id = after(&args, "--session-id").expect("a new session").to_owned();
    // The store is saved when the run ends, a moment after the template shows.
    let want = dir.join("fake-template.md").display().to_string();
    s.wait_until("templates.json has the new session's template", |s| s.env.config_json("templates.json")["sessions"][&id]["template"] == want.as_str());

    // The list has the new template; the pseudo session isn't a session in it.
    s.wait_to_see("Prompt templates · 2");
    assert!(s.sees("Made by the fake claude"));
    assert!(!s.list().contains(&id));

    // The next prompt goes on in the same session, on the same file.
    s.type_in("template-input", "TEMPLATE make it shorter");
    s.press(Key::Enter);
    s.wait_to_see("Echo: TEMPLATE make it shorter");
    let (args, _) = composer_call(&s);
    assert_eq!(after(&args, "--resume"), Some(id.as_str()));
    assert!(after(&args, "--append-system-prompt").unwrap().contains(&format!("You are working on the template {}", dir.join("fake-template.md").display())));

    // Still out of the list after a restart.
    let mut s = s.restart();
    assert!(!s.list().contains(&id));
}

#[test]
fn a_prompt_can_refer_to_a_session_by_its_name() {
    let (mut s, _) = templates_tab();
    s.click("✨  Create template");
    s.click("@ Session");
    s.click_last("alpha prompt");
    assert_eq!(s.field_text("template-input"), "@[alpha prompt] ");
    s.type_in("template-input", "TEMPLATE redo what it did");
    s.press(Key::Enter);
    s.wait_to_see("Do the fake thing to {{target}}");
    let (args, _) = composer_call(&s);
    let prompt = args.last().unwrap();
    let transcript = s.env.session_file("s-a").unwrap();
    assert!(prompt.starts_with("@[alpha prompt] TEMPLATE redo what it did\n\n<referenced-sessions>"), "{prompt}");
    assert!(prompt.contains(&format!("## @[alpha prompt]\nTranscript: {}", transcript.display())), "{prompt}");
    assert!(prompt.contains("USER: alpha prompt\nCLAUDE: alpha reply"), "a digest of it: {prompt}");
    assert!(!s.sees("referenced-sessions"), "the prompt shows as typed:\n{}", s.screen());
    assert!(s.sees("@[alpha prompt]"), "the reference reads as typed:\n{}", s.screen());
}

#[test]
fn a_reference_to_no_session_is_refused_before_claude_runs() {
    let (mut s, _) = templates_tab();
    s.click("✨  Create template");
    s.type_in("template-input", "like @[Nothing like it]");
    s.press(Key::Enter);
    assert!(s.sees("@[Nothing like it]: no session is called"), "{}", s.screen());
    assert!(!s.env.calls().iter().any(|c: &Value| c["args"].as_array().unwrap().iter().any(|a| a == "--tools")));
    assert_eq!(s.field_text("template-input"), "like @[Nothing like it]", "kept to fix");
}

#[test]
fn change_with_claude_reopens_the_session_that_wrote_a_template() {
    let (mut s, _) = templates_tab();
    s.click("✨  Create template");
    s.type_in("template-input", "TEMPLATE first version");
    s.press(Key::Enter);
    s.wait_to_see("Prompt templates · 2");
    let (args, _) = composer_call(&s);
    let id = after(&args, "--session-id").unwrap().to_owned();
    s.middle_click("✨ Fake template");
    s.wait_not_to_see("Echo: TEMPLATE first version");
    s.click("Fake template");
    s.click("✎ Change with Claude");
    s.wait_to_see("Echo: TEMPLATE first version");
    s.type_in("template-input", "TEMPLATE second version");
    s.press(Key::Enter);
    s.wait_to_see("Echo: TEMPLATE second version");
    let (args, _) = composer_call(&s);
    assert_eq!(after(&args, "--resume"), Some(id.as_str()));
}

#[test]
fn templates_can_stay_in_git_and_go_to_another_folder() {
    let s = setup().session(session("s-a", "alpha prompt", 100)).setting("templates_gitignore", json!(false)).setting("templates_folder", json!("prompts"));
    let root = s.env().proj.clone();
    let mut s = s.workspaces_json(json!({"list": [{"name": "Work", "sessions": ["s-a"], "folder": root}]})).launch();
    s.click_first("WORK");
    s.click("Templates");
    s.click("✨  Create template");
    s.type_in("template-input", "TEMPLATE one");
    s.press(Key::Enter);
    s.wait_to_see("Do the fake thing to {{target}}");
    assert!(root.join("prompts/fake-template.md").is_file());
    let (args, _) = composer_call(&s);
    assert!(!args.iter().any(|a| a.contains(".gitignore")), "{args:?}");
}

#[test]
fn skills_sharing_a_prefix_or_a_subproject_fold_into_groups() {
    let s = setup().session(session("s-a", "alpha prompt", 100));
    let root = s.env().proj.clone();
    write(&root.join(".claude/skills/speckit-plan/SKILL.md"), "---\nname: speckit-plan\ndescription: Plans the feature\n---\nPlan it.\n");
    write(&root.join(".claude/skills/speckit-tasks/SKILL.md"), "---\nname: speckit-tasks\ndescription: Lists the tasks\n---\nTask it.\n");
    write(&root.join("apps/api/.claude/skills/migrate/SKILL.md"), "---\nname: migrate\ndescription: Migrates the database\n---\nMigrate it.\n");
    let mut s = s.workspaces_json(json!({"list": [{"name": "Work", "sessions": ["s-a"], "folder": root}]})).launch();
    s.click_first("WORK");
    s.click("Templates");
    assert!(s.sees("Skills · 3"));
    assert!(s.sees("speckit  (2)") && s.sees("apps/api  (1)"), "{}", s.screen());
    assert!(!s.sees("Plans the feature") && !s.sees("Migrates the database"), "groups start closed");
    s.click("speckit  (2)");
    assert!(s.sees("Plans the feature") && s.sees("Lists the tasks"));
    s.click("plan");
    s.wait_to_see("Plan it.");
    s.click("speckit  (2)");
    assert!(!s.sees("Lists the tasks"), "closed again");
    s.click("apps/api  (1)");
    s.click("migrate");
    s.wait_to_see("Migrate it.");
}
