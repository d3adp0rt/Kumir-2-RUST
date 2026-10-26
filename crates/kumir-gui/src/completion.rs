//! Code completion database and fuzzy prefix filtering.

use std::collections::HashSet;

use kumir_core::ast::{BaseType, Program, Stmt, StmtKind, TypeRef};
use kumir_core::builtins;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Type,
    Function,
    Algorithm,
    Variable,
    Module,
}

impl Kind {
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Keyword => "K",
            Kind::Type => "T",
            Kind::Function => "ƒ",
            Kind::Algorithm => "A",
            Kind::Variable => "v",
            Kind::Module => "M",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Keyword => "ключевое слово",
            Kind::Type => "тип",
            Kind::Function => "функция",
            Kind::Algorithm => "алгоритм",
            Kind::Variable => "переменная",
            Kind::Module => "модуль",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Kind::Variable => 0,
            Kind::Algorithm => 1,
            Kind::Function => 2,
            Kind::Type => 3,
            Kind::Keyword => 4,
            Kind::Module => 5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub text: String,
    pub kind: Kind,
    pub detail: String,
}

const KEYWORDS: &[(&str, &str)] = &[
    ("алг", "начало алгоритма"),
    ("нач", "начало тела"),
    ("кон", "конец тела"),
    ("дано", "предусловие"),
    ("надо", "постусловие"),
    ("утв", "утверждение"),
    ("нц", "начало цикла"),
    ("кц", "конец цикла"),
    ("кц_при", "конец цикла с условием"),
    ("для", "цикл для"),
    ("пока", "цикл пока"),
    ("раз", "цикл N раз"),
    ("от", "начало диапазона"),
    ("до", "конец диапазона"),
    ("шаг", "шаг цикла"),
    ("выход", "выход из цикла/алгоритма"),
    ("если", "условие"),
    ("то", "то"),
    ("иначе", "иначе"),
    ("все", "конец условия"),
    ("выбор", "выбор"),
    ("при", "ветвь выбора"),
    ("ввод", "ввод"),
    ("вывод", "вывод"),
    ("нс", "новая строка"),
    ("не", "логическое НЕ"),
    ("и", "логическое И"),
    ("или", "логическое ИЛИ"),
    ("пауза", "пауза"),
    ("стоп", "завершить"),
    ("арг", "входной параметр"),
    ("рез", "выходной параметр"),
    ("аргрез", "входной и выходной параметр"),
    ("знач", "возвращаемое значение"),
    ("да", "истина"),
    ("нет", "ложь"),
    ("использовать", "подключить модуль"),
    ("включить", "включить текст"),
    ("модуль", "модуль"),
    ("кон_исп", "конец модуля"),
    ("файл", "файловый тип"),
    ("цел", "целое"),
    ("вещ", "вещественное"),
    ("лит", "строка"),
    ("сим", "символ"),
    ("лог", "логическое"),
    ("целтаб", "таблица целых"),
    ("вещтаб", "таблица вещественных"),
    ("литтаб", "таблица строк"),
    ("симтаб", "таблица символов"),
    ("логтаб", "таблица логических"),
    ("Фввод", "файловый ввод"),
    ("Фвывод", "файловый вывод"),
    ("algorhitm", "algorithm header"),
    ("begin", "begin"),
    ("end", "end"),
    ("pre", "precondition"),
    ("post", "postcondition"),
    ("assert", "assert"),
    ("do", "loop"),
    ("end_do", "end loop"),
    ("for", "for loop"),
    ("while", "while loop"),
    ("times", "repeat N times"),
    ("from", "from"),
    ("to", "to"),
    ("step", "step"),
    ("exit", "exit"),
    ("if", "if"),
    ("then", "then"),
    ("else", "else"),
    ("done", "end if"),
    ("switch", "switch"),
    ("case", "case"),
    ("scan", "input"),
    ("print", "output"),
    ("endl", "new line"),
    ("not", "logical not"),
    ("and", "logical and"),
    ("or", "logical or"),
    ("true", "true"),
    ("false", "false"),
    ("int", "integer"),
    ("real", "real"),
    ("string", "string"),
    ("char", "char"),
    ("bool", "boolean"),
    ("use", "use module"),
    ("include", "include file"),
    ("array", "array"),
];

const MODULES: &[(&str, &str)] = &[
    ("Файлы", "работа с файлами"),
    ("Строки", "обработка строк"),
    ("Робот", "исполнитель Робот"),
    ("Черепаха", "исполнитель Черепаха"),
    ("Чертежник", "исполнитель Чертежник"),
    ("Водолей", "исполнитель Водолей"),
    ("Кузнечик", "исполнитель Кузнечик"),
    ("Рисователь", "исполнитель Рисователь"),
    ("Files", "file operations"),
    ("String Utilities", "string operations"),
    ("Robot", "Robot actor"),
    ("Turtle", "Turtle actor"),
    ("Draw", "Draw actor"),
    ("Vodoley", "Vodoley actor"),
    ("Grasshopper", "Grasshopper actor"),
    ("Painter", "Painter actor"),
];

fn add_unique(items: &mut Vec<Item>, seen: &mut HashSet<String>, item: Item) {
    if seen.insert(item.text.clone()) {
        items.push(item);
    }
}

fn type_detail(ty: &TypeRef) -> String {
    let name = match &ty.base {
        BaseType::Int => "цел",
        BaseType::Real => "вещ",
        BaseType::Str => "лит",
        BaseType::Char => "сим",
        BaseType::Bool => "лог",
        BaseType::File => "файл",
        BaseType::User(name) => name.as_str(),
        BaseType::Void => "—",
    };
    if ty.is_array {
        format!("{name}таб")
    } else {
        name.to_string()
    }
}

fn collect_decl_names(stmts: &[Stmt], out: &mut Vec<(String, String)>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Decl(decls) => {
                for d in decls {
                    out.push((d.name.clone(), type_detail(&d.typ)));
                }
            }
            StmtKind::If(branches) | StmtKind::Switch(branches) => {
                for b in branches {
                    collect_decl_names(&b.body, out);
                }
            }
            StmtKind::Loop { body, .. } => collect_decl_names(body, out),
            _ => {}
        }
    }
}

/// Suggestions for the identifier prefix `prefix`.
pub fn suggest(prefix: &str, program: Option<&Program>) -> Vec<Item> {
    let prefix_l = prefix.trim().to_lowercase();
    let allow_at = prefix_l.starts_with('@');
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for (name, detail) in [
        ("цел", "целое"),
        ("вещ", "вещественное"),
        ("лит", "строка"),
        ("сим", "символ"),
        ("лог", "логическое"),
        ("файл", "файл"),
        ("целтаб", "таблица целых"),
        ("вещтаб", "таблица вещественных"),
        ("литтаб", "таблица строк"),
        ("симтаб", "таблица символов"),
        ("логтаб", "таблица логических"),
        ("int", "integer"),
        ("real", "real"),
        ("string", "string"),
        ("char", "char"),
        ("bool", "boolean"),
        ("file", "file"),
    ] {
        add_unique(
            &mut items,
            &mut seen,
            Item {
                text: name.to_string(),
                kind: Kind::Type,
                detail: detail.to_string(),
            },
        );
    }
    for (kw, detail) in KEYWORDS {
        add_unique(
            &mut items,
            &mut seen,
            Item {
                text: (*kw).to_string(),
                kind: Kind::Keyword,
                detail: (*detail).to_string(),
            },
        );
    }
    for (name, detail) in MODULES {
        add_unique(
            &mut items,
            &mut seen,
            Item {
                text: (*name).to_string(),
                kind: Kind::Module,
                detail: (*detail).to_string(),
            },
        );
    }
    for spec in kumir_core::actors::commands::COMMANDS
        .iter()
        .chain(kumir_core::actors::commands::COLOR_COMMANDS.iter())
    {
        add_unique(
            &mut items,
            &mut seen,
            Item {
                text: spec.ru.to_string(),
                kind: Kind::Function,
                detail: format!("{}: {}", spec.signature(), spec.doc),
            },
        );
    }
    for name in builtins::known_names() {
        if name.starts_with('@') && !allow_at {
            continue;
        }
        add_unique(
            &mut items,
            &mut seen,
            Item {
                text: (*name).to_string(),
                kind: Kind::Function,
                detail: "стандартная функция/команда".to_string(),
            },
        );
    }
    if let Some(program) = program {
        for alg in &program.algorithms {
            if alg.name.trim().is_empty() || (alg.name.starts_with('@') && !allow_at) {
                continue;
            }
            add_unique(
                &mut items,
                &mut seen,
                Item {
                    text: alg.name.clone(),
                    kind: Kind::Algorithm,
                    detail: if alg.params.is_empty() {
                        "алгоритм".to_string()
                    } else {
                        format!("алгоритм, {} аргумент(ов)", alg.params.len())
                    },
                },
            );
            let mut locals = Vec::new();
            collect_decl_names(&alg.body, &mut locals);
            collect_decl_names(&alg.pre, &mut locals);
            collect_decl_names(&alg.post, &mut locals);
            for p in &alg.params {
                add_unique(
                    &mut items,
                    &mut seen,
                    Item {
                        text: p.name.clone(),
                        kind: Kind::Variable,
                        detail: format!("параметр: {}", type_detail(&p.typ)),
                    },
                );
            }
            for (name, ty) in locals {
                add_unique(
                    &mut items,
                    &mut seen,
                    Item {
                        text: name,
                        kind: Kind::Variable,
                        detail: format!("локальная: {ty}"),
                    },
                );
            }
        }
        for g in &program.globals {
            add_unique(
                &mut items,
                &mut seen,
                Item {
                    text: g.name.clone(),
                    kind: Kind::Variable,
                    detail: format!("глобальная: {}", type_detail(&g.typ)),
                },
            );
        }
        for m in &program.modules {
            add_unique(
                &mut items,
                &mut seen,
                Item {
                    text: m.clone(),
                    kind: Kind::Module,
                    detail: "подключённый модуль".to_string(),
                },
            );
        }
    }

    let mut filtered: Vec<Item> = items
        .into_iter()
        .filter(|item| {
            prefix_l.is_empty()
                || item.text.to_lowercase().starts_with(&prefix_l)
                || (prefix_l.chars().count() >= 2
                    && item
                        .text
                        .split_whitespace()
                        .any(|w| w.to_lowercase().starts_with(&prefix_l)))
        })
        .collect();
    // Only multi-word phrases starting with the prefix are safe to insert:
    // replacing a word in the middle would duplicate the preceding words.
    filtered.retain(|item| {
        if item.text.chars().any(char::is_whitespace) {
            item.text.to_lowercase().starts_with(&prefix_l)
        } else {
            true
        }
    });
    filtered.sort_by(|a, b| {
        (a.kind.rank(), a.text.to_lowercase(), a.text.clone()).cmp(&(
            b.kind.rank(),
            b.text.to_lowercase(),
            b.text.clone(),
        ))
    });
    filtered.truncate(16);
    filtered
}

pub fn word_start(text: &str, cursor_char: usize) -> (usize, String) {
    let chars: Vec<char> = text.chars().collect();
    let mut start = cursor_char.min(chars.len());
    while start > 0 {
        let c = chars[start - 1];
        if is_ident_char(c) {
            start -= 1;
        } else {
            break;
        }
    }
    let prefix: String = chars[start..cursor_char.min(chars.len())].iter().collect();
    (start, prefix)
}

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '@'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_prefix_handles_cyrillic() {
        let (start, word) = word_start("алг Сумма", 9);
        assert_eq!(start, 4);
        assert_eq!(word, "Сумма");
        let (start, word) = word_start("вывод ", 5);
        assert_eq!(start, 0);
        assert_eq!(word, "вывод");
    }

    #[test]
    fn suggests_keywords_functions_and_types() {
        let items = suggest("вы", None);
        assert!(items
            .iter()
            .any(|i| i.text == "вывод" && i.kind == Kind::Keyword));
        let items = suggest("отк", None);
        assert!(items.iter().any(|i| i.text == "открыть на чтение"));
        let items = suggest("цел", None);
        assert!(items
            .iter()
            .any(|i| i.text == "цел" && i.kind == Kind::Type));
        let items = suggest("дли", None);
        assert!(items.iter().any(|i| i.text == "длин"));
    }
}
