//! Runtime values, arrays and type coercion.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ast::{BaseType, TypeRef};
use crate::diag::RuntimeError;

pub type Cell = Rc<RefCell<Value>>;

pub fn new_cell(value: Value) -> Cell {
    Rc::new(RefCell::new(value))
}

#[derive(Debug)]
pub enum Value {
    Undefined,
    Int(i64),
    Real(f64),
    Bool(bool),
    Char(char),
    Str(String),
    /// Handle in the interpreter's file table (0 is the console).
    File(u64),
    Array(Array),
    Record(BTreeMap<String, Value>),
}

impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Value::Undefined => Value::Undefined,
            Value::Int(v) => Value::Int(*v),
            Value::Real(v) => Value::Real(*v),
            Value::Bool(v) => Value::Bool(*v),
            Value::Char(v) => Value::Char(*v),
            Value::Str(v) => Value::Str(v.clone()),
            Value::File(v) => Value::File(*v),
            Value::Array(a) => Value::Array(a.deep_clone()),
            Value::Record(r) => {
                Value::Record(r.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            }
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Undefined => "не определено",
            Value::Int(_) => "цел",
            Value::Real(_) => "вещ",
            Value::Bool(_) => "лог",
            Value::Char(_) => "сим",
            Value::Str(_) => "лит",
            Value::File(_) => "файл",
            Value::Array(_) => "таблица",
            Value::Record(_) => "запись",
        }
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Real(_))
    }

    /// Convert to an integer for indexing and integer arithmetic.
    pub fn to_int(&self, line: u32) -> Result<i64, RuntimeError> {
        match self {
            Value::Int(v) => Ok(*v),
            Value::Real(v) => Ok(v.floor() as i64),
            Value::Bool(v) => Ok(if *v { 1 } else { 0 }),
            Value::Char(c) => Ok(*c as i64),
            other => Err(RuntimeError::new(
                line,
                format!("Ожидалось целое значение, получено {}", other.type_name()),
            )),
        }
    }

    pub fn to_real(&self, line: u32) -> Result<f64, RuntimeError> {
        match self {
            Value::Int(v) => Ok(*v as f64),
            Value::Real(v) => Ok(*v),
            other => Err(RuntimeError::new(
                line,
                format!(
                    "Ожидалось вещественное значение, получено {}",
                    other.type_name()
                ),
            )),
        }
    }

    pub fn to_bool(&self, line: u32) -> Result<bool, RuntimeError> {
        match self {
            Value::Bool(v) => Ok(*v),
            other => Err(RuntimeError::new(
                line,
                format!(
                    "Ожидалось логическое значение, получено {}",
                    other.type_name()
                ),
            )),
        }
    }

    pub fn to_string_value(&self, line: u32) -> Result<String, RuntimeError> {
        match self {
            Value::Str(s) => Ok(s.clone()),
            Value::Char(c) => Ok(c.to_string()),
            other => Err(RuntimeError::new(
                line,
                format!("Ожидалась строка, получено {}", other.type_name()),
            )),
        }
    }
}

#[derive(Debug)]
pub struct Array {
    /// Inclusive lower and upper bounds for every dimension.
    pub bounds: Vec<(i64, i64)>,
    /// Number of elements along every dimension.
    pub dims: Vec<usize>,
    pub data: Vec<Cell>,
}

impl Clone for Array {
    fn clone(&self) -> Self {
        self.deep_clone()
    }
}

impl Array {
    pub fn new(bounds: Vec<(i64, i64)>, default: &Value) -> Result<Self, String> {
        let mut dims = Vec::with_capacity(bounds.len());
        let mut total: usize = 1;
        for (lo, hi) in &bounds {
            if hi < lo {
                return Err(format!(
                    "Неверные границы массива: [{lo}:{hi}] (нижняя граница больше верхней)"
                ));
            }
            let len = (*hi - *lo + 1) as usize;
            total = total
                .checked_mul(len)
                .ok_or_else(|| "Слишком большой массив".to_string())?;
            dims.push(len);
        }
        if total > 50_000_000 {
            return Err("Слишком большой массив (более 50 млн элементов)".to_string());
        }
        let data = (0..total).map(|_| new_cell(default.clone())).collect();
        Ok(Self { bounds, dims, data })
    }

    pub fn deep_clone(&self) -> Self {
        Self {
            bounds: self.bounds.clone(),
            dims: self.dims.clone(),
            data: self
                .data
                .iter()
                .map(|c| new_cell(c.borrow().clone()))
                .collect(),
        }
    }

    pub fn flat_index(&self, indices: &[i64]) -> Result<usize, String> {
        if indices.len() != self.bounds.len() {
            return Err(format!(
                "Массив имеет {} измерение(й), указано индексов: {}",
                self.bounds.len(),
                indices.len()
            ));
        }
        let mut flat = 0usize;
        for (i, &idx) in indices.iter().enumerate() {
            let (lo, hi) = self.bounds[i];
            if idx < lo || idx > hi {
                return Err(format!("Индекс {idx} вне границ [{lo}:{hi}]"));
            }
            flat = flat * self.dims[i] + (idx - lo) as usize;
        }
        Ok(flat)
    }

    pub fn cell(&self, indices: &[i64]) -> Result<Cell, String> {
        let idx = self.flat_index(indices)?;
        Ok(self.data[idx].clone())
    }

    pub fn get(&self, indices: &[i64]) -> Result<Value, String> {
        Ok(self.cell(indices)?.borrow().clone())
    }

    pub fn element_count(&self) -> usize {
        self.data.len()
    }
}

/// Coerce a value to the declared type of a variable/parameter.
pub fn coerce(value: Value, ty: &TypeRef, line: u32) -> Result<Value, RuntimeError> {
    if ty.is_array {
        return match value {
            Value::Array(a) => Ok(Value::Array(a)),
            Value::Undefined => Ok(Value::Undefined),
            other => Err(RuntimeError::new(
                line,
                format!("Нельзя присвоить {} таблице", other.type_name()),
            )),
        };
    }
    match (&ty.base, value) {
        (BaseType::Void, v) => Ok(v),
        (BaseType::Int, Value::Int(v)) => Ok(Value::Int(v)),
        (BaseType::Int, Value::Real(v)) => Ok(Value::Int(v.floor() as i64)),
        (BaseType::Int, Value::Bool(v)) => Ok(Value::Int(if v { 1 } else { 0 })),
        (BaseType::Real, Value::Real(v)) => Ok(Value::Real(v)),
        (BaseType::Real, Value::Int(v)) => Ok(Value::Real(v as f64)),
        (BaseType::Bool, Value::Bool(v)) => Ok(Value::Bool(v)),
        (BaseType::Str, Value::Str(v)) => Ok(Value::Str(v)),
        (BaseType::Str, Value::Char(c)) => Ok(Value::Str(c.to_string())),
        (BaseType::Char, Value::Char(c)) => Ok(Value::Char(c)),
        (BaseType::Char, Value::Str(s)) => {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Ok(Value::Char(c)),
                _ => Err(RuntimeError::new(line, "Строка не является одним символом")),
            }
        }
        (BaseType::File, Value::File(id)) => Ok(Value::File(id)),
        (BaseType::User(_), v @ Value::Record(_)) => Ok(v),
        (BaseType::User(_), Value::Undefined) => Ok(Value::Undefined),
        (expected, Value::Undefined) => Err(RuntimeError::new(
            line,
            format!("Значение типа «{}» не определено", expected.russian_name()),
        )),
        (expected, other) => Err(RuntimeError::new(
            line,
            format!(
                "Нельзя присвоить значение типа «{}» переменной типа «{}»",
                other.type_name(),
                expected.russian_name()
            ),
        )),
    }
}

/// Equality used by `=` and `<>`.
pub fn values_equal(a: &Value, b: &Value, line: u32) -> Result<bool, RuntimeError> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x == y),
        (Value::Real(x), Value::Real(y)) => Ok(x == y),
        (Value::Int(x), Value::Real(y)) => Ok(*x as f64 == *y),
        (Value::Real(x), Value::Int(y)) => Ok(*x == *y as f64),
        (Value::Bool(x), Value::Bool(y)) => Ok(x == y),
        (Value::Char(x), Value::Char(y)) => Ok(x == y),
        (Value::Str(x), Value::Str(y)) => Ok(x == y),
        (Value::Char(x), Value::Str(y)) => Ok(x.to_string() == *y),
        (Value::Str(x), Value::Char(y)) => Ok(*x == y.to_string()),
        (Value::File(x), Value::File(y)) => Ok(x == y),
        (Value::Undefined, Value::Undefined) => Ok(true),
        _ => Err(RuntimeError::new(
            line,
            format!(
                "Нельзя сравнивать значения типов «{}» и «{}»",
                a.type_name(),
                b.type_name()
            ),
        )),
    }
}

/// Ordering used by `<`, `>`, `<=`, `>=`.
pub fn compare_values(a: &Value, b: &Value, line: u32) -> Result<std::cmp::Ordering, RuntimeError> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Real(x), Value::Real(y)) => Ok(x.partial_cmp(y).unwrap_or(Ordering::Equal)),
        (Value::Int(x), Value::Real(y)) => {
            Ok((*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal))
        }
        (Value::Real(x), Value::Int(y)) => {
            Ok(x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal))
        }
        (Value::Char(x), Value::Char(y)) => Ok(x.cmp(y)),
        (Value::Str(x), Value::Str(y)) => Ok(x.cmp(y)),
        (Value::Char(x), Value::Str(y)) => Ok(x.to_string().cmp(y)),
        (Value::Str(x), Value::Char(y)) => Ok(x.cmp(&y.to_string())),
        (Value::Bool(x), Value::Bool(y)) => Ok(x.cmp(y)),
        _ => Err(RuntimeError::new(
            line,
            format!(
                "Нельзя сравнивать значения типов «{}» и «{}»",
                a.type_name(),
                b.type_name()
            ),
        )),
    }
}
