//! Images in prompts: dropped (or pasted) images go in as markers with thumbnails, are sent
//! to Claude as image blocks, and show as thumbnails in the conversation.
use super::harness::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A small PNG at `dir/name`.
fn png(dir: &Path, name: &str, rgb: [u8; 3]) -> PathBuf {
    let path = dir.join(name);
    image::RgbImage::from_pixel(8, 6, image::Rgb(rgb)).save(&path).unwrap();
    path
}

fn saved_images(s: &Scope) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(s.env.config.join("images")).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn a_dropped_image_is_sent_to_claude_as_an_image_block() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    s.type_prompt("look at");
    let file = png(&s.env.root, "shot.png", [255, 0, 0]);
    s.drop_file(&file);
    let saved = saved_images(&s);
    assert_eq!(saved.len(), 1, "the image is kept in the config folder");
    let name = &saved[0];
    assert_eq!(s.prompt(), format!("look at [image:{name}] "), "its marker goes in at the cursor");
    assert!(s.sees(&format!("Image {name}")), "a thumbnail shows above the input");
    s.type_prompt("please");
    s.press(eframe::egui::Key::Enter);
    s.wait_to_see("Echo: look at please (1 images)");
    let call = s.env.calls().into_iter().rfind(|c| c["args"].as_array().unwrap().iter().any(|a| a == "--resume")).unwrap();
    let args: Vec<&str> = call["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(args.windows(2).any(|w| w == ["--input-format", "stream-json"]));
    assert!(!args.iter().any(|a| a.contains("[image:")), "the prompt goes on stdin, not as an argument");
    // Claude Code keeps the image in the session file; reading it back gives the same file.
    let lines = std::fs::read_to_string(s.env.session_file("s-a").unwrap()).unwrap();
    let user: Value = lines.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).rfind(|v| v["type"] == "user").unwrap();
    let kinds: Vec<&str> = user["message"]["content"].as_array().unwrap().iter().map(|b| b["type"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["text", "image", "text"]);
    s.wait_for_runs();
    s.frames(5);
    assert!(s.sees(&format!("Image {name}")), "the conversation shows it as a thumbnail");
    assert_eq!(saved_images(&s).len(), 1);
}

#[test]
fn images_in_a_session_file_show_as_thumbnails() {
    let setup = setup().session(session("s-a", "alpha prompt", 100));
    let file = setup.env().session_file("s-a").unwrap();
    let data = {
        let p = png(&setup.env().root, "a.png", [0, 255, 0]);
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, std::fs::read(p).unwrap())
    };
    let line = json!({"type": "user", "message": {"role": "user", "content": [
        {"type": "text", "text": "[Image #1] what is this"},
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": data}},
    ]}});
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str(&format!("{line}\n"));
    std::fs::write(&file, text).unwrap();
    let mut s = setup.launch();
    s.click("alpha prompt");
    let saved = saved_images(&s);
    assert_eq!(saved.len(), 1, "it is saved once the session is shown");
    assert!(s.sees(&format!("Image {}", saved[0])));
    assert!(s.sees("what is this"), "{}", s.screen());
}

#[test]
fn files_that_are_not_images_are_not_taken() {
    let mut s = setup().session(session("s-a", "alpha prompt", 100)).launch();
    s.click("alpha prompt");
    let notes = s.env.root.join("notes.txt");
    std::fs::write(&notes, "hi").unwrap();
    s.drop_file(&notes);
    assert_eq!(s.prompt(), "");
    assert!(saved_images(&s).is_empty());
}
