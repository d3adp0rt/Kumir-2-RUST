//! End-to-end tests: parse a Kumir source, execute it and compare the output.

use std::collections::VecDeque;
use std::path::PathBuf;

use kumir_core::interpreter::{Host, RunOptions};
use kumir_core::{parse, run};

struct TestHost {
    input: VecDeque<String>,
    output: String,
}

impl Host for TestHost {
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }
    fn read_line(&mut self) -> Result<String, String> {
        self.input
            .pop_front()
            .ok_or_else(|| "Ошибка ввода: текст закончился".to_string())
    }
}

fn exec_with_dir(src: &str, input: &[&str], dir: PathBuf) -> Result<String, String> {
    let program = parse(src).map_err(|e| {
        e.into_iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut host = TestHost {
        input: input.iter().map(|s| s.to_string()).collect(),
        output: String::new(),
    };
    run(
        &program,
        &mut host,
        RunOptions {
            source_dir: dir,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(host.output)
}

fn exec(src: &str, input: &[&str]) -> Result<String, String> {
    exec_with_dir(src, input, PathBuf::new())
}

#[test]
fn arithmetic_and_output_formatting() {
    let src = "\u{feff}алг\nнач\nцел a=5, b=7, c\nc:=a+b\nвывод a,'+',b,'=',c,нс\nвещ x=1.2345678\nвывод '>',x:7:3,'<'\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "5+7=12\n>  1.235<");
}

#[test]
fn loops_and_times_loop() {
    let src = "алг\nнач\nцел i\nнц для i от 1 до 5\nвывод i,' '\nкц\nвывод нс\nнц 3 раз\nвывод 'x'\nкц\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "1 2 3 4 5 \nxxx");
}

#[test]
fn while_and_repeat_condition() {
    let src = "алг\nнач\nцел n=0\nнц пока n<3\nn:=n+1\nкц при n=2\nвывод n\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "2");
}

#[test]
fn recursion_and_return_value() {
    let src = "алг\nнач\nвывод Fact(5)\nкон\nалг цел Fact(цел n)\nнач\nесли n<=1 то знач:=1 иначе знач:=n*Fact(n-1) все\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "120");
}

#[test]
fn arrays_are_one_based() {
    let src = "алг\nнач\nцелтаб A[1:3]\nцел i\nнц для i от 1 до 3\nA[i]:=i*i\nкц\nвывод A[1],' ',A[3]\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "1 9");
}

#[test]
fn array_initializer_braces() {
    let src = "алг\nнач\nцелтаб A[1:3] = {4, 5, 6}\nвывод A[2]\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "5");
}

#[test]
fn strings_slices_and_char_assignment() {
    let src = "алг\nнач\nлит s='привет'\nвывод s[1:3],' ',s[2],нс\ns[1]:='П'\nвывод s\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "при р\nПривет");
}

#[test]
fn switch_statement() {
    let src = "алг\nнач\nцел m=2\nвыбор\nпри m=1: вывод 'один'\nпри m=2: вывод 'два'\nиначе вывод 'нет'\nвсе\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "два");
}

#[test]
fn multiword_names() {
    let src = "алг\nнач\nцел число проходов по таблицам\nчисло проходов по таблицам:=41\nвывод число проходов по таблицам\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "41");
}

#[test]
fn console_input_by_type() {
    let src = "алг\nнач\nцел n\nвещ x\nлит s\nввод n\nввод x\nввод s\nвывод n,' ',x,' ',s\nкон\n";
    assert_eq!(
        exec(src, &["7", "2.5", "привет мир"]).unwrap(),
        "7 2.5 привет мир"
    );
}

#[test]
fn files_read_write() {
    let dir = std::env::temp_dir().join(format!("kumir-rust-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("in.txt"), "1 2 3\n").unwrap();
    let src = "использовать Файлы\nалг\nнач\nфайл f,g\nf:=открыть на чтение('in.txt')\ng:=открыть на запись('out.txt')\nцел x,s=0\nнц пока не конец файла(f)\nввод f,x\ns:=s+x\nкц\nвывод g,s\nзакрыть(f)\nзакрыть(g)\nкон\n";
    let out = exec_with_dir(src, &[], dir.clone()).unwrap();
    assert_eq!(out, "");
    let written = std::fs::read_to_string(dir.join("out.txt")).unwrap();
    assert_eq!(written.trim_start_matches('\u{feff}'), "6");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn halt_and_assert() {
    let src = "алг\nнач\nвывод 'a'\nстоп\nвывод 'b'\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "a");
    let src = "алг\nнач\nутв 1=1\nвывод 'ok'\nкон\n";
    assert_eq!(exec(src, &[]).unwrap(), "ok");
}

#[test]
fn upstream_sample_programs_parse() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../kumir-source/testing/prg/kumir_prog");
    if !base.exists() {
        return;
    }
    let mut checked = 0;
    for entry in std::fs::read_dir(&base).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("kum") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        if name.ends_with(".bad") || name.starts_with('_') {
            continue;
        }
        let file = kumir_core::KumFile::load(&path).unwrap();
        let src = file.source_for_compilation();
        let dir = path.parent().unwrap();
        match kumir_core::parse_source(&src, name.to_string(), Some(dir)) {
            Ok(_) => checked += 1,
            Err(errors) => panic!(
                "{}: {}",
                path.display(),
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
    assert!(checked >= 50, "only {checked} sample programs were checked");
}

#[test]
fn robot_actor_basics() {
    use kumir_core::actors::{ActorKind, ActorState};
    use std::sync::{Arc, Mutex};

    let src = "использовать Робот\nалг\nнач\nвниз\nзакрасить\nвывод клетка закрашена, ' ', сверху свободно\nкон\n";
    let program = parse(src).unwrap();
    let state = Arc::new(Mutex::new(ActorState::for_modules(&program.modules)));
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: PathBuf::new(),
            actor: Some(state.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(host.output, "да да");
    let st = state.lock().unwrap();
    assert_eq!(st.kind, ActorKind::Robot);
    assert_eq!((st.robot.robot_x, st.robot.robot_y), (0, 1));
    assert!(st.robot.current().color);
}

#[test]
fn robot_field_file_loads() {
    use kumir_core::actors::RobotField;
    let dir = std::env::temp_dir().join(format!("kumir-field-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.fil");
    std::fs::write(
        &path,
        "; Field Size: x, y\n3 2\n; Robot position: x, y\n1 1\n; cells\n0 0 8 0 0.0 0 $ $\n1 1 8 1 1.5 20 $ Б\n; End Of File\n",
    )
    .unwrap();
    let f = RobotField::load_fil(&path).unwrap();
    assert_eq!((f.width, f.height), (3, 2));
    assert_eq!((f.robot_x, f.robot_y), (1, 1));
    assert!(f.cell(1, 1).unwrap().color);
    assert_eq!(f.cell(1, 1).unwrap().symbol1, 'Б');
    assert!(f.cell(0, 0).unwrap().walls & kumir_core::actors::UP_WALL != 0);
    assert!(!f.cell(1, 1).unwrap().point);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn turtle_and_draw_actors() {
    use kumir_core::actors::ActorState;
    use std::sync::{Arc, Mutex};

    let src = "использовать Черепаха\nалг\nнач\nвперед(10)\nвлево(90)\nвперед(10)\nкон\n";
    let program = parse(src).unwrap();
    let state = Arc::new(Mutex::new(ActorState::for_modules(&program.modules)));
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: PathBuf::new(),
            actor: Some(state.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let st = state.lock().unwrap();
    assert_eq!(st.turtle.segments.len(), 2);
    assert!((st.turtle.x + 10.0).abs() < 1e-9);
    assert!((st.turtle.y - 10.0).abs() < 1e-9);

    let src = "использовать Чертежник\nалг\nнач\nопустить перо\nсместиться в точку(5, 5)\nподнять перо\nсместиться на вектор(3, 0)\nнаписать(2, 'точка')\nкон\n";
    let program = parse(src).unwrap();
    let state = Arc::new(Mutex::new(ActorState::for_modules(&program.modules)));
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: PathBuf::new(),
            actor: Some(state.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let st = state.lock().unwrap();
    assert_eq!(st.draw.segments.len(), 1);
    assert_eq!(st.draw.captions.len(), 1);
    assert_eq!(st.draw.captions[0].text, "точка");
}

#[test]
fn user_modules_are_linked() {
    let dir = std::env::temp_dir().join(format!("kumir-mod-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lib.kum"),
        "алг цел Double(цел x)\nнач\nзнач:=2*x\nкон\n",
    )
    .unwrap();
    let main = "использовать \"lib.kum\"\nалг\nнач\nвывод Double(21)\nкон\n";
    let file = kumir_core::KumFile::from_str(main);
    let program =
        kumir_core::parse_source(&file.source_for_compilation(), "main.kum", Some(&dir)).unwrap();
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: dir.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(host.output, "42");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn robot_internal_teacher_functions() {
    use kumir_core::actors::ActorState;
    use std::sync::{Arc, Mutex};
    let src = "использовать Робот\nалг\nнач\nвниз\nцел r,c\n@@робот(r,c)\nвывод r,' ',c\nкон\n";
    let program = parse(src).unwrap();
    let state = Arc::new(Mutex::new(ActorState::for_modules(&program.modules)));
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: PathBuf::new(),
            actor: Some(state.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(host.output, "2 1");
}

#[test]
fn teacher_test_algorithm_runs() {
    use kumir_core::actors::ActorState;
    use std::sync::{Arc, Mutex};
    let src = "использовать Робот\nалг Move\nнач\nвниз\nкон\nалг цел @тестирование\nнач\nMove\nцел r,c\n@@робот(r,c)\nесли r=2 то знач:=10 иначе знач:=2 все\nкон\n";
    let program = parse(src).unwrap();
    let state = Arc::new(Mutex::new(ActorState::for_modules(&program.modules)));
    let mut host = TestHost {
        input: VecDeque::new(),
        output: String::new(),
    };
    let outcome = kumir_core::run(
        &program,
        &mut host,
        RunOptions {
            source_dir: PathBuf::new(),
            actor: Some(state),
            evaluate: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(outcome.mark, Some(10));
    assert!(host.output.contains("Оценка: 10"));
}

#[test]
fn debugger_variable_snapshots() {
    struct VarHost {
        output: String,
        seen: Vec<Vec<(String, String, String)>>,
    }
    impl Host for VarHost {
        fn write(&mut self, text: &str) {
            self.output.push_str(text);
        }
        fn read_line(&mut self) -> Result<String, String> {
            Err("нет ввода".to_string())
        }
        fn wants_vars(&self) -> bool {
            true
        }
        fn on_vars(&mut self, vars: Vec<(String, String, String)>) {
            self.seen.push(vars);
        }
    }
    let src = "алг\nнач\nцел a=5\na:=a+1\nвывод a\nкон\n";
    let program = parse(src).unwrap();
    let mut host = VarHost {
        output: String::new(),
        seen: Vec::new(),
    };
    kumir_core::run(&program, &mut host, RunOptions::default()).unwrap();
    assert_eq!(host.output, "6");
    assert!(host
        .seen
        .iter()
        .any(|vars| vars.iter().any(|(n, _, v)| n == "a" && v == "6")));
}

#[test]
fn cp1251_file_encoding() {
    let dir = std::env::temp_dir().join(format!("kumir-enc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = "использовать Файлы\nалг\nнач\nустановить кодировку('cp1251')\nфайл f\nf:=открыть на запись('cp.txt')\nвывод f,'Привет'\nзакрыть(f)\nкон\n";
    let out = exec_with_dir(src, &[], dir.clone()).unwrap();
    assert_eq!(out, "");
    let bytes = std::fs::read(dir.join("cp.txt")).unwrap();
    let (text, _, _) = encoding_rs::WINDOWS_1251.decode(&bytes);
    assert_eq!(text, "Привет");
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn robot_field_serialization_roundtrip() {
    use kumir_core::actors::RobotField;
    let dir = std::env::temp_dir().join(format!("kumir-field-rt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.fil");
    std::fs::write(
        &path,
        "; Field Size: x, y\n2 2\n; Robot position: x, y\n1 0\n; cells\n0 0 1 1 0.0 5 А $ 1\n1 0 2 0 1.5 -3 $ Б 0\n0 1 4 0 0.0 0 $ $ 0\n1 1 8 1 0.0 0 В Г 1\n; End Of File\n",
    )
    .unwrap();
    let field = RobotField::load_fil(&path).unwrap();
    let text = field.to_fil_string();
    let path2 = dir.join("b.fil");
    std::fs::write(&path2, text).unwrap();
    let field2 = RobotField::load_fil(&path2).unwrap();
    assert_eq!((field.width, field.height), (field2.width, field2.height));
    assert_eq!(
        (field.robot_x, field.robot_y),
        (field2.robot_x, field2.robot_y)
    );
    for y in 0..field.height {
        for x in 0..field.width {
            let a = field.cell(x, y).unwrap();
            let b = field2.cell(x, y).unwrap();
            assert_eq!(a.walls, b.walls);
            assert_eq!(a.color, b.color);
            assert_eq!(a.point, b.point);
            assert_eq!(a.symbol, b.symbol);
            assert_eq!(a.symbol1, b.symbol1);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
