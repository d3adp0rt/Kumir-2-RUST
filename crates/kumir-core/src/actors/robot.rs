//! The Robot actor and the `.fil` field format.
//!
//! The file layout follows `src/actors/robot/rfield.cpp`: a size line, the
//! start position, one line per non-empty cell and a comment footer.  Cell
//! coordinates are 0-based with Y growing downwards, exactly like the original.

use std::path::Path;

pub const LEFT_WALL: u8 = 1;
pub const RIGHT_WALL: u8 = 2;
pub const DOWN_WALL: u8 = 4;
pub const UP_WALL: u8 = 8;

pub const MIN_COLUMNS: i32 = 1;
pub const MAX_COLUMNS: i32 = 255;
pub const MIN_ROWS: i32 = 1;
pub const MAX_ROWS: i32 = 128;

#[derive(Clone, Debug)]
pub struct RobotCell {
    pub walls: u8,
    pub color: bool,
    pub radiation: f64,
    pub temperature: i64,
    /// `Symbol` (left top corner) from the `.fil` file.
    pub symbol: char,
    /// `Symbol1` (left bottom corner) from the `.fil` file.
    pub symbol1: char,
    /// `Point` column from `.fil` (the teacher's mark).
    pub point: bool,
}

impl Default for RobotCell {
    fn default() -> Self {
        Self {
            walls: 0,
            color: false,
            radiation: 0.0,
            temperature: 0,
            symbol: '$',
            symbol1: '$',
            point: false,
        }
    }
}

impl RobotCell {
    /// A cell is written to `.fil` only when it differs from the default one,
    /// which is what the original editor does.
    fn is_empty(&self) -> bool {
        self.walls == 0
            && !self.color
            && self.radiation == 0.0
            && self.temperature == 0
            && matches!(self.symbol, '$' | ' ' | '\0')
            && matches!(self.symbol1, '$' | ' ' | '\0')
            && !self.point
    }
}

#[derive(Clone, Debug)]
pub struct RobotField {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<RobotCell>,
    pub robot_x: i32,
    pub robot_y: i32,
    pub start_x: i32,
    pub start_y: i32,
    /// Set when the last movement was refused; the IDE paints a red warning.
    pub crashed: bool,
}

impl RobotField {
    pub fn empty(width: i32, height: i32) -> Self {
        let w = width.clamp(MIN_COLUMNS, MAX_COLUMNS);
        let h = height.clamp(MIN_ROWS, MAX_ROWS);
        Self {
            width: w,
            height: h,
            cells: vec![RobotCell::default(); (w * h) as usize],
            robot_x: 0,
            robot_y: 0,
            start_x: 0,
            start_y: 0,
            crashed: false,
        }
    }

    pub fn reset(&mut self) {
        self.robot_x = self.start_x;
        self.robot_y = self.start_y;
        self.crashed = false;
    }

    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            None
        } else {
            Some((y * self.width + x) as usize)
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&RobotCell> {
        self.index(x, y).map(|i| &self.cells[i])
    }

    pub fn cell_mut(&mut self, x: i32, y: i32) -> Option<&mut RobotCell> {
        self.index(x, y).map(|i| &mut self.cells[i])
    }

    pub fn current(&self) -> &RobotCell {
        self.cell(self.robot_x, self.robot_y)
            .unwrap_or(&self.cells[0])
    }

    /// Walls are considered from both sides: some old `.fil` files only store
    /// the wall in one of the two adjacent cells.
    pub fn wall_up(&self) -> bool {
        self.robot_y <= 0
            || self.current().walls & UP_WALL != 0
            || self
                .cell(self.robot_x, self.robot_y - 1)
                .map(|c| c.walls & DOWN_WALL != 0)
                .unwrap_or(false)
    }
    pub fn wall_down(&self) -> bool {
        self.robot_y + 1 >= self.height
            || self.current().walls & DOWN_WALL != 0
            || self
                .cell(self.robot_x, self.robot_y + 1)
                .map(|c| c.walls & UP_WALL != 0)
                .unwrap_or(false)
    }
    pub fn wall_left(&self) -> bool {
        self.robot_x <= 0
            || self.current().walls & LEFT_WALL != 0
            || self
                .cell(self.robot_x - 1, self.robot_y)
                .map(|c| c.walls & RIGHT_WALL != 0)
                .unwrap_or(false)
    }
    pub fn wall_right(&self) -> bool {
        self.robot_x + 1 >= self.width
            || self.current().walls & RIGHT_WALL != 0
            || self
                .cell(self.robot_x + 1, self.robot_y)
                .map(|c| c.walls & LEFT_WALL != 0)
                .unwrap_or(false)
    }

    fn crash(&mut self, direction: &str) -> String {
        self.crashed = true;
        format!("Робот разбился о стену ({direction})")
    }

    pub fn move_up(&mut self) -> Result<(), String> {
        if self.wall_up() {
            return Err(self.crash("сверху"));
        }
        self.robot_y -= 1;
        self.crashed = false;
        Ok(())
    }
    pub fn move_down(&mut self) -> Result<(), String> {
        if self.wall_down() {
            return Err(self.crash("снизу"));
        }
        self.robot_y += 1;
        self.crashed = false;
        Ok(())
    }
    pub fn move_left(&mut self) -> Result<(), String> {
        if self.wall_left() {
            return Err(self.crash("слева"));
        }
        self.robot_x -= 1;
        self.crashed = false;
        Ok(())
    }
    pub fn move_right(&mut self) -> Result<(), String> {
        if self.wall_right() {
            return Err(self.crash("справа"));
        }
        self.robot_x += 1;
        self.crashed = false;
        Ok(())
    }

    pub fn paint(&mut self) {
        if let Some(c) = self.cell_mut(self.robot_x, self.robot_y) {
            c.color = true;
        }
    }

    /// Serialize the field to the original `.fil` text format.
    pub fn to_fil_string(&self) -> String {
        let mut out = String::new();
        out.push_str("; Field Size: x, y\n");
        out.push_str(&format!("{} {}\n", self.width, self.height));
        out.push_str("; Robot position: x, y\n");
        out.push_str(&format!("{} {}\n", self.start_x, self.start_y));
        out.push_str(
            "; A set of special Fields: x, y, Wall, Color, Radiation, Temperature, Symbol, Symbol1, Point\n",
        );
        for y in 0..self.height {
            for x in 0..self.width {
                let Some(c) = self.cell(x, y) else {
                    continue;
                };
                if c.is_empty() {
                    continue;
                }
                let sym = if matches!(c.symbol, '$' | ' ' | '\0') {
                    "$".to_string()
                } else {
                    c.symbol.to_string()
                };
                let sym1 = if matches!(c.symbol1, '$' | ' ' | '\0') {
                    "$".to_string()
                } else {
                    c.symbol1.to_string()
                };
                out.push_str(&format!(
                    "{} {} {} {} {:.6} {} {} {} {}\n",
                    x,
                    y,
                    c.walls,
                    if c.color { 1 } else { 0 },
                    c.radiation,
                    c.temperature,
                    sym,
                    sym1,
                    if c.point { 1 } else { 0 }
                ));
            }
        }
        out.push_str("; End Of File\n");
        out
    }

    pub fn load_fil(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Не удалось прочитать поле: {e}"))?;
        let text = decode_actor_text(&bytes);
        Self::from_fil_str(&text)
    }

    pub fn from_fil_str(text: &str) -> Result<Self, String> {
        let mut width = 7;
        let mut height = 7;
        let mut robot = (0, 0);
        let mut cells = Vec::new();
        let mut stage = 0;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            match stage {
                0 => {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        width = parts[0].parse().unwrap_or(7);
                        height = parts[1].parse().unwrap_or(7);
                    }
                    stage = 1;
                }
                1 => {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        robot = (parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0));
                    }
                    stage = 2;
                }
                _ => {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() < 7 {
                        continue;
                    }
                    let x: i32 = parts[0].parse().unwrap_or(0);
                    let y: i32 = parts[1].parse().unwrap_or(0);
                    let walls: u8 = parts[2].parse().unwrap_or(0);
                    let color = parts[3] != "0";
                    let radiation: f64 = parts[4].replace(',', ".").parse().unwrap_or(0.0);
                    let temperature: f64 = parts[5].replace(',', ".").parse().unwrap_or(0.0);
                    let symbol = first_symbol(parts[6]);
                    let symbol1 = parts.get(7).map(|s| first_symbol(s)).unwrap_or('$');
                    let point = parts.get(8).map(|s| *s != "0").unwrap_or(false);
                    cells.push((
                        x,
                        y,
                        walls,
                        color,
                        radiation,
                        temperature.round() as i64,
                        symbol,
                        symbol1,
                        point,
                    ));
                }
            }
        }
        let mut field = Self::empty(width, height);
        for (x, y, walls, color, radiation, temperature, symbol, symbol1, point) in cells {
            if let Some(c) = field.cell_mut(x, y) {
                c.walls = walls;
                c.color = color;
                c.radiation = radiation;
                c.temperature = temperature;
                c.symbol = symbol;
                c.symbol1 = symbol1;
                c.point = point;
            }
        }
        field.robot_x = robot.0.clamp(0, field.width - 1);
        field.robot_y = robot.1.clamp(0, field.height - 1);
        field.start_x = field.robot_x;
        field.start_y = field.robot_y;
        field.crashed = false;
        Ok(field)
    }
}

fn first_symbol(s: &str) -> char {
    let mut chars = s.chars();
    let c = chars.next().unwrap_or('$');
    if c == '$' {
        '$'
    } else {
        c
    }
}

pub(super) fn decode_actor_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(rest) {
            return s.to_string();
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.trim_start_matches('\u{feff}').to_string();
    }
    let (s, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_are_read_from_both_sides() {
        let mut field = RobotField::empty(3, 1);
        field.cells[1].walls = LEFT_WALL; // wall at the left of the middle cell
        field.robot_x = 0;
        assert!(field.wall_right());
        field.robot_x = 1;
        assert!(field.wall_left());
    }

    #[test]
    fn empty_cells_are_not_written() {
        let mut field = RobotField::empty(2, 2);
        field.cell_mut(1, 1).unwrap().color = true;
        let text = field.to_fil_string();
        let cells = text.lines().filter(|l| !l.starts_with(';')).count();
        assert_eq!(cells, 3); // size, position, one painted cell
    }
}
