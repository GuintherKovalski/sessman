//! The Changes tab and the read-only file tabs it opens.
use super::harness::*;
use eframe::egui::Key;

/// A session that edited two files in its folder, and another one with no changes.
fn edited() -> Scope {
    let s = setup();
    let proj = s.env().proj.clone();
    std::fs::write(proj.join("main.rs"), "fn main() {\n    println!(\"new\");\n}\n").unwrap();
    std::fs::write(proj.join("README.md"), "# Title\n\nSome *text*.\n").unwrap();
    let mut s = s
        .session(
            session("s-a", "alpha prompt", 100)
                .edit(&proj.join("main.rs"), "    println!(\"old\");\n", "    println!(\"new\");\n")
                .edit(&proj.join("README.md"), "", "# Title\n\nSome *text*.\n"),
        )
        .session(session("s-b", "beta prompt", 50))
        .launch();
    s.click("alpha prompt");
    s
}

#[test]
fn changes_list_each_changed_file_with_counts() {
    let s = edited();
    assert!(s.sees("Files (2)"));
    assert!(s.sees("main.rs"));
    assert!(s.sees("README.md"));
    assert!(s.sees("+1 -1"));
    assert!(s.sees("+3 -0"));
    assert!(s.sees("./"), "each file's folder, relative to the session's");
}

#[test]
fn a_session_without_edits_says_so() {
    let mut s = edited();
    s.click("beta prompt");
    assert!(s.sees("No code changes in this session"));
}

#[test]
fn clicking_a_file_shows_only_its_changes() {
    let mut s = edited();
    s.click_first("main.rs");
    assert!(s.sees("Show all"));
    s.click("Show all");
    assert!(!s.sees("Show all"));
}

#[test]
fn double_clicking_a_file_opens_it_read_only_in_a_tab() {
    let mut s = edited();
    let path = s.env.proj.join("main.rs").display().to_string();
    s.double_click_first("main.rs");
    s.wait_to_see("📄 File");
    assert!(s.sees(&path), "the header shows the full path");
    assert!(s.sees("Main"), "a tab bar appears");
    assert!(s.sees("println!(\"new\");"));
    assert!(s.sees("3 lines"));
    assert!(s.sees("+1 -1 in 1 edit this session"));
}

#[test]
fn markdown_files_open_rendered_with_a_source_toggle() {
    let mut s = edited();
    s.double_click_first("README.md");
    s.wait_to_see("📄 File");
    // The Changes tab shows the file's diff, "# Title" included; only the main area counts.
    s.click("Info");
    assert!(s.sees("Rendered"));
    assert!(s.sees("Title"));
    assert!(!s.sees("# Title"));
    s.click("Source");
    assert!(s.sees("# Title"));
}

#[test]
fn file_tabs_close_with_their_x_or_a_middle_click() {
    let mut s = edited();
    s.double_click_first("main.rs");
    s.wait_to_see("📄 File");
    s.middle_click_last("main.rs");
    s.frames(3);
    assert!(!s.sees("📄 File"), "back to the conversation");
    assert!(!s.sees("Main"), "no tab bar without file tabs");
}

#[test]
fn main_tab_goes_back_to_the_conversation() {
    let mut s = edited();
    s.double_click_first("main.rs");
    s.wait_to_see("📄 File");
    s.click("Main");
    assert!(!s.sees("📄 File"));
    assert!(s.sees("Reply to alpha prompt"));
}

#[test]
fn file_tabs_belong_to_their_session_and_survive_a_restart() {
    let mut s = edited();
    s.double_click_first("main.rs");
    s.wait_to_see("📄 File");
    s.click("beta prompt");
    assert!(!s.sees("📄 File"), "another session shows only its own tabs");
    s.click("alpha prompt");
    assert!(s.sees("📄 File"), "and coming back brings them back, the file tab active");
    let mut s = s.restart();
    s.frames(3);
    assert!(s.sees("📄 File"));
    s.press(Key::Escape);
}


fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success();
    assert!(ok, "git {args:?}");
}

/// `edited`, with main.rs committed in a repository and README.md not.
fn committed_one() -> Scope {
    let s = setup();
    let proj = s.env().proj.clone();
    std::fs::write(proj.join("main.rs"), "fn main() {\n    println!(\"new\");\n}\n").unwrap();
    std::fs::write(proj.join("README.md"), "# Title\n\nSome *text*.\n").unwrap();
    git(&proj, &["init", "-q"]);
    git(&proj, &["add", "main.rs"]);
    git(&proj, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "main"]);
    let mut s = s
        .session(
            session("s-a", "alpha prompt", 100)
                .edit(&proj.join("main.rs"), "    println!(\"old\");\n", "    println!(\"new\");\n")
                .edit(&proj.join("README.md"), "", "# Title\n\nSome *text*.\n"),
        )
        .launch();
    s.click("alpha prompt");
    s
}

#[test]
fn changes_show_committed_files_too_by_default() {
    let s = committed_one();
    assert!(s.sees("Files (2)"), "committed main.rs is listed as well");
    assert!(s.sees("All") && s.sees("Uncommitted"));
}

#[test]
fn uncommitted_shows_only_files_git_reports_as_changed_and_is_remembered() {
    let mut s = committed_one();
    s.click("Uncommitted");
    s.wait_to_see("Files (1)");
    assert!(s.sees("README.md"));
    assert!(!s.sees("+1 -1"), "main.rs is committed, so hidden");
    let mut s = s.restart();
    s.wait_to_see("Files (1)");
    s.click("All");
    s.wait_to_see("Files (2)");
}

#[test]
fn uncommitted_says_so_when_everything_is_committed() {
    let mut s = committed_one();
    let proj = s.env.proj.clone();
    git(&proj, &["add", "README.md"]);
    git(&proj, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "readme"]);
    s.click("Uncommitted");
    s.wait_to_see("No uncommitted changes");
    assert!(s.sees("Files (0)"));
}
