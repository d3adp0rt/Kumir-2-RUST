//! The Draw actor (Чертежник).

use super::color::Rgba;
use super::vector::{Caption, Segment};

#[derive(Clone, Debug)]
pub struct DrawState {
    pub x: f64,
    pub y: f64,
    pub pen_down: bool,
    pub color: Rgba,
    pub segments: Vec<Segment>,
    pub captions: Vec<Caption>,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub font_family: String,
}

impl Default for DrawState {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawState {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            pen_down: false,
            color: Rgba::BLACK,
            segments: Vec::new(),
            captions: Vec::new(),
            font_size: 12.0,
            bold: false,
            italic: false,
            font_family: "Monospace".to_string(),
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        if self.pen_down {
            self.segments.push(Segment {
                x1: self.x,
                y1: self.y,
                x2: x,
                y2: y,
                color: self.color,
            });
        }
        self.x = x;
        self.y = y;
    }

    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.move_to(self.x + dx, self.y + dy);
    }

    /// `написать(ширина, текст)` — the pen ends at the lower right corner of
    /// the text, so the x coordinate advances by the text width.
    pub fn write(&mut self, cell_width: f64, text: &str) {
        let cell_width = if cell_width <= 0.0 { 1.0 } else { cell_width };
        self.captions.push(Caption {
            x: self.x,
            y: self.y,
            text: text.to_string(),
            cell_width,
            color: self.color,
            bold: self.bold,
            italic: self.italic,
            family: self.font_family.clone(),
        });
        self.x += cell_width * text.chars().count() as f64;
        self.y -= self.font_size.max(1.0);
    }

    /// `@есть отрезок в области(x, y, радиус)`.
    pub fn line_at_circle(&self, x: f64, y: f64, radius: f64) -> bool {
        if radius < 0.0 {
            return false;
        }
        self.segments.iter().any(|s| s.distance_to(x, y) <= radius)
    }
}
