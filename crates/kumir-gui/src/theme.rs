//! Visual design of the IDE: colour schemes, fonts and the global egui style.

use egui::{
    Color32, Context, CornerRadius, FontData, FontFamily, FontId, FontInsert, FontPriority,
    InsertFontFamily, Stroke,
};

/// Every colour role of the interface.  The list is also used by the custom
/// scheme editor and by the settings file.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub toolbar: Color32,
    pub header: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub icon: Color32,
    pub accent: Color32,
    pub accent_weak: Color32,
    pub accent_text: Color32,
    pub selection: Color32,
    pub keyword: Color32,
    pub type_: Color32,
    pub builtin: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub ident: Color32,
    pub call: Color32,
    pub run_line: Color32,
    pub find_match: Color32,
    pub error: Color32,
    pub warning: Color32,
    pub ok: Color32,
    pub gutter: Color32,
    pub gutter_active: Color32,
    // Actor canvases.
    pub field_bg: Color32,
    pub field_cell: Color32,
    pub field_wall: Color32,
    pub field_painted: Color32,
    pub field_mark: Color32,
    pub sand: Color32,
    pub water: Color32,
    pub paper: Color32,
}

/// Names of the palette fields (in the order used by the editor and by the
/// saved scheme files).
pub const PALETTE_ROLES: &[(&str, &str)] = &[
    ("bg", "Фон"),
    ("panel", "Панели"),
    ("panel_alt", "Панели (альтернативный)"),
    ("toolbar", "Панель инструментов"),
    ("header", "Заголовки"),
    ("border", "Границы"),
    ("text", "Текст"),
    ("muted", "Приглушённый текст"),
    ("icon", "Иконки"),
    ("accent", "Акцент"),
    ("accent_weak", "Акцент (слабый)"),
    ("accent_text", "Текст на акценте"),
    ("selection", "Выделение"),
    ("keyword", "Ключевые слова"),
    ("type_", "Типы"),
    ("builtin", "Стандартные функции"),
    ("string", "Строки"),
    ("number", "Числа"),
    ("comment", "Комментарии"),
    ("ident", "Идентификаторы"),
    ("call", "Вызовы"),
    ("run_line", "Строка выполнения"),
    ("find_match", "Совпадения поиска"),
    ("error", "Ошибки"),
    ("warning", "Предупреждения"),
    ("ok", "Успех"),
    ("gutter", "Номера строк"),
    ("gutter_active", "Активный номер"),
    ("field_bg", "Поле Робота: фон"),
    ("field_cell", "Поле Робота: клетка"),
    ("field_wall", "Поле Робота: стена"),
    ("field_painted", "Поле Робота: закраска"),
    ("field_mark", "Поле Робота: метка"),
    ("sand", "Арена Черепахи"),
    ("water", "Ров с водой"),
    ("paper", "Бумага Рисователя"),
];

macro_rules! palette_fields {
    ($self:ident) => {
        [
            ("bg", $self.bg),
            ("panel", $self.panel),
            ("panel_alt", $self.panel_alt),
            ("toolbar", $self.toolbar),
            ("header", $self.header),
            ("border", $self.border),
            ("text", $self.text),
            ("muted", $self.muted),
            ("icon", $self.icon),
            ("accent", $self.accent),
            ("accent_weak", $self.accent_weak),
            ("accent_text", $self.accent_text),
            ("selection", $self.selection),
            ("keyword", $self.keyword),
            ("type_", $self.type_),
            ("builtin", $self.builtin),
            ("string", $self.string),
            ("number", $self.number),
            ("comment", $self.comment),
            ("ident", $self.ident),
            ("call", $self.call),
            ("run_line", $self.run_line),
            ("find_match", $self.find_match),
            ("error", $self.error),
            ("warning", $self.warning),
            ("ok", $self.ok),
            ("gutter", $self.gutter),
            ("gutter_active", $self.gutter_active),
            ("field_bg", $self.field_bg),
            ("field_cell", $self.field_cell),
            ("field_wall", $self.field_wall),
            ("field_painted", $self.field_painted),
            ("field_mark", $self.field_mark),
            ("sand", $self.sand),
            ("water", $self.water),
            ("paper", $self.paper),
        ]
    };
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(23, 25, 30),
            panel: Color32::from_rgb(31, 34, 40),
            panel_alt: Color32::from_rgb(40, 44, 52),
            toolbar: Color32::from_rgb(36, 40, 47),
            header: Color32::from_rgb(46, 51, 61),
            border: Color32::from_rgb(54, 60, 71),
            text: Color32::from_rgb(220, 224, 230),
            muted: Color32::from_rgb(140, 148, 162),
            icon: Color32::from_rgb(196, 203, 214),
            accent: Color32::from_rgb(92, 158, 255),
            accent_weak: Color32::from_rgb(48, 78, 122),
            accent_text: Color32::WHITE,
            selection: Color32::from_rgba_unmultiplied(92, 158, 255, 72),
            keyword: Color32::from_rgb(202, 146, 236),
            type_: Color32::from_rgb(128, 214, 192),
            builtin: Color32::from_rgb(229, 192, 123),
            string: Color32::from_rgb(152, 195, 121),
            number: Color32::from_rgb(209, 154, 102),
            comment: Color32::from_rgb(116, 132, 132),
            ident: Color32::from_rgb(220, 224, 230),
            call: Color32::from_rgb(104, 178, 242),
            run_line: Color32::from_rgba_unmultiplied(92, 158, 255, 46),
            find_match: Color32::from_rgba_unmultiplied(229, 192, 123, 90),
            error: Color32::from_rgb(230, 108, 117),
            warning: Color32::from_rgb(229, 192, 123),
            ok: Color32::from_rgb(140, 200, 120),
            gutter: Color32::from_rgb(110, 118, 132),
            gutter_active: Color32::from_rgb(190, 200, 214),
            field_bg: Color32::from_rgb(40, 46, 52),
            field_cell: Color32::from_rgb(46, 74, 54),
            field_wall: Color32::from_rgb(214, 214, 100),
            field_painted: Color32::from_rgb(96, 112, 138),
            field_mark: Color32::from_rgb(229, 192, 123),
            sand: Color32::from_rgb(198, 178, 116),
            water: Color32::from_rgb(92, 150, 190),
            paper: Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(247, 248, 250),
            panel: Color32::from_rgb(255, 255, 255),
            panel_alt: Color32::from_rgb(238, 241, 245),
            toolbar: Color32::from_rgb(243, 245, 248),
            header: Color32::from_rgb(230, 234, 240),
            border: Color32::from_rgb(210, 216, 225),
            text: Color32::from_rgb(31, 35, 40),
            muted: Color32::from_rgb(106, 114, 128),
            icon: Color32::from_rgb(70, 78, 92),
            accent: Color32::from_rgb(37, 99, 235),
            accent_weak: Color32::from_rgb(198, 216, 250),
            accent_text: Color32::WHITE,
            selection: Color32::from_rgba_unmultiplied(37, 99, 235, 56),
            keyword: Color32::from_rgb(166, 38, 164),
            type_: Color32::from_rgb(15, 118, 110),
            builtin: Color32::from_rgb(180, 83, 9),
            string: Color32::from_rgb(21, 128, 61),
            number: Color32::from_rgb(180, 83, 9),
            comment: Color32::from_rgb(112, 118, 128),
            ident: Color32::from_rgb(31, 35, 40),
            call: Color32::from_rgb(29, 78, 216),
            run_line: Color32::from_rgba_unmultiplied(37, 99, 235, 38),
            find_match: Color32::from_rgba_unmultiplied(180, 83, 9, 70),
            error: Color32::from_rgb(185, 28, 28),
            warning: Color32::from_rgb(180, 83, 9),
            ok: Color32::from_rgb(21, 128, 61),
            gutter: Color32::from_rgb(140, 148, 160),
            gutter_active: Color32::from_rgb(55, 65, 81),
            field_bg: Color32::from_rgb(226, 233, 226),
            field_cell: Color32::from_rgb(196, 224, 196),
            field_wall: Color32::from_rgb(150, 150, 40),
            field_painted: Color32::from_rgb(148, 160, 176),
            field_mark: Color32::from_rgb(180, 120, 20),
            sand: Color32::from_rgb(222, 199, 135),
            water: Color32::from_rgb(140, 190, 220),
            paper: Color32::from_rgb(255, 255, 255),
        }
    }

    pub fn high_contrast() -> Self {
        let mut p = Self::dark();
        p.bg = Color32::BLACK;
        p.panel = Color32::from_rgb(8, 8, 8);
        p.panel_alt = Color32::from_rgb(18, 18, 18);
        p.toolbar = Color32::from_rgb(12, 12, 12);
        p.header = Color32::from_rgb(30, 30, 30);
        p.border = Color32::from_rgb(90, 90, 90);
        p.text = Color32::WHITE;
        p.muted = Color32::from_rgb(200, 200, 200);
        p.icon = Color32::WHITE;
        p.accent = Color32::from_rgb(255, 214, 0);
        p.accent_weak = Color32::from_rgb(80, 68, 0);
        p.accent_text = Color32::BLACK;
        p.keyword = Color32::from_rgb(255, 170, 255);
        p.string = Color32::from_rgb(140, 255, 140);
        p.number = Color32::from_rgb(255, 200, 120);
        p.comment = Color32::from_rgb(170, 170, 170);
        p.builtin = Color32::from_rgb(255, 230, 130);
        p.error = Color32::from_rgb(255, 120, 120);
        p.ok = Color32::from_rgb(120, 255, 120);
        p
    }

    pub fn ocean() -> Self {
        let mut p = Self::dark();
        p.bg = Color32::from_rgb(12, 28, 42);
        p.panel = Color32::from_rgb(16, 38, 56);
        p.panel_alt = Color32::from_rgb(22, 50, 72);
        p.toolbar = Color32::from_rgb(19, 44, 63);
        p.header = Color32::from_rgb(28, 62, 88);
        p.border = Color32::from_rgb(38, 78, 106);
        p.text = Color32::from_rgb(220, 236, 245);
        p.muted = Color32::from_rgb(132, 166, 186);
        p.accent = Color32::from_rgb(64, 196, 214);
        p.accent_weak = Color32::from_rgb(24, 78, 94);
        p.accent_text = Color32::from_rgb(4, 24, 32);
        p.keyword = Color32::from_rgb(140, 200, 255);
        p.type_ = Color32::from_rgb(120, 226, 196);
        p.string = Color32::from_rgb(170, 220, 150);
        p.comment = Color32::from_rgb(108, 140, 158);
        p.field_bg = Color32::from_rgb(18, 46, 62);
        p.field_cell = Color32::from_rgb(32, 78, 74);
        p
    }

    pub fn sepia() -> Self {
        let mut p = Self::light();
        p.bg = Color32::from_rgb(243, 235, 216);
        p.panel = Color32::from_rgb(250, 244, 229);
        p.panel_alt = Color32::from_rgb(233, 222, 198);
        p.toolbar = Color32::from_rgb(238, 228, 206);
        p.header = Color32::from_rgb(226, 213, 186);
        p.border = Color32::from_rgb(196, 180, 148);
        p.text = Color32::from_rgb(58, 46, 32);
        p.muted = Color32::from_rgb(126, 110, 86);
        p.icon = Color32::from_rgb(92, 76, 54);
        p.accent = Color32::from_rgb(146, 92, 32);
        p.accent_weak = Color32::from_rgb(224, 200, 158);
        p.keyword = Color32::from_rgb(130, 60, 30);
        p.type_ = Color32::from_rgb(40, 100, 86);
        p.string = Color32::from_rgb(70, 110, 40);
        p.comment = Color32::from_rgb(140, 124, 100);
        p
    }

    /// A named colour of the palette (`"accent"`, `"panel"`, …).
    pub fn get(&self, role: &str) -> Option<Color32> {
        palette_fields!(self)
            .iter()
            .find(|(name, _)| *name == role)
            .map(|(_, c)| *c)
    }

    /// Change one colour of the palette.
    pub fn set(&mut self, role: &str, color: Color32) -> bool {
        macro_rules! set_role {
            ($($field:ident),* $(,)?) => {
                match role {
                    $(stringify!($field) => { self.$field = color; true })*
                    _ => false,
                }
            };
        }
        set_role!(
            bg,
            panel,
            panel_alt,
            toolbar,
            header,
            border,
            text,
            muted,
            icon,
            accent,
            accent_weak,
            accent_text,
            selection,
            keyword,
            type_,
            builtin,
            string,
            number,
            comment,
            ident,
            call,
            run_line,
            find_match,
            error,
            warning,
            ok,
            gutter,
            gutter_active,
            field_bg,
            field_cell,
            field_wall,
            field_painted,
            field_mark,
            sand,
            water,
            paper
        )
    }

    /// `role=#rrggbbaa` lines, used by the settings file and by scheme export.
    pub fn to_text(self) -> String {
        let mut out = format!("dark={}\n", self.dark as u8);
        for (name, color) in palette_fields!(self) {
            out.push_str(&format!("{name}={}\n", color_to_hex(color)));
        }
        out
    }

    /// Parse a scheme produced by [`Palette::to_text`]; unknown keys are
    /// ignored, missing colours keep their current value.
    pub fn apply_text(&mut self, text: &str) -> usize {
        let mut applied = 0;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            if key == "dark" {
                self.dark = value == "1" || value.eq_ignore_ascii_case("true");
                continue;
            }
            if let Some(color) = hex_to_color(value) {
                if self.set(key, color) {
                    applied += 1;
                }
            }
        }
        applied
    }
}

pub fn color_to_hex(color: Color32) -> String {
    if color.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r(),
            color.g(),
            color.b(),
            color.a()
        )
    }
}

pub fn hex_to_color(text: &str) -> Option<Color32> {
    let t = text.trim().trim_start_matches('#');
    if !(6..=8).contains(&t.len()) {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    let a = if t.len() == 8 {
        u8::from_str_radix(&t[6..8], 16).ok()?
    } else {
        255
    };
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Built-in colour schemes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Dark,
    Light,
    HighContrast,
    Ocean,
    Sepia,
    Custom,
}

impl Scheme {
    pub const ALL: [Scheme; 6] = [
        Scheme::Dark,
        Scheme::Light,
        Scheme::HighContrast,
        Scheme::Ocean,
        Scheme::Sepia,
        Scheme::Custom,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Scheme::Dark => "dark",
            Scheme::Light => "light",
            Scheme::HighContrast => "high-contrast",
            Scheme::Ocean => "ocean",
            Scheme::Sepia => "sepia",
            Scheme::Custom => "custom",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.id().eq_ignore_ascii_case(id.trim()))
    }

    pub fn label(self) -> &'static str {
        match self {
            Scheme::Dark => "Тёмная",
            Scheme::Light => "Светлая",
            Scheme::HighContrast => "Высокий контраст",
            Scheme::Ocean => "Океан",
            Scheme::Sepia => "Сепия",
            Scheme::Custom => "Своя схема",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Scheme::Dark => Palette::dark(),
            Scheme::Light => Palette::light(),
            Scheme::HighContrast => Palette::high_contrast(),
            Scheme::Ocean => Palette::ocean(),
            Scheme::Sepia => Palette::sepia(),
            // The custom scheme starts from the dark palette; the real colours
            // come from the settings file.
            Scheme::Custom => Palette::dark(),
        }
    }
}

fn try_font(name: &str, paths: &[&str], family: FontFamily) -> Option<FontInsert> {
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            return Some(FontInsert::new(
                name,
                FontData::from_owned(bytes),
                vec![InsertFontFamily {
                    family,
                    priority: FontPriority::Highest,
                }],
            ));
        }
    }
    None
}

/// Install a Cyrillic-capable UI and monospace font if the system has one.
pub fn install_fonts(ctx: &Context) {
    let mono_paths = [
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/TTF/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "C:\\Windows\\Fonts\\consola.ttf",
    ];
    let ui_paths = [
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    if let Some(f) = try_font("KumirMono", &mono_paths, FontFamily::Monospace) {
        ctx.add_font(f);
    }
    if let Some(f) = try_font("KumirUI", &ui_paths, FontFamily::Proportional) {
        ctx.add_font(f);
    }
}

/// Apply the visual style of the interface.
///
/// `touch` enlarges every control for finger input (Android).
pub fn apply_style(ctx: &Context, palette: &Palette, font_size: f32, touch: bool) {
    let mut style = (*ctx.global_style()).clone();
    let mut visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = palette.bg;
    visuals.faint_bg_color = palette.panel_alt;
    visuals.code_bg_color = palette.bg;
    visuals.override_text_color = Some(palette.text);
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.weak_bg_fill = palette.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(5);
    visuals.widgets.inactive.bg_fill = palette.panel_alt;
    visuals.widgets.inactive.weak_bg_fill = palette.panel_alt;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(5);
    visuals.widgets.hovered.bg_fill = palette.accent_weak;
    visuals.widgets.hovered.weak_bg_fill = palette.accent_weak;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(5);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, palette.accent_text);
    visuals.widgets.active.corner_radius = CornerRadius::same(5);
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.hyperlink_color = palette.accent;
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(if palette.dark { 120 } else { 40 }),
    };
    visuals.popup_shadow = visuals.window_shadow;
    style.visuals = visuals;
    let base = font_size.max(11.0);
    let gap = if touch { 10.0 } else { 8.0 };
    let padding = if touch {
        egui::vec2(12.0, 9.0)
    } else {
        egui::vec2(10.0, 5.0)
    };
    style.spacing.item_spacing = egui::vec2(gap, if touch { 8.0 } else { 6.0 });
    style.spacing.button_padding = padding;
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 18.0;
    style.spacing.interact_size.y = if touch { 38.0 } else { 24.0 };
    style.spacing.scroll.bar_width = if touch { 12.0 } else { 10.0 };
    style.interaction.tooltip_delay = 0.45;
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(base, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(base, FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new((base - 2.0).max(9.0), FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(base, FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(base + 6.0, FontFamily::Proportional),
    );
    ctx.set_global_style(style);
    ctx.set_theme(if palette.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_text_roundtrip() {
        let palette = Palette::ocean();
        let text = palette.to_text();
        let mut parsed = Palette::dark();
        assert!(parsed.apply_text(&text) >= PALETTE_ROLES.len());
        assert_eq!(parsed.accent, palette.accent);
        assert_eq!(parsed.bg, palette.bg);
        assert_eq!(parsed.dark, palette.dark);
    }

    #[test]
    fn hex_colors() {
        assert_eq!(color_to_hex(Color32::from_rgb(1, 2, 3)), "#010203");
        assert_eq!(hex_to_color("#010203").unwrap(), Color32::from_rgb(1, 2, 3));
        assert_eq!(hex_to_color("bad"), None);
    }

    #[test]
    fn schemes_have_ids() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::from_id(scheme.id()), Some(scheme));
        }
    }
}
