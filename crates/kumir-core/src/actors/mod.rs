//! Actors of the Kumir environment.
//!
//! The original project ships the actors as Qt plugins (`src/actors/*`).  Here
//! the simulation lives in the core crate and the IDE paints a snapshot of the
//! state.  Command names, arguments and semantics follow the `*.json`
//! descriptors of the original actors.
//!
//! Implemented: **Робот**, **Черепаха**, **Чертежник**, **Водолей**,
//! **Кузнечик**, **Рисователь** and the shared `_Colorer` functions.  Several
//! actors may be connected at the same time, as in Kumir 2.

use std::path::Path;

use crate::value::Value;

pub mod color;
pub mod commands;
pub mod draw;
pub mod grasshopper;
pub mod painter;
pub mod robot;
pub mod turtle;
pub mod vector;
pub mod vodoley;

pub use color::Rgba;
pub use commands::{lookup, lookup_for, ArgType, CommandSpec, RetType, COMMANDS};
pub use draw::DrawState;
pub use grasshopper::GrasshopperState;
pub use painter::{PainterPage, PainterState, TextItem};
pub use robot::{RobotCell, RobotField, DOWN_WALL, LEFT_WALL, RIGHT_WALL, UP_WALL};
pub use turtle::{TurtleBounds, TurtleState, ARENA_SIZE};
pub use vector::{Caption, Segment};
pub use vodoley::VodoleyState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorKind {
    None,
    Robot,
    Turtle,
    Draw,
    Vodoley,
    Grasshopper,
    Painter,
}

impl ActorKind {
    pub fn russian_name(self) -> &'static str {
        match self {
            ActorKind::None => "нет исполнителя",
            ActorKind::Robot => "Робот",
            ActorKind::Turtle => "Черепаха",
            ActorKind::Draw => "Чертежник",
            ActorKind::Vodoley => "Водолей",
            ActorKind::Grasshopper => "Кузнечик",
            ActorKind::Painter => "Рисователь",
        }
    }

    pub fn module_name(self) -> &'static str {
        self.russian_name()
    }

    pub fn ascii_name(self) -> &'static str {
        match self {
            ActorKind::None => "none",
            ActorKind::Robot => "Robot",
            ActorKind::Turtle => "Turtle",
            ActorKind::Draw => "Draw",
            ActorKind::Vodoley => "Vodoley",
            ActorKind::Grasshopper => "Grasshopper",
            ActorKind::Painter => "Painter",
        }
    }

    /// Match an `использовать <модуль>` name.
    pub fn from_module(module: &str) -> Option<Self> {
        let m = module.trim().to_lowercase();
        let kind = if m.contains("робот") || m == "robot" {
            ActorKind::Robot
        } else if m.contains("черепах") || m.contains("turtle") {
            ActorKind::Turtle
        } else if m.contains("чертеж") || m.contains("чертёж") || m.contains("draw") {
            ActorKind::Draw
        } else if m.contains("водолей") || m.contains("vodoley") {
            ActorKind::Vodoley
        } else if m.contains("кузнечик") || m.contains("grasshopper") {
            ActorKind::Grasshopper
        } else if m.contains("рисователь") || m.contains("painter") {
            ActorKind::Painter
        } else {
            return None;
        };
        Some(kind)
    }
}

/// The result of one actor command: a value plus any `рез` outputs.
#[derive(Clone, Debug)]
pub struct ExecResult {
    pub value: Value,
    pub outs: Vec<(usize, Value)>,
}

#[derive(Clone, Debug)]
pub struct ActorState {
    /// Actors connected in the order of `использовать` lines.
    pub active: Vec<ActorKind>,
    pub robot: RobotField,
    pub turtle: TurtleState,
    pub draw: DrawState,
    pub vodoley: VodoleyState,
    pub grasshopper: GrasshopperState,
    pub painter: PainterState,
    /// Human readable status / last command.
    pub status: String,
    /// The pult protocol: commands in Kumir syntax, newest last.
    pub log: Vec<String>,
}

impl Default for ActorState {
    fn default() -> Self {
        Self {
            active: Vec::new(),
            robot: RobotField::empty(7, 7),
            turtle: TurtleState::new(),
            draw: DrawState::new(),
            vodoley: VodoleyState::new(),
            grasshopper: GrasshopperState::new(),
            painter: PainterState::new(),
            status: String::new(),
            log: Vec::new(),
        }
    }
}

impl ActorState {
    /// Select the actors named in `использовать ...`.
    pub fn for_modules(modules: &[String]) -> Self {
        let mut result = Self::default();
        for module in modules {
            if let Some(kind) = ActorKind::from_module(module) {
                if kind != ActorKind::None && !result.active.contains(&kind) {
                    result.active.push(kind);
                }
            }
        }
        result
    }

    /// The actor shown by default in the IDE panel.
    pub fn kind(&self) -> ActorKind {
        self.active.first().copied().unwrap_or(ActorKind::None)
    }

    pub fn is_active(&self, kind: ActorKind) -> bool {
        self.active.contains(&kind)
    }

    pub fn reset_for_run(&mut self) {
        self.robot.reset();
        let bounds = self.turtle.bounds;
        self.turtle = TurtleState::new();
        self.turtle.bounds = bounds;
        self.draw = DrawState::new();
        self.vodoley.reset();
        self.grasshopper.reset();
        self.painter.reset();
        self.status.clear();
        self.log.clear();
    }

    pub fn load_field_file(&mut self, path: &Path) -> Result<(), String> {
        let field = RobotField::load_fil(path)?;
        self.robot = field;
        if !self.active.contains(&ActorKind::Robot) {
            self.active.push(ActorKind::Robot);
        }
        Ok(())
    }

    /// Execute one command with already evaluated arguments.
    ///
    /// Arguments are aligned with `spec.args`; out-parameters are passed as
    /// [`Value::Undefined`] placeholders and returned in [`ExecResult::outs`].
    pub fn exec(
        &mut self,
        spec: &CommandSpec,
        args: &[Value],
        source_dir: &Path,
    ) -> Result<ExecResult, String> {
        if spec.actor != ActorKind::None && !self.is_active(spec.actor) {
            return Err(format!(
                "Команда «{}» недоступна: не подключён исполнитель «{}» (добавьте строку «использовать {}»)",
                spec.ru,
                spec.actor.russian_name(),
                spec.actor.module_name()
            ));
        }
        if args.len() != spec.args.len() {
            return Err(format!(
                "Команда «{}» ожидает {} аргумент(ов), передано {}",
                spec.ru,
                spec.args.len(),
                args.len()
            ));
        }
        for (ty, value) in spec.args.iter().zip(args.iter()) {
            if ty.is_out() {
                continue;
            }
            check_arg_type(*ty, value)?;
        }
        let result = match spec.actor {
            ActorKind::Robot => self.exec_robot(spec, args),
            ActorKind::Turtle => self.exec_turtle(spec, args),
            ActorKind::Draw => self.exec_draw(spec, args),
            ActorKind::Vodoley => self.exec_vodoley(spec, args),
            ActorKind::Grasshopper => self.exec_grasshopper(spec, args),
            ActorKind::Painter => self.exec_painter(spec, args, source_dir),
            ActorKind::None => exec_colorer(spec, args),
        };
        match result {
            Ok(result) => {
                if !spec.is_query() {
                    let text = log_line(spec, args);
                    self.status = text.clone();
                    self.log.push(text);
                    if self.log.len() > 500 {
                        self.log.remove(0);
                    }
                }
                Ok(result)
            }
            Err(e) => {
                self.status = e.clone();
                Err(e)
            }
        }
    }
}

fn check_arg_type(ty: ArgType, value: &Value) -> Result<(), String> {
    let ok = match ty {
        ArgType::Int => matches!(value, Value::Int(_)),
        ArgType::Real => matches!(value, Value::Int(_) | Value::Real(_)),
        ArgType::Str => matches!(value, Value::Str(_) | Value::Char(_)),
        ArgType::Bool => matches!(value, Value::Bool(_)),
        ArgType::Color => matches!(value, Value::Str(_) | Value::Char(_) | Value::Int(_)),
        ArgType::IntArray => matches!(value, Value::Array(_)),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Неверный тип аргумента: ожидалось «{}», получено «{}»",
            ty.label(),
            value.type_name()
        ))
    }
}

fn arg_int(args: &[Value], i: usize) -> Result<i64, String> {
    match &args[i] {
        Value::Int(v) => Ok(*v),
        other => Err(format!("Ожидалось целое, получено «{}»", other.type_name())),
    }
}

fn arg_real(args: &[Value], i: usize) -> Result<f64, String> {
    match &args[i] {
        Value::Int(v) => Ok(*v as f64),
        Value::Real(v) => Ok(*v),
        other => Err(format!(
            "Ожидалось вещественное, получено «{}»",
            other.type_name()
        )),
    }
}

fn arg_str(args: &[Value], i: usize) -> Result<String, String> {
    match &args[i] {
        Value::Str(s) => Ok(s.clone()),
        Value::Char(c) => Ok(c.to_string()),
        other => Err(format!(
            "Ожидалась строка, получено «{}»",
            other.type_name()
        )),
    }
}

fn arg_bool(args: &[Value], i: usize) -> Result<bool, String> {
    match &args[i] {
        Value::Bool(v) => Ok(*v),
        other => Err(format!(
            "Ожидалось логическое значение, получено «{}»",
            other.type_name()
        )),
    }
}

fn arg_color(args: &[Value], i: usize) -> Result<Rgba, String> {
    match &args[i] {
        Value::Str(text) => {
            color::parse(text).ok_or_else(|| format!("Недопустимый цвет: «{text}»"))
        }
        Value::Char(c) => {
            color::parse(&c.to_string()).ok_or_else(|| format!("Недопустимый цвет: «{c}»"))
        }
        Value::Int(v) => Ok(color::from_rgb((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)),
        other => Err(format!(
            "Ожидался цвет (строка), получено «{}»",
            other.type_name()
        )),
    }
}

fn arg_int_array(args: &[Value], i: usize, count: i64) -> Result<Vec<i64>, String> {
    let Value::Array(array) = &args[i] else {
        return Err("Ожидалась таблица".to_string());
    };
    let lower = array.bounds.first().map(|b| b.0).unwrap_or(1);
    let available = array.element_count();
    let n = count.clamp(0, available as i64) as usize;
    let mut result = Vec::with_capacity(n);
    for k in 0..n {
        let value = array
            .get(&[lower + k as i64])
            .map_err(|e| format!("Ошибка доступа к таблице: {e}"))?;
        result.push(
            value
                .to_int(0)
                .map_err(|_| "Элемент таблицы не целый".to_string())?,
        );
    }
    Ok(result)
}

impl ActorState {
    fn exec_robot(&mut self, spec: &CommandSpec, args: &[Value]) -> Result<ExecResult, String> {
        let field = &mut self.robot;
        let value = match spec.ru {
            "вверх" => {
                field.move_up()?;
                Value::Undefined
            }
            "вниз" => {
                field.move_down()?;
                Value::Undefined
            }
            "влево" => {
                field.move_left()?;
                Value::Undefined
            }
            "вправо" => {
                field.move_right()?;
                Value::Undefined
            }
            "закрасить" => {
                field.paint();
                Value::Undefined
            }
            "сверху стена" => Value::Bool(field.wall_up()),
            "снизу стена" => Value::Bool(field.wall_down()),
            "слева стена" => Value::Bool(field.wall_left()),
            "справа стена" => Value::Bool(field.wall_right()),
            "сверху свободно" => Value::Bool(!field.wall_up()),
            "снизу свободно" => Value::Bool(!field.wall_down()),
            "слева свободно" => Value::Bool(!field.wall_left()),
            "справа свободно" => Value::Bool(!field.wall_right()),
            "клетка закрашена" => Value::Bool(field.current().color),
            "клетка чистая" => Value::Bool(!field.current().color),
            "радиация" => Value::Real(field.current().radiation),
            "температура" => Value::Int(field.current().temperature),
            "@@размер поля" => {
                return Ok(ExecResult {
                    value: Value::Undefined,
                    outs: vec![
                        (0, Value::Int(field.height as i64)),
                        (1, Value::Int(field.width as i64)),
                    ],
                })
            }
            "@@робот" => {
                return Ok(ExecResult {
                    value: Value::Undefined,
                    outs: vec![
                        (0, Value::Int(field.robot_y as i64 + 1)),
                        (1, Value::Int(field.robot_x as i64 + 1)),
                    ],
                })
            }
            "@@метка" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Bool(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.point)
                        .unwrap_or(false),
                )
            }
            "@@закрашена" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Bool(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.color)
                        .unwrap_or(false),
                )
            }
            "@@верхняя буква" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Char(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.symbol)
                        .unwrap_or('$'),
                )
            }
            "@@нижняя буква" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Char(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.symbol1)
                        .unwrap_or('$'),
                )
            }
            "@@температура" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Int(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.temperature)
                        .unwrap_or(0),
                )
            }
            "@@радиация" => {
                let (row, col) = (arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Real(
                    field
                        .cell(col - 1, row - 1)
                        .map(|c| c.radiation)
                        .unwrap_or(0.0),
                )
            }
            other => return Err(format!("Команда Робота «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }

    fn exec_turtle(&mut self, spec: &CommandSpec, args: &[Value]) -> Result<ExecResult, String> {
        let turtle = &mut self.turtle;
        let value = match spec.ru {
            "поднять хвост" => {
                turtle.pen_down = false;
                Value::Undefined
            }
            "опустить хвост" => {
                turtle.pen_down = true;
                Value::Undefined
            }
            "вперед" => {
                turtle.forward(arg_real(args, 0)?)?;
                Value::Undefined
            }
            "назад" => {
                turtle.back(arg_real(args, 0)?)?;
                Value::Undefined
            }
            "влево" => {
                turtle.turn_left(arg_real(args, 0)?);
                Value::Undefined
            }
            "вправо" => {
                turtle.turn_right(arg_real(args, 0)?);
                Value::Undefined
            }
            "выбрать чернила" => {
                turtle.color = arg_color(args, 0)?;
                Value::Undefined
            }
            other => return Err(format!("Команда Черепахи «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }

    fn exec_draw(&mut self, spec: &CommandSpec, args: &[Value]) -> Result<ExecResult, String> {
        let draw = &mut self.draw;
        let value = match spec.ru {
            "опустить перо" => {
                draw.pen_down = true;
                Value::Undefined
            }
            "поднять перо" => {
                draw.pen_down = false;
                Value::Undefined
            }
            "выбрать чернила" | "установить цвет" => {
                draw.color = arg_color(args, 0)?;
                Value::Undefined
            }
            "сместиться в точку" => {
                draw.move_to(arg_real(args, 0)?, arg_real(args, 1)?);
                Value::Undefined
            }
            "сместиться на вектор" => {
                draw.move_by(arg_real(args, 0)?, arg_real(args, 1)?);
                Value::Undefined
            }
            "написать" | "надпись" => {
                let width = arg_real(args, 0)?;
                let text = arg_str(args, 1)?;
                draw.write(width, &text);
                Value::Undefined
            }
            "@есть отрезок в области" => {
                let x = arg_real(args, 0)?;
                let y = arg_real(args, 1)?;
                let r = arg_real(args, 2)?;
                Value::Bool(draw.line_at_circle(x, y, r))
            }
            other => return Err(format!("Команда Чертежника «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }

    fn exec_vodoley(&mut self, spec: &CommandSpec, _args: &[Value]) -> Result<ExecResult, String> {
        let v = &mut self.vodoley;
        let value = match spec.ru {
            "наполни A" => {
                v.fill_vial(0);
                Value::Undefined
            }
            "наполни B" => {
                v.fill_vial(1);
                Value::Undefined
            }
            "наполни C" => {
                v.fill_vial(2);
                Value::Undefined
            }
            "вылей A" => {
                v.empty_vial(0);
                Value::Undefined
            }
            "вылей B" => {
                v.empty_vial(1);
                Value::Undefined
            }
            "вылей C" => {
                v.empty_vial(2);
                Value::Undefined
            }
            "перелей из A в B" => {
                v.pour(0, 1);
                Value::Undefined
            }
            "перелей из A в C" => {
                v.pour(0, 2);
                Value::Undefined
            }
            "перелей из B в A" => {
                v.pour(1, 0);
                Value::Undefined
            }
            "перелей из B в C" => {
                v.pour(1, 2);
                Value::Undefined
            }
            "перелей из C в A" => {
                v.pour(2, 0);
                Value::Undefined
            }
            "перелей из C в B" => {
                v.pour(2, 1);
                Value::Undefined
            }
            "@решено" => Value::Bool(v.solved()),
            "размер A" => Value::Int(v.capacity[0]),
            "размер B" => Value::Int(v.capacity[1]),
            "размер C" => Value::Int(v.capacity[2]),
            "в сосуде A" => Value::Int(v.fill[0]),
            "в сосуде B" => Value::Int(v.fill[1]),
            "в сосуде C" => Value::Int(v.fill[2]),
            other => return Err(format!("Команда Водолея «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }

    fn exec_grasshopper(
        &mut self,
        spec: &CommandSpec,
        args: &[Value],
    ) -> Result<ExecResult, String> {
        let g = &mut self.grasshopper;
        let value = match spec.ru {
            "вперед" => {
                g.jump_forward(arg_int(args, 0)?)?;
                Value::Undefined
            }
            "назад" => {
                g.jump_backward(arg_int(args, 0)?)?;
                Value::Undefined
            }
            "перекрасить" => {
                g.repaint();
                Value::Undefined
            }
            other => return Err(format!("Команда Кузнечика «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }

    fn exec_painter(
        &mut self,
        spec: &CommandSpec,
        args: &[Value],
        source_dir: &Path,
    ) -> Result<ExecResult, String> {
        let p = &mut self.painter;
        let resolve = |name: &str| -> std::path::PathBuf {
            let path = std::path::Path::new(name);
            if path.is_absolute() || source_dir.as_os_str().is_empty() {
                path.to_path_buf()
            } else {
                source_dir.join(path)
            }
        };
        let value = match spec.ru {
            "в точку" => {
                p.move_to(arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Undefined
            }
            "линия в точку" => {
                p.line_to(arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Undefined
            }
            "линия" => {
                p.line(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_int(args, 2)? as i32,
                    arg_int(args, 3)? as i32,
                );
                Value::Undefined
            }
            "прямоугольник" => {
                p.rectangle(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_int(args, 2)? as i32,
                    arg_int(args, 3)? as i32,
                );
                Value::Undefined
            }
            "эллипс" => {
                p.ellipse(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_int(args, 2)? as i32,
                    arg_int(args, 3)? as i32,
                );
                Value::Undefined
            }
            "окружность" => {
                p.circle(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_int(args, 2)? as i32,
                )?;
                Value::Undefined
            }
            "многоугольник" => {
                let n = arg_int(args, 0)?;
                let xs = arg_int_array(args, 1, n)?;
                let ys = arg_int_array(args, 2, n)?;
                p.polygon(&xs, &ys);
                Value::Undefined
            }
            "надпись" => {
                p.write(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    &arg_str(args, 2)?,
                );
                Value::Undefined
            }
            "залить" => {
                p.flood_fill(arg_int(args, 0)? as i32, arg_int(args, 1)? as i32);
                Value::Undefined
            }
            "пиксель" => {
                p.set_pixel(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_color(args, 2)?,
                )?;
                Value::Undefined
            }
            "значение в точке" => {
                let text = p.pixel_color(arg_int(args, 0)? as i32, arg_int(args, 1)? as i32)?;
                Value::Str(text)
            }
            "перо" => {
                let width = arg_int(args, 0)? as i32;
                let color = arg_color(args, 1)?;
                p.set_pen(width, color)?;
                Value::Undefined
            }
            "кисть" => {
                let c = arg_color(args, 0)?;
                p.set_brush(if c.is_transparent() { None } else { Some(c) });
                Value::Undefined
            }
            "убрать кисть" => {
                p.set_brush(None);
                Value::Undefined
            }
            "шрифт" => {
                p.set_font(
                    &arg_str(args, 0)?,
                    arg_int(args, 1)? as i32,
                    arg_bool(args, 2)?,
                    arg_bool(args, 3)?,
                )?;
                Value::Undefined
            }
            "новый лист" => {
                p.new_page(
                    arg_int(args, 0)? as i32,
                    arg_int(args, 1)? as i32,
                    arg_color(args, 2)?,
                )?;
                Value::Undefined
            }
            "загрузить лист" => {
                let path = resolve(&arg_str(args, 0)?);
                *p = PainterState::load_png(&path)?;
                Value::Undefined
            }
            "сохранить лист" => {
                let path = resolve(&arg_str(args, 0)?);
                p.save_png(&path)?;
                Value::Undefined
            }
            "высота листа" => Value::Int(p.page.height as i64),
            "ширина листа" => Value::Int(p.page.width as i64),
            "центр x" => Value::Int((p.page.width / 2) as i64),
            "центр y" => Value::Int((p.page.height / 2) as i64),
            "ширина текста" => Value::Int(p.text_width(&arg_str(args, 0)?) as i64),
            "плотность" => {
                p.density = arg_int(args, 0)? as i32;
                Value::Undefined
            }
            other => return Err(format!("Команда Рисователя «{other}» не реализована")),
        };
        Ok(ExecResult {
            value,
            outs: Vec::new(),
        })
    }
}

fn exec_colorer(spec: &CommandSpec, args: &[Value]) -> Result<ExecResult, String> {
    let value = match spec.ru {
        "RGB" => Value::Str(
            color::from_rgb(arg_int(args, 0)?, arg_int(args, 1)?, arg_int(args, 2)?).to_kumir(),
        ),
        "RGBA" => Value::Str(
            color::from_rgba(
                arg_int(args, 0)?,
                arg_int(args, 1)?,
                arg_int(args, 2)?,
                arg_int(args, 3)?,
            )
            .to_kumir(),
        ),
        "HSL" => Value::Str(
            color::from_hsl(arg_int(args, 0)?, arg_int(args, 1)?, arg_int(args, 2)?).to_kumir(),
        ),
        "HSLA" => {
            let rgb = color::from_hsl(arg_int(args, 0)?, arg_int(args, 1)?, arg_int(args, 2)?);
            Value::Str(
                color::from_rgba(rgb.r as i64, rgb.g as i64, rgb.b as i64, arg_int(args, 3)?)
                    .to_kumir(),
            )
        }
        "HSV" => Value::Str(
            color::from_hsv(arg_int(args, 0)?, arg_int(args, 1)?, arg_int(args, 2)?).to_kumir(),
        ),
        "HSVA" => {
            let rgb = color::from_hsv(arg_int(args, 0)?, arg_int(args, 1)?, arg_int(args, 2)?);
            Value::Str(
                color::from_rgba(rgb.r as i64, rgb.g as i64, rgb.b as i64, arg_int(args, 3)?)
                    .to_kumir(),
            )
        }
        "CMYK" => Value::Str(
            color::from_cmyk(
                arg_int(args, 0)?,
                arg_int(args, 1)?,
                arg_int(args, 2)?,
                arg_int(args, 3)?,
            )
            .to_kumir(),
        ),
        "CMYKA" => {
            let rgb = color::from_cmyk(
                arg_int(args, 0)?,
                arg_int(args, 1)?,
                arg_int(args, 2)?,
                arg_int(args, 3)?,
            );
            Value::Str(
                color::from_rgba(rgb.r as i64, rgb.g as i64, rgb.b as i64, arg_int(args, 4)?)
                    .to_kumir(),
            )
        }
        "разложить в RGB" => {
            let (r, g, b) = color::split_rgb(arg_color(args, 0)?);
            return Ok(ExecResult {
                value: Value::Undefined,
                outs: vec![(1, Value::Int(r)), (2, Value::Int(g)), (3, Value::Int(b))],
            });
        }
        "разложить в CMYK" => {
            let (c, m, y, k) = color::split_cmyk(arg_color(args, 0)?);
            return Ok(ExecResult {
                value: Value::Undefined,
                outs: vec![
                    (1, Value::Int(c)),
                    (2, Value::Int(m)),
                    (3, Value::Int(y)),
                    (4, Value::Int(k)),
                ],
            });
        }
        "разложить в HSL" => {
            let (h, s, l) = color::split_hsl(arg_color(args, 0)?);
            return Ok(ExecResult {
                value: Value::Undefined,
                outs: vec![(1, Value::Int(h)), (2, Value::Int(s)), (3, Value::Int(l))],
            });
        }
        "разложить в HSV" => {
            let (h, s, v) = color::split_hsv(arg_color(args, 0)?);
            return Ok(ExecResult {
                value: Value::Undefined,
                outs: vec![(1, Value::Int(h)), (2, Value::Int(s)), (3, Value::Int(v))],
            });
        }
        other => return Err(format!("Функция цвета «{other}» не реализована")),
    };
    Ok(ExecResult {
        value,
        outs: Vec::new(),
    })
}

/// Text of a command for the pult protocol.
fn log_line(spec: &CommandSpec, args: &[Value]) -> String {
    if args.is_empty() {
        return spec.ru.to_string();
    }
    let args: Vec<String> = spec
        .args
        .iter()
        .zip(args.iter())
        .filter(|(ty, _)| !ty.is_out())
        .map(|(_, value)| format_value(value))
        .collect();
    format!("{}({})", spec.ru, args.join(", "))
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Int(v) => v.to_string(),
        Value::Real(v) => {
            if v.fract().abs() < 1e-12 {
                format!("{v:.1}")
            } else {
                format!("{v}")
            }
        }
        Value::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Char(c) => format!("'{c}'"),
        Value::Bool(true) => "да".to_string(),
        Value::Bool(false) => "нет".to_string(),
        _ => "…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(module: &str) -> ActorState {
        ActorState::for_modules(&[module.to_string()])
    }

    fn run(state: &mut ActorState, name: &str, args: &[Value]) -> Result<Value, String> {
        let spec = commands::lookup_for(name, &state.active, args.len()).expect("known command");
        state.exec(spec, args, Path::new(".")).map(|r| r.value)
    }

    #[test]
    fn actors_are_selected_from_modules() {
        let st = ActorState::for_modules(&[
            "Робот".to_string(),
            "Черепаха".to_string(),
            "Рисователь".to_string(),
        ]);
        assert_eq!(
            st.active,
            vec![ActorKind::Robot, ActorKind::Turtle, ActorKind::Painter]
        );
        assert_eq!(
            ActorState::for_modules(&["Файлы".to_string()]).kind(),
            ActorKind::None
        );
    }

    #[test]
    fn vodoley_commands() {
        let mut st = state("Водолей");
        st.vodoley.set_task([3, 5, 7], [0, 0, 0], 1);
        run(&mut st, "наполни B", &[]).unwrap();
        run(&mut st, "перелей из B в A", &[]).unwrap();
        assert_eq!(
            run(&mut st, "в сосуде A", &[]).unwrap().to_int(0).unwrap(),
            3
        );
        assert!(!run(&mut st, "@решено", &[]).unwrap().to_bool(0).unwrap());
        assert_eq!(st.log.last().unwrap(), "перелей из B в A");
    }

    #[test]
    fn grasshopper_commands_validate_steps() {
        let mut st = state("Кузнечик");
        st.grasshopper.set_task(3, 5, 0, None, vec![]);
        run(&mut st, "вперед", &[Value::Int(3)]).unwrap();
        assert_eq!(st.grasshopper.pos, 3);
        assert!(run(&mut st, "вперед", &[Value::Int(4)]).is_err());
        run(&mut st, "перекрасить", &[]).unwrap();
        assert_eq!(st.grasshopper.painted, vec![3]);
    }

    #[test]
    fn draw_and_color_functions() {
        let mut st = state("Чертежник");
        run(&mut st, "установить цвет", &[Value::Str("красный".into())]).unwrap();
        assert_eq!(st.draw.color, Rgba::rgb(255, 0, 0));
        run(&mut st, "опустить перо", &[]).unwrap();
        run(
            &mut st,
            "сместиться в точку",
            &[Value::Int(10), Value::Int(0)],
        )
        .unwrap();
        assert_eq!(st.draw.segments.len(), 1);
        assert!(run(
            &mut st,
            "@есть отрезок в области",
            &[Value::Int(5), Value::Int(0), Value::Int(2)]
        )
        .unwrap()
        .to_bool(0)
        .unwrap());
        let spec = commands::lookup("RGB").unwrap();
        let out = st
            .exec(
                spec,
                &[Value::Int(1), Value::Int(2), Value::Int(3)],
                Path::new("."),
            )
            .unwrap();
        assert!(matches!(out.value, Value::Str(ref s) if s == "rgb(1, 2, 3)"));
    }

    #[test]
    fn command_requires_its_actor() {
        let mut st = state("Робот");
        let err = run(&mut st, "наполни A", &[]).unwrap_err();
        assert!(err.contains("Водолей"));
    }

    #[test]
    fn painter_commands() {
        let mut st = state("Рисователь");
        run(
            &mut st,
            "новый лист",
            &[Value::Int(100), Value::Int(50), Value::Str("белый".into())],
        )
        .unwrap();
        run(
            &mut st,
            "перо",
            &[Value::Int(2), Value::Str("черный".into())],
        )
        .unwrap();
        run(
            &mut st,
            "линия",
            &[Value::Int(0), Value::Int(0), Value::Int(20), Value::Int(20)],
        )
        .unwrap();
        assert_eq!(st.painter.page.pixel(10, 10), Some(Rgba::BLACK));
        let color = run(
            &mut st,
            "значение в точке",
            &[Value::Int(10), Value::Int(10)],
        )
        .unwrap();
        assert!(matches!(color, Value::Str(ref s) if s == "rgb(0, 0, 0)"));
    }
}
