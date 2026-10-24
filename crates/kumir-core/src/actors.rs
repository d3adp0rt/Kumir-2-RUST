//! Actor environments: Robot, Turtle (Черепаха) and Draw (Чертежник).
//!
//! The original project implements actors as plugins with Qt widgets.  Here
//! the simulation lives in the core crate and the IDE paints a snapshot.  The
//! command names and the `.fil` field format follow `src/actors/robot` and
//! `src/actors/turtle`.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    None,
    Robot,
    Turtle,
    Draw,
}

impl ActorKind {
    pub fn russian_name(self) -> &'static str {
        match self {
            ActorKind::None => "нет исполнителя",
            ActorKind::Robot => "Робот",
            ActorKind::Turtle => "Черепаха",
            ActorKind::Draw => "Чертежник",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActorState {
    pub kind: ActorKind,
    pub robot: RobotField,
    pub turtle: TurtleState,
    pub draw: DrawState,
    /// Human readable status / last command.
    pub status: String,
}

impl Default for ActorState {
    fn default() -> Self {
        Self {
            kind: ActorKind::None,
            robot: RobotField::empty(7, 7),
            turtle: TurtleState::new(),
            draw: DrawState::new(),
            status: String::new(),
        }
    }
}

impl ActorState {
    /// Select the actor from the modules named in `использовать ...`.
    pub fn for_modules(modules: &[String]) -> Self {
        let mut result = Self::default();
        for module in modules {
            let m = module.to_lowercase();
            if m.contains("робот") || m == "robot" {
                result.kind = ActorKind::Robot;
            } else if m.contains("черепах") || m.contains("turtle") {
                result.kind = ActorKind::Turtle;
            } else if m.contains("чертеж") || m.contains("чертёж") || m.contains("draw")
            {
                result.kind = ActorKind::Draw;
            }
        }
        result
    }

    pub fn reset_for_run(&mut self) {
        self.robot.reset();
        self.turtle = TurtleState::new();
        self.draw = DrawState::new();
        self.status.clear();
    }

    pub fn load_field_file(&mut self, path: &Path) -> Result<(), String> {
        let field = RobotField::load_fil(path)?;
        self.robot = field;
        self.kind = ActorKind::Robot;
        Ok(())
    }
}

// ----------------------------------------------------------------------
// Robot
// ----------------------------------------------------------------------

pub const LEFT_WALL: u8 = 1;
pub const RIGHT_WALL: u8 = 2;
pub const DOWN_WALL: u8 = 4;
pub const UP_WALL: u8 = 8;

#[derive(Clone, Debug)]
pub struct RobotCell {
    pub walls: u8,
    pub color: bool,
    pub radiation: f64,
    pub temperature: i64,
    /// `Symbol` and `Symbol1` from the `.fil` file.
    pub symbol: char,
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

#[derive(Clone, Debug)]
pub struct RobotField {
    pub width: i32,
    pub height: i32,
    pub cells: Vec<RobotCell>,
    pub robot_x: i32,
    pub robot_y: i32,
    pub start_x: i32,
    pub start_y: i32,
}

impl RobotField {
    pub fn empty(width: i32, height: i32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        Self {
            width: w,
            height: h,
            cells: vec![RobotCell::default(); (w * h) as usize],
            robot_x: 0,
            robot_y: 0,
            start_x: 0,
            start_y: 0,
        }
    }

    pub fn reset(&mut self) {
        self.robot_x = self.start_x;
        self.robot_y = self.start_y;
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

    pub fn wall_up(&self) -> bool {
        // The `.fil` coordinate system has Y growing downwards (screen-like).
        let c = self.current();
        c.walls & UP_WALL != 0 || self.robot_y <= 0
    }
    pub fn wall_down(&self) -> bool {
        let c = self.current();
        c.walls & DOWN_WALL != 0 || self.robot_y + 1 >= self.height
    }
    pub fn wall_left(&self) -> bool {
        let c = self.current();
        c.walls & LEFT_WALL != 0 || self.robot_x <= 0
    }
    pub fn wall_right(&self) -> bool {
        let c = self.current();
        c.walls & RIGHT_WALL != 0 || self.robot_x + 1 >= self.width
    }

    pub fn move_up(&mut self) -> Result<(), String> {
        if self.wall_up() {
            return Err("Робот разбился о верхнюю стену".to_string());
        }
        self.robot_y -= 1;
        Ok(())
    }
    pub fn move_down(&mut self) -> Result<(), String> {
        if self.wall_down() {
            return Err("Робот разбился о нижнюю стену".to_string());
        }
        self.robot_y += 1;
        Ok(())
    }
    pub fn move_left(&mut self) -> Result<(), String> {
        if self.wall_left() {
            return Err("Робот разбился о левую стену".to_string());
        }
        self.robot_x -= 1;
        Ok(())
    }
    pub fn move_right(&mut self) -> Result<(), String> {
        if self.wall_right() {
            return Err("Робот разбился о правую стену".to_string());
        }
        self.robot_x += 1;
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
                if let Some(c) = self.cell(x, y) {
                    let sym = if c.symbol == '$' || c.symbol == '\0' {
                        "$".to_string()
                    } else {
                        c.symbol.to_string()
                    };
                    let sym1 = if c.symbol1 == '$' || c.symbol1 == '\0' {
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
        }
        out.push_str("; End Of File\n");
        out
    }

    pub fn load_fil(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Не удалось прочитать поле: {e}"))?;
        let text = decode_actor_text(&bytes);
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
                        width = parts[0].parse().unwrap_or(7).max(1);
                        height = parts[1].parse().unwrap_or(7).max(1);
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
                    let radiation: f64 = parts[4].parse().unwrap_or(0.0);
                    let temperature: i64 = parts[5].parse().unwrap_or(0);
                    let symbol = first_symbol(parts[6]);
                    let symbol1 = parts.get(7).map(|s| first_symbol(s)).unwrap_or('$');
                    let point = parts.get(8).map(|s| *s != "0").unwrap_or(false);
                    cells.push((
                        x,
                        y,
                        walls,
                        color,
                        radiation,
                        temperature,
                        symbol,
                        symbol1,
                        point,
                    ));
                }
            }
        }
        let mut field = RobotField::empty(width, height);
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
        field.robot_x = robot.0;
        field.robot_y = robot.1;
        field.start_x = robot.0;
        field.start_y = robot.1;
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

fn decode_actor_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
            return s.to_string();
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.trim_start_matches('\u{feff}').to_string();
    }
    let (s, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
    s.to_string()
}

// ----------------------------------------------------------------------
// Turtle
// ----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: String,
}

#[derive(Clone, Debug)]
pub struct Caption {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct TurtleState {
    pub x: f64,
    pub y: f64,
    /// Degrees, counter-clockwise, 0 = right.
    pub angle: f64,
    pub pen_down: bool,
    pub color: String,
    pub segments: Vec<Segment>,
}

impl TurtleState {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            angle: 90.0,
            pen_down: true,
            color: "#1f6f3f".to_string(),
            segments: Vec::new(),
        }
    }

    pub fn forward(&mut self, distance: f64) -> Result<(), String> {
        let rad = self.angle.to_radians();
        let nx = self.x + distance * rad.cos();
        let ny = self.y + distance * rad.sin();
        if self.pen_down {
            self.segments.push(Segment {
                x1: self.x,
                y1: self.y,
                x2: nx,
                y2: ny,
                color: self.color.clone(),
            });
        }
        self.x = nx;
        self.y = ny;
        Ok(())
    }

    pub fn back(&mut self, distance: f64) -> Result<(), String> {
        self.forward(-distance)
    }

    pub fn turn_left(&mut self, angle: f64) {
        self.angle += angle;
    }

    pub fn turn_right(&mut self, angle: f64) {
        self.angle -= angle;
    }
}

// ----------------------------------------------------------------------
// Draw (Чертежник)
// ----------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DrawState {
    pub x: f64,
    pub y: f64,
    pub pen_down: bool,
    pub color: String,
    pub segments: Vec<Segment>,
    pub captions: Vec<Caption>,
}

impl DrawState {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            pen_down: false,
            color: "#204070".to_string(),
            segments: Vec::new(),
            captions: Vec::new(),
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        if self.pen_down {
            self.segments.push(Segment {
                x1: self.x,
                y1: self.y,
                x2: x,
                y2: y,
                color: self.color.clone(),
            });
        }
        self.x = x;
        self.y = y;
    }

    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.move_to(self.x + dx, self.y + dy);
    }
}
