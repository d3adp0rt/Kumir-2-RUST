//! `.kum` file format compatibility.
//!
//! A `.kum` file is plain text which may contain teacher-only (hidden) text
//! and protected lines.  The markers are exactly those of the original
//! `KumFileHandler`:
//!
//! * `|@hidden` at the end of a line marks a hidden line;
//! * `|@protected` marks a line which cannot be edited by a student;
//! * `|@signature <base64>|@hidden` stores the hidden-text signature.
//!
//! The parser works on `visible + "\n" + hidden`, which matches the original
//! compiler: hidden teacher algorithms are compiled but not shown.

use std::collections::BTreeSet;
use std::path::Path;

use encoding_rs::WINDOWS_1251;

#[derive(Clone, Debug, Default)]
pub struct KumFile {
    pub visible: String,
    pub hidden: String,
    /// 0-based line numbers in `visible`.
    pub protected_lines: BTreeSet<usize>,
    pub signature_base64: Option<String>,
    pub had_bom: bool,
}

impl KumFile {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        let mut text = text.to_string();
        let had_bom = text.starts_with('\u{feff}');
        if had_bom {
            text.remove(0);
        }
        let mut visible_lines: Vec<String> = Vec::new();
        let mut hidden_lines: Vec<String> = Vec::new();
        let mut protected = BTreeSet::new();
        let mut signature = None;

        let mut visible_index = 0usize;
        for raw in text.split('\n') {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let trimmed = line.trim_start();
            if trimmed.starts_with("|@signature ") && line.trim_end().ends_with("|@hidden") {
                let l = line.trim();
                let inner = &l[12..l.len().saturating_sub(8)];
                signature = Some(inner.trim().to_string());
            } else if line.trim_end().ends_with("|@hidden") {
                let l = line.trim_end();
                hidden_lines.push(l[..l.len() - 8].to_string());
            } else if line.trim_end().ends_with("|@protected") {
                let l = line.trim_end();
                protected.insert(visible_index);
                visible_lines.push(l[..l.len() - 11].to_string());
                visible_index += 1;
            } else {
                visible_lines.push(line.to_string());
                visible_index += 1;
            }
        }
        // A trailing newline produces one empty element; keep the visible
        // text byte-for-byte except for marker removal.
        Self {
            visible: visible_lines.join("\n"),
            hidden: hidden_lines.join("\n"),
            protected_lines: protected,
            signature_base64: signature,
            had_bom,
        }
    }

    pub fn to_string_with_markers(&self) -> String {
        let visible: Vec<&str> = self.visible.split('\n').collect();
        let mut out = String::new();
        for (i, line) in visible.iter().enumerate() {
            out.push_str(line);
            if self.protected_lines.contains(&i) {
                out.push_str("|@protected");
            }
            if i + 1 < visible.len() {
                out.push('\n');
            }
        }
        if !self.hidden.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            let hidden: Vec<&str> = self.hidden.split('\n').collect();
            for (i, line) in hidden.iter().enumerate() {
                out.push_str(line);
                out.push_str("|@hidden");
                if i + 1 < hidden.len() {
                    out.push('\n');
                }
            }
        }
        if let Some(sig) = &self.signature_base64 {
            if !self.hidden.is_empty() || !out.is_empty() {
                out.push('\n');
            }
            out.push_str("|@signature ");
            out.push_str(sig);
            out.push_str("|@hidden");
        }
        out
    }

    /// Text handed to the compiler.
    pub fn source_for_compilation(&self) -> String {
        if self.hidden.trim().is_empty() {
            self.visible.clone()
        } else {
            format!("{}\n{}", self.visible, self.hidden)
        }
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_bytes(&bytes))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        // UTF-8 BOM.
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
                return Self::from_str(&format!("\u{feff}{s}"));
            }
        }
        if let Ok(s) = std::str::from_utf8(bytes) {
            return Self::from_str(s);
        }
        // Old Windows Kumir files were usually saved in CP1251.
        let (decoded, _, _) = WINDOWS_1251.decode(bytes);
        let mut f = Self::from_str(&decoded);
        f.had_bom = false;
        f
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut text = self.to_string_with_markers();
        if self.had_bom {
            text.insert(0, '\u{feff}');
        }
        std::fs::write(path, text.as_bytes())
    }
}

/// Insert a teacher mark before the first algorithm, as the original
/// `KumFile::insertTeacherMark` does. Used by the course editor functionality.
pub fn insert_teacher_mark(data: &mut KumFile) {
    if data.visible.contains("|#%%") || data.hidden.contains("|#%%") {
        return;
    }
    if let Some(pos) = find_algorithm_start(&data.hidden) {
        data.hidden.insert_str(pos, "\n|#%%\n");
        return;
    }
    if let Some(pos) = find_algorithm_start(&data.visible) {
        data.visible.insert_str(pos, "\n|#%%\n");
    }
}

fn find_algorithm_start(text: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("алг") || trimmed.starts_with("algorhitm") {
            return Some(offset + (line.len() - trimmed.len()));
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_roundtrip() {
        let src = "алг Тест|@protected\nнач\nвывод 1\nкон\n| Скрыто|@hidden\nалг Секрет|@hidden\nнач|@hidden\nкон|@hidden";
        let f = KumFile::from_str(src);
        assert_eq!(f.visible, "алг Тест\nнач\nвывод 1\nкон");
        assert!(f.protected_lines.contains(&0));
        assert!(f.hidden.contains("Секрет"));
        assert_eq!(f.to_string_with_markers(), src);
        let compiled = f.source_for_compilation();
        assert!(compiled.contains("Секрет"));
    }

    #[test]
    fn plain_text() {
        let src = "алг\nнач\nкон\n";
        let f = KumFile::from_str(src);
        assert_eq!(f.visible, src);
        assert_eq!(f.to_string_with_markers(), src);
    }
}
