//! Standard library: function registry, pure built-ins and output formatting.
//!
//! Names follow `src/plugins/kumiranalizer/stdlibmodules.h` and
//! `share/kumir2/kumiranalizer/*.keywords`.  Both the Russian localized names
//! and the ASCII names of the original implementation are accepted.

use crate::ast::normalize_name;
use crate::diag::RuntimeError;
use crate::value::{compare_values, Value};

/// All callable standard-library names, used by the parser to join
/// multi-word names such as `открыть на чтение`.
pub fn known_names() -> &'static [&'static str] {
    &[
        // --- Стандартные функции (RTL) ---
        "abs",
        "arccos",
        "arcctg",
        "arcsin",
        "arctg",
        "cos",
        "ctg",
        "ждать",
        "wait",
        "div",
        "exp",
        "iabs",
        "imax",
        "imin",
        "int",
        "irand",
        "irnd",
        "lg",
        "ln",
        "max",
        "min",
        "mod",
        "rand",
        "rnd",
        "sign",
        "sin",
        "sqrt",
        "tg",
        "МАКСВЕЩ",
        "MAXREAL",
        "МАКСЦЕЛ",
        "MAXINT",
        "вещ_в_лит",
        "string_of_real",
        "время",
        "time",
        "длин",
        "length",
        "код",
        "code",
        "лит_в_вещ",
        "string_to_real",
        "лит_в_цел",
        "string_to_int",
        "символ",
        "symbol",
        "юнисимвол",
        "unisymbol",
        "цел_в_лит",
        "string_of_int",
        "юникод",
        "unicode",
        "Цел",
        "Integer",
        "Вещ",
        "Real",
        "Лог",
        "Boolean",
        "@поставить оценку",
        "@Evaluate",
        "НАЗНАЧИТЬ ВВОД",
        "ASSIGN INPUT",
        "НАЗНАЧИТЬ ВЫВОД",
        "ASSIGN OUTPUT",
        // --- Файлы ---
        "открыть на чтение",
        "read open",
        "открыть на запись",
        "write open",
        "открыть на добавление",
        "append open",
        "закрыть",
        "close",
        "начать чтение",
        "start reading",
        "конец файла",
        "eof",
        "установить кодировку",
        "set encoding",
        "можно открыть на чтение",
        "readable",
        "можно открыть на запись",
        "writable",
        "есть данные",
        "nonempty",
        "существует",
        "exists",
        "существует файл",
        "file exists",
        "является каталогом",
        "isdir",
        "создать каталог",
        "mkdir",
        "создать файл",
        "touch",
        "create file",
        "полный путь",
        "abspath",
        "удалить_файл",
        "rm",
        "РАБОЧИЙ КАТАЛОГ",
        "CURRENT DIR",
        "КАТАЛОГ ПРОГРАММЫ",
        "PROGRAM DIR",
        "удалить_каталог",
        "rmdir",
        "консоль",
        "get console",
        // --- Исполнители (Робот, Черепаха, Чертежник) ---
        "вверх",
        "вниз",
        "влево",
        "вправо",
        "закрасить",
        "сверху стена",
        "снизу стена",
        "слева стена",
        "справа стена",
        "сверху свободно",
        "снизу свободно",
        "слева свободно",
        "справа свободно",
        "клетка закрашена",
        "клетка чистая",
        "радиация",
        "температура",
        "@@размер поля",
        "@@метка",
        "@@закрашена",
        "@@робот",
        "@@верхняя буква",
        "@@нижняя буква",
        "@@температура",
        "@@радиация",
        "поднять хвост",
        "опустить хвост",
        "вперед",
        "назад",
        "выбрать чернила",
        "опустить перо",
        "поднять перо",
        "сместиться в точку",
        "сместиться на вектор",
        "написать",
        "@есть отрезок в области",
        "go up",
        "go down",
        "go left",
        "go right",
        "do paint",
        "is wall at top",
        "is wall at bottom",
        "is wall at left",
        "is wall at right",
        "is free at top",
        "is free at bottom",
        "is free at left",
        "is free at right",
        "is Color",
        "is Clear",
        "radiation",
        "temperature",
        "field size",
        "mark",
        "colored",
        "robot pos",
        "upChar",
        "downChar",
        "cellTemp",
        "cellRad",
        "tailUp",
        "tailDown",
        "forward",
        "back",
        "left",
        "right",
        "set pen color",
        "setup pen",
        "release pen",
        "move to",
        "move by",
        "add caption",
        "@is line at circle",
        // --- Строки ---
        "верхний регистр",
        "uppercase",
        "нижний регистр",
        "lowercase",
        "позиция после",
        "index after",
        "позиция",
        "index",
        "поз после",
        "find after",
        "поз",
        "find",
        "вставить",
        "insert",
        "заменить",
        "replace",
        "удалить",
        "remove",
    ]
}

pub fn is_file_builtin(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "открыть на чтение"
            | "read open"
            | "открыть на запись"
            | "write open"
            | "открыть на добавление"
            | "append open"
            | "закрыть"
            | "close"
            | "начать чтение"
            | "start reading"
            | "конец файла"
            | "eof"
            | "установить кодировку"
            | "set encoding"
            | "можно открыть на чтение"
            | "readable"
            | "можно открыть на запись"
            | "writable"
            | "есть данные"
            | "nonempty"
            | "существует"
            | "exists"
            | "существует файл"
            | "file exists"
            | "создать файл"
            | "touch"
            | "create file"
            | "является каталогом"
            | "isdir"
            | "создать каталог"
            | "mkdir"
            | "полный путь"
            | "abspath"
            | "удалить_файл"
            | "rm"
            | "РАБОЧИЙ КАТАЛОГ"
            | "CURRENT DIR"
            | "КАТАЛОГ ПРОГРАММЫ"
            | "PROGRAM DIR"
            | "удалить_каталог"
            | "rmdir"
            | "консоль"
            | "get console"
            | "="
            | "<>"
    )
}

/// Built-ins which modify one of their arguments in place.
pub fn is_inout_builtin(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "вставить"
            | "insert"
            | "заменить"
            | "replace"
            | "удалить"
            | "remove"
            | "лит_в_цел"
            | "string_to_int"
            | "лит_в_вещ"
            | "string_to_real"
    )
}

fn num(v: &Value, line: u32) -> Result<f64, RuntimeError> {
    v.to_real(line)
}

fn int(v: &Value, line: u32) -> Result<i64, RuntimeError> {
    v.to_int(line)
}

/// Evaluate a pure built-in. Returns `None` for names handled by the
/// interpreter (I/O, files, random, in-out string procedures).
pub fn call_pure(name: &str, args: &[Value], line: u32) -> Option<Result<Value, RuntimeError>> {
    let name = normalize_name(name);
    let result =
        match name.as_str() {
            "abs" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Real(num(&args[0], line)?.abs()))),
            "iabs" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Int(int(&args[0], line)?.abs()))),
            "arccos" => math1(&name, args, line, |x| x.acos()),
            "arcsin" => math1(&name, args, line, |x| x.asin()),
            "arctg" => math1(&name, args, line, |x| x.atan()),
            "arcctg" => math1(&name, args, line, |x| {
                if x == 0.0 {
                    std::f64::consts::FRAC_PI_2
                } else {
                    std::f64::consts::FRAC_PI_2 - x.atan()
                }
            }),
            "cos" => math1(&name, args, line, |x| x.cos()),
            "sin" => math1(&name, args, line, |x| x.sin()),
            "tg" => math1(&name, args, line, |x| x.tan()),
            "ctg" => math1(&name, args, line, |x| 1.0 / x.tan()),
            "exp" => math1(&name, args, line, |x| x.exp()),
            "ln" => math1(&name, args, line, |x| x.ln()),
            "lg" => math1(&name, args, line, |x| x.log10()),
            "sqrt" => math1(&name, args, line, |x| x.sqrt()),
            "int" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Int(num(&args[0], line)?.floor() as i64))),
            "sign" => require(args, 1, &name, line).and_then(|_| {
                let x = num(&args[0], line)?;
                Ok(Value::Int(if x > 0.0 {
                    1
                } else if x < 0.0 {
                    -1
                } else {
                    0
                }))
            }),
            "max" => require(args, 2, &name, line)
                .and_then(|_| Ok(Value::Real(num(&args[0], line)?.max(num(&args[1], line)?)))),
            "min" => require(args, 2, &name, line)
                .and_then(|_| Ok(Value::Real(num(&args[0], line)?.min(num(&args[1], line)?)))),
            "imax" => require(args, 2, &name, line)
                .and_then(|_| Ok(Value::Int(int(&args[0], line)?.max(int(&args[1], line)?)))),
            "imin" => require(args, 2, &name, line)
                .and_then(|_| Ok(Value::Int(int(&args[0], line)?.min(int(&args[1], line)?)))),
            "div" => require(args, 2, &name, line).and_then(|_| {
                let a = int(&args[0], line)?;
                let b = int(&args[1], line)?;
                if b <= 0 {
                    Err(RuntimeError::new(line, "Деление на ненатуральное число"))
                } else {
                    Ok(Value::Int(a.div_euclid(b)))
                }
            }),
            "mod" => require(args, 2, &name, line).and_then(|_| {
                let a = int(&args[0], line)?;
                let b = int(&args[1], line)?;
                if b <= 0 {
                    Err(RuntimeError::new(line, "Деление на ненатуральное число"))
                } else {
                    Ok(Value::Int(a.rem_euclid(b)))
                }
            }),
            "MAXREAL" | "МАКСВЕЩ" => Ok(Value::Real(f64::MAX)),
            "MAXINT" | "МАКСЦЕЛ" => Ok(Value::Int(i64::from(i32::MAX))),
            "длин" | "length" => require(args, 1, &name, line).and_then(|_| {
                Ok(Value::Int(
                    args[0].to_string_value(line)?.chars().count() as i64
                ))
            }),
            "код" | "code" | "юникод" | "unicode" => require(args, 1, &name, line)
                .and_then(|_| match &args[0] {
                    Value::Char(c) => Ok(Value::Int(*c as i64)),
                    Value::Str(s) if s.chars().count() == 1 => {
                        Ok(Value::Int(s.chars().next().unwrap() as i64))
                    }
                    other => Err(RuntimeError::new(
                        line,
                        format!("Ожидался символ, получено {}", other.type_name()),
                    )),
                }),
            "символ" | "symbol" | "юнисимвол" | "unisymbol" => {
                require(args, 1, &name, line).and_then(|_| {
                    let code = int(&args[0], line)?;
                    char::from_u32(code as u32)
                        .map(Value::Char)
                        .ok_or_else(|| RuntimeError::new(line, "Неверный код символа"))
                })
            }
            "цел_в_лит" | "string_of_int" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Str(int(&args[0], line)?.to_string()))),
            "вещ_в_лит" | "string_of_real" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Str(format_real_default(num(&args[0], line)?)))),
            "Цел" | "Integer" => parse_int_fn(args, line),
            "Вещ" | "Real" => parse_real_fn(args, line),
            "Лог" | "Boolean" => parse_bool_fn(args, line),
            "время" | "time" => Ok(Value::Int(now_ms_since_midnight())),
            "ждать" | "wait" => require(args, 1, &name, line).and_then(|_| {
                let ms = int(&args[0], line)?.max(0);
                std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                Ok(Value::Undefined)
            }),
            "верхний регистр" | "uppercase" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Str(args[0].to_string_value(line)?.to_uppercase()))),
            "нижний регистр" | "lowercase" => require(args, 1, &name, line)
                .and_then(|_| Ok(Value::Str(args[0].to_string_value(line)?.to_lowercase()))),
            "позиция" | "index" | "поз" | "find" => require(args, 2, &name, line)
                .and_then(|_| {
                    let pattern = args[0].to_string_value(line)?;
                    let source = args[1].to_string_value(line)?;
                    Ok(Value::Int(find_substring(&source, &pattern, 1)))
                }),
            "позиция после" | "index after" | "поз после" | "find after" => {
                require(args, 3, &name, line).and_then(|_| {
                    let from = int(&args[0], line)?;
                    if from < 1 {
                        return Err(RuntimeError::new(line, "Индекс меньше 1"));
                    }
                    let pattern = args[1].to_string_value(line)?;
                    let source = args[2].to_string_value(line)?;
                    Ok(Value::Int(find_substring(&source, &pattern, from)))
                })
            }
            "@поставить оценку" | "@Evaluate" => Ok(Value::Undefined),
            _ => return None,
        };
    Some(result)
}

fn require(args: &[Value], n: usize, name: &str, line: u32) -> Result<(), RuntimeError> {
    if args.len() < n {
        Err(RuntimeError::new(
            line,
            format!("Функция «{name}» требует {n} аргумент(а/ов)"),
        ))
    } else {
        Ok(())
    }
}

fn math1(
    name: &str,
    args: &[Value],
    line: u32,
    f: impl Fn(f64) -> f64,
) -> Result<Value, RuntimeError> {
    require(args, 1, name, line)?;
    let x = num(&args[0], line)?;
    let y = f(x);
    if y.is_nan() {
        return Err(RuntimeError::new(
            line,
            format!("Недопустимый аргумент функции «{name}»"),
        ));
    }
    Ok(Value::Real(y))
}

fn parse_int_fn(args: &[Value], line: u32) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new(
            line,
            "Функция «Цел» требует два аргумента",
        ));
    }
    let s = args[0].to_string_value(line)?;
    let def = int(&args[1], line)?;
    let t = s.trim();
    let parsed = if let Some(hex) = t.strip_prefix('$') {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    };
    Ok(Value::Int(parsed.unwrap_or(def)))
}

fn parse_real_fn(args: &[Value], line: u32) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new(
            line,
            "Функция «Вещ» требует два аргумента",
        ));
    }
    let s = args[0].to_string_value(line)?;
    let def = num(&args[1], line)?;
    Ok(Value::Real(parse_real_str(&s).unwrap_or(def)))
}

fn parse_bool_fn(args: &[Value], line: u32) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::new(
            line,
            "Функция «Лог» требует два аргумента",
        ));
    }
    let s = args[0].to_string_value(line)?;
    let def = args[1].to_bool(line)?;
    Ok(Value::Bool(parse_bool_str(&s).unwrap_or(def)))
}

pub fn parse_real_str(s: &str) -> Option<f64> {
    let t = s.trim();
    t.parse::<f64>().ok().or_else(|| {
        // Accept the Russian decimal comma.
        t.replace(',', ".").parse::<f64>().ok()
    })
}

pub fn parse_bool_str(s: &str) -> Option<bool> {
    let t = s.trim().to_lowercase();
    match t.as_str() {
        "1" | "true" | "yes" | "да" | "истина" => Some(true),
        "0" | "false" | "no" | "нет" | "ложь" => Some(false),
        _ => None,
    }
}

pub fn find_substring(source: &str, pattern: &str, from: i64) -> i64 {
    if from < 1 {
        return 0;
    }
    let chars: Vec<char> = source.chars().collect();
    let pat: Vec<char> = pattern.chars().collect();
    let start = (from - 1) as usize;
    if pat.is_empty() {
        return from;
    }
    if start >= chars.len() && !chars.is_empty() {
        return 0;
    }
    let mut i = start;
    while i + pat.len() <= chars.len() {
        if chars[i..i + pat.len()] == pat[..] {
            return (i + 1) as i64;
        }
        i += 1;
    }
    0
}

fn now_ms_since_midnight() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Local time offset is not important for the school algorithms that use
    // this function; UTC midnight is used for reproducibility.
    (secs % 86400) * 1000
}

/// Format used by `вещ_в_лит` and by default output of real values.
pub fn format_real_default(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".to_string()
        } else if value > 0.0 {
            "∞".to_string()
        } else {
            "-∞".to_string()
        };
    }
    let abs = value.abs();
    let exp_form = abs != 0.0 && (abs < 1e-4 || abs >= 1e6);
    if exp_form {
        let s = format!("{value:.13e}");
        // Split mantissa and exponent and trim trailing zeroes.
        if let Some((mantissa, exp)) = s.split_once('e') {
            let mut m = mantissa.trim_end_matches('0').to_string();
            if m.ends_with('.') {
                m.pop();
            }
            let exp_num: i32 = exp.parse().unwrap_or(0);
            let sign = if exp_num < 0 { "-" } else { "+" };
            format!("{m}e{sign}{:02}", exp_num.abs())
        } else {
            s
        }
    } else {
        let decimals = if abs == 0.0 {
            13
        } else {
            (13 - abs.log10().floor() as i64).max(0)
        };
        let prec = decimals as usize;
        let s = format!("{value:.prec$}");
        let s = s.trim_end_matches('0').to_string();
        if s.ends_with('.') {
            format!("{s}0")
        } else if s.is_empty() {
            "0.0".to_string()
        } else {
            s
        }
    }
}

/// Format a real for `вывод` (optionally with width and decimals).
pub fn format_real(value: f64, width: Option<i64>, decimals: Option<i64>) -> String {
    let mut s = match decimals {
        Some(d) if d >= 0 => {
            let prec = d as usize;
            format!("{value:.prec$}")
        }
        _ => format_real_default(value),
    };
    if let Some(w) = width {
        let w = w.unsigned_abs() as usize;
        if s.chars().count() < w {
            let pad = " ".repeat(w - s.chars().count());
            s = if w > 0 && width.unwrap_or(0) < 0 {
                format!("{s}{pad}")
            } else {
                format!("{pad}{s}")
            };
        }
    }
    s
}

pub fn format_int(value: i64, width: Option<i64>) -> String {
    let mut s = value.to_string();
    if let Some(w) = width {
        let w = w.unsigned_abs() as usize;
        if s.chars().count() < w {
            let pad = " ".repeat(w - s.chars().count());
            s = if width.unwrap_or(0) < 0 {
                format!("{s}{pad}")
            } else {
                format!("{pad}{s}")
            };
        }
    }
    s
}

/// Output representation of a value (without width/decimals).
pub fn format_value(value: &Value, line: u32) -> Result<String, RuntimeError> {
    Ok(match value {
        Value::Undefined => {
            return Err(RuntimeError::new(
                line,
                "Нельзя вывести неопределённое значение",
            ))
        }
        Value::Int(v) => v.to_string(),
        Value::Real(v) => format_real_default(*v),
        Value::Bool(v) => {
            if *v {
                "да".to_string()
            } else {
                "нет".to_string()
            }
        }
        Value::Char(c) => c.to_string(),
        Value::Str(s) => s.clone(),
        Value::File(_) => "[файл]".to_string(),
        Value::Array(a) => format!("[таблица {} элементов]", a.element_count()),
        Value::Record(_) => "[запись]".to_string(),
    })
}

/// Numeric comparison helper used by `min`/`max` and by tests.
pub fn less(a: &Value, b: &Value, line: u32) -> Result<bool, RuntimeError> {
    Ok(compare_values(a, b, line)? == std::cmp::Ordering::Less)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_real_format() {
        assert_eq!(format_real_default(1.2345678), "1.2345678");
        assert_eq!(format_real_default(12.0), "12.0");
        assert_eq!(format_real_default(0.0), "0.0");
        assert_eq!(format_real_default(1234567.0), "1.234567e+06");
    }

    #[test]
    fn width_and_decimals() {
        assert_eq!(format_int(123, Some(5)), "  123");
        assert_eq!(format_real(1.2345678, Some(7), Some(3)), "  1.235");
    }

    #[test]
    fn integer_division() {
        assert_eq!(
            match call_pure("div", &[Value::Int(-7), Value::Int(2)], 1) {
                Some(Ok(Value::Int(v))) => v,
                _ => panic!(),
            },
            -4
        );
        assert_eq!(
            match call_pure("mod", &[Value::Int(-7), Value::Int(2)], 1) {
                Some(Ok(Value::Int(v))) => v,
                _ => panic!(),
            },
            1
        );
    }
}
