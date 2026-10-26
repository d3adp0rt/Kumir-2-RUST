//! Small shared widgets that keep the whole interface in one style.

use egui::{Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::theme::Palette;

pub fn panel_frame(palette: &Palette) -> Frame {
    Frame::NONE
        .fill(palette.panel)
        .inner_margin(Margin::same(8))
        .stroke(Stroke::new(1.0, palette.border))
}

/// A small upper-case section title.
pub fn section_header(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.add_space(2.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .small()
            .strong()
            .color(palette.muted),
    );
}

pub fn icon_button(
    ui: &mut Ui,
    _palette: &Palette,
    icon: Icon,
    selected: bool,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let size = if ui.style().spacing.interact_size.y > 30.0 {
        34.0
    } else {
        26.0
    };
    let response = icons::button(ui, icon, selected, enabled, size, tooltip);
    if !enabled {
        return response;
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A flat tab-like selector used in the panels.
pub fn tab(ui: &mut Ui, palette: &Palette, label: &str, selected: bool) -> Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, palette.text);
    let height = if ui.style().spacing.interact_size.y > 30.0 {
        30.0
    } else {
        22.0
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 18.0, height), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.accent_weak);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.panel_alt);
    }
    let color = if selected {
        palette.accent
    } else {
        palette.muted
    };
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        ui.painter().layout_no_wrap(
            label.to_string(),
            egui::TextStyle::Button.resolve(ui.style()),
            color,
        ),
        color,
    );
    if selected {
        let line = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 4.0, rect.bottom() - 2.0),
            Vec2::new(rect.width() - 8.0, 2.0),
        );
        ui.painter()
            .rect_filled(line, CornerRadius::same(1), palette.accent);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small coloured badge (e.g. the number of errors).
pub fn badge(ui: &mut Ui, palette: &Palette, text: &str, background: Color32) {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::TextStyle::Small.resolve(ui.style()),
        palette.accent_text,
    );
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + 10.0, galley.size().y + 2.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), background);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        ui.painter().layout_no_wrap(
            text.to_string(),
            egui::TextStyle::Small.resolve(ui.style()),
            palette.accent_text,
        ),
        palette.accent_text,
    );
}

/// An empty-state message with an optional icon.
pub fn empty_state(ui: &mut Ui, palette: &Palette, icon: Icon, title: &str, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(14.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
        icons::draw(ui.painter(), icon, rect, palette.muted);
        ui.label(RichText::new(title).strong().color(palette.text));
        ui.label(RichText::new(text).small().color(palette.muted));
        ui.add_space(8.0);
    });
}

/// An icon button with a solid background (the green Run button, the red Stop
/// button and similar primary actions).
pub fn filled_icon_button(
    ui: &mut Ui,
    icon: Icon,
    fill: Color32,
    icon_color: Color32,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let size = if ui.style().spacing.interact_size.y > 30.0 {
        34.0
    } else {
        26.0
    };
    let enabled = enabled && ui.is_enabled();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let fill = if enabled {
        fill
    } else {
        fill.gamma_multiply(0.4)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    if response.hovered() && enabled {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, icon_color.gamma_multiply(0.8)),
            egui::StrokeKind::Inside,
        );
    }
    icons::draw(
        ui.painter(),
        icon,
        egui::Rect::from_center_size(rect.center(), Vec2::splat(size * 0.6)),
        if enabled {
            icon_color
        } else {
            icon_color.gamma_multiply(0.5)
        },
    );
    let response = response.on_hover_text(tooltip);
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A full-width list row with an icon and a label (explorer, structure,
/// breakpoints).  Returns the click response.
pub fn list_row(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    text: &str,
    selected: bool,
    muted: bool,
) -> Response {
    let height = if ui.style().spacing.interact_size.y > 30.0 {
        32.0
    } else {
        21.0
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.selection);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.panel_alt);
    }
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + height * 0.5, rect.center().y),
        Vec2::splat(height * 0.6),
    );
    icons::draw(
        ui.painter(),
        icon,
        icon_rect,
        if muted { palette.muted } else { palette.icon },
    );
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = if selected {
        palette.text
    } else if muted {
        palette.muted
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    ui.painter().galley(
        egui::pos2(
            rect.left() + height,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
