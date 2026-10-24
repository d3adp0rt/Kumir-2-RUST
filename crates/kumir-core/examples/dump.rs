//! Developer helper: dump the parsed AST and/or execute a program.
//!
//! cargo run -p kumir-core --example dump -- path/to/program.kum

use std::path::PathBuf;

use kumir_core::interpreter::{Host, RunOptions};

struct DebugHost;
impl Host for DebugHost {
    fn write(&mut self, text: &str) {
        print!("{text}");
    }
    fn read_line(&mut self) -> Result<String, String> {
        println!("<ввод>");
        Ok(String::new())
    }
    fn on_line(&mut self, line: u32) {
        eprintln!("-- line {line}");
    }
}

fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("usage: dump FILE.kum"));
    let loaded = kumir_core::load_program(&path).unwrap_or_else(|errors| {
        for e in errors {
            eprintln!("{e}");
        }
        std::process::exit(1);
    });
    println!("{:#?}", loaded.program);
    let dir = path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut host = DebugHost;
    let result = kumir_core::run(
        &loaded.program,
        &mut host,
        RunOptions {
            source_dir: dir,
            ..Default::default()
        },
    );
    eprintln!("result: {result:?}");
}
