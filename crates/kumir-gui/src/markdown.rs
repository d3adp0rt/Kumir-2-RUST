//! A very small Markdown renderer for the built-in help.
//!
//! It understands the subset used by `docs/*.md`: headings, paragraphs,
//! bullet and numbered lists, fenced code blocks, tables, inline code, bold
//! text and links.  Code blocks have a copy button.

use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, Frame, Label, Margin, RichText, Stroke, Ui};

use crate::theme::Palette;

pub fn show(ui: &mut Ui, palette: &Palette, text: &str) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            ui.add_space(4.0);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("```") {
            let _ = rest;
            let mut code = String::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push_str(lines[i]);
                code.push('\n');
                i += 1;
            }
            i += 1;
            code_block(ui, palette, &code);
            continue;
        }
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            let title = trimmed.trim_start_matches('#').trim();
            let size = match level {
                1 => 21.0,
                2 => 17.0,
                _ => 14.5,
            };
            ui.add_space(if level == 1 { 2.0 } else { 8.0 });
            ui.label(
                RichText::new(title)
                    .size(size)
                    .strong()
                    .color(if level <= 2 {
                        palette.accent
                    } else {
                        palette.text
                    }),
            );
            if level <= 2 {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 1.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 0.0, palette.border);
            }
            i += 1;
            continue;
        }
        if trimmed.starts_with('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                let row: Vec<String> = lines[i]
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_string())
                    .collect();
                let separator = row
                    .iter()
                    .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'));
                if !separator {
                    rows.push(row);
                }
                i += 1;
            }
            if !rows.is_empty() {
                table(ui, palette, &rows);
            }
            continue;
        }
        if is_list_item(trimmed) {
            let indent = trimmed.len() - trimmed.trim_start().len();
            let mut items = Vec::new();
            while i < lines.len() && is_list_item(lines[i].trim_end()) {
                let item = lines[i].trim_start();
                let text = item
                    .trim_start_matches("- ")
                    .trim_start_matches("* ")
                    .split_once(". ")
                    .map(|(_, rest)| rest)
                    .unwrap_or_else(|| item.trim_start_matches("- "));
                items.push(text.to_string());
                i += 1;
            }
            for item in items {
                ui.horizontal_top(|ui| {
                    ui.add_space(indent as f32 + 4.0);
                    ui.label(RichText::new("•").color(palette.muted));
                    inline(ui, palette, &item, None);
                });
            }
            continue;
        }
        // Paragraph: join consecutive plain lines.
        let mut paragraph = String::new();
        while i < lines.len() {
            let l = lines[i].trim_end();
            if l.trim().is_empty()
                || l.starts_with('#')
                || l.starts_with("```")
                || l.starts_with('|')
                || is_list_item(l)
            {
                break;
            }
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(l.trim());
            i += 1;
        }
        inline(ui, palette, &paragraph, None);
    }
}

fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- ")
        || t.starts_with("* ")
        || (t
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && t.contains(". "))
}

fn table(ui: &mut Ui, palette: &Palette, rows: &[Vec<String>]) {
    let columns = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    egui::Grid::new(("kumir_help_table", rows.len(), rows[0].len()))
        .num_columns(columns)
        .spacing([14.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for (i, row) in rows.iter().enumerate() {
                for c in 0..columns {
                    let text = row.get(c).cloned().unwrap_or_default();
                    if i == 0 {
                        ui.label(RichText::new(text).strong().color(palette.accent));
                    } else {
                        ui.label(RichText::new(text).color(palette.text));
                    }
                }
                ui.end_row();
            }
        });
}

fn code_block(ui: &mut Ui, palette: &Palette, code: &str) {
    Frame::NONE
        .fill(palette.panel_alt)
        .corner_radius(egui::CornerRadius::same(5))
        .stroke(Stroke::new(1.0, palette.border))
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .small_button("Копировать")
                    .on_hover_text("Скопировать пример")
                    .clicked()
                {
                    ui.ctx().copy_text(code.trim_end().to_string());
                }
                ui.label(RichText::new("пример").small().color(palette.muted));
            });
            ui.add(
                Label::new(
                    RichText::new(code.trim_end())
                        .monospace()
                        .color(palette.text),
                )
                .wrap(),
            );
        });
    ui.add_space(4.0);
}

fn inline(ui: &mut Ui, palette: &Palette, text: &str, size: Option<f32>) {
    let base = size.unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()).size);
    let normal = FontId::proportional(base);
    let mono = FontId::monospace(base * 0.95);
    let chars: Vec<char> = text.chars().collect();
    let byte_at = |k: usize| -> usize {
        chars[..k.min(chars.len())]
            .iter()
            .map(|c| c.len_utf8())
            .sum()
    };
    let mut runs: Vec<(usize, usize, TextFormat)> = Vec::new();
    let mut pos = 0usize;
    let mut k = 0usize;
    while k < chars.len() {
        let mut formatted: Option<(usize, TextFormat)> = None;
        if chars[k] == '`' {
            if let Some(rel) = chars[k + 1..].iter().position(|c| *c == '`') {
                let end = k + 1 + rel + 1;
                formatted = Some((
                    end,
                    TextFormat {
                        font_id: mono.clone(),
                        color: palette.builtin,
                        background: palette.panel_alt,
                        ..Default::default()
                    },
                ));
            }
        } else if chars[k] == '*' && chars.get(k + 1) == Some(&'*') {
            let mut j = k + 2;
            while j + 1 < chars.len() {
                if chars[j] == '*' && chars[j + 1] == '*' {
                    break;
                }
                j += 1;
            }
            if j + 1 < chars.len() {
                formatted = Some((
                    j + 2,
                    TextFormat {
                        font_id: normal.clone(),
                        color: palette.text,
                        ..Default::default()
                    },
                ));
            }
        } else if chars[k] == '[' {
            if let Some(rel) = chars[k + 1..].iter().position(|c| *c == ']') {
                let mut end = k + 1 + rel + 1;
                if chars.get(end) == Some(&'(') {
                    if let Some(rel) = chars[end + 1..].iter().position(|c| *c == ')') {
                        end = end + 1 + rel + 1;
                    }
                }
                formatted = Some((
                    end,
                    TextFormat {
                        font_id: normal.clone(),
                        color: palette.accent,
                        ..Default::default()
                    },
                ));
            }
        }
        if let Some((end, format)) = formatted {
            if k > pos {
                runs.push((
                    pos,
                    k,
                    TextFormat {
                        font_id: normal.clone(),
                        color: palette.text,
                        ..Default::default()
                    },
                ));
            }
            runs.push((k, end, format));
            k = end;
            pos = end;
        } else {
            k += 1;
        }
    }
    if pos < chars.len() {
        runs.push((
            pos,
            chars.len(),
            TextFormat {
                font_id: normal,
                color: palette.text,
                ..Default::default()
            },
        ));
    }
    if runs.is_empty() {
        ui.label("");
        return;
    }
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    job.text = text.to_string();
    for (start, end, format) in runs {
        let a = byte_at(start);
        let b = byte_at(end).min(text.len());
        if b > a {
            job.sections.push(egui::text::LayoutSection {
                leading_space: 0.0,
                byte_range: egui::text::ByteIndex(a)..egui::text::ByteIndex(b),
                format,
            });
        }
    }
    ui.add(Label::new(job).wrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_without_panicking() {
        egui::__run_test_ui(|ui| {
            let palette = Palette::dark();
            let text = "# Заголовок\n\nАбзац с `кодом` и **жирным**.\n\n- пункт\n\n```\nвывод 1\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
            show(ui, &palette, text);
        });
    }
}
