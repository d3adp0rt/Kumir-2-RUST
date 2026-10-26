//! Shared geometry of the vector actors (Turtle and Draw).

use super::color::Rgba;

#[derive(Clone, Debug)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: Rgba,
}

impl Segment {
    /// Shortest distance from `(px, py)` to the segment.
    pub fn distance_to(&self, px: f64, py: f64) -> f64 {
        let dx = self.x2 - self.x1;
        let dy = self.y2 - self.y1;
        let len2 = dx * dx + dy * dy;
        if len2 <= f64::EPSILON {
            return ((px - self.x1).powi(2) + (py - self.y1).powi(2)).sqrt();
        }
        let t = (((px - self.x1) * dx + (py - self.y1) * dy) / len2).clamp(0.0, 1.0);
        let cx = self.x1 + t * dx;
        let cy = self.y1 + t * dy;
        ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
    }
}

#[derive(Clone, Debug)]
pub struct Caption {
    pub x: f64,
    pub y: f64,
    pub text: String,
    /// Width of one character in drawing units (the `написать(width, text)`
    /// argument).
    pub cell_width: f64,
    pub color: Rgba,
    pub bold: bool,
    pub italic: bool,
    pub family: String,
}
