//! Pure text-editing helpers: automatic indentation, document formatting,
//! comment toggling and line operations.
//!
//! The functions work on plain source text and character offsets, so they are
//! easy to test without an egui context.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    Begin,
    Loop,
    If,
    Switch,
    Case,
    Module,
}

/// Words of a line without the comment part.
fn code_words(line: &str) -> Vec<String> {
    let mut code = line;
    for marker in ["|", "!", "//", "#"] {
        if let Some(pos) = code.find(marker) {
            code = &code[..pos];
        }
    }
    code.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Apply one line to the block stack before computing its indentation.
fn dedent_before(words: &[String], stack: &mut Vec<Block>) {
    let Some(first) = words.first() else {
        return;
    };
    match first.as_str() {
        "кц" | "end_do" => {
            while let Some(block) = stack.pop() {
                if block == Block::Loop {
                    break;
                }
            }
        }
        "все" | "done" => {
            if stack.last() == Some(&Block::Case) {
                stack.pop();
            }
            while let Some(block) = stack.pop() {
                if matches!(block, Block::If | Block::Switch) {
                    break;
                }
            }
        }
        "кон" | "end" => {
            while let Some(block) = stack.pop() {
                if block == Block::Begin {
                    break;
                }
            }
        }
        "кон_исп" | "кон_модуля" | "end_module" => {
            while let Some(block) = stack.pop() {
                if block == Block::Module {
                    break;
                }
            }
        }
        "иначе" | "else" => match stack.last() {
            Some(Block::If) => {
                stack.pop();
            }
            Some(Block::Case) => {
                stack.pop();
            }
            _ => {}
        },
        "при" | "case" if stack.last() == Some(&Block::Case) => {
            stack.pop();
        }
        _ => {}
    }
}

/// Apply one line to the block stack after computing its indentation.
fn indent_after(words: &[String], stack: &mut Vec<Block>) {
    let Some(first) = words.first() else {
        return;
    };
    // Single-line constructs do not open a block.
    let closes_same_line = words
        .iter()
        .skip(1)
        .any(|w| matches!(w.as_str(), "все" | "кц" | "кон" | "done" | "end_do" | "end"));
    match first.as_str() {
        "нач" | "begin" => stack.push(Block::Begin),
        "нц" | "do" => stack.push(Block::Loop),
        "если" | "if" if !closes_same_line => stack.push(Block::If),
        "выбор" | "switch" if !closes_same_line => stack.push(Block::Switch),
        "при" | "case" if !closes_same_line => stack.push(Block::Case),
        "иначе" | "else" => stack.push(Block::If),
        "модуль" | "исп" | "module" if !closes_same_line => stack.push(Block::Module),
        _ => {}
    }
}

pub fn leading_whitespace(line: &str) -> &str {
    let end = line
        .char_indices()
        .find(|(_, c)| !matches!(c, ' ' | '\t'))
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    &line[..end]
}

/// Re-indent the whole document, keeping blank lines and comments aligned
/// with the surrounding code.
pub fn format_source(text: &str, tab_width: usize) -> String {
    let width = tab_width.max(1);
    let mut stack = Vec::new();
    let mut out = String::with_capacity(text.len() + 64);
    for (i, line) in text.lines().enumerate() {
        let words = code_words(line);
        dedent_before(&words, &mut stack);
        let depth = stack.len();
        let trimmed = line.trim_start();
        if i > 0 {
            out.push('\n');
        }
        if !trimmed.trim().is_empty() {
            for _ in 0..depth * width {
                out.push(' ');
            }
            out.push_str(trimmed.trim_end());
        }
        indent_after(&words, &mut stack);
    }
    out
}

/// Indentation to insert after pressing Enter at `char_index`.
pub fn indent_for_new_line(text: &str, char_index: usize, tab_width: usize) -> String {
    let width = tab_width.max(1);
    let chars: Vec<char> = text.chars().collect();
    let line_start = chars[..char_index.min(chars.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix: String = chars[..line_start].iter().collect();
    let mut stack = Vec::new();
    for line in prefix.lines() {
        let words = code_words(line);
        dedent_before(&words, &mut stack);
        indent_after(&words, &mut stack);
    }
    let current: String = chars[line_start..char_index.min(chars.len())]
        .iter()
        .collect();
    let words = code_words(&current);
    dedent_before(&words, &mut stack);
    indent_after(&words, &mut stack);
    " ".repeat(stack.len() * width)
}

/// Toggle `|` comments on every line touched by the selection.
/// Returns the new text and the new selection (character offsets).
pub fn toggle_comment(
    text: &str,
    selection: (usize, usize),
    _tab_width: usize,
) -> (String, (usize, usize)) {
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = if selection.0 <= selection.1 {
        selection
    } else {
        (selection.1, selection.0)
    };
    let line_start = chars[..a.min(chars.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut line_end = b.min(chars.len());
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }
    let selected: String = chars[line_start..line_end].iter().collect();
    let lines: Vec<&str> = selected.split('\n').collect();
    let all_commented = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .all(|l| l.trim_start().starts_with('|'));
    let mut out_lines = Vec::new();
    for line in &lines {
        if line.trim().is_empty() {
            out_lines.push((*line).to_string());
            continue;
        }
        if all_commented {
            if let Some(pos) = line.find('|') {
                let mut new_line = line.to_string();
                new_line.remove(pos);
                if new_line.get(pos..pos + 1) == Some(" ") {
                    new_line.remove(pos);
                }
                out_lines.push(new_line);
            } else {
                out_lines.push((*line).to_string());
            }
        } else {
            let indent = leading_whitespace(line).len();
            let mut new_line = line.to_string();
            new_line.insert_str(indent, "| ");
            out_lines.push(new_line);
        }
    }
    let replacement = out_lines.join("\n");
    let mut result: String = chars[..line_start].iter().collect();
    result.push_str(&replacement);
    result.extend(chars[line_end..].iter());
    let new_end = line_start + replacement.chars().count();
    (result, (line_start, new_end))
}

/// The character range of the line containing `char_index`.
pub fn line_range(text: &str, char_index: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let index = char_index.min(chars.len());
    let start = chars[..index]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut end = index;
    while end < chars.len() && chars[end] != '\n' {
        end += 1;
    }
    (start, end)
}

/// Insert a copy of the current line right after it.
pub fn duplicate_line(text: &str, char_index: usize) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = line_range(text, char_index);
    let line: String = chars[start..end].iter().collect();
    let mut result: String = chars[..end].iter().collect();
    result.push('\n');
    result.push_str(&line);
    result.extend(chars[end..].iter());
    let cursor = end + 1 + (char_index.saturating_sub(start));
    (result, cursor)
}

/// Move the current line up or down.
pub fn move_line(text: &str, char_index: usize, up: bool) -> (String, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut pos = 0usize;
    let mut current = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let len = line.chars().count() + 1;
        if char_index < pos + len || i + 1 == lines.len() {
            current = i;
            break;
        }
        pos += len;
    }
    let other = if up {
        current.checked_sub(1)
    } else if current + 1 < lines.len() {
        Some(current + 1)
    } else {
        None
    };
    let Some(other) = other else {
        return (text.to_string(), char_index);
    };
    let mut lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    lines.swap(current, other);
    let new_text = lines.join("\n");
    let offset_in_line = char_index.saturating_sub(pos);
    let new_line_start: usize = lines[..current].iter().map(|l| l.chars().count() + 1).sum();
    (new_text, new_line_start + offset_in_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_indents_blocks() {
        let src = "алг Сумма\nнач\nцел a, b\nввод a, b\nесли a > b то\nвывод a\nиначе\nвывод b\nвсе\nнц пока a > 0\na := a - 1\nкц\nкон\n";
        let expected = "алг Сумма\nнач\n    цел a, b\n    ввод a, b\n    если a > b то\n        вывод a\n    иначе\n        вывод b\n    все\n    нц пока a > 0\n        a := a - 1\n    кц\nкон";
        assert_eq!(format_source(src, 4), expected);
    }

    #[test]
    fn single_line_if_does_not_open_a_block() {
        let src = "нач\nесли a то b := 1 все\nc := 2\nкон";
        assert_eq!(
            format_source(src, 2),
            "нач\n  если a то b := 1 все\n  c := 2\nкон"
        );
    }

    #[test]
    fn newline_after_block_opener_is_deeper() {
        let src = "алг Тест\nнач\nесли x > 0 то";
        assert_eq!(
            indent_for_new_line(src, src.chars().count(), 4),
            " ".repeat(8)
        );
    }

    #[test]
    fn newline_after_plain_statement_keeps_level() {
        let src = "алг Тест\nнач\n    вывод 1";
        assert_eq!(indent_for_new_line(src, src.chars().count(), 4), "    ");
    }

    #[test]
    fn comment_toggle_roundtrip() {
        let text = "нач\nвывод 1\nкон";
        let all = (0, text.chars().count());
        let (commented, sel) = toggle_comment(text, all, 4);
        assert_eq!(commented, "| нач\n| вывод 1\n| кон");
        let (plain, _) = toggle_comment(&commented, sel, 4);
        assert_eq!(plain, text);
    }

    #[test]
    fn line_operations() {
        let text = "a\nb\nc";
        let (dup, cursor) = duplicate_line(text, 2);
        assert_eq!(dup, "a\nb\nb\nc");
        assert_eq!(cursor, 4);
        let (moved, _) = move_line(text, 2, true);
        assert_eq!(moved, "b\na\nc");
    }
}

/// Insert spaces up to the next tab stop at the cursor.
pub fn tab_at_cursor(text: &str, char_index: usize, width: usize) -> (String, usize) {
    let width = width.max(1);
    let chars: Vec<char> = text.chars().collect();
    let index = char_index.min(chars.len());
    let line_start = chars[..index]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let column = index - line_start;
    let spaces = width - (column % width);
    let mut result: String = chars[..index].iter().collect();
    result.push_str(&" ".repeat(spaces));
    result.extend(chars[index..].iter());
    (result, index + spaces)
}

/// Indent every line of the selection by one tab (or insert a tab at the
/// cursor when the selection is empty).
pub fn indent_selection(
    text: &str,
    selection: (usize, usize),
    width: usize,
) -> (String, (usize, usize)) {
    let (a, b) = sorted(selection);
    if a == b {
        let (text, cursor) = tab_at_cursor(text, a, width);
        return (text, (cursor, cursor));
    }
    let chars: Vec<char> = text.chars().collect();
    let start = chars[..a.min(chars.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut end = b.min(chars.len());
    while end < chars.len() && chars[end] != '\n' {
        end += 1;
    }
    let block: String = chars[start..end].iter().collect();
    let insert = " ".repeat(width.max(1));
    let lines: Vec<String> = block
        .split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_string()
            } else {
                format!("{insert}{line}")
            }
        })
        .collect();
    let replacement = lines.join("\n");
    let added = replacement.chars().count() - block.chars().count();
    let mut result: String = chars[..start].iter().collect();
    result.push_str(&replacement);
    result.extend(chars[end..].iter());
    (result, (a + width.max(1), b + added))
}

/// Remove one tab of indentation from every selected line.
pub fn unindent_selection(
    text: &str,
    selection: (usize, usize),
    width: usize,
) -> (String, (usize, usize)) {
    let (a, b) = sorted(selection);
    let chars: Vec<char> = text.chars().collect();
    let start = chars[..a.min(chars.len())]
        .iter()
        .rposition(|c| *c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut end = b.min(chars.len());
    while end < chars.len() && chars[end] != '\n' {
        end += 1;
    }
    let block: String = chars[start..end].iter().collect();
    let width = width.max(1);
    let lines: Vec<String> = block
        .split('\n')
        .map(|line| {
            let remove = line.chars().take(width).take_while(|c| *c == ' ').count();
            line[remove..].to_string()
        })
        .collect();
    let replacement = lines.join("\n");
    let removed = block.chars().count() - replacement.chars().count();
    let mut result: String = chars[..start].iter().collect();
    result.push_str(&replacement);
    result.extend(chars[end..].iter());
    (
        result,
        (
            a.saturating_sub(if a > start { width } else { 0 }),
            b - removed,
        ),
    )
}

fn sorted(selection: (usize, usize)) -> (usize, usize) {
    if selection.0 <= selection.1 {
        selection
    } else {
        (selection.1, selection.0)
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn tab_stops() {
        let (text, cursor) = tab_at_cursor("abc", 3, 4);
        assert_eq!(text, "abc ");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn indent_and_unindent_selection() {
        let text = "a\nb";
        let (indented, sel) = indent_selection(text, (0, 2), 2);
        assert_eq!(indented, "  a\n  b");
        assert_eq!(sel, (2, 6));
        let (plain, _) = unindent_selection(&indented, sel, 2);
        assert_eq!(plain, text);
    }
}

/// Replace every whole-word occurrence of `from` with `to`.
/// Returns the new text and the number of replacements.
pub fn replace_word_all(text: &str, from: &str, to: &str) -> (String, usize) {
    if from.is_empty() {
        return (text.to_string(), 0);
    }
    let is_boundary = |c: Option<char>| c.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true);
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    let mut count = 0usize;
    let mut search = 0usize;
    while let Some(pos) = text[search..].find(from) {
        let start = search + pos;
        let end = start + from.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if is_boundary(before) && is_boundary(after) {
            out.push_str(&text[last..start]);
            out.push_str(to);
            last = end;
            count += 1;
        }
        search = end;
        if search >= text.len() {
            break;
        }
    }
    out.push_str(&text[last..]);
    (out, count)
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    #[test]
    fn replaces_whole_words_only() {
        let text = "цел счёт\nсчёт := счёт + счётчик";
        let (result, n) = replace_word_all(text, "счёт", "сумма");
        assert_eq!(result, "цел сумма\nсумма := сумма + счётчик");
        assert_eq!(n, 3);
    }
}

/// For every line, the line of the matching block keyword (`нц`↔`кц`,
/// `нач`↔`кон`, `если`/`выбор`↔`все`).  Used by the editor to highlight the
/// counterpart of the block under the cursor.
pub fn matching_blocks(text: &str) -> Vec<Option<usize>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = vec![None; lines.len()];
    let mut stack: Vec<(Block, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let words = code_words(line);
        let Some(first) = words.first() else {
            continue;
        };
        let closes_here = words
            .iter()
            .skip(1)
            .any(|w| matches!(w.as_str(), "все" | "кц" | "кон" | "done" | "end"));
        match first.as_str() {
            "нач" | "begin" => stack.push((Block::Begin, index)),
            "нц" | "do" if !closes_here => stack.push((Block::Loop, index)),
            "если" | "if" if !closes_here => stack.push((Block::If, index)),
            "выбор" | "switch" if !closes_here => stack.push((Block::Switch, index)),
            "кц" | "end_do" => {
                while let Some((block, start)) = stack.pop() {
                    if block == Block::Loop {
                        result[start] = Some(index);
                        result[index] = Some(start);
                        break;
                    }
                }
            }
            "кон" | "end" => {
                while let Some((block, start)) = stack.pop() {
                    if block == Block::Begin {
                        result[start] = Some(index);
                        result[index] = Some(start);
                        break;
                    }
                }
            }
            "все" | "done" => {
                while let Some((block, start)) = stack.pop() {
                    if matches!(block, Block::If | Block::Switch) {
                        result[start] = Some(index);
                        result[index] = Some(start);
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod block_tests {
    use super::*;

    #[test]
    fn matches_loop_if_and_algorithm() {
        let text = "алг\nнач\nесли a то\nвывод 1\nвсе\nнц пока b\nb := b - 1\nкц\nкон";
        let map = matching_blocks(text);
        assert_eq!(map[1], Some(8));
        assert_eq!(map[2], Some(4));
        assert_eq!(map[5], Some(7));
        assert_eq!(map[0], None);
    }
}
