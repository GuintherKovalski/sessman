//! Images in prompts. A pasted or dropped image is kept as a file in the config folder's
//! `images/` and stands in the prompt text as `[image:<name>]`, so drafts, the queue and the
//! history stay plain text. Sending turns each marker into an image block (`claude -p
//! --input-format stream-json`); reading a session file turns its image blocks back into
//! markers, named after their data, so a sent image and its saved copy are the same file.
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const OPEN: &str = "[image:";
/// What the API takes per image, base64 aside (its limit is 5 MB encoded).
const MAX_BYTES: usize = 3_700_000;
/// Longer sides are scaled down to this: the model sees no more than about 1.5k pixels anyway.
const MAX_SIDE: u32 = 2000;

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("images")
}

pub fn path(name: &str) -> PathBuf {
    dir().join(name)
}

pub fn marker(name: &str) -> String {
    format!("{OPEN}{name}]")
}

/// The URI egui's image loaders open the image with.
pub fn uri(name: &str) -> String {
    format!("file://{}", path(name).display())
}

#[derive(Debug, PartialEq)]
pub enum Part<'a> {
    Text(&'a str),
    Image(&'a str),
}

/// The text split at its image markers.
pub fn parts(text: &str) -> Vec<Part<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut text_start = 0; // into `rest`: where the current text part began
    let mut scan = 0;
    while let Some(at) = rest[scan..].find(OPEN).map(|i| scan + i) {
        let after = at + OPEN.len();
        match rest[after..].find(']').map(|i| after + i).filter(|&end| valid_name(&rest[after..end])) {
            Some(end) => {
                if at > text_start {
                    out.push(Part::Text(&rest[text_start..at]));
                }
                out.push(Part::Image(&rest[after..end]));
                rest = &rest[end + 1..];
                text_start = 0;
                scan = 0;
            }
            None => scan = after,
        }
    }
    if text_start < rest.len() {
        out.push(Part::Text(&rest[text_start..]));
    }
    out
}

/// The names of the images in `text`, in order.
pub fn names(text: &str) -> Vec<&str> {
    parts(text).into_iter().filter_map(|p| if let Part::Image(n) = p { Some(n) } else { None }).collect()
}

pub fn has_images(text: &str) -> bool {
    !names(text).is_empty()
}

/// `text` without the `n`th image marker (and the space it was set off with).
pub fn remove(text: &str, n: usize) -> String {
    let mut out = String::new();
    let mut seen = 0;
    let mut skip_space = false;
    for part in parts(text) {
        match part {
            Part::Text(t) => out.push_str(if skip_space { t.strip_prefix(' ').unwrap_or(t) } else { t }),
            Part::Image(_) if seen == n => {
                seen += 1;
                skip_space = true;
                continue;
            }
            Part::Image(name) => {
                seen += 1;
                out.push_str(&marker(name));
            }
        }
        skip_space = false;
    }
    out
}

fn valid_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    !stem.is_empty() && stem.chars().all(|c| c.is_ascii_hexdigit()) && media_type(ext).is_some()
}

fn media_type(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

fn ext_of(media_type: &str) -> Option<&'static str> {
    Some(match media_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return None,
    })
}

/// Whether a file looks like an image, by its extension.
pub fn is_image_file(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "tif" | "tiff" | "bmp" | "heic")
}

/// FNV-1a of the image's base64 data: the same image is the same file, however it arrived.
fn name_for(data: &str, ext: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for b in data.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x01000193);
    }
    format!("{h:08x}.{ext}")
}

/// Keeps an image (any common format) where its marker points, in a form Claude takes:
/// PNG, JPEG, GIF or WebP, no more than `MAX_SIDE` pixels a side and `MAX_BYTES` big.
/// Returns its marker.
pub fn add(bytes: &[u8]) -> Result<String, String> {
    let (bytes, ext) = normalize(bytes)?;
    let data = BASE64.encode(&bytes);
    let name = name_for(&data, ext);
    write(&name, &bytes)?;
    Ok(marker(&name))
}

pub fn add_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    // HEIC (iPhone photos) isn't something the image crate reads; macOS's sips is.
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("heic")) {
        return add(&heic_to_png(path)?);
    }
    add(&bytes)
}

#[cfg(not(target_os = "macos"))]
fn heic_to_png(_path: &Path) -> Result<Vec<u8>, String> {
    Err("HEIC images can only be added on macOS; save it as PNG or JPEG first".into())
}

#[cfg(target_os = "macos")]
fn heic_to_png(path: &Path) -> Result<Vec<u8>, String> {
    let out = std::env::temp_dir().join(format!("scope-heic-{}.png", std::process::id()));
    let ok = std::process::Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(path)
        .arg("--out")
        .arg(&out)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let bytes = std::fs::read(&out).ok().filter(|_| ok).ok_or("can't convert the HEIC image")?;
    let _ = std::fs::remove_file(&out);
    Ok(bytes)
}

fn normalize(bytes: &[u8]) -> Result<(Vec<u8>, &'static str), String> {
    use image::ImageFormat;
    let format = image::guess_format(bytes).map_err(|_| "not an image scope can read".to_owned())?;
    let ext = match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::WebP => Some("webp"),
        _ => None,
    };
    let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let (w, h) = reader.into_dimensions().map_err(|e| format!("can't read the image: {e}"))?;
    if let Some(ext) = ext
        && bytes.len() <= MAX_BYTES
        && w.max(h) <= MAX_SIDE
    {
        return Ok((bytes.to_vec(), ext));
    }
    let mut img = image::load_from_memory_with_format(bytes, format).map_err(|e| format!("can't read the image: {e}"))?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).map_err(|e| e.to_string())?;
    if png.len() <= MAX_BYTES {
        return Ok((png, "png"));
    }
    let mut jpg = Vec::new();
    let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 85).encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok((jpg, "jpg"))
}

fn write(name: &str, bytes: &[u8]) -> Result<(), String> {
    let path = path(name);
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    // Whole or not at all: the thumbnail may be read while it is being written.
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| format!("can't save the image: {e}"))
}

/// An image block from a session file: its name, and its data when that isn't saved yet.
pub fn from_block(b: &Value) -> Option<(String, Option<String>)> {
    let src = &b["source"];
    if src["type"] != "base64" {
        return None;
    }
    let data = src["data"].as_str()?;
    let name = name_for(data, ext_of(src["media_type"].as_str()?)?);
    let missing = !path(&name).exists();
    Some((name, missing.then(|| data.to_owned())))
}

/// Saves images read from a session file (see `from_block`), so their thumbnails can show.
pub fn save_all(images: &mut Vec<(String, String)>) {
    for (name, data) in images.drain(..) {
        if let Ok(bytes) = BASE64.decode(data) {
            let _ = write(&name, &bytes);
        }
    }
}

/// The prompt as a `claude -p --input-format stream-json` line: its text and images in order.
pub fn user_line(prompt: &str) -> Result<String, String> {
    let mut content = Vec::new();
    for part in parts(prompt) {
        match part {
            // The API refuses empty text blocks.
            Part::Text(t) if t.trim().is_empty() => {}
            Part::Text(t) => content.push(json!({"type": "text", "text": t})),
            Part::Image(name) => {
                let bytes = std::fs::read(path(name)).map_err(|_| format!("the image {name} is gone; remove it from the prompt"))?;
                let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
                content.push(json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": media_type(ext), "data": BASE64.encode(bytes)},
                }));
            }
        }
    }
    Ok(json!({"type": "user", "message": {"role": "user", "content": content}}).to_string())
}

/// Text and image markers joined back into one prompt, the way `user_line` split it.
pub fn join(pieces: &[String]) -> String {
    let mut out = String::new();
    let mut last_text = false;
    for p in pieces {
        let text = !p.starts_with(OPEN);
        if !out.is_empty() && !out.ends_with(char::is_whitespace) && !p.starts_with(char::is_whitespace) {
            out.push(if text && last_text { '\n' } else { ' ' });
        }
        out.push_str(p);
        last_text = text;
    }
    out
}

/// Opens an image in the default viewer (Preview, Photos).
pub fn open(name: &str) {
    crate::platform::open(path(name));
}

/// The text with its markers as chips, for the prompt input's layouter.
pub fn layout(text: &str, font: eframe::egui::FontId, color: eframe::egui::Color32, chip: eframe::egui::Color32, wrap: f32) -> eframe::egui::text::LayoutJob {
    use eframe::egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob { wrap: eframe::egui::text::TextWrapping { max_width: wrap, ..Default::default() }, ..Default::default() };
    let plain = TextFormat { font_id: font.clone(), color, ..Default::default() };
    let marked = TextFormat { font_id: font, color, background: chip, ..Default::default() };
    for part in parts(text) {
        match part {
            Part::Text(t) => job.append(t, 0.0, plain.clone()),
            Part::Image(n) => job.append(&marker(n), 0.0, marked.clone()),
        }
    }
    job
}

/// Cmd+V, as seen by the window before egui: egui only hears about pastes that carry text.
static PASTE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether Cmd+V was pressed since the last call.
#[cfg(not(windows))]
pub fn paste_pressed() -> bool {
    PASTE.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Whether Ctrl+V went down (in scope's window) since the last call. egui-winit swallows a
/// Ctrl+V with no text on the clipboard, so the keys themselves are asked.
#[cfg(windows)]
pub fn paste_pressed() -> bool {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_V};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    // SAFETY: plain queries of the keyboard and the foreground window.
    let down = unsafe { GetAsyncKeyState(VK_CONTROL as i32) < 0 && GetAsyncKeyState(VK_V as i32) < 0 };
    let was = PASTE.swap(down, Ordering::Relaxed);
    let mine = unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == std::process::id()
    };
    down && !was && mine
}

/// Starts watching for Cmd+V. Call on the main thread, before the window opens.
#[cfg(target_os = "macos")]
pub fn watch_paste() {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use std::ptr::NonNull;
    let block = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        let e = unsafe { event.as_ref() };
        let cmd = e.modifierFlags().contains(NSEventModifierFlags::Command);
        if cmd && e.charactersIgnoringModifiers().is_some_and(|c| c.to_string().eq_ignore_ascii_case("v")) {
            PASTE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        event.as_ptr()
    });
    // The monitor lives as long as the app.
    std::mem::forget(unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) });
}

#[cfg(not(target_os = "macos"))]
pub fn watch_paste() {}

/// What a paste carries, when it is images: copied image files (Finder), or image data
/// (screenshots, Copy Image) unless there is text too. Each saved; their markers.
/// None: nothing image-like, the paste is text.
#[cfg(target_os = "macos")]
pub fn from_clipboard() -> Option<Vec<Result<String, String>>> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSString, NSURL};
    let pb = NSPasteboard::generalPasteboard();
    let files: Vec<PathBuf> = pb
        .pasteboardItems()
        .into_iter()
        .flatten()
        .filter_map(|item| item.stringForType(&NSString::from_str("public.file-url")))
        .filter_map(|url| NSURL::URLWithString(&url)?.path().map(|p| PathBuf::from(p.to_string())))
        .filter(|p| is_image_file(p))
        .collect();
    if !files.is_empty() {
        return Some(files.iter().map(|f| add_file(f)).collect());
    }
    if pb.stringForType(&NSString::from_str("public.utf8-plain-text")).is_some_and(|s| !s.to_string().trim().is_empty()) {
        return None;
    }
    ["public.png", "public.jpeg", "public.tiff", "com.compuserve.gif"].iter().find_map(|t| {
        let data = pb.dataForType(&NSString::from_str(t))?;
        Some(vec![add(&data.to_vec())])
    })
}

/// What a paste carries, when it is images: copied image files (Explorer), or image data
/// (screenshots, Copy image) unless there is text too. Each saved; their markers.
/// None: nothing image-like, the paste is text.
#[cfg(windows)]
pub fn from_clipboard() -> Option<Vec<Result<String, String>>> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let files: Vec<PathBuf> = clipboard.get().file_list().unwrap_or_default().into_iter().filter(|p| is_image_file(p)).collect();
    if !files.is_empty() {
        return Some(files.iter().map(|f| add_file(f)).collect());
    }
    if clipboard.get_text().is_ok_and(|t| !t.trim().is_empty()) {
        return None;
    }
    let image = clipboard.get_image().ok()?;
    Some(vec![rgba_to_png(image.width, image.height, &image.bytes).and_then(|png| add(&png))])
}

/// Raw RGBA pixels (as the Windows clipboard gives them) as a PNG.
#[cfg_attr(not(windows), allow(dead_code))]
fn rgba_to_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let image = image::RgbaImage::from_raw(width as u32, height as u32, rgba.to_vec()).ok_or("the clipboard's image is incomplete")?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).map_err(|e| format!("can't encode the clipboard's image: {e}"))?;
    Ok(png.into_inner())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn from_clipboard() -> Option<Vec<Result<String, String>>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_split_the_text_and_junk_stays_text() {
        let t = "look [image:0a1b2c3d.png] and [image:ff.jpg]! not [image:x.png] or [image:12.exe] [image:";
        assert_eq!(
            parts(t),
            vec![
                Part::Text("look "),
                Part::Image("0a1b2c3d.png"),
                Part::Text(" and "),
                Part::Image("ff.jpg"),
                Part::Text("! not [image:x.png] or [image:12.exe] [image:"),
            ]
        );
        assert_eq!(names(t), ["0a1b2c3d.png", "ff.jpg"]);
        assert!(!has_images("no images [image:]"));
    }

    #[test]
    fn removing_an_image_takes_its_marker_and_space() {
        let t = "a [image:01.png] b [image:01.png] c";
        assert_eq!(remove(t, 1), "a [image:01.png] b c");
        assert_eq!(remove(t, 0), "a b [image:01.png] c");
        assert_eq!(remove(t, 5), t);
        assert_eq!(remove("x [image:01.png] [image:02.png]", 1), "x [image:01.png] ");
    }

    #[test]
    fn joining_gives_back_the_prompt_that_was_split() {
        let pieces: Vec<String> = ["what is ", "[image:01.png]", " here"].map(String::from).into();
        assert_eq!(join(&pieces), "what is [image:01.png] here");
        let pieces: Vec<String> = ["[Image #1] describe", "[image:01.png]", "[image:02.png]"].map(String::from).into();
        assert_eq!(join(&pieces), "[Image #1] describe [image:01.png] [image:02.png]");
        assert_eq!(join(&["one".into(), "two".into()]), "one\ntwo");
    }

    #[test]
    fn a_sent_image_and_its_saved_copy_have_one_name() {
        let png = {
            let img = image::RgbImage::from_pixel(4, 3, image::Rgb([255, 0, 0]));
            let mut out = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        };
        let (bytes, ext) = normalize(&png).unwrap();
        assert_eq!((bytes.as_slice(), ext), (png.as_slice(), "png"), "a small PNG is taken as it is");
        let block = json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": BASE64.encode(&png)}});
        let (name, _) = from_block(&block).unwrap();
        assert_eq!(name, name_for(&BASE64.encode(&png), "png"));
        assert!(normalize(b"not an image").is_err());
    }

    #[test]
    fn big_or_unusual_images_are_converted() {
        let img = image::RgbImage::from_pixel(3000, 10, image::Rgb([0, 0, 255]));
        let mut bmp = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageFormat::Bmp).unwrap();
        let (bytes, ext) = normalize(&bmp).unwrap();
        assert_eq!(ext, "png");
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!(back.width(), MAX_SIDE);
    }
}

#[cfg(test)]
mod clipboard_tests {
    #[test]
    fn clipboard_pixels_become_a_png() {
        let png = super::rgba_to_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (2, 1));
        assert_eq!(back.get_pixel(1, 0).0, [0, 255, 0, 255]);
        assert!(super::rgba_to_png(2, 2, &[0; 4]).is_err(), "too few bytes");
    }
}
