//! Kumir 2 core, rewritten in Rust.
//!
//! The crate provides:
//! * a lexer and parser for the Russian/English Kumir language;
//! * a tree-walking interpreter with the standard library;
//! * console and file input/output;
//! * the `.kum` file format including hidden and protected teacher lines;
//! * token spans for the IDE syntax highlighter.

pub mod actors;
pub mod ast;
pub mod builtins;
pub mod diag;
pub mod highlight;
pub mod interpreter;
pub mod kumfile;
pub mod lexer;
pub mod parser;
pub mod value;

use std::path::Path;

pub use actors::{ActorKind, ActorState};
pub use ast::Program;
pub use diag::{Diagnostic, RuntimeError, Severity};
pub use interpreter::{run, Host, RunOptions, RunOutcome, STOP_MESSAGE};
pub use kumfile::KumFile;
pub use parser::{parse, parse_source};

/// A program loaded from a `.kum` file together with its file metadata.
pub struct LoadedProgram {
    pub file: KumFile,
    pub program: Program,
}

/// Load, decode and parse a `.kum` file. The hidden teacher text is compiled
/// as well, but returned separately in `file` for the editor.
pub fn load_program(path: &Path) -> Result<LoadedProgram, Vec<Diagnostic>> {
    let file = KumFile::load(path).map_err(|e| {
        vec![Diagnostic::error(
            1,
            1,
            1,
            format!("Не удалось открыть файл «{}»: {e}", path.display()),
        )]
    })?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let source = file.source_for_compilation();
    let program = parse_source(
        &source,
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        Some(dir),
    )?;
    Ok(LoadedProgram { file, program })
}

/// Parse source text that is already in memory.
pub fn compile(source: &str, source_dir: Option<&Path>) -> Result<Program, Vec<Diagnostic>> {
    parse_source(source, "<программа>", source_dir)
}
