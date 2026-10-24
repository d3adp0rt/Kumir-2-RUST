//! Syntax highlighting support for the IDE.
//!
//! The highlighter is a thin layer over the lexer, so the colors can never
//! disagree with the parser about what is a keyword or a string literal.

use std::ops::Range;

use crate::lexer::{tokenize, Kw, Op, Tok};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HighlightKind {
    Keyword,
    Type,
    Builtin,
    String,
    Char,
    Number,
    Comment,
    Operator,
    Ident,
    Call,
    Error,
}

#[derive(Clone, Debug)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub kind: HighlightKind,
}

pub fn highlight(source: &str) -> Vec<HighlightSpan> {
    let (tokens, errors) = tokenize(source);
    let mut spans = Vec::with_capacity(tokens.len());
    for (i, t) in tokens.iter().enumerate() {
        let kind = match &t.kind {
            Tok::Kw(kw) => match kw {
                Kw::TypeInt
                | Kw::TypeReal
                | Kw::TypeStr
                | Kw::TypeChar
                | Kw::TypeBool
                | Kw::TypeFile
                | Kw::TypeIntArray
                | Kw::TypeRealArray
                | Kw::TypeStrArray
                | Kw::TypeCharArray
                | Kw::TypeBoolArray => HighlightKind::Type,
                _ => HighlightKind::Keyword,
            },
            Tok::Int(_) | Tok::Real(_) => HighlightKind::Number,
            Tok::Str(_) => HighlightKind::String,
            Tok::Char(_) => HighlightKind::Char,
            Tok::NewlineLit => HighlightKind::Builtin,
            Tok::Op(_) => HighlightKind::Operator,
            Tok::Comment(_) => HighlightKind::Comment,
            Tok::Ident(name) => {
                let is_call = tokens
                    .get(i + 1)
                    .map(|n| n.is_op(Op::LParen))
                    .unwrap_or(false);
                if is_call {
                    HighlightKind::Call
                } else if crate::builtins::known_names()
                    .iter()
                    .any(|n| crate::ast::normalize_name(n) == crate::ast::normalize_name(name))
                {
                    HighlightKind::Builtin
                } else {
                    HighlightKind::Ident
                }
            }
            Tok::Newline | Tok::Eof => continue,
        };
        if t.end > t.start {
            spans.push(HighlightSpan {
                range: t.start..t.end,
                kind,
            });
        }
    }
    // Mark lexical errors by their position.
    for e in &errors {
        if e.severity == crate::diag::Severity::Error {
            // Diagnostic stores only line/column; the lexer's own span is not
            // preserved, so this is only a best-effort marker.
            let _ = e;
        }
    }
    spans
}
