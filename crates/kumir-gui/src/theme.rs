//! Visual design of the IDE: palette, fonts and global egui style.

use egui::{
    Color32, Context, CornerRadius, FontData, FontFamily, FontId, FontInsert, FontPriority,
    InsertFontFamily, Stroke,
};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_weak: Color32,
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
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(27, 29, 34),
            panel: Color32::from_rgb(34, 37, 43),
            panel_alt: Color32::from_rgb(42, 46, 54),
            border: Color32::from_rgb(52, 58, 68),
            text: Color32::from_rgb(215, 218, 224),
            muted: Color32::from_rgb(139, 147, 161),
            accent: Color32::from_rgb(91, 157, 255),
            accent_weak: Color32::from_rgb(47, 77, 122),
            keyword: Color32::from_rgb(199, 146, 234),
            type_: Color32::from_rgb(127, 214, 192),
            builtin: Color32::from_rgb(229, 192, 123),
            string: Color32::from_rgb(152, 195, 121),
            number: Color32::from_rgb(209, 154, 102),
            comment: Color32::from_rgb(111, 128, 128),
            ident: Color32::from_rgb(215, 218, 224),
            call: Color32::from_rgb(97, 175, 239),
            run_line: Color32::from_rgba_unmultiplied(91, 157, 255, 46),
            find_match: Color32::from_rgba_unmultiplied(229, 192, 123, 80),
            error: Color32::from_rgb(224, 108, 117),
            warning: Color32::from_rgb(229, 192, 123),
            ok: Color32::from_rgb(152, 195, 121),
            gutter: Color32::from_rgb(110, 118, 132),
            gutter_active: Color32::from_rgb(180, 190, 205),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(246, 247, 249),
            panel: Color32::from_rgb(255, 255, 255),
            panel_alt: Color32::from_rgb(236, 239, 243),
            border: Color32::from_rgb(214, 219, 227),
            text: Color32::from_rgb(31, 35, 40),
            muted: Color32::from_rgb(107, 114, 128),
            accent: Color32::from_rgb(37, 99, 235),
            accent_weak: Color32::from_rgb(191, 210, 250),
            keyword: Color32::from_rgb(166, 38, 164),
            type_: Color32::from_rgb(15, 118, 110),
            builtin: Color32::from_rgb(180, 83, 9),
            string: Color32::from_rgb(21, 128, 61),
            number: Color32::from_rgb(180, 83, 9),
            comment: Color32::from_rgb(107, 114, 128),
            ident: Color32::from_rgb(31, 35, 40),
            call: Color32::from_rgb(29, 78, 216),
            run_line: Color32::from_rgba_unmultiplied(37, 99, 235, 38),
            find_match: Color32::from_rgba_unmultiplied(180, 83, 9, 60),
            error: Color32::from_rgb(185, 28, 28),
            warning: Color32::from_rgb(180, 83, 9),
            ok: Color32::from_rgb(21, 128, 61),
            gutter: Color32::from_rgb(140, 148, 160),
            gutter_active: Color32::from_rgb(55, 65, 81),
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
        "/usr/share/fonts/TTF/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "C:\\Windows\\Fonts\\consola.ttf",
    ];
    let ui_paths = [
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    if let Some(f) = try_font("KumirMono", &mono_paths, FontFamily::Monospace) {
        ctx.add_font(f);
    }
    if let Some(f) = try_font("KumirUI", &ui_paths, FontFamily::Proportional) {
        ctx.add_font(f);
    }
}

pub fn apply_style(ctx: &Context, palette: &Palette, font_size: f32) {
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
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.bg_fill = palette.panel_alt;
    visuals.widgets.inactive.weak_bg_fill = palette.panel_alt;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.bg_fill = palette.accent_weak;
    visuals.widgets.hovered.weak_bg_fill = palette.accent_weak;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = palette.accent_weak;
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
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 18.0;
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(font_size.max(11.0), FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(font_size.max(11.0), FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        FontId::new((font_size - 2.0).max(9.0), FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(font_size.max(11.0), FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(font_size + 6.0, FontFamily::Proportional),
    );
    ctx.set_global_style(style);
    ctx.set_theme(if palette.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
}
