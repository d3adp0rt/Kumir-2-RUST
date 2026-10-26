//! Persisted IDE settings: colour scheme, editor behaviour, recent files and
//! the last session.
//!
//! The file is a simple `key=value` text file (no extra dependencies), written
//! atomically next to the user's configuration directory.

use std::path::{Path, PathBuf};

use kumir_core::actors::TurtleBounds;

use crate::theme::{Palette, Scheme};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct Settings {
    pub scheme: Scheme,
    pub custom_palette: Palette,
    pub font_size: f32,
    pub editor_font_size: f32,
    pub ui_scale: f32,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub highlight_current_line: bool,
    pub indent_guides: bool,
    pub auto_indent: bool,
    pub tab_width: usize,
    pub autocomplete: bool,
    pub inline_suggestions: bool,
    pub auto_save: bool,
    pub auto_open_output: bool,
    pub auto_open_errors: bool,
    pub animate_actors: bool,
    pub actor_delay_ms: u64,
    pub turtle_bounds: TurtleBounds,
    pub recent: Vec<PathBuf>,
    pub session: Vec<PathBuf>,
    pub last_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scheme: Scheme::Dark,
            custom_palette: Palette::dark(),
            font_size: 14.0,
            editor_font_size: 14.0,
            ui_scale: 1.0,
            show_line_numbers: true,
            word_wrap: false,
            highlight_current_line: true,
            indent_guides: false,
            auto_indent: true,
            tab_width: 4,
            autocomplete: true,
            inline_suggestions: true,
            auto_save: false,
            auto_open_output: false,
            auto_open_errors: true,
            animate_actors: true,
            actor_delay_ms: 30,
            turtle_bounds: TurtleBounds::Refuse,
            recent: Vec::new(),
            session: Vec::new(),
            last_dir: None,
        }
    }
}

impl Settings {
    /// The active palette: a built-in scheme or the user's own colours.
    pub fn palette(&self) -> Palette {
        match self.scheme {
            Scheme::Custom => self.custom_palette,
            other => other.palette(),
        }
    }

    pub fn set_scheme(&mut self, scheme: Scheme) {
        // Keep the current look as the starting point of the custom scheme.
        if scheme == Scheme::Custom && self.custom_palette.dark != self.palette().dark {
            self.custom_palette.dark = self.palette().dark;
        }
        self.scheme = scheme;
    }

    pub fn load() -> Self {
        let mut settings = Settings::default();
        let Some(path) = config_path() else {
            return settings;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return settings;
        };
        settings.apply(&text);
        settings
    }

    pub fn apply(&mut self, text: &str) {
        let mut palette_lines = String::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r').trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            if let Some(role) = key.strip_prefix("palette.") {
                palette_lines.push_str(&format!("{role}={value}\n"));
                continue;
            }
            match key {
                "scheme" => {
                    if let Some(scheme) = Scheme::from_id(value) {
                        self.scheme = scheme;
                    }
                }
                "font_size" => self.font_size = parse_f32(value, self.font_size).clamp(9.0, 40.0),
                "editor_font_size" => {
                    self.editor_font_size = parse_f32(value, self.editor_font_size).clamp(9.0, 40.0)
                }
                "ui_scale" => self.ui_scale = parse_f32(value, self.ui_scale).clamp(0.55, 3.0),
                "line_numbers" => self.show_line_numbers = parse_bool(value, true),
                "word_wrap" => self.word_wrap = parse_bool(value, false),
                "highlight_current_line" => self.highlight_current_line = parse_bool(value, true),
                "indent_guides" => self.indent_guides = parse_bool(value, false),
                "auto_indent" => self.auto_indent = parse_bool(value, true),
                "tab_width" => self.tab_width = value.parse::<usize>().unwrap_or(4).clamp(1, 12),
                "autocomplete" => self.autocomplete = parse_bool(value, true),
                "inline_suggestions" => self.inline_suggestions = parse_bool(value, true),
                "auto_save" => self.auto_save = parse_bool(value, false),
                "auto_open_output" => self.auto_open_output = parse_bool(value, false),
                "auto_open_errors" => self.auto_open_errors = parse_bool(value, true),
                "animate_actors" => self.animate_actors = parse_bool(value, true),
                "actor_delay_ms" => {
                    self.actor_delay_ms = value.parse::<u64>().unwrap_or(30).min(1000)
                }
                "turtle_bounds" => {
                    self.turtle_bounds = if value == "free" {
                        TurtleBounds::Free
                    } else {
                        TurtleBounds::Refuse
                    }
                }
                "recent" => push_unique(&mut self.recent, value),
                "session" => push_unique(&mut self.session, value),
                "last_dir" => self.last_dir = Some(PathBuf::from(value)),
                _ => {}
            }
        }
        if !palette_lines.is_empty() {
            let mut custom = self.custom_palette;
            custom.apply_text(&palette_lines);
            self.custom_palette = custom;
        }
        self.recent.truncate(20);
        self.session.truncate(40);
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "# КуМир — настройки интерфейса\nversion={CONFIG_VERSION}\nscheme={}\n",
            self.scheme.id()
        );
        for (key, value) in [
            ("font_size", format!("{}", self.font_size)),
            ("editor_font_size", format!("{}", self.editor_font_size)),
            ("ui_scale", format!("{}", self.ui_scale)),
            ("line_numbers", bool_str(self.show_line_numbers)),
            ("word_wrap", bool_str(self.word_wrap)),
            (
                "highlight_current_line",
                bool_str(self.highlight_current_line),
            ),
            ("indent_guides", bool_str(self.indent_guides)),
            ("auto_indent", bool_str(self.auto_indent)),
            ("tab_width", self.tab_width.to_string()),
            ("autocomplete", bool_str(self.autocomplete)),
            ("inline_suggestions", bool_str(self.inline_suggestions)),
            ("auto_save", bool_str(self.auto_save)),
            ("auto_open_output", bool_str(self.auto_open_output)),
            ("auto_open_errors", bool_str(self.auto_open_errors)),
            ("animate_actors", bool_str(self.animate_actors)),
            ("actor_delay_ms", self.actor_delay_ms.to_string()),
            (
                "turtle_bounds",
                match self.turtle_bounds {
                    TurtleBounds::Free => "free".to_string(),
                    TurtleBounds::Refuse => "refuse".to_string(),
                },
            ),
        ] {
            out.push_str(&format!("{key}={value}\n"));
        }
        if let Some(dir) = &self.last_dir {
            out.push_str(&format!("last_dir={}\n", dir.display()));
        }
        for path in &self.recent {
            out.push_str(&format!("recent={}\n", path.display()));
        }
        for path in &self.session {
            out.push_str(&format!("session={}\n", path.display()));
        }
        out.push_str("\n# Пользовательская цветовая схема\n");
        for line in self.custom_palette.to_text().lines() {
            out.push_str(&format!("palette.{line}\n"));
        }
        out
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path =
            config_path().ok_or_else(|| "Не удалось определить каталог настроек".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Не удалось создать каталог «{}»: {e}", parent.display()))?;
        }
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, self.to_text())
            .map_err(|e| format!("Не удалось записать настройки: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("Не удалось сохранить настройки: {e}"))?;
        Ok(path)
    }

    pub fn remember_recent(&mut self, path: &Path) {
        let path = path.to_path_buf();
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(20);
    }
}

fn push_unique(list: &mut Vec<PathBuf>, value: &str) {
    let path = PathBuf::from(value);
    if path.as_os_str().is_empty() {
        return;
    }
    if !list.contains(&path) {
        list.push(path);
    }
}

fn bool_str(value: bool) -> String {
    if value {
        "1".to_string()
    } else {
        "0".to_string()
    }
}

fn parse_bool(value: &str, default: bool) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn parse_f32(value: &str, default: f32) -> f32 {
    let parsed = value.replace(',', ".").parse::<f32>().unwrap_or(default);
    if parsed.is_finite() {
        parsed
    } else {
        default
    }
}

/// Directory of the settings file.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        if let Some(dir) = crate::android_platform::config_dir() {
            return Some(dir);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return Some(PathBuf::from(appdata).join("Kumir"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("kumir"),
            );
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("kumir"));
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return Some(PathBuf::from(home).join(".config").join("kumir"));
            }
        }
    }
    Some(std::env::current_dir().ok()?.join(".kumir"))
}

pub fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("kumir.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn settings_roundtrip() {
        let mut settings = Settings::default();
        settings.font_size = 18.0;
        settings.auto_indent = false;
        settings.turtle_bounds = TurtleBounds::Free;
        settings.recent = vec![PathBuf::from("/tmp/a.kum"), PathBuf::from("/tmp/b.kum")];
        settings.custom_palette.accent = eframe::egui::Color32::from_rgb(1, 2, 3);
        let text = settings.to_text();
        let mut parsed = Settings::default();
        parsed.apply(&text);
        assert_eq!(parsed.font_size, 18.0);
        assert!(!parsed.auto_indent);
        assert_eq!(parsed.turtle_bounds, TurtleBounds::Free);
        assert_eq!(parsed.recent.len(), 2);
        assert_eq!(parsed.custom_palette.accent, settings.custom_palette.accent);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let mut settings = Settings::default();
        settings.apply("unknown=42\nfont_size=abc\nscheme=nope\n");
        assert_eq!(settings.font_size, 14.0);
        assert_eq!(settings.scheme, Scheme::Dark);
    }

    #[test]
    fn scheme_switch_keeps_custom_palette() {
        let mut settings = Settings::default();
        let current = settings.palette();
        settings.set_scheme(Scheme::Custom);
        assert_eq!(settings.custom_palette.dark, current.dark);
    }
}
