//! Diagnostics shared by the lexer, parser, interpreter and UI.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number (in Unicode scalar values).
    pub col: u32,
    /// Length in characters.
    pub len: u32,
    pub message: String,
    pub severity: Severity,
}

impl Diagnostic {
    pub fn error(line: u32, col: u32, len: u32, message: impl Into<String>) -> Self {
        Self {
            line,
            col,
            len,
            message: message.into(),
            severity: Severity::Error,
        }
    }

    pub fn warning(line: u32, col: u32, len: u32, message: impl Into<String>) -> Self {
        Self {
            line,
            col,
            len,
            message: message.into(),
            severity: Severity::Warning,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.severity {
            Severity::Error => "ошибка",
            Severity::Warning => "предупреждение",
        };
        write!(
            f,
            "{kind} в строке {} (позиция {}): {}",
            self.line, self.col, self.message
        )
    }
}

/// Runtime failure. Always carries the source line which caused it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub line: u32,
    pub message: String,
}

impl RuntimeError {
    pub fn new(line: u32, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Ошибка выполнения в строке {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for RuntimeError {}
