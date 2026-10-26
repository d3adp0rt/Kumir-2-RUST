//! The Turtle actor (Черепаха).
//!
//! Following the original specification the turtle lives on a 500×500 pixel
//! sandy arena surrounded by a water moat.  The tail tip must stay on the
//! arena; depending on [`TurtleBounds`] the turtle either refuses to leave it
//! (the original behaviour) or is allowed to draw anywhere (a convenient mode
//! for programs imported from other implementations).

use super::color::Rgba;
use super::vector::Segment;

/// Side of the square arena, in turtle units.
pub const ARENA_SIZE: f64 = 500.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TurtleBounds {
    /// The original behaviour: a step out of the arena is an execution error.
    #[default]
    Refuse,
    /// Clip nothing and let the turtle draw outside the arena.
    Free,
}

impl TurtleBounds {
    pub fn label(self) -> &'static str {
        match self {
            TurtleBounds::Refuse => "арена 500×500 (отказ у воды)",
            TurtleBounds::Free => "без границ",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TurtleState {
    pub x: f64,
    pub y: f64,
    /// Degrees, counter-clockwise, 0 = right, 90 = up.
    pub angle: f64,
    pub pen_down: bool,
    pub color: Rgba,
    pub segments: Vec<Segment>,
    pub bounds: TurtleBounds,
}

impl Default for TurtleState {
    fn default() -> Self {
        Self::new()
    }
}

impl TurtleState {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            angle: 90.0,
            pen_down: true,
            color: Rgba::rgb(0, 0, 0),
            segments: Vec::new(),
            bounds: TurtleBounds::Refuse,
        }
    }

    pub fn in_arena(&self, x: f64, y: f64) -> bool {
        let half = ARENA_SIZE / 2.0;
        (-half..=half).contains(&x) && (-half..=half).contains(&y)
    }

    pub fn forward(&mut self, distance: f64) -> Result<(), String> {
        let rad = self.angle.to_radians();
        let nx = self.x + distance * rad.cos();
        let ny = self.y + distance * rad.sin();
        if self.bounds == TurtleBounds::Refuse && !self.in_arena(nx, ny) {
            return Err(
                "Черепаха упёрлась в ров с водой: конец хвоста должен оставаться на арене \
                 500×500"
                    .to_string(),
            );
        }
        if self.pen_down {
            self.segments.push(Segment {
                x1: self.x,
                y1: self.y,
                x2: nx,
                y2: ny,
                color: self.color,
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
        self.angle = (self.angle + angle).rem_euclid(360.0);
    }

    pub fn turn_right(&mut self, angle: f64) {
        self.angle = (self.angle - angle).rem_euclid(360.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_to_leave_the_arena_by_default() {
        let mut turtle = TurtleState::new();
        assert!(turtle.forward(300.0).is_err());
        assert_eq!(turtle.y, 0.0);
        turtle.forward(200.0).unwrap();
        assert_eq!(turtle.y, 200.0);
    }

    #[test]
    fn free_mode_allows_any_distance() {
        let mut turtle = TurtleState::new();
        turtle.bounds = TurtleBounds::Free;
        turtle.forward(1000.0).unwrap();
        assert_eq!(turtle.y, 1000.0);
        assert_eq!(turtle.segments.len(), 1);
    }

    #[test]
    fn turns_are_normalised() {
        let mut turtle = TurtleState::new();
        turtle.turn_left(360.0 + 90.0);
        assert!((turtle.angle - 180.0).abs() < 1e-9);
        turtle.turn_right(270.0);
        assert!((turtle.angle - 270.0).abs() < 1e-9);
    }
}
