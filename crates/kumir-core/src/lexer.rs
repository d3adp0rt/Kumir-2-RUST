//! Lexer for the Kumir algorithmic language.
//!
//! Supports both the Russian and the English keyword sets (the original
//! project ships `russian.keywords` and `english.keywords`).  Token spans
//! are kept in bytes and in Unicode character columns; the same token stream
//! is used by the parser and by the syntax highlighter of the IDE.

use crate::diag::Diagnostic;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kw {
    Module,
    EndModule,
    Use,
    Import,
    Include,
    Alg,
    Begin,
    End,
    Pre,
    Post,
    Assert,
    LoopBegin,
    LoopEnd,
    LoopEndIf,
    For,
    While,
    Times,
    From,
    To,
    Step,
    Exit,
    If,
    Then,
    Else,
    Fi,
    Switch,
    Case,
    Input,
    Output,
    NewlineLit,
    Not,
    And,
    Or,
    Pause,
    Halt,
    Arg,
    Res,
    ArgRes,
    Val,
    True,
    False,
    TypeInt,
    TypeReal,
    TypeStr,
    TypeChar,
    TypeBool,
    TypeFile,
    TypeIntArray,
    TypeRealArray,
    TypeStrArray,
    TypeCharArray,
    TypeBoolArray,
    Tab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Assign,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semi,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Real(f64),
    Str(String),
    Char(char),
    /// `нс` — the new-line symbol inside output lists.
    NewlineLit,
    Kw(Kw),
    Op(Op),
    Newline,
    Comment(String),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    /// 1-based line.
    pub line: u32,
    /// 1-based column in Unicode scalar values.
    pub col: u32,
    /// Length in Unicode scalar values.
    pub len: u32,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.kind, Tok::Kw(k) if k == kw)
    }

    pub fn is_op(&self, op: Op) -> bool {
        matches!(self.kind, Tok::Op(o) if o == op)
    }

    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            Tok::Ident(s) => Some(s),
            _ => None,
        }
    }
}

fn keyword_ru(s: &str) -> Option<Kw> {
    use Kw::*;
    Some(match s {
        "исп" | "модуль" => Module,
        "кон_исп" | "кон_модуля" => EndModule,
        "использовать" => Use,
        "импорт" => Import,
        "включить" | "ВКЛЮЧИТЬ" | "Включить" => Include,
        "алг" | "алгоритм" => Alg,
        "нач" => Begin,
        "кон" => End,
        "дано" => Pre,
        "надо" => Post,
        "утв" => Assert,
        "нц" => LoopBegin,
        "кц" | "кц_" => LoopEnd,
        "кц_при" => LoopEndIf,
        "для" => For,
        "пока" => While,
        "раз" | "раза" => Times,
        "от" => From,
        "до" => To,
        "шаг" => Step,
        "выход" => Exit,
        "если" => If,
        "то" => Then,
        "иначе" => Else,
        "все" | "всё" => Fi,
        "выбор" => Switch,
        "при" => Case,
        "ввод" | "Фввод" => Input,
        "вывод" | "Фвывод" => Output,
        "нс" => NewlineLit,
        "не" => Not,
        "и" => And,
        "или" => Or,
        "пауза" => Pause,
        "стоп" => Halt,
        "арг" => Arg,
        "рез" => Res,
        "аргрез" => ArgRes,
        "знач" => Val,
        "да" | "истина" => True,
        "нет" | "ложь" => False,
        "цел" => TypeInt,
        "вещ" => TypeReal,
        "лит" => TypeStr,
        "сим" => TypeChar,
        "лог" => TypeBool,
        "файл" => TypeFile,
        "целтаб" | "цел_таб" => TypeIntArray,
        "вещтаб" | "вещ_таб" => TypeRealArray,
        "литтаб" | "лит_таб" => TypeStrArray,
        "симтаб" | "сим_таб" => TypeCharArray,
        "логтаб" | "лог_таб" => TypeBoolArray,
        "таб" => Tab,
        _ => return None,
    })
}

fn keyword_en(s: &str) -> Option<Kw> {
    use Kw::*;
    Some(match s {
        "module" => Module,
        "end_module" => EndModule,
        "use" => Use,
        "import" => Import,
        "include" => Include,
        "algorhitm" | "algorithm" => Alg,
        "begin" => Begin,
        "end" => End,
        "pre" => Pre,
        "post" => Post,
        "assert" => Assert,
        "do" => LoopBegin,
        "end_do" => LoopEnd,
        "end_do_if" => LoopEndIf,
        "for" => For,
        "while" => While,
        "times" => Times,
        "from" => From,
        "to" => To,
        "step" => Step,
        "exit" => Exit,
        "if" => If,
        "then" => Then,
        "else" => Else,
        "done" => Fi,
        "switch" => Switch,
        "case" => Case,
        "scan" | "f_scan" => Input,
        "print" | "f_print" => Output,
        "endl" => NewlineLit,
        "not" => Not,
        "and" => And,
        "or" => Or,
        "pause" => Pause,
        "halt" => Halt,
        "read" => Arg,
        "write" => Res,
        "argrez" | "readwrite" | "read_write" => ArgRes,
        "value" => Val,
        "true" => True,
        "false" => False,
        "int" | "integer" => TypeInt,
        "real" => TypeReal,
        "string" => TypeStr,
        "char" => TypeChar,
        "bool" | "boolean" => TypeBool,
        "file" => TypeFile,
        "intarray" => TypeIntArray,
        "realarray" => TypeRealArray,
        "stringarray" => TypeStrArray,
        "chararray" => TypeCharArray,
        "boolarray" => TypeBoolArray,
        "array" => Tab,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
}

/// Detect the keyword language. The original IDE lets the user choose the
/// language explicitly; for files we use the presence of Russian keywords.
pub fn detect_language(src: &str) -> Language {
    // Any Cyrillic letter (even in a comment) is a reliable sign of the
    // Russian keyword set in school practice. Pure ASCII files use English.
    if src.chars().any(|c| matches!(c, 'А'..='я' | 'Ё' | 'ё')) {
        Language::Russian
    } else {
        Language::English
    }
}

fn keyword(s: &str, lang: Language) -> Option<Kw> {
    match lang {
        Language::Russian => keyword_ru(s),
        Language::English => keyword_en(s),
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c == '@' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c == '@' || c.is_alphanumeric()
}

fn is_cyrillic_e(c: char) -> bool {
    c == 'е' || c == 'Е'
}

pub struct Lexer<'a> {
    chars: Vec<char>,
    /// Byte offset of every char, plus the end offset.
    offsets: Vec<usize>,
    pos: usize,
    line: u32,
    col: u32,
    source_len: usize,
    lang: Language,
    _src: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        let lang = detect_language(src);
        let mut chars = Vec::with_capacity(src.len());
        let mut offsets = Vec::with_capacity(src.len() + 1);
        for (off, ch) in src.char_indices() {
            chars.push(ch);
            offsets.push(off);
        }
        offsets.push(src.len());
        Self {
            chars,
            offsets,
            pos: 0,
            line: 1,
            col: 1,
            source_len: src.len(),
            lang,
            _src: src,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn byte_at(&self, char_pos: usize) -> usize {
        self.offsets
            .get(char_pos)
            .copied()
            .unwrap_or(self.source_len)
    }

    fn token(&self, kind: Tok, start: usize, line: u32, col: u32) -> Token {
        Token {
            kind,
            line,
            col,
            len: (self.pos - start) as u32,
            start: self.byte_at(start),
            end: self.byte_at(self.pos),
        }
    }

    pub fn tokenize(mut self) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();
        // UTF-8 BOM at the beginning is not a part of the program.
        if self.peek() == Some('\u{feff}') {
            self.bump();
        }
        loop {
            let ch = match self.peek() {
                Some(c) => c,
                None => break,
            };
            if ch == '\n' {
                let start = self.pos;
                let line = self.line;
                let col = self.col;
                self.bump();
                tokens.push(self.token(Tok::Newline, start, line, col));
                continue;
            }
            if ch == '\r' {
                self.bump();
                continue;
            }
            if ch == ' ' || ch == '\t' || ch == '\u{00a0}' {
                self.bump();
                continue;
            }
            // Comments: `|` and `!` (Kumir), `#` (documentation), `//`.
            if ch == '|' || ch == '!' || ch == '#' || (ch == '/' && self.peek_at(1) == Some('/')) {
                let start = self.pos;
                let line = self.line;
                let col = self.col;
                let mut text = String::new();
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    text.push(c);
                    self.bump();
                }
                tokens.push(self.token(Tok::Comment(text), start, line, col));
                continue;
            }
            let start = self.pos;
            let line = self.line;
            let col = self.col;

            // String or character literal.
            if ch == '\'' || ch == '"' {
                let quote = ch;
                self.bump();
                let mut text = String::new();
                let mut closed = false;
                while let Some(c) = self.peek() {
                    self.bump();
                    if c == quote {
                        closed = true;
                        break;
                    }
                    text.push(c);
                }
                if !closed {
                    errors.push(Diagnostic::error(
                        line,
                        col,
                        (self.pos - start) as u32,
                        "Непарная кавычка в строковом литерале",
                    ));
                }
                let kind = if text.chars().count() == 1 {
                    Tok::Char(text.chars().next().unwrap())
                } else {
                    Tok::Str(text)
                };
                tokens.push(self.token(kind, start, line, col));
                continue;
            }

            // Numbers (decimal, real, exponent form and $hex).
            if ch.is_ascii_digit()
                || (ch == '.' && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()))
            {
                let mut raw = String::new();
                if ch == '$' {
                    // unreachable, handled below
                }
                let mut is_real = false;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        raw.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                if self.peek() == Some('.') && self.peek_at(1) != Some('.') {
                    is_real = true;
                    raw.push('.');
                    self.bump();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            raw.push(c);
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                if let Some(c) = self.peek() {
                    if c == 'e' || c == 'E' || is_cyrillic_e(c) {
                        is_real = true;
                        raw.push('e');
                        self.bump();
                        if let Some(sign) = self.peek() {
                            if sign == '+' || sign == '-' {
                                raw.push(sign);
                                self.bump();
                            }
                        }
                        let mut any = false;
                        while let Some(d) = self.peek() {
                            if d.is_ascii_digit() {
                                any = true;
                                raw.push(d);
                                self.bump();
                            } else {
                                break;
                            }
                        }
                        if !any {
                            errors.push(Diagnostic::error(
                                line,
                                col,
                                (self.pos - start) as u32,
                                "Неверная запись экспоненциальной формы числа",
                            ));
                        }
                    }
                }
                if is_real {
                    match raw.parse::<f64>() {
                        Ok(v) => tokens.push(self.token(Tok::Real(v), start, line, col)),
                        Err(_) => {
                            errors.push(Diagnostic::error(
                                line,
                                col,
                                (self.pos - start) as u32,
                                "Неверная запись вещественного числа",
                            ));
                            tokens.push(self.token(Tok::Real(0.0), start, line, col));
                        }
                    }
                } else {
                    match raw.parse::<i64>() {
                        Ok(v) => tokens.push(self.token(Tok::Int(v), start, line, col)),
                        Err(_) => {
                            errors.push(Diagnostic::error(
                                line,
                                col,
                                (self.pos - start) as u32,
                                "Слишком большое целое число",
                            ));
                            tokens.push(self.token(Tok::Int(0), start, line, col));
                        }
                    }
                }
                continue;
            }

            // $FF hexadecimal integer.
            if ch == '$' {
                let mut raw = String::new();
                self.bump();
                while let Some(c) = self.peek() {
                    if c.is_ascii_hexdigit() {
                        raw.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                if raw.is_empty() {
                    errors.push(Diagnostic::error(
                        line,
                        col,
                        (self.pos - start) as u32,
                        "Пустая шестнадцатеричная константа",
                    ));
                    tokens.push(self.token(Tok::Int(0), start, line, col));
                } else {
                    match i64::from_str_radix(&raw, 16) {
                        Ok(v) => tokens.push(self.token(Tok::Int(v), start, line, col)),
                        Err(_) => {
                            errors.push(Diagnostic::error(
                                line,
                                col,
                                (self.pos - start) as u32,
                                "Слишком большое шестнадцатеричное число",
                            ));
                            tokens.push(self.token(Tok::Int(0), start, line, col));
                        }
                    }
                }
                continue;
            }

            // Identifiers and keywords. Multi-word keywords with an
            // underscore are single tokens; "цел таб" is handled by the parser.
            if is_ident_start(ch) {
                let mut raw = String::new();
                while let Some(c) = self.peek() {
                    if is_ident_continue(c) {
                        raw.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                let kind = match keyword(&raw, self.lang) {
                    Some(kw) => Tok::Kw(kw),
                    None => Tok::Ident(raw),
                };
                tokens.push(self.token(kind, start, line, col));
                continue;
            }

            // Operators and punctuation.
            let two = {
                let a = self.peek();
                let b = self.peek_at(1);
                a.zip(b).map(|(a, b)| (a, b))
            };
            let op2 = match two {
                Some((':', '=')) => Some((Op::Assign, 2)),
                Some(('*', '*')) => Some((Op::Power, 2)),
                Some(('<', '>')) => Some((Op::Ne, 2)),
                Some(('!', '=')) => Some((Op::Ne, 2)),
                Some(('<', '=')) => Some((Op::Le, 2)),
                Some(('>', '=')) => Some((Op::Ge, 2)),
                _ => None,
            };
            if let Some((op, n)) = op2 {
                for _ in 0..n {
                    self.bump();
                }
                tokens.push(self.token(Tok::Op(op), start, line, col));
                continue;
            }
            let op1 = match ch {
                '+' => Some(Op::Plus),
                '-' => Some(Op::Minus),
                '*' => Some(Op::Star),
                '/' => Some(Op::Slash),
                '=' => Some(Op::Eq),
                '<' => Some(Op::Lt),
                '>' => Some(Op::Gt),
                '(' => Some(Op::LParen),
                ')' => Some(Op::RParen),
                '[' => Some(Op::LBracket),
                ']' => Some(Op::RBracket),
                '{' => Some(Op::LBrace),
                '}' => Some(Op::RBrace),
                ',' => Some(Op::Comma),
                ':' => Some(Op::Colon),
                ';' => Some(Op::Semi),
                '≠' => Some(Op::Ne),
                '≤' => Some(Op::Le),
                '≥' => Some(Op::Ge),
                _ => None,
            };
            if let Some(op) = op1 {
                self.bump();
                tokens.push(self.token(Tok::Op(op), start, line, col));
                continue;
            }

            // Unknown character: report and skip it.
            self.bump();
            errors.push(Diagnostic::error(
                line,
                col,
                1,
                format!("Недопустимый символ «{ch}»"),
            ));
        }
        let end = self.byte_at(self.pos);
        tokens.push(Token {
            kind: Tok::Eof,
            line: self.line,
            col: self.col,
            len: 0,
            start: end,
            end,
        });
        // The original language allows semicolons as statement separators.
        // Convert them to newlines to simplify the parser.
        for t in &mut tokens {
            if t.kind == Tok::Op(Op::Semi) {
                t.kind = Tok::Newline;
            }
        }
        (tokens, errors)
    }
}

/// Convenience wrapper used by tests and by the highlighter.
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    Lexer::new(src).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_numbers() {
        let (tokens, errs) = tokenize("цел x := 1 + 2.5e-3");
        assert!(errs.is_empty());
        assert!(matches!(tokens[0].kind, Tok::Kw(Kw::TypeInt)));
        assert!(matches!(tokens[1].kind, Tok::Ident(_)));
        assert!(matches!(tokens[2].kind, Tok::Op(Op::Assign)));
        assert!(matches!(tokens[3].kind, Tok::Int(1)));
        assert!(matches!(tokens[5].kind, Tok::Real(v) if (v - 0.0025).abs() < 1e-12));
    }

    #[test]
    fn strings_and_comments() {
        let (tokens, errs) = tokenize("вывод 'привет', нс | комментарий\n");
        assert!(errs.is_empty());
        assert!(matches!(&tokens[1].kind, Tok::Str(s) if s == "привет"));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, Tok::Kw(Kw::NewlineLit))));
        assert!(tokens.iter().any(|t| matches!(t.kind, Tok::Comment(_))));
    }
}
