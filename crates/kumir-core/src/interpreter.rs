//! Tree-walking interpreter for Kumir programs.
//!
//! This replaces the bytecode VM from `src/kumir2-libs/vm` while keeping the
//! observable semantics of the language: 1-based arrays and strings, floor
//! division in `div`, non-negative `mod`, value semantics for arrays, output
//! formatting, console and file I/O, `арг`/`рез`/`аргрез` parameters.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::actors::{ActorKind, ActorState};
use crate::ast::*;
use crate::builtins;
use crate::diag::RuntimeError;
use crate::value::{coerce, compare_values, new_cell, values_equal, Array, Cell, Value};

pub const STOP_MESSAGE: &str = "Выполнение остановлено пользователем";

/// Interface between the interpreter and the environment (terminal or IDE).
pub trait Host {
    fn write(&mut self, text: &str);
    /// Read one line. An `Err` aborts the program (used by the IDE's Stop).
    fn read_line(&mut self) -> Result<String, String>;
    /// Called before every statement; the IDE uses it for the current-line
    /// indicator and for breakpoints.
    fn on_line(&mut self, _line: u32) {}
    fn on_pause(&mut self) {}
    /// Whether the environment wants variable snapshots (usually only while
    /// the debugger is paused).
    fn wants_vars(&self) -> bool {
        false
    }
    fn on_vars(&mut self, _vars: Vec<(String, String, String)>) {}
    fn should_stop(&self) -> bool {
        false
    }
}

#[derive(Clone, Default)]
pub struct RunOptions {
    /// Directory used to resolve relative file names and `КАТАЛОГ ПРОГРАММЫ`.
    pub source_dir: PathBuf,
    /// Optional shared actor state (used by the IDE to draw the Robot/Turtle).
    pub actor: Option<Arc<Mutex<ActorState>>>,
    /// Run the hidden teacher algorithm `@тестирование` after the main one.
    pub evaluate: bool,
    /// Delay after each visual actor command (Robot/Turtle/Draw), for smooth
    /// on-screen movement in the IDE. Zero means no artificial delay.
    pub actor_delay: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunOutcome {
    pub steps: u64,
    pub mark: Option<i64>,
}

pub fn run(
    program: &Program,
    host: &mut dyn Host,
    options: RunOptions,
) -> Result<RunOutcome, RuntimeError> {
    let mut interp = Interpreter::new(program, host, options);
    interp.execute()
}

#[derive(Default)]
struct Frame {
    vars: HashMap<String, Cell>,
    types: HashMap<String, TypeRef>,
    ret: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Normal,
    Break,
    Return,
    Halt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingKind {
    Utf8,
    Cp1251,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileMode {
    Read,
    Write,
    Append,
}

struct OpenFile {
    path: PathBuf,
    mode: FileMode,
    content: String,
    pos: usize,
    write_bom: bool,
    encoding: EncodingKind,
    closed: bool,
}

struct Rng {
    state: u64,
}

impl Rng {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678_9abc_def0);
        Self { state: seed | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    fn irand(&mut self, a: i64, b: i64, line: u32) -> Result<i64, RuntimeError> {
        if a > b {
            return Err(RuntimeError::new(line, "Неверный диапазон чисел"));
        }
        if a == b {
            return Ok(a);
        }
        let width = (b - a + 1) as u64;
        Ok(a + (self.next_u64() % width) as i64)
    }

    fn rrand(&mut self, a: f64, b: f64, line: u32) -> Result<f64, RuntimeError> {
        if !(a <= b) {
            return Err(RuntimeError::new(line, "Неверный диапазон чисел"));
        }
        Ok(a + self.unit() * (b - a))
    }
}

pub struct Interpreter<'a> {
    program: &'a Program,
    host: &'a mut dyn Host,
    options: RunOptions,
    globals: Frame,
    frames: Vec<Frame>,
    files: HashMap<u64, OpenFile>,
    next_file_id: u64,
    console_buf: Vec<char>,
    console_pos: usize,
    assigned_in: Option<u64>,
    assigned_out: Option<u64>,
    rng: Rng,
    steps: u64,
    call_depth: usize,
    halted: bool,
    actor: Arc<Mutex<ActorState>>,
    mark: Option<i64>,
    file_encoding: EncodingKind,
}

impl<'a> Interpreter<'a> {
    pub fn new(program: &'a Program, host: &'a mut dyn Host, options: RunOptions) -> Self {
        let actor = options
            .actor
            .clone()
            .unwrap_or_else(|| Arc::new(Mutex::new(ActorState::for_modules(&program.modules))));
        Self {
            program,
            host,
            options,
            globals: Frame::default(),
            frames: Vec::new(),
            files: HashMap::new(),
            next_file_id: 1,
            console_buf: Vec::new(),
            console_pos: 0,
            assigned_in: None,
            assigned_out: None,
            rng: Rng::new(),
            steps: 0,
            call_depth: 0,
            halted: false,
            actor,
            mark: None,
            file_encoding: EncodingKind::Utf8,
        }
    }

    fn execute(&mut self) -> Result<RunOutcome, RuntimeError> {
        if let Ok(mut actor) = self.actor.lock() {
            actor.reset_for_run();
        }
        // Declare and initialize globals in source order.
        let program: &'a Program = self.program;
        for decl in &program.globals {
            self.declare_one(decl)?;
        }
        // Statements before the first algorithm form the module initializer.
        let flow = self.exec_block(&program.initializer, 0)?;
        if flow == Flow::Halt {
            self.close_all_files();
            return Ok(RunOutcome {
                steps: self.steps,
                mark: self.mark,
            });
        }
        let test_idx = program.algorithm_index("@тестирование");
        // In evaluation mode the hidden teacher algorithm is the entry point:
        // it calls the student's algorithm itself and checks the result.
        let run_main = !self.options.evaluate || test_idx.is_none();
        if run_main {
            if let Some(main_idx) = program.main {
                let alg = &program.algorithms[main_idx];
                self.call_algorithm(alg, &[], alg.line)?;
            }
        }
        if self.options.evaluate && !self.halted {
            if let Some(idx) = test_idx {
                let alg = &program.algorithms[idx];
                let v = self.call_algorithm(alg, &[], alg.line)?;
                if let Value::Int(m) = v {
                    self.mark = Some(m);
                }
            }
        }
        if let Some(mark) = self.mark {
            self.host
                .write(&format!("\n=== Оценка: {mark} из 10 ===\n"));
        }
        self.close_all_files();
        Ok(RunOutcome {
            steps: self.steps,
            mark: self.mark,
        })
    }

    // ------------------------------------------------------------------
    // Scopes
    // ------------------------------------------------------------------

    fn find_cell(&self, name: &str) -> Option<Cell> {
        if let Some(frame) = self.frames.last() {
            if let Some(c) = frame.vars.get(name) {
                return Some(c.clone());
            }
        }
        self.globals.vars.get(name).cloned()
    }

    fn find_type(&self, name: &str) -> Option<TypeRef> {
        if let Some(frame) = self.frames.last() {
            if let Some(t) = frame.types.get(name) {
                return Some(t.clone());
            }
        }
        self.globals.types.get(name).cloned()
    }

    fn insert_var(&mut self, name: String, cell: Cell, ty: TypeRef) {
        if let Some(frame) = self.frames.last_mut() {
            frame.vars.insert(name.clone(), cell);
            frame.types.insert(name, ty);
        } else {
            self.globals.vars.insert(name.clone(), cell);
            self.globals.types.insert(name, ty);
        }
    }

    fn declare_one(&mut self, decl: &VarDecl) -> Result<(), RuntimeError> {
        let ty = decl.typ.clone();
        let value = if ty.is_array {
            let mut bounds = Vec::new();
            for (start_expr, end_expr) in &decl.bounds {
                let start = self.eval(start_expr)?.to_int(decl.line)?;
                let end = match end_expr {
                    Some(e) => self.eval(e)?.to_int(decl.line)?,
                    None => start,
                };
                bounds.push((start, end));
            }
            if bounds.is_empty() {
                return Err(RuntimeError::new(
                    decl.line,
                    format!("Для таблицы «{}» не указаны границы", decl.name),
                ));
            }
            // Kumir leaves array elements uninitialized; Undefined makes
            // reading an unassigned element a proper runtime error.
            let mut array = Array::new(bounds, &Value::Undefined)
                .map_err(|e| RuntimeError::new(decl.line, e))?;
            if let Some(init) = &decl.init {
                let source = self.eval(init)?;
                match source {
                    Value::Array(src) => {
                        for (dst, src_cell) in array.data.iter_mut().zip(src.data.iter()) {
                            *dst.borrow_mut() = src_cell.borrow().clone();
                        }
                    }
                    Value::Int(v) => {
                        for dst in &array.data {
                            *dst.borrow_mut() = Value::Int(v);
                        }
                    }
                    Value::Real(v) => {
                        for dst in &array.data {
                            *dst.borrow_mut() = Value::Real(v);
                        }
                    }
                    other => {
                        return Err(RuntimeError::new(
                            decl.line,
                            format!(
                                "Нельзя инициализировать таблицу значением типа «{}»",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Value::Array(array)
        } else if let Some(init) = &decl.init {
            let raw = self.eval(init)?;
            coerce(raw, &ty, decl.line)?
        } else {
            Value::Undefined
        };
        self.insert_var(decl.name.clone(), new_cell(value), ty);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn exec_block(&mut self, stmts: &[Stmt], loop_depth: usize) -> Result<Flow, RuntimeError> {
        for stmt in stmts {
            if self.halted {
                return Ok(Flow::Halt);
            }
            if self.host.should_stop() {
                return Err(RuntimeError::new(stmt.line, STOP_MESSAGE));
            }
            self.host.on_line(stmt.line);
            if self.host.wants_vars() {
                let snapshot = self.snapshot_vars();
                self.host.on_vars(snapshot);
            }
            self.steps += 1;
            let flow = self.exec_stmt(stmt, loop_depth)?;
            if flow != Flow::Normal {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn snapshot_vars(&self) -> Vec<(String, String, String)> {
        let mut result = Vec::new();
        let mut push_frame = |frame: &Frame| {
            for (name, cell) in &frame.vars {
                if name == "знач" {
                    continue;
                }
                let ty = frame
                    .types
                    .get(name)
                    .map(|t| t.base.russian_name().to_string())
                    .unwrap_or_default();
                let value = {
                    let v = cell.borrow();
                    match &*v {
                        Value::Undefined => "—".to_string(),
                        Value::Array(a) => format!("[{} элем.]", a.element_count()),
                        other => crate::builtins::format_value(other, 0)
                            .unwrap_or_else(|_| "—".to_string()),
                    }
                };
                result.push((name.clone(), ty, value));
            }
        };
        if let Some(frame) = self.frames.last() {
            push_frame(frame);
        }
        push_frame(&self.globals);
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    fn exec_stmt(&mut self, stmt: &Stmt, loop_depth: usize) -> Result<Flow, RuntimeError> {
        match &stmt.kind {
            StmtKind::Decl(decls) => {
                for d in decls {
                    self.declare_one(d)?;
                }
                Ok(Flow::Normal)
            }
            StmtKind::Assign(lv, expr) => {
                let value = self.eval(expr)?;
                self.assign_lvalue(lv, value)?;
                Ok(Flow::Normal)
            }
            StmtKind::Expr(expr) => {
                self.eval(expr)?;
                Ok(Flow::Normal)
            }
            StmtKind::Output { file, args } => {
                self.exec_output(file.as_ref(), args, stmt.line)?;
                Ok(Flow::Normal)
            }
            StmtKind::Input { file, targets } => {
                self.exec_input(file.as_ref(), targets, stmt.line)?;
                Ok(Flow::Normal)
            }
            StmtKind::If(branches) => {
                for b in branches {
                    let take = match &b.condition {
                        Some(c) => self.eval(c)?.to_bool(b.line)?,
                        None => true,
                    };
                    if take {
                        return self.exec_block(&b.body, loop_depth);
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Switch(branches) => {
                for b in branches {
                    let take = match &b.condition {
                        Some(c) => self.eval(c)?.to_bool(b.line)?,
                        None => true,
                    };
                    if take {
                        return self.exec_block(&b.body, loop_depth);
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Loop {
                kind,
                end_cond,
                body,
            } => self.exec_loop(kind, end_cond.as_ref(), body, stmt.line),
            StmtKind::Assert(expr) => {
                let ok = self.eval(expr)?.to_bool(stmt.line)?;
                if !ok {
                    return Err(RuntimeError::new(
                        stmt.line,
                        "Утверждение ложно (утв не выполнено)",
                    ));
                }
                Ok(Flow::Normal)
            }
            StmtKind::Return(expr) => {
                // `знач := ...` assigns the return value but does not leave
                // the algorithm: an early exit is written as `выход`.
                let value = self.eval(expr)?;
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(cell) = frame.vars.get("знач").cloned() {
                        let ty = frame
                            .types
                            .get("знач")
                            .cloned()
                            .unwrap_or_else(|| TypeRef::simple(BaseType::Int));
                        *cell.borrow_mut() = coerce(value, &ty, stmt.line)?;
                    } else {
                        frame.ret = Some(value);
                    }
                }
                Ok(Flow::Normal)
            }
            StmtKind::Break => {
                if loop_depth > 0 {
                    Ok(Flow::Break)
                } else {
                    Ok(Flow::Return)
                }
            }
            StmtKind::Pause => {
                self.host.on_pause();
                Ok(Flow::Normal)
            }
            StmtKind::Halt => {
                self.halted = true;
                Ok(Flow::Halt)
            }
        }
    }

    fn exec_loop(
        &mut self,
        kind: &LoopKind,
        end_cond: Option<&Expr>,
        body: &[Stmt],
        line: u32,
    ) -> Result<Flow, RuntimeError> {
        match kind {
            LoopKind::For {
                var,
                from,
                to,
                step,
            } => {
                let mut current = self.eval(from)?.to_int(line)?;
                let limit = self.eval(to)?.to_int(line)?;
                let step_value = match step {
                    Some(e) => self.eval(e)?.to_int(line)?,
                    None => 1,
                };
                if step_value == 0 {
                    return Err(RuntimeError::new(line, "Шаг цикла не может быть нулём"));
                }
                loop {
                    if self.host.should_stop() {
                        return Err(RuntimeError::new(line, STOP_MESSAGE));
                    }
                    let keep_going = if step_value > 0 {
                        current <= limit
                    } else {
                        current >= limit
                    };
                    if !keep_going {
                        break;
                    }
                    let lv = LValue {
                        name: var.clone(),
                        subs: Vec::new(),
                        line,
                    };
                    self.assign_lvalue(&lv, Value::Int(current))?;
                    self.steps += 1;
                    let flow = self.exec_block(body, 1)?;
                    match flow {
                        Flow::Break => break,
                        Flow::Return | Flow::Halt => return Ok(flow),
                        Flow::Normal => {}
                    }
                    if let Some(cond) = end_cond {
                        if self.eval(cond)?.to_bool(line)? {
                            break;
                        }
                    }
                    current += step_value;
                }
                Ok(Flow::Normal)
            }
            LoopKind::While(cond) => {
                loop {
                    if self.host.should_stop() {
                        return Err(RuntimeError::new(line, STOP_MESSAGE));
                    }
                    if !self.eval(cond)?.to_bool(line)? {
                        break;
                    }
                    let flow = self.exec_block(body, 1)?;
                    match flow {
                        Flow::Break => break,
                        Flow::Return | Flow::Halt => return Ok(flow),
                        Flow::Normal => {}
                    }
                    if let Some(c) = end_cond {
                        if self.eval(c)?.to_bool(line)? {
                            break;
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            LoopKind::Times(count) => {
                let n = self.eval(count)?.to_int(line)?.max(0);
                for _ in 0..n {
                    if self.host.should_stop() {
                        return Err(RuntimeError::new(line, STOP_MESSAGE));
                    }
                    let flow = self.exec_block(body, 1)?;
                    match flow {
                        Flow::Break => break,
                        Flow::Return | Flow::Halt => return Ok(flow),
                        Flow::Normal => {}
                    }
                    if let Some(c) = end_cond {
                        if self.eval(c)?.to_bool(line)? {
                            break;
                        }
                    }
                }
                Ok(Flow::Normal)
            }
            LoopKind::Forever => {
                loop {
                    if self.host.should_stop() {
                        return Err(RuntimeError::new(line, STOP_MESSAGE));
                    }
                    let flow = self.exec_block(body, 1)?;
                    match flow {
                        Flow::Break => break,
                        Flow::Return | Flow::Halt => return Ok(flow),
                        Flow::Normal => {}
                    }
                    if let Some(c) = end_cond {
                        if self.eval(c)?.to_bool(line)? {
                            break;
                        }
                    }
                }
                Ok(Flow::Normal)
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match &expr.kind {
            ExprKind::Int(v) => Ok(Value::Int(*v)),
            ExprKind::Real(v) => Ok(Value::Real(*v)),
            ExprKind::Str(s) => Ok(Value::Str(s.clone())),
            ExprKind::Char(c) => Ok(Value::Char(*c)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Newline => Ok(Value::Str("\n".to_string())),
            ExprKind::Var(name) => {
                let cell = self.find_cell(name).ok_or_else(|| {
                    RuntimeError::new(expr.line, format!("Величина «{name}» не описана"))
                })?;
                let value = cell.borrow().clone();
                if matches!(value, Value::Undefined) {
                    return Err(RuntimeError::new(
                        expr.line,
                        format!("Нет значения у величины «{name}»"),
                    ));
                }
                Ok(value)
            }
            ExprKind::Index(base, subs) => {
                let base_value = self.eval(base)?;
                let subs = self.eval_subscripts(subs)?;
                self.index_value(base_value, &subs, expr.line)
            }
            ExprKind::Call(name, args) => self.call_function(name, args, expr.line),
            ExprKind::ArrayLit(items) => {
                let bounds = vec![(1i64, items.len() as i64)];
                let array = Array::new(bounds, &Value::Undefined)
                    .map_err(|e| RuntimeError::new(expr.line, e))?;
                for (cell, item) in array.data.iter().zip(items.iter()) {
                    let v = self.eval(item)?;
                    *cell.borrow_mut() = v;
                }
                Ok(Value::Array(array))
            }
            ExprKind::Unary(op, operand) => {
                let v = self.eval(operand)?;
                match op {
                    UnOp::Plus => Ok(v),
                    UnOp::Neg => match v {
                        Value::Int(i) => Ok(Value::Int(-i)),
                        Value::Real(r) => Ok(Value::Real(-r)),
                        other => Err(RuntimeError::new(
                            expr.line,
                            format!("Нельзя применить унарный минус к {}", other.type_name()),
                        )),
                    },
                    UnOp::Not => {
                        if let Value::Bool(b) = v {
                            Ok(Value::Bool(!b))
                        } else {
                            Err(RuntimeError::new(
                                expr.line,
                                format!(
                                    "«не» требует логическое значение, получено {}",
                                    v.type_name()
                                ),
                            ))
                        }
                    }
                }
            }
            ExprKind::Binary(op, left, right) => {
                // Short-circuit logical operators.
                if *op == BinOp::And {
                    let l = self.eval(left)?;
                    if !l.to_bool(expr.line)? {
                        return Ok(Value::Bool(false));
                    }
                    return Ok(Value::Bool(self.eval(right)?.to_bool(expr.line)?));
                }
                if *op == BinOp::Or {
                    let l = self.eval(left)?;
                    if l.to_bool(expr.line)? {
                        return Ok(Value::Bool(true));
                    }
                    return Ok(Value::Bool(self.eval(right)?.to_bool(expr.line)?));
                }
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                self.binary(*op, l, r, expr.line)
            }
        }
    }

    fn eval_subscripts(
        &mut self,
        subs: &[Subscript],
    ) -> Result<Vec<(i64, Option<i64>)>, RuntimeError> {
        let mut result = Vec::with_capacity(subs.len());
        for s in subs {
            let start = self.eval(&s.start)?.to_int(s.start.line)?;
            let end = match &s.end {
                Some(e) => Some(self.eval(e)?.to_int(e.line)?),
                None => None,
            };
            result.push((start, end));
        }
        Ok(result)
    }

    fn index_value(
        &mut self,
        base: Value,
        subs: &[(i64, Option<i64>)],
        line: u32,
    ) -> Result<Value, RuntimeError> {
        match base {
            Value::Array(a) => {
                let indices: Vec<i64> = subs.iter().map(|(i, _)| *i).collect();
                a.get(&indices).map_err(|e| RuntimeError::new(line, e))
            }
            Value::Str(s) => {
                if subs.len() != 1 {
                    return Err(RuntimeError::new(
                        line,
                        "Для строки допустим только один индекс",
                    ));
                }
                let chars: Vec<char> = s.chars().collect();
                let (start, end) = subs[0];
                if let Some(end) = end {
                    if start < 1 {
                        return Err(RuntimeError::new(line, "Индекс меньше 1"));
                    }
                    let end = end.min(chars.len() as i64);
                    if start > end + 1 {
                        return Err(RuntimeError::new(line, "Неверные границы фрагмента строки"));
                    }
                    if start > end {
                        return Ok(Value::Str(String::new()));
                    }
                    let slice: String = chars[(start - 1) as usize..end as usize].iter().collect();
                    Ok(Value::Str(slice))
                } else {
                    if start < 1 || start > chars.len() as i64 {
                        return Err(RuntimeError::new(
                            line,
                            format!("Индекс {start} вне границ строки (длина {})", chars.len()),
                        ));
                    }
                    Ok(Value::Char(chars[(start - 1) as usize]))
                }
            }
            other => Err(RuntimeError::new(
                line,
                format!(
                    "Нельзя применить индекс к значению типа «{}»",
                    other.type_name()
                ),
            )),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        left: Value,
        right: Value,
        line: u32,
    ) -> Result<Value, RuntimeError> {
        use BinOp::*;
        match op {
            Add => {
                if matches!(left, Value::Str(_) | Value::Char(_))
                    || matches!(right, Value::Str(_) | Value::Char(_))
                {
                    let a = builtins::format_value(&left, line)?;
                    let b = builtins::format_value(&right, line)?;
                    Ok(Value::Str(format!("{a}{b}")))
                } else {
                    match (left, right) {
                        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                        (a, b) => Ok(Value::Real(a.to_real(line)? + b.to_real(line)?)),
                    }
                }
            }
            Sub => match (left, right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                (a, b) => Ok(Value::Real(a.to_real(line)? - b.to_real(line)?)),
            },
            Mul => {
                if matches!(left, Value::Str(_)) && matches!(right, Value::Int(_)) {
                    // Not part of Kumir, but harmless convenience.
                    let s = left.to_string_value(line)?;
                    let n = right.to_int(line)?;
                    Ok(Value::Str(s.repeat(n.max(0) as usize)))
                } else {
                    match (left, right) {
                        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                        (a, b) => Ok(Value::Real(a.to_real(line)? * b.to_real(line)?)),
                    }
                }
            }
            Div => {
                let b = right.to_real(line)?;
                if b == 0.0 {
                    return Err(RuntimeError::new(line, "Деление на ноль"));
                }
                Ok(Value::Real(left.to_real(line)? / b))
            }
            Pow => {
                if let (Value::Int(a), Value::Int(b)) = (&left, &right) {
                    if *b >= 0 && *b <= 63 {
                        let mut result: i64 = 1;
                        for _ in 0..*b {
                            result = result.saturating_mul(*a);
                        }
                        return Ok(Value::Int(result));
                    }
                }
                Ok(Value::Real(left.to_real(line)?.powf(right.to_real(line)?)))
            }
            Eq => Ok(Value::Bool(values_equal(&left, &right, line)?)),
            Ne => Ok(Value::Bool(!values_equal(&left, &right, line)?)),
            Lt => Ok(Value::Bool(
                compare_values(&left, &right, line)? == std::cmp::Ordering::Less,
            )),
            Gt => Ok(Value::Bool(
                compare_values(&left, &right, line)? == std::cmp::Ordering::Greater,
            )),
            Le => Ok(Value::Bool(
                compare_values(&left, &right, line)? != std::cmp::Ordering::Greater,
            )),
            Ge => Ok(Value::Bool(
                compare_values(&left, &right, line)? != std::cmp::Ordering::Less,
            )),
            And | Or => unreachable!("short-circuited above"),
        }
    }

    // ------------------------------------------------------------------
    // L-values and assignment
    // ------------------------------------------------------------------

    fn assign_lvalue(&mut self, lv: &LValue, value: Value) -> Result<(), RuntimeError> {
        let cell = self.find_cell(&lv.name).ok_or_else(|| {
            RuntimeError::new(lv.line, format!("Величина «{}» не описана", lv.name))
        })?;
        if lv.subs.is_empty() {
            let ty = self
                .find_type(&lv.name)
                .unwrap_or_else(|| TypeRef::simple(BaseType::Int));
            let value = coerce(value, &ty, lv.line)?;
            *cell.borrow_mut() = value;
            return Ok(());
        }
        let subs = self.eval_subscripts(&lv.subs)?;
        let is_array = matches!(&*cell.borrow(), Value::Array(_));
        if is_array {
            let indices: Vec<i64> = subs.iter().map(|(i, _)| *i).collect();
            let element = {
                let borrowed = cell.borrow();
                if let Value::Array(a) = &*borrowed {
                    a.cell(&indices)
                        .map_err(|e| RuntimeError::new(lv.line, e))?
                } else {
                    unreachable!()
                }
            };
            let base_ty = self
                .find_type(&lv.name)
                .map(|t| TypeRef::simple(t.base))
                .unwrap_or_else(|| TypeRef::simple(BaseType::Int));
            let value = coerce(value, &base_ty, lv.line)?;
            *element.borrow_mut() = value;
            return Ok(());
        }
        // String element or slice.
        let current = {
            let borrowed = cell.borrow();
            match &*borrowed {
                Value::Str(s) => s.clone(),
                other => {
                    return Err(RuntimeError::new(
                        lv.line,
                        format!("Нельзя индексировать значение типа «{}»", other.type_name()),
                    ))
                }
            }
        };
        if subs.len() != 1 {
            return Err(RuntimeError::new(
                lv.line,
                "Для строки допустим только один индекс",
            ));
        }
        let (start, end) = subs[0];
        if start < 1 {
            return Err(RuntimeError::new(lv.line, "Индекс меньше 1"));
        }
        let chars: Vec<char> = current.chars().collect();
        let new_value = if let Some(end) = end {
            let end = end.min(chars.len() as i64);
            if start > end + 1 {
                return Err(RuntimeError::new(
                    lv.line,
                    "Неверные границы фрагмента строки",
                ));
            }
            let inserted = value.to_string_value(lv.line)?;
            let mut result: String = chars[..(start - 1) as usize].iter().collect();
            result.push_str(&inserted);
            if (end as usize) < chars.len() {
                result.extend(chars[end as usize..].iter());
            }
            Value::Str(result)
        } else {
            if start > chars.len() as i64 {
                return Err(RuntimeError::new(
                    lv.line,
                    format!("Индекс {start} вне границ строки (длина {})", chars.len()),
                ));
            }
            let ch = match value {
                Value::Char(c) => c,
                Value::Str(s) if s.chars().count() == 1 => s.chars().next().unwrap(),
                other => {
                    return Err(RuntimeError::new(
                        lv.line,
                        format!("Ожидался символ, получено {}", other.type_name()),
                    ))
                }
            };
            let mut result: String = chars[..(start - 1) as usize].iter().collect();
            result.push(ch);
            if (start as usize) < chars.len() {
                result.extend(chars[start as usize..].iter());
            }
            Value::Str(result)
        };
        *cell.borrow_mut() = new_value;
        Ok(())
    }

    /// Resolve an argument to a cell for `рез`/`аргрез` parameters.
    fn lvalue_cell(&mut self, lv: &LValue) -> Result<Cell, RuntimeError> {
        let cell = self.find_cell(&lv.name).ok_or_else(|| {
            RuntimeError::new(lv.line, format!("Величина «{}» не описана", lv.name))
        })?;
        if lv.subs.is_empty() {
            return Ok(cell);
        }
        let subs = self.eval_subscripts(&lv.subs)?;
        let indices: Vec<i64> = subs.iter().map(|(i, _)| *i).collect();
        let result = {
            let borrowed = cell.borrow();
            match &*borrowed {
                Value::Array(a) => a.cell(&indices).map_err(|e| RuntimeError::new(lv.line, e))?,
                other => {
                    return Err(RuntimeError::new(
                        lv.line,
                        format!(
                            "Параметр-результат должен быть переменной или элементом массива, получено «{}»",
                            other.type_name()
                        ),
                    ))
                }
            }
        };
        Ok(result)
    }

    fn lvalue_type(&self, lv: &LValue) -> Option<TypeRef> {
        let ty = self.find_type(&lv.name)?;
        if lv.subs.is_empty() {
            Some(ty)
        } else if ty.is_array {
            Some(TypeRef::simple(ty.base))
        } else if ty.base == BaseType::Str {
            if lv.subs.len() == 1 && lv.subs[0].end.is_none() {
                Some(TypeRef::simple(BaseType::Char))
            } else {
                Some(TypeRef::simple(BaseType::Str))
            }
        } else {
            Some(TypeRef::simple(ty.base))
        }
    }

    // ------------------------------------------------------------------
    // Function and builtin calls
    // ------------------------------------------------------------------

    fn call_function(
        &mut self,
        name: &str,
        args: &[Expr],
        line: u32,
    ) -> Result<Value, RuntimeError> {
        if let Some(idx) = self.program.algorithm_index(name) {
            let program: &'a Program = self.program;
            let alg = &program.algorithms[idx];
            return self.call_algorithm(alg, args, line);
        }
        self.call_builtin(name, args, line)
    }

    fn call_algorithm(
        &mut self,
        alg: &Algorithm,
        args: &[Expr],
        line: u32,
    ) -> Result<Value, RuntimeError> {
        if args.len() != alg.params.len() {
            return Err(RuntimeError::new(
                line,
                format!(
                    "Алгоритм «{}» ожидает {} аргумент(а/ов), передано {}",
                    alg.name,
                    alg.params.len(),
                    args.len()
                ),
            ));
        }
        if self.call_depth > 500 {
            return Err(RuntimeError::new(
                line,
                "Слишком большая глубина рекурсии (возможно, бесконечная рекурсия)",
            ));
        }
        let mut frame = Frame::default();
        if let Some(ret_ty) = &alg.ret {
            frame
                .vars
                .insert("знач".to_string(), new_cell(Value::Undefined));
            frame.types.insert("знач".to_string(), ret_ty.clone());
        }
        for (param, arg) in alg.params.iter().zip(args.iter()) {
            match param.access {
                Access::Regular | Access::In => {
                    let value = self.eval(arg)?;
                    let value = coerce(value, &param.typ, line)?;
                    frame.vars.insert(param.name.clone(), new_cell(value));
                }
                Access::Out | Access::InOut => {
                    let lv = expr_to_lvalue(arg).ok_or_else(|| {
                        RuntimeError::new(
                            line,
                            format!(
                                "Аргумент «{}» должен быть переменной (параметр {})",
                                param.name,
                                if param.access == Access::Out {
                                    "рез"
                                } else {
                                    "аргрез"
                                }
                            ),
                        )
                    })?;
                    let cell = self.lvalue_cell(&lv)?;
                    if param.access == Access::Out {
                        *cell.borrow_mut() = Value::Undefined;
                    }
                    frame.vars.insert(param.name.clone(), cell);
                }
            }
            frame.types.insert(param.name.clone(), param.typ.clone());
        }
        self.frames.push(frame);
        self.call_depth += 1;

        let result = (|| -> Result<Value, RuntimeError> {
            let flow = self.exec_block(&alg.pre, 0)?;
            if flow == Flow::Halt {
                return Ok(Value::Undefined);
            }
            let flow = self.exec_block(&alg.body, 0)?;
            if flow == Flow::Halt {
                return Ok(Value::Undefined);
            }
            let flow = self.exec_block(&alg.post, 0)?;
            if flow == Flow::Halt {
                return Ok(Value::Undefined);
            }
            let frame_ret = self.frames.last().and_then(|f| {
                if let Some(cell) = f.vars.get("знач") {
                    let v = cell.borrow().clone();
                    if matches!(v, Value::Undefined) {
                        f.ret.clone()
                    } else {
                        Some(v)
                    }
                } else {
                    f.ret.clone()
                }
            });
            match &alg.ret {
                Some(ty) => {
                    if ty.base == BaseType::Void {
                        Ok(Value::Undefined)
                    } else {
                        match frame_ret {
                            Some(v) => coerce(v, ty, line),
                            None => Err(RuntimeError::new(
                                line,
                                format!("Алгоритм «{}» не вернул значение (знач)", alg.name),
                            )),
                        }
                    }
                }
                None => Ok(Value::Undefined),
            }
        })();

        self.frames.pop();
        self.call_depth -= 1;
        result
    }

    fn call_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        line: u32,
    ) -> Result<Value, RuntimeError> {
        let n = normalize_name(name);
        // Random numbers need mutable state.
        match n.as_str() {
            "irand" => {
                let a = self.eval_arg_int(args, 0, line)?;
                let b = self.eval_arg_int(args, 1, line)?;
                return self.rng.irand(a, b, line).map(Value::Int);
            }
            "irnd" => {
                let x = self.eval_arg_int(args, 0, line)?;
                return self.rng.irand(1, x, line).map(Value::Int);
            }
            "rand" => {
                let a = self.eval_arg_real(args, 0, line)?;
                let b = self.eval_arg_real(args, 1, line)?;
                return self.rng.rrand(a, b, line).map(Value::Real);
            }
            "rnd" => {
                let x = self.eval_arg_real(args, 0, line)?;
                return self.rng.rrand(0.0, x, line).map(Value::Real);
            }
            _ => {}
        }

        // In-place string procedures and conversion with "рез" flag.
        match n.as_str() {
            "вставить" | "insert" => {
                if args.len() < 3 {
                    return Err(RuntimeError::new(line, "«вставить» требует три аргумента"));
                }
                let data = self.eval(&args[0])?.to_string_value(line)?;
                let lv = expr_to_lvalue(&args[1]).ok_or_else(|| {
                    RuntimeError::new(
                        line,
                        "Второй аргумент «вставить» должен быть строкой-переменной",
                    )
                })?;
                let pos = self.eval(&args[2])?.to_int(line)?;
                let cell = self.lvalue_cell(&lv)?;
                let mut s = cell.borrow().to_string_value(line)?;
                let chars: Vec<char> = s.chars().collect();
                if pos < 1 {
                    return Err(RuntimeError::new(line, "Индекс меньше 1"));
                }
                if pos as usize > chars.len() {
                    s.push_str(&data);
                } else {
                    let mut result: String = chars[..(pos - 1) as usize].iter().collect();
                    result.push_str(&data);
                    result.extend(chars[(pos - 1) as usize..].iter());
                    s = result;
                }
                *cell.borrow_mut() = Value::Str(s);
                return Ok(Value::Undefined);
            }
            "заменить" | "replace" => {
                if args.len() < 4 {
                    return Err(RuntimeError::new(
                        line,
                        "«заменить» требует четыре аргумента",
                    ));
                }
                let lv = expr_to_lvalue(&args[0]).ok_or_else(|| {
                    RuntimeError::new(
                        line,
                        "Первый аргумент «заменить» должен быть строкой-переменной",
                    )
                })?;
                let pattern = self.eval(&args[1])?.to_string_value(line)?;
                let data = self.eval(&args[2])?.to_string_value(line)?;
                let each = self.eval(&args[3])?.to_bool(line)?;
                let cell = self.lvalue_cell(&lv)?;
                let s = cell.borrow().to_string_value(line)?;
                let replaced = if each {
                    s.replace(&pattern, &data)
                } else {
                    s.replacen(&pattern, &data, 1)
                };
                *cell.borrow_mut() = Value::Str(replaced);
                return Ok(Value::Undefined);
            }
            "удалить" | "remove" => {
                if args.len() < 3 {
                    return Err(RuntimeError::new(line, "«удалить» требует три аргумента"));
                }
                let lv = expr_to_lvalue(&args[0]).ok_or_else(|| {
                    RuntimeError::new(
                        line,
                        "Первый аргумент «удалить» должен быть строкой-переменной",
                    )
                })?;
                let pos = self.eval(&args[1])?.to_int(line)?;
                let count = self.eval(&args[2])?.to_int(line)?;
                let cell = self.lvalue_cell(&lv)?;
                let s = cell.borrow().to_string_value(line)?;
                let chars: Vec<char> = s.chars().collect();
                if pos < 1 {
                    return Err(RuntimeError::new(line, "Индекс меньше 1"));
                }
                if count < 0 {
                    return Err(RuntimeError::new(
                        line,
                        "Количество удаляемых символов меньше 0",
                    ));
                }
                let start = (pos - 1) as usize;
                let end = (start + count as usize).min(chars.len());
                let mut result: String = chars[..start.min(chars.len())].iter().collect();
                if end < chars.len() {
                    result.extend(chars[end..].iter());
                }
                *cell.borrow_mut() = Value::Str(result);
                return Ok(Value::Undefined);
            }
            "лит_в_цел" | "string_to_int" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(line, "«лит_в_цел» требует два аргумента"));
                }
                let s = self.eval(&args[0])?.to_string_value(line)?;
                let parsed = s.trim().parse::<i64>();
                let ok = parsed.is_ok();
                let value = parsed.unwrap_or(0);
                let lv = expr_to_lvalue(&args[1]).ok_or_else(|| {
                    RuntimeError::new(line, "Второй аргумент «лит_в_цел» должен быть переменной")
                })?;
                self.assign_lvalue(&lv, Value::Bool(ok))?;
                return Ok(Value::Int(value));
            }
            "лит_в_вещ" | "string_to_real" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(line, "«лит_в_вещ» требует два аргумента"));
                }
                let s = self.eval(&args[0])?.to_string_value(line)?;
                let parsed = builtins::parse_real_str(&s);
                let ok = parsed.is_some();
                let value = parsed.unwrap_or(0.0);
                let lv = expr_to_lvalue(&args[1]).ok_or_else(|| {
                    RuntimeError::new(line, "Второй аргумент «лит_в_вещ» должен быть переменной")
                })?;
                self.assign_lvalue(&lv, Value::Bool(ok))?;
                return Ok(Value::Real(value));
            }
            "НАЗНАЧИТЬ ВВОД" | "ASSIGN INPUT" => {
                let path = self.eval_arg_string(args, 0, line)?;
                if path.trim().is_empty() {
                    self.assigned_in = None;
                } else {
                    self.assigned_in = Some(self.open_file(&path, FileMode::Read, line)?);
                }
                return Ok(Value::Undefined);
            }
            "НАЗНАЧИТЬ ВЫВОД" | "ASSIGN OUTPUT" => {
                let path = self.eval_arg_string(args, 0, line)?;
                if path.trim().is_empty() {
                    self.assigned_out = None;
                } else {
                    self.assigned_out = Some(self.open_file(&path, FileMode::Write, line)?);
                }
                return Ok(Value::Undefined);
            }
            _ => {}
        }

        if let Some(result) = self.call_actor_builtin(&n, args, line) {
            return result;
        }

        if builtins::is_file_builtin(&n) {
            return self.call_file_builtin(&n, args, line);
        }

        if matches!(name, "@поставить оценку" | "@Evaluate") {
            let value = self.eval_arg_int(args, 0, line)?;
            self.mark = Some(value);
            return Ok(Value::Undefined);
        }

        // Pure built-ins.
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.eval(a)?);
        }
        match builtins::call_pure(&n, &values, line) {
            Some(result) => result,
            None => Err(RuntimeError::new(
                line,
                format!("Алгоритм или функция «{name}» не найдена"),
            )),
        }
    }

    fn eval_arg_int(&mut self, args: &[Expr], i: usize, line: u32) -> Result<i64, RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        self.eval(e)?.to_int(line)
    }

    fn eval_arg_real(&mut self, args: &[Expr], i: usize, line: u32) -> Result<f64, RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        self.eval(e)?.to_real(line)
    }

    fn eval_arg_string(
        &mut self,
        args: &[Expr],
        i: usize,
        line: u32,
    ) -> Result<String, RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        self.eval(e)?.to_string_value(line)
    }

    // ------------------------------------------------------------------
    // Files
    // ------------------------------------------------------------------

    fn resolve_path(&self, name: &str) -> PathBuf {
        let p = Path::new(name);
        if p.is_absolute() {
            p.to_path_buf()
        } else if self.options.source_dir.as_os_str().is_empty() {
            p.to_path_buf()
        } else {
            self.options.source_dir.join(p)
        }
    }

    fn open_file(&mut self, name: &str, mode: FileMode, line: u32) -> Result<u64, RuntimeError> {
        let path = self.resolve_path(name);
        let (content, write_bom, pos) = match mode {
            FileMode::Read => {
                let bytes = std::fs::read(&path).map_err(|e| {
                    RuntimeError::new(
                        line,
                        format!("Невозможно открыть файл «{}»: {e}", path.display()),
                    )
                })?;
                let (text, bom) = decode_text(&bytes);
                (text, bom, 0)
            }
            FileMode::Write => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() && !parent.exists() {
                        return Err(RuntimeError::new(
                            line,
                            format!("Каталог не существует: {}", parent.display()),
                        ));
                    }
                }
                (String::new(), true, 0)
            }
            FileMode::Append => {
                if path.exists() {
                    let bytes = std::fs::read(&path).map_err(|e| {
                        RuntimeError::new(
                            line,
                            format!("Невозможно открыть файл «{}»: {e}", path.display()),
                        )
                    })?;
                    let (text, bom) = decode_text(&bytes);
                    let len = text.chars().count();
                    (text, bom, len)
                } else {
                    (String::new(), true, 0)
                }
            }
        };
        let id = self.next_file_id;
        self.next_file_id += 1;
        self.files.insert(
            id,
            OpenFile {
                path,
                mode,
                content,
                pos,
                write_bom,
                encoding: self.file_encoding,
                closed: false,
            },
        );
        Ok(id)
    }

    fn close_file(&mut self, id: u64, line: u32) -> Result<(), RuntimeError> {
        let file = self
            .files
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
        if file.closed {
            return Err(RuntimeError::new(line, "Файл уже закрыт"));
        }
        if matches!(file.mode, FileMode::Write | FileMode::Append) {
            let bytes = match file.encoding {
                EncodingKind::Utf8 => {
                    let mut text = String::new();
                    if file.write_bom {
                        text.push('\u{feff}');
                    }
                    text.push_str(&file.content);
                    text.into_bytes()
                }
                EncodingKind::Cp1251 => {
                    let (encoded, _, _) = encoding_rs::WINDOWS_1251.encode(&file.content);
                    encoded.into_owned()
                }
            };
            std::fs::write(&file.path, &bytes).map_err(|e| {
                RuntimeError::new(
                    line,
                    format!("Ошибка записи файла «{}»: {e}", file.path.display()),
                )
            })?;
        }
        file.closed = true;
        Ok(())
    }

    fn close_all_files(&mut self) {
        let ids: Vec<u64> = self.files.keys().copied().collect();
        for id in ids {
            let _ = self.close_file(id, 0);
        }
    }

    // ------------------------------------------------------------------
    // Actors: Robot, Turtle, Draw
    // ------------------------------------------------------------------

    fn call_actor_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        line: u32,
    ) -> Option<Result<Value, RuntimeError>> {
        let kind = {
            let actor = self.actor.lock().ok()?;
            actor.kind
        };
        let robot_cmd = matches!(
            name,
            "вверх"
                | "вниз"
                | "влево"
                | "вправо"
                | "закрасить"
                | "сверху стена"
                | "снизу стена"
                | "слева стена"
                | "справа стена"
                | "сверху свободно"
                | "снизу свободно"
                | "слева свободно"
                | "справа свободно"
                | "клетка закрашена"
                | "клетка чистая"
                | "радиация"
                | "температура"
                | "@@размер поля"
                | "@@метка"
                | "@@закрашена"
                | "@@робот"
                | "@@верхняя буква"
                | "@@нижняя буква"
                | "@@температура"
                | "@@радиация"
                | "go up"
                | "go down"
                | "go left"
                | "go right"
                | "do paint"
                | "is wall at top"
                | "is wall at bottom"
                | "is wall at left"
                | "is wall at right"
                | "is free at top"
                | "is free at bottom"
                | "is free at left"
                | "is free at right"
                | "is Color"
                | "is Clear"
                | "radiation"
                | "temperature"
                | "field size"
                | "mark"
                | "colored"
                | "robot pos"
                | "upChar"
                | "downChar"
                | "cellTemp"
                | "cellRad"
        );
        let turtle_cmd = matches!(
            name,
            "поднять хвост"
                | "опустить хвост"
                | "вперед"
                | "назад"
                | "влево"
                | "вправо"
                | "выбрать чернила"
                | "tailUp"
                | "tailDown"
                | "forward"
                | "back"
                | "left"
                | "right"
                | "set pen color"
        );
        let draw_cmd = matches!(
            name,
            "опустить перо"
                | "поднять перо"
                | "выбрать чернила"
                | "сместиться в точку"
                | "сместиться на вектор"
                | "написать"
                | "@есть отрезок в области"
                | "setup pen"
                | "release pen"
                | "set pen color"
                | "move to"
                | "move by"
                | "add caption"
                | "@is line at circle"
        );
        if !robot_cmd && !turtle_cmd && !draw_cmd {
            return None;
        }
        if kind == ActorKind::None {
            return Some(Err(RuntimeError::new(
                line,
                format!(
                    "Команда «{name}» недоступна: не подключён исполнитель (используйте «использовать Робот», «использовать Черепаха» или «использовать Чертежник»)"
                ),
            )));
        }

        // Evaluate all ordinary arguments before locking the actor state, so
        // the interpreter can still call user algorithms from an argument.
        let mut d1 = 0.0f64;
        let mut d2 = 0.0f64;
        let mut color = String::new();
        let mut cell_args = (0i32, 0i32);
        let mut text = String::new();
        let numeric1 = kind == ActorKind::Turtle
            && matches!(
                name,
                "вперед"
                    | "назад"
                    | "влево"
                    | "вправо"
                    | "forward"
                    | "back"
                    | "left"
                    | "right"
            );
        let numeric2 = kind == ActorKind::Draw
            && matches!(
                name,
                "сместиться в точку" | "сместиться на вектор" | "move to" | "move by"
            );
        let color_cmd = matches!(kind, ActorKind::Turtle | ActorKind::Draw)
            && matches!(name, "выбрать чернила" | "set pen color");
        let cell_cmd = kind == ActorKind::Robot
            && matches!(
                name,
                "@@метка"
                    | "@@закрашена"
                    | "@@верхняя буква"
                    | "@@нижняя буква"
                    | "@@температура"
                    | "@@радиация"
                    | "mark"
                    | "colored"
                    | "upChar"
                    | "downChar"
                    | "cellTemp"
                    | "cellRad"
            );
        let caption_cmd = kind == ActorKind::Draw && matches!(name, "написать" | "add caption");
        if numeric1 {
            d1 = match self.eval_arg_real(args, 0, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
        } else if numeric2 {
            d1 = match self.eval_arg_real(args, 0, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            d2 = match self.eval_arg_real(args, 1, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
        } else if color_cmd {
            color = match self.eval_color(args, 0, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
        } else if cell_cmd {
            cell_args = match self.eval_cell_args(args, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
        } else if caption_cmd {
            text = match self.eval_arg_string(args, 1, line) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
        }

        let result = (|| -> Result<Value, RuntimeError> {
            let mut actor = self
                .actor
                .lock()
                .map_err(|_| RuntimeError::new(line, "Ошибка доступа к исполнителю"))?;
            actor.status = name.to_string();
            match kind {
                ActorKind::Robot => {
                    if !robot_cmd {
                        return Err(RuntimeError::new(
                            line,
                            format!("Команда «{name}» не поддерживается исполнителем Робот"),
                        ));
                    }
                    match name {
                        "вверх" | "go up" => actor
                            .robot
                            .move_up()
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "вниз" | "go down" => actor
                            .robot
                            .move_down()
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "влево" | "go left" => actor
                            .robot
                            .move_left()
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "вправо" | "go right" => actor
                            .robot
                            .move_right()
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "закрасить" | "do paint" => {
                            actor.robot.paint();
                            Ok(Value::Undefined)
                        }
                        "сверху стена" | "is wall at top" => {
                            Ok(Value::Bool(actor.robot.wall_up()))
                        }
                        "снизу стена" | "is wall at bottom" => {
                            Ok(Value::Bool(actor.robot.wall_down()))
                        }
                        "слева стена" | "is wall at left" => {
                            Ok(Value::Bool(actor.robot.wall_left()))
                        }
                        "справа стена" | "is wall at right" => {
                            Ok(Value::Bool(actor.robot.wall_right()))
                        }
                        "сверху свободно" | "is free at top" => {
                            Ok(Value::Bool(!actor.robot.wall_up()))
                        }
                        "снизу свободно" | "is free at bottom" => {
                            Ok(Value::Bool(!actor.robot.wall_down()))
                        }
                        "слева свободно" | "is free at left" => {
                            Ok(Value::Bool(!actor.robot.wall_left()))
                        }
                        "справа свободно" | "is free at right" => {
                            Ok(Value::Bool(!actor.robot.wall_right()))
                        }
                        "клетка закрашена" | "is Color" => {
                            Ok(Value::Bool(actor.robot.current().color))
                        }
                        "клетка чистая" | "is Clear" => {
                            Ok(Value::Bool(!actor.robot.current().color))
                        }
                        "радиация" | "radiation" => {
                            Ok(Value::Real(actor.robot.current().radiation))
                        }
                        "температура" | "temperature" => {
                            Ok(Value::Int(actor.robot.current().temperature))
                        }
                        "@@размер поля" | "field size" => {
                            let rows = actor.robot.height;
                            let cols = actor.robot.width;
                            drop(actor);
                            self.assign_out(args, 0, Value::Int(rows as i64), line)?;
                            self.assign_out(args, 1, Value::Int(cols as i64), line)?;
                            Ok(Value::Undefined)
                        }
                        "@@робот" | "robot pos" => {
                            let row = actor.robot.robot_y + 1;
                            let col = actor.robot.robot_x + 1;
                            drop(actor);
                            self.assign_out(args, 0, Value::Int(row as i64), line)?;
                            self.assign_out(args, 1, Value::Int(col as i64), line)?;
                            Ok(Value::Undefined)
                        }
                        "@@метка" | "mark" => {
                            let (y, x) = cell_args;
                            Ok(Value::Bool(
                                actor.robot.cell(x, y).map(|c| c.point).unwrap_or(false),
                            ))
                        }
                        "@@закрашена" | "colored" => {
                            let (y, x) = cell_args;
                            Ok(Value::Bool(
                                actor.robot.cell(x, y).map(|c| c.color).unwrap_or(false),
                            ))
                        }
                        "@@верхняя буква" | "upChar" => {
                            let (y, x) = cell_args;
                            Ok(Value::Char(
                                actor.robot.cell(x, y).map(|c| c.symbol).unwrap_or('$'),
                            ))
                        }
                        "@@нижняя буква" | "downChar" => {
                            let (y, x) = cell_args;
                            Ok(Value::Char(
                                actor.robot.cell(x, y).map(|c| c.symbol1).unwrap_or('$'),
                            ))
                        }
                        "@@температура" | "cellTemp" => {
                            let (y, x) = cell_args;
                            Ok(Value::Int(
                                actor.robot.cell(x, y).map(|c| c.temperature).unwrap_or(0),
                            ))
                        }
                        "@@радиация" | "cellRad" => {
                            let (y, x) = cell_args;
                            Ok(Value::Real(
                                actor.robot.cell(x, y).map(|c| c.radiation).unwrap_or(0.0),
                            ))
                        }
                        _ => Ok(Value::Undefined),
                    }
                }
                ActorKind::Turtle => {
                    if !turtle_cmd {
                        return Err(RuntimeError::new(
                            line,
                            format!("Команда «{name}» не поддерживается исполнителем Черепаха"),
                        ));
                    }
                    match name {
                        "поднять хвост" | "tailUp" => {
                            actor.turtle.pen_down = false;
                            Ok(Value::Undefined)
                        }
                        "опустить хвост" | "tailDown" => {
                            actor.turtle.pen_down = true;
                            Ok(Value::Undefined)
                        }
                        "вперед" | "forward" => actor
                            .turtle
                            .forward(d1)
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "назад" | "back" => actor
                            .turtle
                            .back(d1)
                            .map_err(|e| RuntimeError::new(line, e))
                            .map(|_| Value::Undefined),
                        "влево" | "left" => {
                            actor.turtle.turn_left(d1);
                            Ok(Value::Undefined)
                        }
                        "вправо" | "right" => {
                            actor.turtle.turn_right(d1);
                            Ok(Value::Undefined)
                        }
                        "выбрать чернила" | "set pen color" => {
                            actor.turtle.color = color;
                            Ok(Value::Undefined)
                        }
                        _ => Ok(Value::Undefined),
                    }
                }
                ActorKind::Draw => {
                    if !draw_cmd {
                        return Err(RuntimeError::new(
                            line,
                            format!("Команда «{name}» не поддерживается исполнителем Чертежник"),
                        ));
                    }
                    match name {
                        "опустить перо" | "setup pen" => {
                            actor.draw.pen_down = true;
                            Ok(Value::Undefined)
                        }
                        "поднять перо" | "release pen" => {
                            actor.draw.pen_down = false;
                            Ok(Value::Undefined)
                        }
                        "выбрать чернила" | "set pen color" => {
                            actor.draw.color = color;
                            Ok(Value::Undefined)
                        }
                        "сместиться в точку" | "move to" => {
                            actor.draw.move_to(d1, d2);
                            Ok(Value::Undefined)
                        }
                        "сместиться на вектор" | "move by" => {
                            actor.draw.move_by(d1, d2);
                            Ok(Value::Undefined)
                        }
                        "написать" | "add caption" => {
                            let x = actor.draw.x;
                            let y = actor.draw.y;
                            actor
                                .draw
                                .captions
                                .push(crate::actors::Caption { x, y, text });
                            Ok(Value::Undefined)
                        }
                        "@есть отрезок в области" | "@is line at circle" => {
                            Ok(Value::Bool(false))
                        }
                        _ => Ok(Value::Undefined),
                    }
                }
                ActorKind::None => Ok(Value::Undefined),
            }
        })();
        let visual = matches!(
            name,
            "вверх"
                | "вниз"
                | "влево"
                | "вправо"
                | "закрасить"
                | "go up"
                | "go down"
                | "go left"
                | "go right"
                | "do paint"
                | "вперед"
                | "назад"
                | "forward"
                | "back"
                | "left"
                | "right"
                | "опустить перо"
                | "поднять перо"
                | "setup pen"
                | "release pen"
                | "сместиться в точку"
                | "сместиться на вектор"
                | "move to"
                | "move by"
        );
        if visual && !self.options.actor_delay.is_zero() {
            std::thread::sleep(self.options.actor_delay);
        }
        Some(result)
    }

    fn eval_cell_args(&mut self, args: &[Expr], line: u32) -> Result<(i32, i32), RuntimeError> {
        // Public @@-functions use 1-based row/column numbers.
        let row = self.eval_arg_int(args, 0, line)? as i32;
        let col = self.eval_arg_int(args, 1, line)? as i32;
        Ok((row - 1, col - 1))
    }

    fn eval_color(&mut self, args: &[Expr], i: usize, line: u32) -> Result<String, RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        match self.eval(e)? {
            Value::Str(s) => Ok(s),
            Value::Char(c) => Ok(c.to_string()),
            Value::Int(v) => Ok(format!("#{v:06x}")),
            other => Err(RuntimeError::new(
                line,
                format!("Ожидался цвет, получено {}", other.type_name()),
            )),
        }
    }

    fn assign_out(
        &mut self,
        args: &[Expr],
        i: usize,
        value: Value,
        line: u32,
    ) -> Result<(), RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        let lv = expr_to_lvalue(e)
            .ok_or_else(|| RuntimeError::new(line, "Аргумент-результат должен быть переменной"))?;
        self.assign_lvalue(&lv, value)
    }

    fn call_file_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        line: u32,
    ) -> Result<Value, RuntimeError> {
        match name {
            "открыть на чтение" | "read open" => {
                let p = self.eval_arg_string(args, 0, line)?;
                Ok(Value::File(self.open_file(&p, FileMode::Read, line)?))
            }
            "открыть на запись" | "write open" => {
                let p = self.eval_arg_string(args, 0, line)?;
                Ok(Value::File(self.open_file(&p, FileMode::Write, line)?))
            }
            "открыть на добавление" | "append open" => {
                let p = self.eval_arg_string(args, 0, line)?;
                Ok(Value::File(self.open_file(&p, FileMode::Append, line)?))
            }
            "закрыть" | "close" => {
                let id = self.eval_arg_file(args, 0, line)?;
                self.close_file(id, line)?;
                Ok(Value::Undefined)
            }
            "начать чтение" | "start reading" => {
                let id = self.eval_arg_file(args, 0, line)?;
                let f = self
                    .files
                    .get_mut(&id)
                    .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
                f.pos = 0;
                Ok(Value::Undefined)
            }
            "конец файла" | "eof" => {
                let id = self.eval_arg_file(args, 0, line)?;
                if id == 0 {
                    return Ok(Value::Bool(false));
                }
                let f = self
                    .files
                    .get(&id)
                    .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
                // Treat trailing whitespace as end of file, like the original
                // VM does with its skip-delimiters logic.
                let rest: String = f.content.chars().skip(f.pos).collect();
                Ok(Value::Bool(rest.trim().is_empty()))
            }
            "есть данные" | "nonempty" => {
                let id = self.eval_arg_file(args, 0, line)?;
                let f = self
                    .files
                    .get(&id)
                    .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
                let rest: String = f.content.chars().skip(f.pos).collect();
                Ok(Value::Bool(!rest.trim().is_empty()))
            }
            "установить кодировку" | "set encoding" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let name = arg.to_lowercase();
                self.file_encoding = if name.contains("1251") || name.contains("cp-1251") {
                    EncodingKind::Cp1251
                } else if name.contains("utf") {
                    EncodingKind::Utf8
                } else {
                    self.file_encoding
                };
                Ok(Value::Undefined)
            }
            "можно открыть на чтение" | "readable" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(p.is_file()))
            }
            "можно открыть на запись" | "writable" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                let ok = if p.exists() {
                    !p.is_dir()
                } else {
                    p.parent().map(|d| d.exists()).unwrap_or(false)
                };
                Ok(Value::Bool(ok))
            }
            "существует" | "exists" | "существует файл" | "file exists" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(p.exists()))
            }
            "является каталогом" | "isdir" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(p.is_dir()))
            }
            "создать файл" | "touch" | "create file" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                match std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(false)
                    .open(&p)
                {
                    Ok(_) => Ok(Value::Bool(true)),
                    Err(_) => Ok(Value::Bool(false)),
                }
            }
            "создать каталог" | "mkdir" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(std::fs::create_dir_all(&p).is_ok()))
            }
            "полный путь" | "abspath" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Str(
                    p.canonicalize().unwrap_or(p).to_string_lossy().to_string(),
                ))
            }
            "удалить_файл" | "rm" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(std::fs::remove_file(&p).is_ok()))
            }
            "удалить_каталог" | "rmdir" => {
                let arg = self.eval_arg_string(args, 0, line)?;
                let p = self.resolve_path(&arg);
                Ok(Value::Bool(std::fs::remove_dir(&p).is_ok()))
            }
            "РАБОЧИЙ КАТАЛОГ" | "CURRENT DIR" | "КАТАЛОГ ПРОГРАММЫ" | "PROGRAM DIR" =>
            {
                let dir = if self.options.source_dir.as_os_str().is_empty() {
                    std::env::current_dir().unwrap_or_default()
                } else {
                    self.options.source_dir.clone()
                };
                Ok(Value::Str(dir.to_string_lossy().to_string()))
            }
            "консоль" | "get console" => Ok(Value::File(0)),
            _ => Err(RuntimeError::new(
                line,
                format!("Функция работы с файлами «{name}» не реализована"),
            )),
        }
    }

    fn eval_arg_file(&mut self, args: &[Expr], i: usize, line: u32) -> Result<u64, RuntimeError> {
        let e = args
            .get(i)
            .ok_or_else(|| RuntimeError::new(line, "Недостаточно аргументов"))?;
        match self.eval(e)? {
            Value::File(id) => Ok(id),
            other => Err(RuntimeError::new(
                line,
                format!("Ожидался файл, получено {}", other.type_name()),
            )),
        }
    }

    // ------------------------------------------------------------------
    // Console / file I/O
    // ------------------------------------------------------------------

    fn exec_output(
        &mut self,
        _file: Option<&Expr>,
        args: &[OutputArg],
        line: u32,
    ) -> Result<(), RuntimeError> {
        let mut target = self.assigned_out;
        let mut start = 0usize;
        let mut first_value = None;
        if !args.is_empty() {
            let first = self.eval(&args[0].expr)?;
            if args.len() > 1 {
                if let Value::File(id) = first {
                    target = Some(id);
                    start = 1;
                } else {
                    first_value = Some(first);
                }
            } else {
                first_value = Some(first);
            }
        }
        for (index, arg) in args.iter().enumerate().skip(start) {
            let value = if index == 0 {
                first_value.take().unwrap_or(Value::Undefined)
            } else {
                self.eval(&arg.expr)?
            };
            let width = match &arg.width {
                Some(e) => Some(self.eval(e)?.to_int(line)?),
                None => None,
            };
            let decimals = match &arg.decimals {
                Some(e) => Some(self.eval(e)?.to_int(line)?),
                None => None,
            };
            let text = match &value {
                Value::Real(v) => builtins::format_real(*v, width, decimals),
                Value::Int(v) => builtins::format_int(*v, width),
                _ => builtins::format_value(&value, line)?,
            };
            self.write_to(target, &text, line)?;
        }
        Ok(())
    }

    fn write_to(&mut self, target: Option<u64>, text: &str, line: u32) -> Result<(), RuntimeError> {
        match target {
            None | Some(0) => {
                self.host.write(text);
                Ok(())
            }
            Some(id) => {
                let f = self
                    .files
                    .get_mut(&id)
                    .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
                if f.closed {
                    return Err(RuntimeError::new(line, "Файл уже закрыт"));
                }
                if !matches!(f.mode, FileMode::Write | FileMode::Append) {
                    return Err(RuntimeError::new(line, "Файл открыт только для чтения"));
                }
                f.content.push_str(text);
                f.pos = f.content.chars().count();
                Ok(())
            }
        }
    }

    fn exec_input(
        &mut self,
        _file: Option<&Expr>,
        targets: &[LValue],
        line: u32,
    ) -> Result<(), RuntimeError> {
        let mut file_id: Option<u64> = None;
        let mut start = 0usize;
        if targets.len() > 1 {
            let first = &targets[0];
            if first.subs.is_empty() {
                if let Some(cell) = self.find_cell(&first.name) {
                    let v = cell.borrow().clone();
                    if let Value::File(id) = v {
                        file_id = Some(id);
                        start = 1;
                    }
                }
            }
        }
        for target in &targets[start..] {
            let ty = self.lvalue_type(target).ok_or_else(|| {
                RuntimeError::new(
                    target.line,
                    format!("Величина «{}» не описана", target.name),
                )
            })?;
            let value = self.read_typed(&ty.base, file_id, target.line)?;
            self.assign_lvalue(target, value)?;
        }
        let _ = line;
        Ok(())
    }

    fn read_typed(
        &mut self,
        base: &BaseType,
        file_id: Option<u64>,
        line: u32,
    ) -> Result<Value, RuntimeError> {
        match base {
            BaseType::Int => {
                let token = self.read_token(file_id, line)?;
                token
                    .trim()
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| RuntimeError::new(line, "Ошибка ввода целого числа"))
            }
            BaseType::Real => {
                let token = self.read_token(file_id, line)?;
                builtins::parse_real_str(&token)
                    .map(Value::Real)
                    .ok_or_else(|| RuntimeError::new(line, "Ошибка ввода вещественного числа"))
            }
            BaseType::Bool => {
                let token = self.read_token(file_id, line)?;
                builtins::parse_bool_str(&token)
                    .map(Value::Bool)
                    .ok_or_else(|| RuntimeError::new(line, "Ошибка ввода логического значения"))
            }
            BaseType::Char => self.read_char(file_id, line).map(Value::Char),
            BaseType::Str => self.read_line_value(file_id, line).map(Value::Str),
            BaseType::File => Err(RuntimeError::new(
                line,
                "Нельзя вводить значение типа «файл»",
            )),
            BaseType::User(_) => self.read_line_value(file_id, line).map(Value::Str),
            BaseType::Void => Err(RuntimeError::new(line, "Нельзя вводить значение")),
        }
    }

    fn read_token(&mut self, file_id: Option<u64>, line: u32) -> Result<String, RuntimeError> {
        match file_id {
            Some(id) if id != 0 => self.file_read_token(id, line),
            _ => self.console_read_token(line),
        }
    }

    fn read_line_value(&mut self, file_id: Option<u64>, line: u32) -> Result<String, RuntimeError> {
        match file_id {
            Some(id) if id != 0 => self.file_read_line(id, line),
            _ => self.console_read_line_value(line),
        }
    }

    fn read_char(&mut self, file_id: Option<u64>, line: u32) -> Result<char, RuntimeError> {
        match file_id {
            Some(id) if id != 0 => self.file_read_char(id, line),
            _ => self.console_read_char(line),
        }
    }

    // Console buffer -----------------------------------------------------

    fn console_read_token(&mut self, line: u32) -> Result<String, RuntimeError> {
        loop {
            while self.console_pos < self.console_buf.len()
                && self.console_buf[self.console_pos].is_whitespace()
            {
                self.console_pos += 1;
            }
            if self.console_pos < self.console_buf.len() {
                let start = self.console_pos;
                while self.console_pos < self.console_buf.len()
                    && !self.console_buf[self.console_pos].is_whitespace()
                {
                    self.console_pos += 1;
                }
                return Ok(self.console_buf[start..self.console_pos].iter().collect());
            }
            if let Some(id) = self.assigned_in {
                if id != 0 {
                    let text = self.file_read_line(id, line)?;
                    if text.is_empty() {
                        let f = self.files.get(&id).unwrap();
                        if f.pos >= f.content.chars().count() {
                            return Err(RuntimeError::new(line, "Ошибка ввода: текст закончился"));
                        }
                    }
                    self.console_buf = text.chars().collect();
                    self.console_pos = 0;
                    if self.console_buf.is_empty() {
                        continue;
                    }
                    continue;
                }
            }
            let text = self
                .host
                .read_line()
                .map_err(|e| RuntimeError::new(line, e))?;
            self.console_buf = text.chars().collect();
            self.console_pos = 0;
            if self.console_buf.is_empty() {
                // An empty line does not contain a token; ask for another one.
                continue;
            }
        }
    }

    fn console_read_line_value(&mut self, line: u32) -> Result<String, RuntimeError> {
        if self.console_pos < self.console_buf.len() {
            let mut start = self.console_pos;
            while start < self.console_buf.len() && self.console_buf[start].is_whitespace() {
                start += 1;
            }
            let rest: String = self.console_buf[start..].iter().collect();
            self.console_pos = self.console_buf.len();
            return Ok(rest);
        }
        if let Some(id) = self.assigned_in {
            if id != 0 {
                return self.file_read_line(id, line);
            }
        }
        self.host
            .read_line()
            .map_err(|e| RuntimeError::new(line, e))
    }

    fn console_read_char(&mut self, line: u32) -> Result<char, RuntimeError> {
        while self.console_pos >= self.console_buf.len() {
            if let Some(id) = self.assigned_in {
                if id != 0 {
                    let text = self.file_read_line(id, line)?;
                    if text.is_empty() {
                        let f = self.files.get(&id).unwrap();
                        if f.pos >= f.content.chars().count() {
                            return Err(RuntimeError::new(
                                line,
                                "Ошибка ввода символа: текст закончился",
                            ));
                        }
                        return Ok('\n');
                    }
                    self.console_buf = text.chars().collect();
                    self.console_pos = 0;
                    continue;
                }
            }
            let text = self
                .host
                .read_line()
                .map_err(|e| RuntimeError::new(line, e))?;
            if text.is_empty() {
                return Ok('\n');
            }
            self.console_buf = text.chars().collect();
            self.console_pos = 0;
        }
        let c = self.console_buf[self.console_pos];
        self.console_pos += 1;
        Ok(c)
    }

    // File buffer --------------------------------------------------------

    fn file_read_token(&mut self, id: u64, line: u32) -> Result<String, RuntimeError> {
        let f = self
            .files
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
        if f.closed {
            return Err(RuntimeError::new(line, "Файл уже закрыт"));
        }
        let chars: Vec<char> = f.content.chars().collect();
        let mut pos = f.pos;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() {
            f.pos = pos;
            return Err(RuntimeError::new(line, "Ошибка ввода: текст закончился"));
        }
        let start = pos;
        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }
        f.pos = pos;
        Ok(chars[start..pos].iter().collect())
    }

    fn file_read_line(&mut self, id: u64, line: u32) -> Result<String, RuntimeError> {
        let f = self
            .files
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
        if f.closed {
            return Err(RuntimeError::new(line, "Файл уже закрыт"));
        }
        let chars: Vec<char> = f.content.chars().collect();
        let mut pos = f.pos;
        let start = pos;
        while pos < chars.len() && chars[pos] != '\n' {
            pos += 1;
        }
        let mut result: String = chars[start..pos].iter().collect();
        if result.ends_with('\r') {
            result.pop();
        }
        if pos < chars.len() {
            pos += 1;
        }
        f.pos = pos;
        Ok(result)
    }

    fn file_read_char(&mut self, id: u64, line: u32) -> Result<char, RuntimeError> {
        let f = self
            .files
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::new(line, "Неверный ключ файла"))?;
        if f.closed {
            return Err(RuntimeError::new(line, "Файл уже закрыт"));
        }
        let chars: Vec<char> = f.content.chars().collect();
        if f.pos >= chars.len() {
            return Err(RuntimeError::new(
                line,
                "Ошибка ввода символа: текст закончился",
            ));
        }
        let c = chars[f.pos];
        f.pos += 1;
        Ok(c)
    }
}

fn decode_text(bytes: &[u8]) -> (String, bool) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
            return (s.to_string(), true);
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        let (s, bom) = if let Some(stripped) = s.strip_prefix('\u{feff}') {
            (stripped.to_string(), true)
        } else {
            (s.to_string(), false)
        };
        return (s, bom);
    }
    // Fall back to the Windows-1251 encoding used by old Kumir files.
    let (s, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
    (s.to_string(), false)
}

fn expr_to_lvalue(expr: &Expr) -> Option<LValue> {
    match &expr.kind {
        ExprKind::Var(name) => Some(LValue {
            name: name.clone(),
            subs: Vec::new(),
            line: expr.line,
        }),
        ExprKind::Index(base, subs) => match &base.kind {
            ExprKind::Var(name) => Some(LValue {
                name: name.clone(),
                subs: subs.clone(),
                line: expr.line,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Record type helper for modules which expose records (Files).
pub fn record_type(fields: &[&str], value: &Value) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for f in fields {
        map.insert((*f).to_string(), value.clone());
    }
    map
}
