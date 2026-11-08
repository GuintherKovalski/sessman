//! How scope looks, beyond dark / light: a preset (a set of colors for each theme), an accent,
//! corner roundness, spacing, text size, and any single color overridden. Kept in
//! settings.json under `style`, so it applies at once in every scope on the config folder;
//! `scope style` (below) lets Claude, asked by the developer, read and change it.
use crate::theme::Palette;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Theme, Visuals};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Style {
    /// One of [`PRESETS`]; anything else is "classic".
    pub preset: String,
    /// "#rrggbb", or empty for the preset's.
    pub accent: String,
    /// Corners of buttons, fields and menus, in points.
    pub radius: f32,
    /// Space between and inside widgets, as a factor of the default.
    pub spacing: f32,
    /// Size of the proportional text (body and buttons; headings and small text follow).
    pub text_size: f32,
    /// Single colors: a key of [`COLOR_KEYS`], or `dark.<key>` / `light.<key>` for one theme
    /// only, to "#rrggbb".
    pub colors: BTreeMap<String, String>,
}

impl Default for Style {
    fn default() -> Self {
        Self { preset: "classic".into(), accent: String::new(), radius: 2.0, spacing: 1.0, text_size: 13.0, colors: BTreeMap::new() }
    }
}

/// The colors a preset sets for one theme; everything else is mixed from them.
#[derive(Clone, Copy)]
struct Tint {
    background: Color32,
    text: Color32,
    accent: Color32,
    /// "Claude" in the conversation.
    assistant: Color32,
    /// The input's `>` sign.
    sign: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const fn tint(background: u32, text: u32, accent: u32, assistant: u32, sign: u32) -> Tint {
    Tint { background: hex(background), text: hex(text), accent: hex(accent), assistant: hex(assistant), sign: hex(sign) }
}

/// Name, what it looks like, and its dark and light tints. "classic" has none: egui's own
/// look, as scope always had it.
pub const PRESETS: &[(&str, &str)] = &[
    ("classic", "scope's original grey look"),
    ("midnight", "deep blue-black, sky blue accent"),
    ("forest", "green-tinted, leaf accent"),
    ("ember", "warm brown, orange accent"),
    ("paper", "cream and ink, ochre accent"),
    ("violet", "purple-tinted, lavender accent"),
    ("slate", "cool blue-grey, teal accent"),
    ("contrast", "black and white, strong accents"),
];

fn tints(preset: &str) -> Option<(Tint, Tint)> {
    Some(match preset {
        "midnight" => (tint(0x0f1420, 0xd6deeb, 0x5fb3ff, 0xe0a96d, 0x7fdbca), tint(0xeef3fb, 0x1b2433, 0x1f6fd1, 0xb06a1c, 0x11806a)),
        "forest" => (tint(0x141c17, 0xd7e3d6, 0x7fc97f, 0xe3b462, 0x9bd770), tint(0xf1f5ee, 0x1d2a1f, 0x2f7d3a, 0xa8641a, 0x3b8a2e)),
        "ember" => (tint(0x1d1714, 0xeadbd0, 0xff8a4c, 0xf2c46d, 0xffb36b), tint(0xfbf3ec, 0x2e211a, 0xc2541b, 0x8f5a12, 0xb5651d)),
        "paper" => (tint(0x22201c, 0xe6dfcf, 0xc9a45c, 0xd38b5d, 0xa8b86b), tint(0xf7f1e3, 0x3b352b, 0x8a6a2b, 0x9a4f22, 0x6b7f2a)),
        "violet" => (tint(0x18151f, 0xe2dcf0, 0xb28cff, 0xf0a6ca, 0x8be0c0), tint(0xf5f2fb, 0x251f33, 0x6a3fd1, 0xb03f7a, 0x237a5e)),
        "slate" => (tint(0x1a2027, 0xd5dde5, 0x4fd1c5, 0xf0b27a, 0x7ee0a1), tint(0xeef2f5, 0x1e2a35, 0x137f78, 0xa35a1c, 0x1f7a45)),
        "contrast" => (tint(0x000000, 0xffffff, 0xffd400, 0xff9e3d, 0x3dff7a), tint(0xffffff, 0x000000, 0x0040c0, 0xa03c00, 0x006b1f)),
        _ => return None,
    })
}

/// "classic" as a tint: used only when one of its base colors is overridden.
const CLASSIC: (Tint, Tint) = (tint(0x1b1b1b, 0xdcdcdc, 0x5aaaff, 0xe6965a, 0x6ed278), tint(0xf8f8f8, 0x1c1c1c, 0x1e5ac8, 0xbe5a14, 0x148c32));

/// Every color that can be set on its own, and what it colors.
pub const COLOR_KEYS: &[(&str, &str)] = &[
    ("background", "panels and windows (the rest of the surfaces are mixed from it)"),
    ("text", "text and widget outlines"),
    ("accent", "selection, links, the open tab's mark (same as setting accent)"),
    ("user", "\"You\" and the active tab's marks"),
    ("assistant", "\"Claude\""),
    ("error", "errors"),
    ("border", "frames around messages and cards"),
    ("line_number", "line numbers in files"),
    ("prompt_bg", "the prompt input's background"),
    ("prompt_text", "the prompt input's text"),
    ("prompt_sign", "the input's > sign"),
    ("add_fg", "added lines' text in diffs"),
    ("del_fg", "removed lines' text in diffs"),
    ("add_bg", "added lines' background"),
    ("del_bg", "removed lines' background"),
    ("context", "unchanged lines in diffs"),
    ("note", "notes and pinned frames"),
];

/// Parses "#rgb", "#rrggbb" (the # optional) into a color.
pub fn parse_color(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    let digits = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => h.to_owned(),
        _ => return None,
    };
    let v = u32::from_str_radix(&digits, 16).ok()?;
    Some(hex(v))
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// `a` moved `t` (0..1) of the way to `b`.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl Style {
    fn known_preset(&self) -> &str {
        if tints(&self.preset).is_some() { &self.preset } else { "classic" }
    }

    /// The override for `key` in this theme: `dark.key` / `light.key` before plain `key`.
    fn color(&self, key: &str, dark: bool) -> Option<Color32> {
        let mode = if dark { "dark" } else { "light" };
        self.colors.get(&format!("{mode}.{key}")).or_else(|| self.colors.get(key)).and_then(|c| parse_color(c))
    }

    fn accent_override(&self, dark: bool) -> Option<Color32> {
        self.color("accent", dark).or_else(|| parse_color(&self.accent))
    }

    /// The preset's tint with the base colors overridden; None for an untouched classic look.
    fn tint(&self, dark: bool) -> Option<Tint> {
        let base = tints(self.known_preset());
        let touched = ["background", "text"].iter().any(|k| self.color(k, dark).is_some());
        let (d, l) = match base {
            Some(t) => t,
            None if touched => CLASSIC,
            None => return None,
        };
        let mut t = if dark { d } else { l };
        if let Some(c) = self.color("background", dark) {
            t.background = c;
        }
        if let Some(c) = self.color("text", dark) {
            t.text = c;
        }
        if let Some(c) = self.accent_override(dark) {
            t.accent = c;
        }
        Some(t)
    }

    /// egui's colors for this theme: its own (classic), or mixed from the tint.
    pub fn visuals(&self, dark: bool) -> Visuals {
        let mut v = if dark { Visuals::dark() } else { Visuals::light() };
        let Some(t) = self.tint(dark) else {
            if let Some(a) = self.accent_override(dark) {
                v.hyperlink_color = a;
                v.selection.bg_fill = mix(v.panel_fill, a, 0.5);
            }
            return v;
        };
        let (bg, text) = (t.background, t.text);
        let at = |k: f32| mix(bg, text, k);
        v.panel_fill = bg;
        v.window_fill = bg;
        v.faint_bg_color = at(0.03);
        v.extreme_bg_color = if dark { mix(bg, Color32::BLACK, 0.45) } else { mix(bg, Color32::WHITE, 0.7) };
        v.code_bg_color = at(0.12);
        v.window_stroke = Stroke::new(1.0, at(0.16));
        v.hyperlink_color = t.accent;
        v.selection.bg_fill = mix(bg, t.accent, 0.45);
        v.selection.stroke = Stroke::new(1.0, mix(t.accent, text, 0.5));
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = bg;
        w.noninteractive.weak_bg_fill = bg;
        w.noninteractive.bg_stroke = Stroke::new(1.0, at(0.16));
        w.noninteractive.fg_stroke = Stroke::new(1.0, at(0.7));
        for (wv, fill, fg) in [(&mut w.inactive, 0.13, 0.85), (&mut w.hovered, 0.19, 1.0), (&mut w.active, 0.24, 1.0), (&mut w.open, 0.09, 0.85)] {
            wv.bg_fill = at(fill);
            wv.weak_bg_fill = at(fill);
            wv.fg_stroke = Stroke::new(wv.fg_stroke.width, at(fg));
        }
        w.hovered.bg_stroke = Stroke::new(1.0, at(0.5));
        w.active.bg_stroke = Stroke::new(1.0, t.accent);
        v
    }

    /// scope's own colors for this theme: the classic ones, the tint's, then each override.
    pub fn palette(&self, dark: bool) -> Palette {
        let mut p = crate::theme::classic(dark);
        if let Some(t) = self.tint(dark) {
            let (bg, text) = (t.background, t.text);
            let at = |k: f32| mix(bg, text, k);
            p.user = t.accent;
            p.assistant = t.assistant;
            p.border = at(0.2);
            p.line_number = at(0.35);
            p.prompt_bg = if dark { mix(bg, Color32::BLACK, 0.45) } else { mix(bg, Color32::WHITE, 0.7) };
            p.prompt_text = text;
            p.prompt_sign = t.sign;
            p.context = at(0.55);
            p.add_bg = mix(bg, p.add_fg, 0.18);
            p.del_bg = mix(bg, p.del_fg, 0.18);
        } else if let Some(a) = self.accent_override(dark) {
            p.user = a;
        }
        for (key, _) in COLOR_KEYS {
            let Some(c) = self.color(key, dark) else { continue };
            let slot = match *key {
                "user" => &mut p.user,
                "assistant" => &mut p.assistant,
                "error" => &mut p.error,
                "border" => &mut p.border,
                "line_number" => &mut p.line_number,
                "prompt_bg" => &mut p.prompt_bg,
                "prompt_text" => &mut p.prompt_text,
                "prompt_sign" => &mut p.prompt_sign,
                "add_fg" => &mut p.add_fg,
                "del_fg" => &mut p.del_fg,
                "add_bg" => &mut p.add_bg,
                "del_bg" => &mut p.del_bg,
                "context" => &mut p.context,
                "note" => &mut p.note,
                _ => continue,
            };
            *slot = c;
        }
        p
    }

    /// The accent in this theme, whatever set it (for the Settings page's color button).
    pub fn accent_shown(&self, dark: bool) -> Color32 {
        self.palette(dark).user
    }

    /// Sets egui's colors, corners, spacing and text sizes for both themes, and scope's
    /// palette.
    pub fn apply(&self, ctx: &egui::Context) {
        let r = self.radius.clamp(0.0, 16.0);
        let corner = |x: f32| CornerRadius::same(x.round() as u8);
        let spacing = self.spacing.clamp(0.5, 2.5);
        let scale = self.text_size.clamp(8.0, 32.0) / 13.0;
        for (theme, dark) in [(Theme::Dark, true), (Theme::Light, false)] {
            ctx.style_mut_of(theme, |s| {
                let mut v = self.visuals(dark);
                for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.active, &mut v.widgets.open] {
                    w.corner_radius = corner(r);
                }
                v.widgets.hovered.corner_radius = corner(r + 1.0);
                v.window_corner_radius = corner(r + 4.0);
                v.menu_corner_radius = corner(r + 4.0);
                s.visuals = v;
                let d = egui::style::Spacing::default();
                s.spacing.item_spacing = d.item_spacing * spacing;
                s.spacing.button_padding = d.button_padding * spacing;
                for (ts, font) in egui::style::default_text_styles() {
                    if ts != egui::TextStyle::Monospace {
                        s.text_styles.insert(ts, egui::FontId::new(font.size * scale, font.family));
                    }
                }
            });
        }
        crate::theme::set_palettes(self.palette(true), self.palette(false));
    }
}

/// `scope style ...`: shows the style, or changes it (and the theme, zoom and monospace size,
/// which are settings of their own) and says what changed. `args` come after `style`.
pub fn command(s: &mut crate::settings::Settings, args: &[String]) -> Result<String, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["show"] => return Ok(describe(s)),
        ["presets"] => return Ok(presets_text()),
        ["help"] => return Ok(HELP.to_owned()),
        ["reset"] => {
            s.style = Style::default();
            return Ok(format!("Style reset to classic.\n\n{}", describe(s)));
        }
        ["reset", keys @ ..] => {
            let mut done = Vec::new();
            for k in keys {
                done.push(reset(s, k)?);
            }
            return Ok(format!("Reset {}.", done.join(", ")));
        }
        ["preset", name] => return set(s, &[("preset", name)]),
        [name] if tints(&name.to_lowercase()).is_some() || name.eq_ignore_ascii_case("classic") => return set(s, &[("preset", name)]),
        _ => {}
    }
    let rest = if words.first() == Some(&"set") { &words[1..] } else { &words[..] };
    let mut pairs = Vec::new();
    let mut it = rest.iter();
    while let Some(w) = it.next() {
        match w.split_once('=') {
            Some((k, v)) => pairs.push((k, v)),
            None => pairs.push((w, it.next().ok_or_else(|| format!("{w}: what value?\n\n{HELP}"))?)),
        }
    }
    if pairs.is_empty() {
        return Err(HELP.to_owned());
    }
    set(s, &pairs)
}

/// Sets each key (all or none: a bad one changes nothing) and says what it did.
fn set(s: &mut crate::settings::Settings, pairs: &[(&str, &str)]) -> Result<String, String> {
    let mut next = s.clone();
    let mut done = Vec::new();
    for (key, value) in pairs {
        let key = key.trim().to_lowercase().replace('-', "_");
        let value = value.trim();
        let num = |lo: f32, hi: f32| -> Result<f32, String> {
            let n: f32 = value.parse().map_err(|_| format!("{key}: \"{value}\" is not a number"))?;
            if (lo..=hi).contains(&n) { Ok(n) } else { Err(format!("{key}: {value} is out of range ({lo} to {hi})")) }
        };
        match key.as_str() {
            "preset" => {
                let p = value.to_lowercase();
                if !PRESETS.iter().any(|(n, _)| *n == p) {
                    return Err(format!("no preset is called \"{value}\"\n\n{}", presets_text()));
                }
                next.style.preset = p;
            }
            "theme" => {
                let t = value.to_lowercase();
                if !["dark", "light", "system"].contains(&t.as_str()) {
                    return Err(format!("theme: dark, light or system, not \"{value}\""));
                }
                next.theme = t;
            }
            "accent" if matches!(value, "" | "none" | "default") => next.style.accent.clear(),
            "accent" => next.style.accent = to_hex(parse_color(value).ok_or_else(|| bad_color(&key, value))?),
            "radius" | "corner_radius" | "roundness" => next.style.radius = num(0.0, 16.0)?,
            "spacing" => next.style.spacing = num(0.5, 2.5)?,
            "text_size" | "font_size" => next.style.text_size = num(8.0, 32.0)?,
            "mono_size" => next.mono_size = num(8.0, 32.0)?,
            "zoom" => next.zoom = num(0.5, 3.0)?,
            k => {
                let bare = k.strip_prefix("dark.").or_else(|| k.strip_prefix("light.")).unwrap_or(k);
                if !COLOR_KEYS.iter().any(|(n, _)| *n == bare) {
                    return Err(format!("no style key is called \"{key}\"\n\n{HELP}"));
                }
                if matches!(value, "" | "none" | "default") {
                    next.style.colors.remove(k);
                } else {
                    let c = parse_color(value).ok_or_else(|| bad_color(k, value))?;
                    next.style.colors.insert(k.to_owned(), to_hex(c));
                }
            }
        }
        done.push(format!("{key} = {value}"));
    }
    *s = next;
    Ok(format!("Set {}. It shows now in every scope window.", done.join(", ")))
}

fn bad_color(key: &str, value: &str) -> String {
    format!("{key}: \"{value}\" is not a color; use hex like #ff8800 (or none to drop it)")
}

/// Puts one key back to its default.
fn reset(s: &mut crate::settings::Settings, key: &str) -> Result<String, String> {
    let key = key.trim().to_lowercase().replace('-', "_");
    let d = Style::default();
    match key.as_str() {
        "preset" => s.style.preset = d.preset,
        "accent" => {
            s.style.accent.clear();
            s.style.colors.retain(|k, _| !k.ends_with("accent"));
        }
        "radius" | "corner_radius" | "roundness" => s.style.radius = d.radius,
        "spacing" => s.style.spacing = d.spacing,
        "text_size" | "font_size" => s.style.text_size = d.text_size,
        "colors" => s.style.colors.clear(),
        "theme" => s.theme = crate::settings::Settings::default().theme,
        "mono_size" => s.mono_size = crate::settings::Settings::default().mono_size,
        "zoom" => s.zoom = 1.0,
        k => {
            let bare = k.strip_prefix("dark.").or_else(|| k.strip_prefix("light.")).unwrap_or(k);
            if !COLOR_KEYS.iter().any(|(n, _)| *n == bare) {
                return Err(format!("no style key is called \"{key}\"\n\n{HELP}"));
            }
            if bare == k {
                s.style.colors.retain(|n, _| n != k && n != &format!("dark.{k}") && n != &format!("light.{k}"));
            } else {
                s.style.colors.remove(k);
            }
        }
    }
    Ok(key)
}

fn presets_text() -> String {
    let mut out = String::from("Presets (each has a dark and a light variant; the theme picks which):\n");
    for (name, what) in PRESETS {
        out.push_str(&format!("  {name:<10} {what}\n"));
    }
    out.trim_end().to_owned()
}

pub const HELP: &str = "usage:
  scope style                          the current style, its keys and every color
  scope style <preset>                 switch preset (scope style presets lists them)
  scope style set <key> <value> ...    or key=value; several at once
  scope style reset [key ...]          everything, or only those keys
keys: preset, theme (dark|light|system), accent, radius (0-16), spacing (0.5-2.5),
      text_size (8-32), mono_size (8-32), zoom (0.5-3), and any color key below
colors are hex (#ff8800); prefix a color key with dark. or light. for one theme only;
the value none drops an override.";

/// The style as `scope style` shows it: settings, presets, and each color in both themes.
fn describe(s: &crate::settings::Settings) -> String {
    let st = &s.style;
    let mut out = format!("Style (in {}, applied live):\n", crate::settings::path().display());
    let line = |out: &mut String, k: &str, v: String, note: &str| out.push_str(&format!("  {k:<11} {v:<9} {note}\n"));
    let names: Vec<&str> = PRESETS.iter().map(|p| p.0).collect();
    line(&mut out, "preset", st.known_preset().to_owned(), &format!("({})", names.join(", ")));
    line(&mut out, "theme", s.theme.clone(), "(dark, light, system)");
    line(&mut out, "accent", if st.accent.is_empty() { "-".into() } else { st.accent.clone() }, "(- = the preset's)");
    line(&mut out, "radius", format!("{}", st.radius), "(corners, 0-16)");
    line(&mut out, "spacing", format!("{}", st.spacing), "(0.5-2.5)");
    line(&mut out, "text_size", format!("{}", st.text_size), "(8-32)");
    line(&mut out, "mono_size", format!("{}", s.mono_size), "(input, code, files; 8-32)");
    line(&mut out, "zoom", format!("{}", s.zoom), "(0.5-3)");
    out.push_str("\nColors now (dark / light), * = overridden:\n");
    let (vd, vl) = (st.visuals(true), st.visuals(false));
    let (pd, pl) = (st.palette(true), st.palette(false));
    for (key, what) in COLOR_KEYS {
        let pick = |v: &Visuals, p: &Palette| match *key {
            "background" => v.panel_fill,
            "text" => v.widgets.inactive.fg_stroke.color,
            "accent" => v.hyperlink_color,
            _ => p.get(key).unwrap_or_default(),
        };
        let star = |dark: bool| if st.color(key, dark).is_some() { "*" } else { " " };
        out.push_str(&format!("  {key:<12} {}{} / {}{}  {what}\n", to_hex(pick(&vd, &pd)), star(true), to_hex(pick(&vl, &pl)), star(false)));
    }
    out.push('\n');
    out.push_str(HELP);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn run(s: &mut Settings, args: &str) -> Result<String, String> {
        command(s, &args.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
    }

    #[test]
    fn colors_parse_short_and_long_hex() {
        assert_eq!(parse_color("#f80"), Some(Color32::from_rgb(255, 136, 0)));
        assert_eq!(parse_color("FF8800"), Some(Color32::from_rgb(255, 136, 0)));
        assert_eq!(parse_color("orange"), None);
        assert_eq!(to_hex(Color32::from_rgb(1, 2, 255)), "#0102ff");
    }

    #[test]
    fn classic_keeps_eguis_look_and_scopes_palette() {
        let st = Style::default();
        assert_eq!(st.visuals(true), Visuals::dark());
        assert_eq!(st.visuals(false), Visuals::light());
        assert_eq!(st.palette(true).user, crate::theme::classic(true).user);
    }

    #[test]
    fn a_preset_colors_both_themes() {
        let st = Style { preset: "ember".into(), ..Style::default() };
        assert_eq!(st.visuals(true).panel_fill, hex(0x1d1714));
        assert_eq!(st.visuals(false).panel_fill, hex(0xfbf3ec));
        assert_eq!(st.palette(true).user, hex(0xff8a4c));
        assert!(st.visuals(true).dark_mode && !st.visuals(false).dark_mode);
    }

    #[test]
    fn overrides_go_per_theme_before_both() {
        let mut st = Style::default();
        st.colors.insert("background".into(), "#102030".into());
        st.colors.insert("light.background".into(), "#fafafa".into());
        st.colors.insert("user".into(), "#00ff00".into());
        assert_eq!(st.visuals(true).panel_fill, hex(0x102030), "a classic background override tints it");
        assert_eq!(st.visuals(false).panel_fill, hex(0xfafafa));
        assert_eq!(st.palette(false).user, hex(0x00ff00));
    }

    #[test]
    fn the_command_sets_resets_and_refuses_bad_values() {
        let mut s = Settings::default();
        assert!(run(&mut s, "").unwrap().contains("preset      classic"));
        run(&mut s, "midnight").unwrap();
        assert_eq!(s.style.preset, "midnight");
        run(&mut s, "set accent #f80 radius 6 theme=dark dark.prompt_bg #000").unwrap();
        assert_eq!((s.style.accent.as_str(), s.style.radius, s.theme.as_str()), ("#ff8800", 6.0, "dark"));
        assert_eq!(s.style.colors["dark.prompt_bg"], "#000000");
        let before = s.clone();
        assert!(run(&mut s, "set spacing 1.5 radius 99").unwrap_err().contains("out of range"));
        assert!(run(&mut s, "set wobble 3").unwrap_err().contains("no style key"));
        assert!(run(&mut s, "set user purple").unwrap_err().contains("not a color"));
        assert!(run(&mut s, "preset nope").unwrap_err().contains("no preset"));
        assert_eq!(s, before, "a refused change changes nothing");
        run(&mut s, "reset accent prompt_bg").unwrap();
        assert!(s.style.accent.is_empty() && s.style.colors.is_empty());
        run(&mut s, "reset").unwrap();
        assert_eq!(s.style, Style::default());
        assert_eq!(s.theme, "dark", "the theme is a setting of its own");
    }

    #[test]
    fn describe_lists_every_color_key() {
        let s = Settings::default();
        let text = describe(&s);
        for (k, _) in COLOR_KEYS {
            assert!(text.contains(&format!("  {k} ")), "{k} missing:\n{text}");
        }
    }
}
