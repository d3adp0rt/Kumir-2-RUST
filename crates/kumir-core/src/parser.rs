//! Recursive-descent parser for the Kumir language.
//!
//! The grammar is deliberately tolerant: the original language is used in
//! schools for decades and many accepted programs use slightly loose syntax
//! (optional `:` in `выбор`, declarations anywhere in the body, multi-word
//! algorithm names, `арг рез` written with a space, and so on).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::builtins;
use crate::diag::Diagnostic;
use crate::lexer::{tokenize, Kw, Op, Tok, Token};

type PResult<T> = Result<T, Diagnostic>;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    group_depth: usize,
    alg_names: Vec<String>,
    source_name: String,
    globals: HashSet<String>,
    locals: HashSet<String>,
    in_algorithm: bool,
}

/// Parse a program from source text. `source_dir` is used for `ВКЛЮЧИТЬ`.
pub fn parse_source(
    src: &str,
    source_name: impl Into<String>,
    source_dir: Option<&Path>,
) -> Result<Program, Vec<Diagnostic>> {
    let mut visited = HashSet::new();
    parse_source_inner(src, &source_name.into(), source_dir, 0, &mut visited)
}

fn parse_source_inner(
    src: &str,
    source_name: &str,
    source_dir: Option<&Path>,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
) -> Result<Program, Vec<Diagnostic>> {
    let expanded = match source_dir {
        Some(dir) => expand_includes(src, source_name, dir, 0),
        None => src.to_string(),
    };
    let (tokens, lex_errors) = tokenize(&expanded);
    let mut parser = Parser::new(tokens, source_name.to_string());
    let mut program = match parser.parse_program() {
        Ok(program) => {
            if !lex_errors.is_empty() {
                return Err(lex_errors);
            }
            program
        }
        Err(mut err) => {
            let mut all = lex_errors;
            all.push(std::mem::replace(
                &mut err,
                Diagnostic::error(1, 1, 1, "parse error"),
            ));
            return Err(all);
        }
    };
    if depth < 8 {
        if let Some(dir) = source_dir {
            resolve_modules(&mut program, dir, depth, visited);
        }
    }
    Ok(program)
}

fn is_builtin_module(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("файлы")
        || n == "files"
        || n.contains("строки")
        || n.contains("string")
        || n.contains("робот")
        || n == "robot"
        || n.contains("черепах")
        || n.contains("turtle")
        || n.contains("чертеж")
        || n.contains("чертёж")
        || n.contains("draw")
        || n.contains("рисователь")
        || n.contains("painter")
        || n.contains("_colorer")
        || n.contains("colorer")
        || n.contains("комплексные")
        || n.contains("complex")
}

/// Load user modules referenced by `использовать Name` or
/// `использовать "file.kum"`. Their globals, initializer statements and
/// algorithms are merged into the main program, mirroring the original
/// analyser's module linking.
fn resolve_modules(
    program: &mut Program,
    dir: &Path,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
) {
    let requested = program.modules.clone();
    for module in requested {
        if is_builtin_module(&module) {
            continue;
        }
        let trimmed = module.trim();
        let candidate = if trimmed.ends_with(".kum") || trimmed.contains('/') {
            let p = PathBuf::from(trimmed);
            if p.is_absolute() {
                p
            } else {
                dir.join(p)
            }
        } else {
            let direct = dir.join(trimmed);
            if direct.is_file() {
                direct
            } else {
                dir.join(format!("{trimmed}.kum"))
            }
        };
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate.canonicalize().unwrap_or(candidate.clone());
        if !visited.insert(canonical.clone()) {
            continue;
        }
        let bytes = match std::fs::read(&canonical) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let kum = crate::kumfile::KumFile::from_bytes(&bytes);
        let source = kum.source_for_compilation();
        let module_dir = canonical.parent().unwrap_or(dir);
        if let Ok(sub) = parse_source_inner(
            &source,
            &canonical.to_string_lossy(),
            Some(module_dir),
            depth + 1,
            visited,
        ) {
            // Merge globals first so algorithms can reference them.
            for g in sub.globals {
                if !program.globals.iter().any(|x| x.name == g.name) {
                    program.globals.push(g);
                }
            }
            for s in sub.initializer {
                program.initializer.push(s);
            }
            for alg in sub.algorithms {
                let name = normalize_name(&alg.name);
                if name.is_empty() {
                    continue;
                }
                if !program
                    .algorithms
                    .iter()
                    .any(|a| normalize_name(&a.name) == name)
                {
                    program.algorithms.push(alg);
                }
            }
            for m in sub.modules {
                if !program.modules.contains(&m) {
                    program.modules.push(m);
                }
            }
        }
    }
}

/// Parse without a base directory (no includes).
pub fn parse(src: &str) -> Result<Program, Vec<Diagnostic>> {
    parse_source(src, "<программа>", None)
}

fn expand_includes(src: &str, name: &str, dir: &Path, depth: usize) -> String {
    if depth > 16 {
        return src.to_string();
    }
    let mut out = String::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        let lower = trimmed.to_lowercase();
        let rest = if lower.starts_with("включить") {
            Some(trimmed["включить".len()..].trim())
        } else if lower.starts_with("include") {
            Some(trimmed["include".len()..].trim())
        } else {
            None
        };
        if let Some(rest) = rest {
            let path_part = rest.trim();
            if path_part.len() >= 2 && (path_part.starts_with('\'') || path_part.starts_with('"')) {
                let quote = path_part.chars().next().unwrap();
                if let Some(end) = path_part[1..].find(quote) {
                    let rel = &path_part[1..1 + end];
                    let path = dir.join(rel);
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        out.push_str(&expand_includes(&text, name, dir, depth + 1));
                        out.push('\n');
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

impl Parser {
    fn new(all_tokens: Vec<Token>, source_name: String) -> Self {
        let tokens: Vec<Token> = all_tokens
            .into_iter()
            .filter(|t| !matches!(t.kind, Tok::Comment(_)))
            .collect();
        let mut p = Self {
            tokens,
            pos: 0,
            group_depth: 0,
            alg_names: Vec::new(),
            source_name,
            globals: HashSet::new(),
            locals: HashSet::new(),
            in_algorithm: false,
        };
        p.alg_names = p.collect_algorithm_names();
        p
    }

    // ------------------------------------------------------------------
    // Token helpers
    // ------------------------------------------------------------------

    fn tok(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn kind(&self) -> &Tok {
        &self.tok().kind
    }

    fn peek_kind(&self, n: usize) -> &Tok {
        &self
            .tokens
            .get(self.pos + n)
            .unwrap_or_else(|| self.tokens.last().unwrap())
            .kind
    }

    fn line(&self) -> u32 {
        self.tok().line
    }

    fn advance(&mut self) -> Token {
        let t = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| self.tokens.last().unwrap().clone());
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), Tok::Eof)
    }

    fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.kind(), Tok::Kw(k) if *k == kw)
    }

    fn is_op(&self, op: Op) -> bool {
        matches!(self.kind(), Tok::Op(o) if *o == op)
    }

    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.is_kw(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, op: Op) -> bool {
        if self.is_op(op) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: Kw, what: &str) -> PResult<Token> {
        if self.is_kw(kw) {
            Ok(self.advance())
        } else {
            Err(self.error_here(format!("Ожидалось «{what}»")))
        }
    }

    fn expect_op(&mut self, op: Op, what: &str) -> PResult<Token> {
        if self.is_op(op) {
            Ok(self.advance())
        } else {
            Err(self.error_here(format!("Ожидалось «{what}»")))
        }
    }

    fn expect_ident(&mut self) -> PResult<String> {
        match self.kind().clone() {
            Tok::Ident(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.error_here("Ожидалось имя")),
        }
    }

    /// Variable and parameter names in Kumir may consist of several words
    /// (`цел число проходов по таблицам`). Words are separated by spaces and
    /// never by commas, so the whole identifier run belongs to one name.
    fn parse_name(&mut self) -> PResult<String> {
        let mut parts = Vec::new();
        while let Tok::Ident(word) = self.kind().clone() {
            parts.push(word);
            self.advance();
        }
        if parts.is_empty() {
            return Err(self.error_here("Ожидалось имя"));
        }
        Ok(normalize_name(&parts.join(" ")))
    }

    fn declare_name(&mut self, name: &str) {
        let n = normalize_name(name);
        if self.in_algorithm {
            self.locals.insert(n);
        } else {
            self.globals.insert(n);
        }
    }

    fn declared_prefix(&self, run: &[String]) -> Option<usize> {
        for take in (1..=run.len()).rev() {
            let candidate = normalize_name(&run[..take].join(" "));
            if self.locals.contains(&candidate) || self.globals.contains(&candidate) {
                return Some(take);
            }
        }
        None
    }

    fn error_here(&self, msg: impl Into<String>) -> Diagnostic {
        let t = self.tok();
        Diagnostic::error(t.line, t.col, t.len.max(1), msg)
    }

    fn skip_seps(&mut self) {
        while matches!(self.kind(), Tok::Newline) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        self.skip_seps();
    }

    fn starts_type(&self) -> bool {
        matches!(
            self.kind(),
            Tok::Kw(
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
                    | Kw::TypeBoolArray
            )
        )
    }

    // ------------------------------------------------------------------
    // Pre-scan of algorithm names (they may contain spaces)
    // ------------------------------------------------------------------

    fn collect_algorithm_names(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < self.tokens.len() {
            if self.tokens[i].is_kw(Kw::Alg) {
                let mut j = i + 1;
                // Optional return type.
                if j < self.tokens.len() && is_type_token(&self.tokens[j].kind) {
                    j += 1;
                    if j < self.tokens.len() && self.tokens[j].is_kw(Kw::Tab) {
                        j += 1;
                    }
                }
                let mut parts = Vec::new();
                while j < self.tokens.len() {
                    match &self.tokens[j].kind {
                        Tok::Ident(s) => {
                            parts.push(s.clone());
                            j += 1;
                        }
                        _ => break,
                    }
                }
                if !parts.is_empty() {
                    result.push(normalize_name(&parts.join(" ")));
                }
                i = j;
            } else {
                i += 1;
            }
        }
        result
    }

    fn known_callables(&self) -> impl Iterator<Item = &str> {
        self.alg_names
            .iter()
            .map(|s| s.as_str())
            .chain(builtins::known_names().iter().copied())
    }

    // ------------------------------------------------------------------
    // Top level
    // ------------------------------------------------------------------

    fn parse_program(&mut self) -> PResult<Program> {
        let mut program = Program {
            modules: Vec::new(),
            globals: Vec::new(),
            initializer: Vec::new(),
            algorithms: Vec::new(),
            main: None,
            source_name: self.source_name.clone(),
        };
        loop {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if self.is_kw(Kw::Use) || self.is_kw(Kw::Import) {
                self.advance();
                loop {
                    match self.kind().clone() {
                        Tok::Ident(name) => {
                            self.advance();
                            program.modules.push(name);
                        }
                        Tok::Str(name) => {
                            self.advance();
                            program.modules.push(name);
                        }
                        _ => break,
                    }
                    if !self.eat_op(Op::Comma) {
                        break;
                    }
                }
                continue;
            }
            if self.is_kw(Kw::Module) {
                self.advance();
                if matches!(self.kind(), Tok::Ident(_)) {
                    self.advance();
                }
                continue;
            }
            if self.is_kw(Kw::EndModule) {
                self.advance();
                continue;
            }
            // "кон исп" (two tokens) as end-module.
            if self.is_kw(Kw::End) && self.peek_kind(1) == &Tok::Kw(Kw::Module) {
                self.advance();
                self.advance();
                continue;
            }
            if self.is_kw(Kw::Alg) {
                let alg = self.parse_algorithm()?;
                let is_main = alg.name.trim().is_empty();
                let idx = program.algorithms.len();
                program.algorithms.push(alg);
                // The first algorithm of the file is the entry point, even
                // when it has a name (this is how Kumir 2 works).
                let _ = is_main;
                if program.main.is_none() {
                    program.main = Some(idx);
                }
                continue;
            }
            if self.starts_type() {
                let decls = self.parse_declaration()?;
                program.globals.extend(decls);
                continue;
            }
            let stmt = self.parse_statement()?;
            program.initializer.push(stmt);
        }
        Ok(program)
    }

    fn parse_algorithm(&mut self) -> PResult<Algorithm> {
        let alg_tok = self.expect_kw(Kw::Alg, "алг")?;
        let ret = if self.starts_type() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let mut parts = Vec::new();
        while let Tok::Ident(s) = self.kind().clone() {
            // A name cannot cross the end of the header line.
            parts.push(s);
            self.advance();
        }
        let name = normalize_name(&parts.join(" "));
        let previous_in_algorithm = self.in_algorithm;
        self.in_algorithm = true;
        self.locals.clear();
        let mut params = Vec::new();
        if self.eat_op(Op::LParen) {
            self.group_depth += 1;
            self.skip_newlines();
            params = self.parse_params()?;
            self.expect_op(Op::RParen, ")")?;
            self.group_depth -= 1;
        }
        for p in &params {
            self.locals.insert(normalize_name(&p.name));
        }
        self.skip_newlines();

        let mut pre = Vec::new();
        if self.eat_kw(Kw::Pre) {
            self.skip_newlines();
            pre = self.parse_block(&[Kw::Post, Kw::Begin])?;
            self.skip_newlines();
        }
        let mut post = Vec::new();
        if self.eat_kw(Kw::Post) {
            self.skip_newlines();
            post = self.parse_block(&[Kw::Begin])?;
            self.skip_newlines();
        }
        self.expect_kw(Kw::Begin, "нач")?;
        let body = self.parse_block(&[Kw::End])?;
        self.expect_kw(Kw::End, "кон")?;
        self.in_algorithm = previous_in_algorithm;
        Ok(Algorithm {
            name,
            params,
            ret,
            pre,
            post,
            body,
            line: alg_tok.line,
        })
    }

    fn parse_params(&mut self) -> PResult<Vec<VarDecl>> {
        let mut result = Vec::new();
        let mut current_access = Access::In;
        let mut current_type: Option<TypeRef> = None;
        loop {
            self.skip_newlines();
            if self.is_op(Op::RParen) || self.at_eof() {
                break;
            }
            // Access modifiers (арг, рез, аргрез, арг рез).
            let mut access_changed = false;
            match self.kind() {
                Tok::Kw(Kw::Arg) => {
                    self.advance();
                    if self.eat_kw(Kw::Res) {
                        current_access = Access::InOut;
                    } else {
                        current_access = Access::In;
                    }
                    access_changed = true;
                }
                Tok::Kw(Kw::Res) => {
                    self.advance();
                    current_access = Access::Out;
                    access_changed = true;
                }
                Tok::Kw(Kw::ArgRes) => {
                    self.advance();
                    current_access = Access::InOut;
                    access_changed = true;
                }
                _ => {}
            }
            if self.starts_type() {
                current_type = Some(self.parse_type()?);
            }
            let line = self.line();
            let name = match self.kind() {
                Tok::Ident(_) => self.parse_name()?,
                _ => break,
            };
            let bounds = self.parse_bounds()?;
            let mut typ = current_type
                .clone()
                .unwrap_or(TypeRef::simple(BaseType::Int));
            if !bounds.is_empty() {
                typ.is_array = true;
            }
            result.push(VarDecl {
                name,
                typ,
                bounds,
                init: None,
                access: current_access,
                line,
            });
            if access_changed {
                current_access = Access::In;
            }
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        Ok(result)
    }

    fn parse_bounds(&mut self) -> PResult<Vec<(Expr, Option<Expr>)>> {
        let mut bounds = Vec::new();
        if self.eat_op(Op::LBracket) {
            self.group_depth += 1;
            self.skip_newlines();
            loop {
                let start = self.parse_expr()?;
                let end = if self.eat_op(Op::Colon) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                bounds.push((start, end));
                if self.eat_op(Op::Comma) {
                    self.skip_newlines();
                    continue;
                }
                break;
            }
            self.expect_op(Op::RBracket, "]")?;
            self.group_depth -= 1;
        }
        Ok(bounds)
    }

    fn parse_type(&mut self) -> PResult<TypeRef> {
        let (base, mut is_array) = match self.kind() {
            Tok::Kw(Kw::TypeInt) => (BaseType::Int, false),
            Tok::Kw(Kw::TypeReal) => (BaseType::Real, false),
            Tok::Kw(Kw::TypeStr) => (BaseType::Str, false),
            Tok::Kw(Kw::TypeChar) => (BaseType::Char, false),
            Tok::Kw(Kw::TypeBool) => (BaseType::Bool, false),
            Tok::Kw(Kw::TypeFile) => (BaseType::File, false),
            Tok::Kw(Kw::TypeIntArray) => (BaseType::Int, true),
            Tok::Kw(Kw::TypeRealArray) => (BaseType::Real, true),
            Tok::Kw(Kw::TypeStrArray) => (BaseType::Str, true),
            Tok::Kw(Kw::TypeCharArray) => (BaseType::Char, true),
            Tok::Kw(Kw::TypeBoolArray) => (BaseType::Bool, true),
            _ => return Err(self.error_here("Ожидался тип")),
        };
        self.advance();
        if self.eat_kw(Kw::Tab) {
            is_array = true;
        } else if matches!(self.kind(), Tok::Ident(s) if s.eq_ignore_ascii_case("array")) {
            self.advance();
            is_array = true;
        }
        Ok(TypeRef { base, is_array })
    }

    fn parse_declaration(&mut self) -> PResult<Vec<VarDecl>> {
        let mut typ = self.parse_type()?;
        let mut result = Vec::new();
        loop {
            let line = self.line();
            let name = self.parse_name()?;
            let bounds = self.parse_bounds()?;
            let init = if self.eat_op(Op::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            let mut this_type = typ.clone();
            if !bounds.is_empty() {
                this_type.is_array = true;
            }
            self.declare_name(&name);
            result.push(VarDecl {
                name,
                typ: this_type,
                bounds,
                init,
                access: Access::Regular,
                line,
            });
            if self.eat_op(Op::Comma) {
                // Kumir allows the type to change in the middle of a list:
                // `файл f, вещ x, y`.
                if self.starts_type() {
                    typ = self.parse_type()?;
                }
                continue;
            }
            break;
        }
        Ok(result)
    }

    // ------------------------------------------------------------------
    // Blocks and statements
    // ------------------------------------------------------------------

    fn at_terminator(&self, terms: &[Kw]) -> bool {
        if self.at_eof() {
            return true;
        }
        if let Tok::Kw(k) = self.kind() {
            terms.contains(k)
        } else {
            false
        }
    }

    fn parse_block(&mut self, terms: &[Kw]) -> PResult<Vec<Stmt>> {
        let mut result = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_terminator(terms) {
                break;
            }
            result.push(self.parse_statement()?);
        }
        Ok(result)
    }

    fn parse_statement(&mut self) -> PResult<Stmt> {
        let line = self.line();
        if self.starts_type() {
            let decls = self.parse_declaration()?;
            return Ok(Stmt {
                kind: StmtKind::Decl(decls),
                line,
            });
        }
        match self.kind() {
            Tok::Kw(Kw::Input) => self.parse_input(),
            Tok::Kw(Kw::Output) => self.parse_output(),
            Tok::Kw(Kw::If) => self.parse_if(),
            Tok::Kw(Kw::Switch) => self.parse_switch(),
            Tok::Kw(Kw::LoopBegin) => self.parse_loop(),
            Tok::Kw(Kw::Assert) => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(Stmt {
                    kind: StmtKind::Assert(expr),
                    line,
                })
            }
            Tok::Kw(Kw::Exit) => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Break,
                    line,
                })
            }
            Tok::Kw(Kw::Pause) => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Pause,
                    line,
                })
            }
            Tok::Kw(Kw::Halt) => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Halt,
                    line,
                })
            }
            Tok::Kw(Kw::Val) => {
                self.advance();
                self.expect_op(Op::Assign, ":=")?;
                let expr = self.parse_expr()?;
                Ok(Stmt {
                    kind: StmtKind::Assign(
                        LValue {
                            name: "знач".to_string(),
                            subs: Vec::new(),
                            line,
                        },
                        expr,
                    ),
                    line,
                })
            }
            _ => {
                let lhs = self.parse_expr()?;
                if self.eat_op(Op::Assign) {
                    let lv = expr_to_lvalue(lhs)?;
                    let rhs = self.parse_expr()?;
                    Ok(Stmt {
                        kind: StmtKind::Assign(lv, rhs),
                        line,
                    })
                } else {
                    Ok(Stmt {
                        kind: StmtKind::Expr(lhs),
                        line,
                    })
                }
            }
        }
    }

    fn parse_output(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Output, "вывод")?;
        let mut args = Vec::new();
        loop {
            if matches!(self.kind(), Tok::Newline | Tok::Eof) {
                break;
            }
            let expr = self.parse_expr()?;
            let mut width = None;
            let mut decimals = None;
            if self.eat_op(Op::Colon) {
                width = Some(self.parse_expr()?);
                if self.eat_op(Op::Colon) {
                    decimals = Some(self.parse_expr()?);
                }
            }
            args.push(OutputArg {
                expr,
                width,
                decimals,
            });
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        Ok(Stmt {
            kind: StmtKind::Output { file: None, args },
            line,
        })
    }

    fn parse_input(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Input, "ввод")?;
        let mut targets = Vec::new();
        loop {
            if matches!(self.kind(), Tok::Newline | Tok::Eof) {
                break;
            }
            let expr = self.parse_expr()?;
            targets.push(expr_to_lvalue(expr)?);
            if !self.eat_op(Op::Comma) {
                break;
            }
        }
        Ok(Stmt {
            kind: StmtKind::Input {
                file: None,
                targets,
            },
            line,
        })
    }

    fn parse_if(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::If, "если")?;
        let condition = self.parse_expr()?;
        self.skip_newlines();
        self.expect_kw(Kw::Then, "то")?;
        let body = self.parse_block(&[Kw::Else, Kw::Fi])?;
        let mut branches = vec![Branch {
            condition: Some(condition),
            body,
            line,
        }];
        self.skip_newlines();
        if self.eat_kw(Kw::Else) {
            let else_line = self.line();
            let body = self.parse_block(&[Kw::Fi])?;
            branches.push(Branch {
                condition: None,
                body,
                line: else_line,
            });
        }
        self.skip_newlines();
        self.expect_kw(Kw::Fi, "все")?;
        Ok(Stmt {
            kind: StmtKind::If(branches),
            line,
        })
    }

    fn parse_switch(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::Switch, "выбор")?;
        let mut branches = Vec::new();
        self.skip_newlines();
        while !self.is_kw(Kw::Fi) && !self.at_eof() {
            if self.eat_kw(Kw::Case) {
                let branch_line = self.line();
                let mut condition = self.parse_expr()?;
                while self.eat_op(Op::Comma) {
                    let next = self.parse_expr()?;
                    condition = Expr {
                        kind: ExprKind::Binary(BinOp::Or, Box::new(condition), Box::new(next)),
                        line: branch_line,
                    };
                }
                self.eat_op(Op::Colon);
                let body = self.parse_block(&[Kw::Case, Kw::Else, Kw::Fi])?;
                branches.push(Branch {
                    condition: Some(condition),
                    body,
                    line: branch_line,
                });
            } else if self.eat_kw(Kw::Else) {
                let branch_line = self.line();
                self.eat_op(Op::Colon);
                let body = self.parse_block(&[Kw::Fi])?;
                branches.push(Branch {
                    condition: None,
                    body,
                    line: branch_line,
                });
            } else {
                return Err(self.error_here("В «выбор» ожидалось «при», «иначе» или «все»"));
            }
            self.skip_newlines();
        }
        self.expect_kw(Kw::Fi, "все")?;
        Ok(Stmt {
            kind: StmtKind::Switch(branches),
            line,
        })
    }

    fn parse_loop(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect_kw(Kw::LoopBegin, "нц")?;
        let kind = if self.eat_kw(Kw::For) {
            let var = self.expect_ident()?;
            self.expect_kw(Kw::From, "от")?;
            let from = self.parse_expr()?;
            self.expect_kw(Kw::To, "до")?;
            let to = self.parse_expr()?;
            let step = if self.eat_kw(Kw::Step) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            LoopKind::For {
                var,
                from,
                to,
                step,
            }
        } else if self.eat_kw(Kw::While) {
            LoopKind::While(self.parse_expr()?)
        } else if self.eat_kw(Kw::Times) {
            LoopKind::Times(self.parse_expr()?)
        } else if !matches!(self.kind(), Tok::Newline | Tok::Eof) {
            // `нц N раз` — the count may precede the keyword.
            let count = self.parse_expr()?;
            self.expect_kw(Kw::Times, "раз")?;
            LoopKind::Times(count)
        } else {
            LoopKind::Forever
        };
        self.skip_newlines();
        let body = self.parse_block(&[Kw::LoopEnd, Kw::LoopEndIf])?;
        let mut end_cond = None;
        if self.eat_kw(Kw::LoopEndIf) {
            end_cond = Some(self.parse_expr()?);
        } else {
            self.expect_kw(Kw::LoopEnd, "кц")?;
            if self.eat_kw(Kw::Case) {
                end_cond = Some(self.parse_expr()?);
            }
        }
        Ok(Stmt {
            kind: StmtKind::Loop {
                kind,
                end_cond,
                body,
            },
            line,
        })
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut left = self.parse_and()?;
        loop {
            self.maybe_skip_newlines_in_group();
            if self.eat_kw(Kw::Or) {
                let right = self.parse_and()?;
                let line = left.line;
                left = Expr {
                    kind: ExprKind::Binary(BinOp::Or, Box::new(left), Box::new(right)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut left = self.parse_not()?;
        loop {
            self.maybe_skip_newlines_in_group();
            if self.eat_kw(Kw::And) {
                let right = self.parse_not()?;
                let line = left.line;
                left = Expr {
                    kind: ExprKind::Binary(BinOp::And, Box::new(left), Box::new(right)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> PResult<Expr> {
        if self.eat_kw(Kw::Not) {
            let line = self.line();
            let operand = self.parse_not()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Not, Box::new(operand)),
                line,
            });
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut left = self.parse_add()?;
        loop {
            self.maybe_skip_newlines_in_group();
            let op = match self.kind() {
                Tok::Op(Op::Eq) => Some(BinOp::Eq),
                Tok::Op(Op::Ne) => Some(BinOp::Ne),
                Tok::Op(Op::Lt) => Some(BinOp::Lt),
                Tok::Op(Op::Gt) => Some(BinOp::Gt),
                Tok::Op(Op::Le) => Some(BinOp::Le),
                Tok::Op(Op::Ge) => Some(BinOp::Ge),
                _ => None,
            };
            if let Some(op) = op {
                self.advance();
                self.skip_newlines();
                let right = self.parse_add()?;
                let line = left.line;
                left = Expr {
                    kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> PResult<Expr> {
        let mut left = self.parse_mul()?;
        loop {
            self.maybe_skip_newlines_in_group();
            let op = if self.is_op(Op::Plus) {
                Some(BinOp::Add)
            } else if self.is_op(Op::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };
            if let Some(op) = op {
                self.advance();
                self.skip_newlines();
                let right = self.parse_mul()?;
                let line = left.line;
                left = Expr {
                    kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            self.maybe_skip_newlines_in_group();
            let op = if self.is_op(Op::Star) {
                Some(BinOp::Mul)
            } else if self.is_op(Op::Slash) {
                Some(BinOp::Div)
            } else {
                None
            };
            if let Some(op) = op {
                self.advance();
                self.skip_newlines();
                let right = self.parse_unary()?;
                let line = left.line;
                left = Expr {
                    kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                    line,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        if self.eat_kw(Kw::Not) {
            let line = self.line();
            let operand = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Not, Box::new(operand)),
                line,
            });
        }
        if self.eat_op(Op::Minus) {
            let line = self.line();
            let operand = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Neg, Box::new(operand)),
                line,
            });
        }
        if self.eat_op(Op::Plus) {
            let line = self.line();
            let operand = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Unary(UnOp::Plus, Box::new(operand)),
                line,
            });
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> PResult<Expr> {
        let base = self.parse_primary()?;
        if self.eat_op(Op::Power) {
            let line = base.line;
            let exponent = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Binary(BinOp::Pow, Box::new(base), Box::new(exponent)),
                line,
            });
        }
        Ok(base)
    }

    fn maybe_skip_newlines_in_group(&mut self) {
        if self.group_depth > 0 {
            self.skip_newlines();
        }
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let t = self.tok().clone();
        let line = t.line;
        match t.kind {
            Tok::Int(v) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Int(v),
                    line,
                })
            }
            Tok::Real(v) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Real(v),
                    line,
                })
            }
            Tok::Str(s) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Str(s),
                    line,
                })
            }
            Tok::Char(c) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Char(c),
                    line,
                })
            }
            Tok::Kw(Kw::True) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    line,
                })
            }
            Tok::Kw(Kw::False) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    line,
                })
            }
            Tok::Kw(Kw::NewlineLit) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Newline,
                    line,
                })
            }
            Tok::Kw(Kw::Val) => {
                // `знач` is read as an ordinary value in expressions.
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Var("знач".to_string()),
                    line,
                })
            }
            Tok::Op(Op::LBrace) => {
                self.advance();
                self.group_depth += 1;
                self.skip_newlines();
                let mut items = Vec::new();
                if !self.is_op(Op::RBrace) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.eat_op(Op::Comma) {
                            self.skip_newlines();
                            continue;
                        }
                        break;
                    }
                }
                self.skip_newlines();
                self.expect_op(Op::RBrace, "}")?;
                self.group_depth -= 1;
                Ok(Expr {
                    kind: ExprKind::ArrayLit(items),
                    line,
                })
            }
            Tok::Op(Op::LParen) => {
                self.advance();
                self.group_depth += 1;
                self.skip_newlines();
                let expr = self.parse_expr()?;
                self.skip_newlines();
                self.expect_op(Op::RParen, ")")?;
                self.group_depth -= 1;
                Ok(expr)
            }
            Tok::Ident(_) => self.parse_ident_expr(),
            _ => Err(self.error_here("Ожидалось выражение")),
        }
    }

    /// Parse a variable, array element, or a call. Kumir allows spaces in
    /// built-in and user algorithm names ("открыть на чтение"), so a longest
    /// match over consecutive identifiers followed by `(` is attempted.
    fn parse_ident_expr(&mut self) -> PResult<Expr> {
        let line = self.line();
        let first = match self.kind().clone() {
            Tok::Ident(s) => s,
            _ => return Err(self.error_here("Ожидалось имя")),
        };

        // Collect a run of identifiers. `файл` is a keyword but occurs in
        // multi-word standard functions such as `существует файл`.
        let mut run: Vec<String> = Vec::new();
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                Tok::Ident(s) => {
                    run.push(s.clone());
                    i += 1;
                }
                Tok::Kw(Kw::TypeFile) if !run.is_empty() => {
                    run.push("файл".to_string());
                    i += 1;
                }
                _ => break,
            }
        }
        let followed_by_paren = i < self.tokens.len() && self.tokens[i].is_op(Op::LParen);
        if followed_by_paren {
            // Longest known name first.
            let mut matched: Option<usize> = None;
            for take in (1..=run.len()).rev() {
                let candidate = normalize_name(&run[..take].join(" "));
                if self
                    .known_callables()
                    .any(|n| normalize_name(n) == candidate)
                {
                    matched = Some(take);
                    break;
                }
            }
            // Unknown calls are still parsed as calls: the interpreter will
            // report a meaningful "algorithm not found" error at runtime.
            let take = matched.unwrap_or(1);
            let candidate = normalize_name(&run[..take].join(" "));
            self.pos += take;
            self.advance(); // '('
            self.group_depth += 1;
            self.skip_newlines();
            let mut args = Vec::new();
            if !self.is_op(Op::RParen) {
                loop {
                    args.push(self.parse_expr()?);
                    if self.eat_op(Op::Comma) {
                        self.skip_newlines();
                        continue;
                    }
                    break;
                }
            }
            self.skip_newlines();
            self.expect_op(Op::RParen, ")")?;
            self.group_depth -= 1;
            return Ok(Expr {
                kind: ExprKind::Call(candidate, args),
                line,
            });
        }

        // A declared variable always wins over a same-named built-in or
        // actor command.
        let declared = if !followed_by_paren {
            self.declared_prefix(&run)
        } else {
            None
        };

        // A bare identifier may be a call of a zero-argument built-in or
        // algorithm: `вверх`, `закрасить`, `сверху стена`.
        let mut bare_call: Option<usize> = None;
        if declared.is_none() {
            for take in (1..=run.len()).rev() {
                let candidate = normalize_name(&run[..take].join(" "));
                if self
                    .known_callables()
                    .any(|n| normalize_name(n) == candidate)
                {
                    bare_call = Some(take);
                    break;
                }
            }
        }
        if let Some(take) = bare_call {
            let candidate = normalize_name(&run[..take].join(" "));
            self.pos += take;
            return Ok(Expr {
                kind: ExprKind::Call(candidate, Vec::new()),
                line,
            });
        }

        // Plain (possibly multi-word) identifier with subscripts.
        let take = run.len().max(1);
        self.pos += take;
        let name = if take > 1 {
            normalize_name(&run[..take].join(" "))
        } else {
            first
        };
        let mut expr = Expr {
            kind: ExprKind::Var(name),
            line,
        };
        while self.is_op(Op::LBracket) {
            self.advance();
            self.group_depth += 1;
            self.skip_newlines();
            let mut subs = Vec::new();
            loop {
                let start = self.parse_expr()?;
                let end = if self.eat_op(Op::Colon) {
                    if self.is_op(Op::RBracket) || self.is_op(Op::Comma) {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    }
                } else {
                    None
                };
                subs.push(Subscript { start, end });
                if self.eat_op(Op::Comma) {
                    self.skip_newlines();
                    continue;
                }
                break;
            }
            self.expect_op(Op::RBracket, "]")?;
            self.group_depth -= 1;
            expr = Expr {
                kind: ExprKind::Index(Box::new(expr), subs),
                line,
            };
        }
        Ok(expr)
    }
}

fn is_type_token(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Kw(
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
                | Kw::TypeBoolArray
        )
    )
}

fn expr_to_lvalue(expr: Expr) -> PResult<LValue> {
    let line = expr.line;
    match expr.kind {
        ExprKind::Var(name) => Ok(LValue {
            name,
            subs: Vec::new(),
            line,
        }),
        ExprKind::Index(base, subs) => match base.kind {
            ExprKind::Var(name) => Ok(LValue { name, subs, line }),
            _ => Err(Diagnostic::error(
                line,
                1,
                1,
                "Слева от := должна быть переменная или элемент массива",
            )),
        },
        _ => Err(Diagnostic::error(
            line,
            1,
            1,
            "Слева от := должна быть переменная или элемент массива",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_program() {
        let program = parse("алг\nнач\nцел a=2\nвывод a+2\nкон\n").unwrap();
        assert_eq!(program.algorithms.len(), 1);
        assert!(program.main.is_some());
        assert_eq!(program.algorithms[0].body.len(), 2);
    }

    #[test]
    fn parses_named_algorithm_and_loop() {
        let src = "алг Сумма чисел\nнач\nцел s=0\nнц для i от 1 до 10\ns:=s+i\nкц\nвывод s\nкон\n";
        let program = parse(src).unwrap();
        assert_eq!(program.algorithms[0].name, "Сумма чисел");
        assert_eq!(program.algorithms[0].body.len(), 3);
    }

    #[test]
    fn parses_switch_and_multiline_params() {
        let src = "алг F(аргрез лит s, арг лит w, n)\nнач\nвыбор\nпри n=1: вывод s\nиначе вывод w\nвсе\nкон\n";
        let program = parse(src).unwrap();
        assert_eq!(program.algorithms[0].params.len(), 3);
    }

    #[test]
    fn joins_multiword_call() {
        let src = "алг\nнач\nфайл Fin\nFin:=открыть на чтение('a.txt')\nзакрыть(Fin)\nкон\n";
        let program = parse(src).unwrap();
        // Just check that parsing succeeded and the call was recorded.
        assert_eq!(program.algorithms[0].body.len(), 3);
    }
}
