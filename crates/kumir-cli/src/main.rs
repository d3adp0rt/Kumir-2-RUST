//! `kumir-run` — command-line runner for Kumir 2 `.kum` programs.
//!
//! Examples:
//!   kumir-run program.kum
//!   kumir-run --check program.kum
//!   kumir-run --input answers.txt program.kum

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use kumir_core::actors::{ActorState, RobotField};

use kumir_core::interpreter::{Host, RunOptions};
use kumir_core::{load_program, Diagnostic};

struct ConsoleHost {
    input: Option<BufReader<File>>,
    stop: Arc<AtomicBool>,
}

impl Host for ConsoleHost {
    fn write(&mut self, text: &str) {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(text.as_bytes());
        let _ = lock.flush();
    }

    fn read_line(&mut self) -> Result<String, String> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(kumir_core::STOP_MESSAGE.to_string());
        }
        match &mut self.input {
            Some(r) => {
                let mut line = String::new();
                let n = r
                    .read_line(&mut line)
                    .map_err(|e| format!("Ошибка чтения ввода: {e}"))?;
                if n == 0 {
                    return Err("Ошибка ввода: текст закончился".to_string());
                }
                Ok(line.trim_end_matches(['\r', '\n']).to_string())
            }
            None => {
                let mut line = String::new();
                let n = std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| format!("Ошибка чтения ввода: {e}"))?;
                if n == 0 {
                    return Err("Ошибка ввода: текст закончился".to_string());
                }
                Ok(line.trim_end_matches(['\r', '\n']).to_string())
            }
        }
    }

    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

fn usage() {
    println!(
        "kumir-run {} — исполнитель программ Кумир\n\
         \n\
         Использование: kumir-run [параметры] файл.kum\n\
         \n\
         Параметры:\n\
           -h, --help            показать эту справку\n\
           -V, --version         показать версию\n\
           -c, --check           только проверить синтаксис\n\
           -i, --input FILE      читать консольный ввод из FILE\n\
           -d, --dir DIR         каталог программы (для относительных файлов)\n\
           -f, --field FILE      загрузить поле Робота (.fil)\n\
               --field-size WxH      размер пустого поля Робота (например, 9x7)\n\
               --actor-delay MS      задержка анимации исполнителя в мс\n\
           -e, --evaluate        выполнить скрытый алгоритм @тестирование\n\
           -t, --trace           печатать номера исполняемых строк в stderr\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_diagnostics(errors: &[Diagnostic]) {
    for e in errors {
        eprintln!("{e}");
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut file: Option<PathBuf> = None;
    let mut input: Option<PathBuf> = None;
    let mut dir: Option<PathBuf> = None;
    let mut check_only = false;
    let mut trace = false;
    let mut evaluate = false;
    let mut field: Option<PathBuf> = None;
    let mut field_size: Option<(i32, i32)> = None;
    let mut actor_delay_ms: u64 = 0;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("kumir-run {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-c" | "--check" => check_only = true,
            "-t" | "--trace" => trace = true,
            "-e" | "--evaluate" => evaluate = true,
            "-f" | "--field" => match args.next() {
                Some(p) => field = Some(PathBuf::from(p)),
                None => {
                    eprintln!("kumir-run: после --field требуется имя файла .fil");
                    return ExitCode::from(2);
                }
            },
            "--actor-delay" => match args.next().and_then(|s| s.parse::<u64>().ok()) {
                Some(ms) => actor_delay_ms = ms.min(10_000),
                None => {
                    eprintln!("kumir-run: после --actor-delay требуется число миллисекунд");
                    return ExitCode::from(2);
                }
            },
            "--field-size" | "--size" => {
                match args.next() {
                    Some(s) => match s
                        .split(['x', 'х', ','])
                        .map(|p| p.trim().parse::<i32>())
                        .collect::<Result<Vec<_>, _>>()
                    {
                        Ok(v) if v.len() == 2 && v[0] > 0 && v[1] > 0 => {
                            field_size = Some((v[0], v[1]))
                        }
                        _ => {
                            eprintln!("kumir-run: размер поля должен быть в виде ШИРИНАxВЫСОТА, например 9x7");
                            return ExitCode::from(2);
                        }
                    },
                    None => {
                        eprintln!("kumir-run: после --field-size требуется размер, например 9x7");
                        return ExitCode::from(2);
                    }
                }
            }
            "-i" | "--input" => match args.next() {
                Some(p) => input = Some(PathBuf::from(p)),
                None => {
                    eprintln!("kumir-run: после --input требуется имя файла");
                    return ExitCode::from(2);
                }
            },
            "-d" | "--dir" => match args.next() {
                Some(p) => dir = Some(PathBuf::from(p)),
                None => {
                    eprintln!("kumir-run: после --dir требуется имя каталога");
                    return ExitCode::from(2);
                }
            },
            s if s.starts_with('-') => {
                eprintln!("kumir-run: неизвестный параметр «{s}» (см. --help)");
                return ExitCode::from(2);
            }
            _ => {
                if file.is_some() {
                    eprintln!("kumir-run: можно указать только один файл программы");
                    return ExitCode::from(2);
                }
                file = Some(PathBuf::from(arg));
            }
        }
    }

    let file = match file {
        Some(f) => f,
        None => {
            usage();
            return ExitCode::from(2);
        }
    };
    if file.extension().and_then(|e| e.to_str()) == Some("kod") {
        eprintln!(
            "kumir-run: файлы .kod — это скомпилированный байткод старой версии.\n\
             Пересоберите программу из исходного .kum файла; исходник обычно\n\
             поставляется вместе с .kod."
        );
        return ExitCode::from(2);
    }

    let loaded = match load_program(&file) {
        Ok(p) => p,
        Err(errors) => {
            print_diagnostics(&errors);
            return ExitCode::from(2);
        }
    };
    if check_only {
        println!(
            "Синтаксис в порядке: {} алгоритм(ов), {} глобальных величин.",
            loaded.program.algorithms.len(),
            loaded.program.globals.len()
        );
        return ExitCode::SUCCESS;
    }

    let mut source_dir = dir.unwrap_or_else(|| {
        file.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    });
    if source_dir.as_os_str().is_empty() {
        source_dir = std::env::current_dir().unwrap_or_default();
    }

    let input_file = input.and_then(|p| File::open(p).ok());
    let stop = Arc::new(AtomicBool::new(false));
    let mut host = ConsoleHost {
        input: input_file.map(BufReader::new),
        stop,
    };
    let mut actor_state = ActorState::for_modules(&loaded.program.modules);
    if let Some(field_path) = &field {
        if let Err(e) = actor_state.load_field_file(field_path) {
            eprintln!("kumir-run: {e}");
            return ExitCode::from(2);
        }
    } else if let Some((w, h)) = field_size {
        actor_state.robot = RobotField::empty(w, h);
    }
    let actor = Arc::new(Mutex::new(actor_state));
    if trace {
        eprintln!("Трассировка включена: выводятся только итоговые шаги.");
    }
    match kumir_core::run(
        &loaded.program,
        &mut host,
        RunOptions {
            source_dir,
            actor: Some(actor.clone()),
            evaluate,
            actor_delay: std::time::Duration::from_millis(actor_delay_ms),
            ..Default::default()
        },
    ) {
        Ok(outcome) => {
            let _ = std::io::stdout().flush();
            if trace {
                eprintln!("Выполнено шагов: {}", outcome.steps);
            }
            if let Some(mark) = outcome.mark {
                eprintln!("Оценка: {mark}");
            }
            if let Ok(st) = actor.lock() {
                match st.kind {
                    kumir_core::actors::ActorKind::None => {}
                    kumir_core::actors::ActorKind::Robot => {
                        eprintln!(
                            "Робот: ({}, {}) на поле {}×{}",
                            st.robot.robot_x, st.robot.robot_y, st.robot.width, st.robot.height
                        );
                    }
                    k => eprintln!("Исполнитель: {}", k.russian_name()),
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = std::io::stdout().flush();
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
