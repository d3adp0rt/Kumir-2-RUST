//! The command catalogue of the Kumir actors.
//!
//! The table is the single source of truth for the interpreter, the IDE
//! completion, the built-in help and the parser (which needs the names to join
//! multi-word commands such as `перелей из A в B`).  Signatures and names are
//! taken from `src/actors/*/*.json` of the original Kumir 2 project.

use super::ActorKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgType {
    Int,
    Real,
    Str,
    Bool,
    Color,
    /// A one-dimensional table (`целтаб`) passed by value.
    IntArray,
    /// A `рез` parameter assigned by the command.
    OutInt,
    OutReal,
    OutChar,
    OutColor,
}

impl ArgType {
    pub fn is_out(self) -> bool {
        matches!(
            self,
            ArgType::OutInt | ArgType::OutReal | ArgType::OutChar | ArgType::OutColor
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            ArgType::Int => "цел",
            ArgType::Real => "вещ",
            ArgType::Str => "лит",
            ArgType::Bool => "лог",
            ArgType::Color => "цвет",
            ArgType::IntArray => "целтаб",
            ArgType::OutInt => "рез цел",
            ArgType::OutReal => "рез вещ",
            ArgType::OutChar => "рез сим",
            ArgType::OutColor => "рез цвет",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetType {
    Void,
    Bool,
    Int,
    Real,
    Char,
    Color,
}

impl RetType {
    pub fn label(self) -> Option<&'static str> {
        match self {
            RetType::Void => None,
            RetType::Bool => Some("лог"),
            RetType::Int => Some("цел"),
            RetType::Real => Some("вещ"),
            RetType::Char => Some("сим"),
            RetType::Color => Some("цвет"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    pub actor: ActorKind,
    pub ru: &'static str,
    pub en: &'static str,
    pub args: &'static [ArgType],
    pub ret: RetType,
    pub doc: &'static str,
}

impl CommandSpec {
    /// Russian signature such as `перелей из A в B` or `вперед(вещ)`.
    pub fn signature(&self) -> String {
        if self.args.is_empty() {
            self.ru.to_string()
        } else {
            let args: Vec<&str> = self.args.iter().map(|a| a.label()).collect();
            format!("{}({})", self.ru, args.join(", "))
        }
    }

    pub fn is_query(&self) -> bool {
        self.ret != RetType::Void
    }
}

macro_rules! cmd {
    ($actor:ident, $ru:expr, $en:expr, [$($arg:ident),*], $ret:ident, $doc:expr $(,)?) => {
        CommandSpec {
            actor: ActorKind::$actor,
            ru: $ru,
            en: $en,
            args: &[$(ArgType::$arg),*],
            ret: RetType::$ret,
            doc: $doc,
        }
    };
}

/// Every command of every implemented actor, in documentation order.
pub const COMMANDS: &[CommandSpec] = &[
    // ---------------------------------------------------------------- Робот
    cmd!(Robot, "вверх", "go up", [], Void, "шаг на клетку вверх"),
    cmd!(Robot, "вниз", "go down", [], Void, "шаг на клетку вниз"),
    cmd!(Robot, "влево", "go left", [], Void, "шаг на клетку влево"),
    cmd!(
        Robot,
        "вправо",
        "go right",
        [],
        Void,
        "шаг на клетку вправо"
    ),
    cmd!(
        Robot,
        "закрасить",
        "do paint",
        [],
        Void,
        "закрасить текущую клетку"
    ),
    cmd!(
        Robot,
        "сверху стена",
        "is wall at top",
        [],
        Bool,
        "сверху стена"
    ),
    cmd!(
        Robot,
        "снизу стена",
        "is wall at bottom",
        [],
        Bool,
        "снизу стена"
    ),
    cmd!(
        Robot,
        "слева стена",
        "is wall at left",
        [],
        Bool,
        "слева стена"
    ),
    cmd!(
        Robot,
        "справа стена",
        "is wall at right",
        [],
        Bool,
        "справа стена"
    ),
    cmd!(
        Robot,
        "сверху свободно",
        "is free at top",
        [],
        Bool,
        "сверху нет стены"
    ),
    cmd!(
        Robot,
        "снизу свободно",
        "is free at bottom",
        [],
        Bool,
        "снизу нет стены"
    ),
    cmd!(
        Robot,
        "слева свободно",
        "is free at left",
        [],
        Bool,
        "слева нет стены"
    ),
    cmd!(
        Robot,
        "справа свободно",
        "is free at right",
        [],
        Bool,
        "справа нет стены"
    ),
    cmd!(
        Robot,
        "клетка закрашена",
        "is Color",
        [],
        Bool,
        "текущая клетка закрашена"
    ),
    cmd!(
        Robot,
        "клетка чистая",
        "is Clear",
        [],
        Bool,
        "текущая клетка не закрашена"
    ),
    cmd!(
        Robot,
        "радиация",
        "radiation",
        [],
        Real,
        "радиация текущей клетки"
    ),
    cmd!(
        Robot,
        "температура",
        "temperature",
        [],
        Int,
        "температура текущей клетки"
    ),
    cmd!(
        Robot,
        "@@размер поля",
        "field size",
        [OutInt, OutInt],
        Void,
        "размеры поля: строки, столбцы"
    ),
    cmd!(
        Robot,
        "@@робот",
        "robot pos",
        [OutInt, OutInt],
        Void,
        "позиция Робота: строка, столбец (от 1)"
    ),
    cmd!(
        Robot,
        "@@метка",
        "mark",
        [Int, Int],
        Bool,
        "есть ли метка в клетке (строка, столбец)"
    ),
    cmd!(
        Robot,
        "@@закрашена",
        "colored",
        [Int, Int],
        Bool,
        "закрашена ли клетка (строка, столбец)"
    ),
    cmd!(
        Robot,
        "@@верхняя буква",
        "upChar",
        [Int, Int],
        Char,
        "символ в левом верхнем углу клетки"
    ),
    cmd!(
        Robot,
        "@@нижняя буква",
        "downChar",
        [Int, Int],
        Char,
        "символ в левом нижнем углу клетки"
    ),
    cmd!(
        Robot,
        "@@температура",
        "cellTemp",
        [Int, Int],
        Int,
        "температура клетки (строка, столбец)"
    ),
    cmd!(
        Robot,
        "@@радиация",
        "cellRad",
        [Int, Int],
        Real,
        "радиация клетки (строка, столбец)"
    ),
    // ------------------------------------------------------------- Черепаха
    cmd!(
        Turtle,
        "поднять хвост",
        "tailUp",
        [],
        Void,
        "поднять перо черепахи"
    ),
    cmd!(
        Turtle,
        "опустить хвост",
        "tailDown",
        [],
        Void,
        "опустить перо черепахи"
    ),
    cmd!(
        Turtle,
        "вперед",
        "forward",
        [Real],
        Void,
        "проползти вперёд(расстояние)"
    ),
    cmd!(
        Turtle,
        "назад",
        "back",
        [Real],
        Void,
        "проползти назад(расстояние)"
    ),
    cmd!(
        Turtle,
        "влево",
        "left",
        [Real],
        Void,
        "повернуть влево(угол в градусах)"
    ),
    cmd!(
        Turtle,
        "вправо",
        "right",
        [Real],
        Void,
        "повернуть вправо(угол в градусах)"
    ),
    cmd!(
        Turtle,
        "выбрать чернила",
        "set pen color",
        [Color],
        Void,
        "выбрать цвет пера"
    ),
    // ------------------------------------------------------------ Чертежник
    cmd!(
        Draw,
        "опустить перо",
        "setup pen",
        [],
        Void,
        "рисовать при перемещении"
    ),
    cmd!(
        Draw,
        "поднять перо",
        "release pen",
        [],
        Void,
        "перемещаться без рисования"
    ),
    cmd!(
        Draw,
        "выбрать чернила",
        "set pen color",
        [Color],
        Void,
        "выбрать цвет пера"
    ),
    cmd!(
        Draw,
        "установить цвет",
        "set pen color",
        [Color],
        Void,
        "установить цвет пера (старое имя)"
    ),
    cmd!(
        Draw,
        "сместиться в точку",
        "move to",
        [Real, Real],
        Void,
        "сместиться в точку(x, y)"
    ),
    cmd!(
        Draw,
        "сместиться на вектор",
        "move by",
        [Real, Real],
        Void,
        "сместиться на вектор(dx, dy)"
    ),
    cmd!(
        Draw,
        "написать",
        "add caption",
        [Real, Str],
        Void,
        "надпись(ширина знакоместа, текст)"
    ),
    cmd!(
        Draw,
        "надпись",
        "add caption",
        [Real, Str],
        Void,
        "надпись(ширина знакоместа, текст)"
    ),
    cmd!(
        Draw,
        "@есть отрезок в области",
        "@is line at circle",
        [Real, Real, Real],
        Bool,
        "есть ли отрезок в круге(x, y, радиус)"
    ),
    // -------------------------------------------------------------- Водолей
    cmd!(
        Vodoley,
        "наполни A",
        "fill A",
        [],
        Void,
        "наполнить сосуд A"
    ),
    cmd!(
        Vodoley,
        "наполни B",
        "fill B",
        [],
        Void,
        "наполнить сосуд B"
    ),
    cmd!(
        Vodoley,
        "наполни C",
        "fill C",
        [],
        Void,
        "наполнить сосуд C"
    ),
    cmd!(Vodoley, "вылей A", "empty A", [], Void, "вылить сосуд A"),
    cmd!(Vodoley, "вылей B", "empty B", [], Void, "вылить сосуд B"),
    cmd!(Vodoley, "вылей C", "empty C", [], Void, "вылить сосуд C"),
    cmd!(
        Vodoley,
        "перелей из A в B",
        "from A to B",
        [],
        Void,
        "перелить из A в B"
    ),
    cmd!(
        Vodoley,
        "перелей из A в C",
        "from A to C",
        [],
        Void,
        "перелить из A в C"
    ),
    cmd!(
        Vodoley,
        "перелей из B в A",
        "from B to A",
        [],
        Void,
        "перелить из B в A"
    ),
    cmd!(
        Vodoley,
        "перелей из B в C",
        "from B to C",
        [],
        Void,
        "перелить из B в C"
    ),
    cmd!(
        Vodoley,
        "перелей из C в A",
        "from C to A",
        [],
        Void,
        "перелить из C в A"
    ),
    cmd!(
        Vodoley,
        "перелей из C в B",
        "from C to B",
        [],
        Void,
        "перелить из C в B"
    ),
    cmd!(
        Vodoley,
        "@решено",
        "task complited",
        [],
        Bool,
        "получено ли нужное количество воды"
    ),
    cmd!(Vodoley, "размер A", "size A", [], Int, "объём сосуда A"),
    cmd!(Vodoley, "размер B", "size B", [], Int, "объём сосуда B"),
    cmd!(Vodoley, "размер C", "size C", [], Int, "объём сосуда C"),
    cmd!(
        Vodoley,
        "в сосуде A",
        "in A",
        [],
        Int,
        "сколько воды сейчас в A"
    ),
    cmd!(
        Vodoley,
        "в сосуде B",
        "in B",
        [],
        Int,
        "сколько воды сейчас в B"
    ),
    cmd!(
        Vodoley,
        "в сосуде C",
        "in C",
        [],
        Int,
        "сколько воды сейчас в C"
    ),
    // ------------------------------------------------------------- Кузнечик
    cmd!(
        Grasshopper,
        "вперед",
        "go forward",
        [Int],
        Void,
        "прыжок вперёд(длина прыжка)"
    ),
    cmd!(
        Grasshopper,
        "назад",
        "go backward",
        [Int],
        Void,
        "прыжок назад(длина прыжка)"
    ),
    cmd!(
        Grasshopper,
        "перекрасить",
        "do repaint",
        [],
        Void,
        "перекрасить точку, где стоит Кузнечик"
    ),
    // ------------------------------------------------------------ Рисователь
    cmd!(
        Painter,
        "в точку",
        "move to",
        [Int, Int],
        Void,
        "переместить перо в точку(x, y)"
    ),
    cmd!(
        Painter,
        "линия в точку",
        "line to",
        [Int, Int],
        Void,
        "линия из текущей точки в (x, y)"
    ),
    cmd!(
        Painter,
        "линия",
        "line",
        [Int, Int, Int, Int],
        Void,
        "линия(x1, y1, x2, y2)"
    ),
    cmd!(
        Painter,
        "прямоугольник",
        "rectangle",
        [Int, Int, Int, Int],
        Void,
        "прямоугольник(x1, y1, x2, y2)"
    ),
    cmd!(
        Painter,
        "эллипс",
        "ellipse",
        [Int, Int, Int, Int],
        Void,
        "эллипс, вписанный в прямоугольник"
    ),
    cmd!(
        Painter,
        "окружность",
        "circle",
        [Int, Int, Int],
        Void,
        "окружность(x, y, радиус)"
    ),
    cmd!(
        Painter,
        "многоугольник",
        "polygon",
        [Int, IntArray, IntArray],
        Void,
        "многоугольник(N, X, Y)"
    ),
    cmd!(
        Painter,
        "надпись",
        "write",
        [Int, Int, Str],
        Void,
        "надпись(x, y, текст)"
    ),
    cmd!(
        Painter,
        "залить",
        "fill",
        [Int, Int],
        Void,
        "залить область цветом кисти"
    ),
    cmd!(
        Painter,
        "пиксель",
        "point",
        [Int, Int, Color],
        Void,
        "пиксель(x, y, цвет)"
    ),
    cmd!(
        Painter,
        "значение в точке",
        "point sample",
        [Int, Int],
        Color,
        "цвет пикселя в точке"
    ),
    cmd!(
        Painter,
        "перо",
        "set pen",
        [Int, Color],
        Void,
        "перо(толщина, цвет)"
    ),
    cmd!(
        Painter,
        "кисть",
        "set brush",
        [Color],
        Void,
        "кисть(цвет заливки)"
    ),
    cmd!(
        Painter,
        "убрать кисть",
        "unset brush",
        [],
        Void,
        "отключить заливку"
    ),
    cmd!(
        Painter,
        "шрифт",
        "set font",
        [Str, Int, Bool, Bool],
        Void,
        "шрифт(название, размер, жирный, курсив)"
    ),
    cmd!(
        Painter,
        "новый лист",
        "new page",
        [Int, Int, Color],
        Void,
        "новый лист(ширина, высота, цвет фона)"
    ),
    cmd!(
        Painter,
        "загрузить лист",
        "load page",
        [Str],
        Void,
        "загрузить лист из PNG"
    ),
    cmd!(
        Painter,
        "сохранить лист",
        "save page",
        [Str],
        Void,
        "сохранить лист в PNG"
    ),
    cmd!(
        Painter,
        "высота листа",
        "page height",
        [],
        Int,
        "высота листа"
    ),
    cmd!(
        Painter,
        "ширина листа",
        "page width",
        [],
        Int,
        "ширина листа"
    ),
    cmd!(
        Painter,
        "центр x",
        "center x",
        [],
        Int,
        "середина листа по X"
    ),
    cmd!(
        Painter,
        "центр y",
        "center y",
        [],
        Int,
        "середина листа по Y"
    ),
    cmd!(
        Painter,
        "ширина текста",
        "text width",
        [Str],
        Int,
        "ширина текста в пикселях"
    ),
    cmd!(
        Painter,
        "плотность",
        "density",
        [Int],
        Void,
        "плотность штриховки (совместимость)"
    ),
];

/// Colour constructors and decompositions of `_Colorer`; they are available
/// whenever an actor that uses colours is connected.
pub const COLOR_COMMANDS: &[CommandSpec] = &[
    cmd!(
        None,
        "RGB",
        "RGB",
        [Int, Int, Int],
        Color,
        "цвет по модели RGB"
    ),
    cmd!(
        None,
        "RGBA",
        "RGBA",
        [Int, Int, Int, Int],
        Color,
        "цвет по модели RGBA"
    ),
    cmd!(
        None,
        "HSL",
        "HSL",
        [Int, Int, Int],
        Color,
        "цвет по модели HSL"
    ),
    cmd!(
        None,
        "HSLA",
        "HSLA",
        [Int, Int, Int, Int],
        Color,
        "цвет по модели HSLA"
    ),
    cmd!(
        None,
        "HSV",
        "HSV",
        [Int, Int, Int],
        Color,
        "цвет по модели HSV"
    ),
    cmd!(
        None,
        "HSVA",
        "HSVA",
        [Int, Int, Int, Int],
        Color,
        "цвет по модели HSVA"
    ),
    cmd!(
        None,
        "CMYK",
        "CMYK",
        [Int, Int, Int, Int],
        Color,
        "цвет по модели CMYK"
    ),
    cmd!(
        None,
        "CMYKA",
        "CMYKA",
        [Int, Int, Int, Int, Int],
        Color,
        "цвет по модели CMYKA"
    ),
    cmd!(
        None,
        "разложить в RGB",
        "split to RGB",
        [Color, OutInt, OutInt, OutInt],
        Void,
        "разложить цвет на R, G, B"
    ),
    cmd!(
        None,
        "разложить в CMYK",
        "split to CMYK",
        [Color, OutInt, OutInt, OutInt, OutInt],
        Void,
        "разложить цвет на C, M, Y, K"
    ),
    cmd!(
        None,
        "разложить в HSL",
        "split to HSL",
        [Color, OutInt, OutInt, OutInt],
        Void,
        "разложить цвет на H, S, L"
    ),
    cmd!(
        None,
        "разложить в HSV",
        "split to HSV",
        [Color, OutInt, OutInt, OutInt],
        Void,
        "разложить цвет на H, S, V"
    ),
];

/// All specs with this name (`вперед` belongs to both the Turtle and the
/// Grasshopper, `выбрать чернила` to the Turtle and the Draw).
pub fn lookup_all(name: &str) -> Vec<&'static CommandSpec> {
    let lower = name.to_lowercase();
    COMMANDS
        .iter()
        .chain(COLOR_COMMANDS.iter())
        .filter(|c| {
            c.ru == name
                || c.en == name
                || c.ru.to_lowercase() == lower
                || c.en.to_lowercase() == lower
        })
        .collect()
}

/// Choose the spec for an active actor, preferring the one whose arity matches
/// the call.  Colour functions are always available.
pub fn lookup_for(name: &str, active: &[ActorKind], argc: usize) -> Option<&'static CommandSpec> {
    let all = lookup_all(name);
    all.iter()
        .copied()
        .find(|c| (c.actor == ActorKind::None || active.contains(&c.actor)) && c.args.len() == argc)
        .or_else(|| {
            all.iter()
                .copied()
                .find(|c| c.actor == ActorKind::None || active.contains(&c.actor))
        })
        .or_else(|| all.first().copied())
}

/// Look a command up by its Russian or English name.
pub fn lookup(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS
        .iter()
        .chain(COLOR_COMMANDS.iter())
        .find(|c| c.ru == name || c.en == name)
        .or_else(|| {
            let query = name.to_lowercase();
            COMMANDS
                .iter()
                .chain(COLOR_COMMANDS.iter())
                .find(|c| c.ru.to_lowercase() == query || c.en.to_lowercase() == query)
        })
}

pub fn for_actor(actor: ActorKind) -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS.iter().filter(move |c| c.actor == actor)
}

/// All command names (Russian first, then English aliases) for the parser and
/// the syntax highlighter.
pub fn names() -> impl Iterator<Item = &'static str> {
    COMMANDS
        .iter()
        .chain(COLOR_COMMANDS.iter())
        .flat_map(|c| [c.ru, c.en])
}

/// Whether `name` is a command of any actor (fast path for the highlighter).
pub fn is_known(name: &str) -> bool {
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static EXACT: OnceLock<HashSet<&'static str>> = OnceLock::new();
    static LOWER: OnceLock<HashSet<String>> = OnceLock::new();
    let exact = EXACT.get_or_init(|| names().collect());
    if exact.contains(name) {
        return true;
    }
    let lower = LOWER.get_or_init(|| names().map(|n| n.to_lowercase()).collect());
    lower.contains(&name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_unique_and_found_by_both_names() {
        let mut seen = std::collections::HashSet::new();
        for spec in COMMANDS.iter().chain(COLOR_COMMANDS.iter()) {
            assert!(
                seen.insert((spec.actor, spec.ru)),
                "duplicate command {}",
                spec.ru
            );
            assert!(lookup(spec.ru).is_some(), "{} not found", spec.ru);
            assert!(lookup(spec.en).is_some(), "{} not found", spec.en);
        }
    }

    #[test]
    fn lookup_is_tolerant_to_case() {
        assert_eq!(lookup("НАПОЛНИ A").unwrap().ru, "наполни A");
        assert_eq!(lookup("go forward").unwrap().actor, ActorKind::Grasshopper);
    }

    #[test]
    fn signatures_look_like_kumir() {
        assert_eq!(lookup("вперед").unwrap().signature(), "вперед(вещ)");
        assert_eq!(lookup("наполни A").unwrap().signature(), "наполни A");
    }
}
