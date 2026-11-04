use eframe::egui::{Color32, Visuals};
use std::sync::RwLock;

/// Colors that egui's visuals don't cover, in a dark and a light variant.
#[derive(Clone, Copy)]
pub struct Palette {
    pub user: Color32,
    pub assistant: Color32,
    pub error: Color32,
    pub border: Color32,
    pub line_number: Color32,
    pub prompt_bg: Color32,
    pub prompt_text: Color32,
    pub prompt_sign: Color32,
    pub add_fg: Color32,
    pub del_fg: Color32,
    pub add_bg: Color32,
    pub del_bg: Color32,
    pub context: Color32,
    pub note: Color32,
    /// Find (Ctrl+F) matches: yellow, the current one orange, black text on both (in either
    /// theme, so they never look like the selection).
    pub find: crate::find::Colors,
}

const FIND: crate::find::Colors = crate::find::Colors {
    bg: Color32::from_rgb(255, 225, 0),
    current_bg: Color32::from_rgb(255, 150, 40),
    text: Color32::BLACK,
};

/// The palettes the style made (dark, light), set when settings are applied (style.rs).
static CURRENT: RwLock<Option<(Palette, Palette)>> = RwLock::new(None);

pub fn set_palettes(dark: Palette, light: Palette) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = Some((dark, light));
}

/// scope's colors for the theme `v` is in, as the style has them.
pub fn palette(v: &Visuals) -> Palette {
    match *CURRENT.read().unwrap_or_else(|e| e.into_inner()) {
        Some((dark, light)) => if v.dark_mode { dark } else { light },
        None => classic(v.dark_mode),
    }
}

impl Palette {
    /// A color by its style key (style.rs's COLOR_KEYS).
    pub fn get(&self, key: &str) -> Option<Color32> {
        Some(match key {
            "user" => self.user,
            "assistant" => self.assistant,
            "error" => self.error,
            "border" => self.border,
            "line_number" => self.line_number,
            "prompt_bg" => self.prompt_bg,
            "prompt_text" => self.prompt_text,
            "prompt_sign" => self.prompt_sign,
            "add_fg" => self.add_fg,
            "del_fg" => self.del_fg,
            "add_bg" => self.add_bg,
            "del_bg" => self.del_bg,
            "context" => self.context,
            "note" => self.note,
            _ => return None,
        })
    }
}

/// The colors scope always had: the "classic" style.
pub fn classic(dark: bool) -> Palette {
    if dark {
        Palette {
            user: Color32::from_rgb(120, 170, 255),
            assistant: Color32::from_rgb(230, 150, 90),
            error: Color32::LIGHT_RED,
            border: Color32::from_gray(60),
            line_number: Color32::from_gray(90),
            prompt_bg: Color32::from_rgb(14, 14, 16),
            prompt_text: Color32::from_gray(230),
            prompt_sign: Color32::from_rgb(110, 210, 120),
            add_fg: Color32::from_rgb(110, 210, 120),
            del_fg: Color32::from_rgb(240, 110, 110),
            add_bg: Color32::from_rgb(28, 48, 32),
            del_bg: Color32::from_rgb(56, 28, 30),
            context: Color32::GRAY,
            note: Color32::from_rgb(200, 170, 90),
            find: FIND,
        }
    } else {
        Palette {
            user: Color32::from_rgb(30, 90, 200),
            assistant: Color32::from_rgb(190, 90, 20),
            error: Color32::from_rgb(190, 30, 30),
            border: Color32::from_gray(200),
            line_number: Color32::from_gray(150),
            prompt_bg: Color32::from_gray(250),
            prompt_text: Color32::from_gray(20),
            prompt_sign: Color32::from_rgb(20, 140, 50),
            add_fg: Color32::from_rgb(20, 130, 40),
            del_fg: Color32::from_rgb(190, 40, 40),
            add_bg: Color32::from_rgb(220, 245, 222),
            del_bg: Color32::from_rgb(252, 225, 225),
            context: Color32::from_gray(110),
            note: Color32::from_rgb(170, 130, 30),
            find: FIND,
        }
    }
}
