//! The Grasshopper actor (Кузнечик): jumps along a number line.
//!
//! Task files (`.kz`) follow the original layout:
//!
//! ```text
//! ;Step forward Step Back
//! 3 5
//! ;StartPos
//! 0
//! ;Left border Right border
//! -1024 1024
//! ;Flags
//! 3 7
//! ;END
//! ```
//!
//! Borders `-1024 … 1024` (or wider) mean "no borders", exactly like the
//! original.  Flags do not stop the grasshopper; they are only visual marks.

use std::path::Path;

use super::robot::decode_actor_text;

/// Borders outside this range disable the border check.
pub const NO_BORDER: i64 = 1024;

#[derive(Clone, Debug)]
pub struct GrasshopperState {
    pub forward: i64,
    pub backward: i64,
    pub start: i64,
    pub pos: i64,
    /// Inclusive borders, `None` when the task has no borders.
    pub borders: Option<(i64, i64)>,
    pub flags: Vec<i64>,
    /// Points painted by `перекрасить`.
    pub painted: Vec<i64>,
    pub jumps: u64,
}

impl Default for GrasshopperState {
    fn default() -> Self {
        Self::new()
    }
}

impl GrasshopperState {
    pub fn new() -> Self {
        Self {
            forward: 3,
            backward: 5,
            start: 0,
            pos: 0,
            borders: None,
            flags: Vec::new(),
            painted: Vec::new(),
            jumps: 0,
        }
    }

    pub fn set_task(
        &mut self,
        forward: i64,
        backward: i64,
        start: i64,
        borders: Option<(i64, i64)>,
        flags: Vec<i64>,
    ) {
        self.forward = forward.max(1);
        self.backward = backward.max(1);
        self.start = start;
        self.pos = start;
        self.borders = borders.filter(|(l, r)| r >= l);
        self.flags = flags;
        self.painted.clear();
        self.jumps = 0;
    }

    pub fn reset(&mut self) {
        self.pos = self.start;
        self.painted.clear();
        self.jumps = 0;
    }

    fn in_borders(&self, pos: i64) -> bool {
        match self.borders {
            Some((left, right)) => pos >= left && pos <= right,
            None => true,
        }
    }

    pub fn jump_forward(&mut self, distance: i64) -> Result<(), String> {
        if distance != self.forward {
            return Err(format!(
                "Кузнечик умеет прыгать вперёд только на {} (в задании указано {distance})",
                self.forward
            ));
        }
        let next = self.pos + distance;
        if !self.in_borders(next) {
            return Err(format!(
                "Кузнечик не может прыгнуть за границу области (точка {next})"
            ));
        }
        self.pos = next;
        self.jumps += 1;
        Ok(())
    }

    pub fn jump_backward(&mut self, distance: i64) -> Result<(), String> {
        if distance != self.backward {
            return Err(format!(
                "Кузнечик умеет прыгать назад только на {} (в задании указано {distance})",
                self.backward
            ));
        }
        let next = self.pos - distance;
        if !self.in_borders(next) {
            return Err(format!(
                "Кузнечик не может прыгнуть за границу области (точка {next})"
            ));
        }
        self.pos = next;
        self.jumps += 1;
        Ok(())
    }

    pub fn repaint(&mut self) {
        if !self.painted.contains(&self.pos) {
            self.painted.push(self.pos);
        }
    }

    pub fn on_flag(&self, pos: i64) -> bool {
        self.flags.contains(&pos)
    }

    pub fn to_kz_string(&self) -> String {
        let (left, right) = self.borders.unwrap_or((-NO_BORDER, NO_BORDER));
        let flags: Vec<String> = self.flags.iter().map(|f| f.to_string()).collect();
        format!(
            ";Step forward Step Back\n{} {}\n;StartPos\n{}\n;Left border Right border\n{} {}\n;Flags\n{} \n;END\n",
            self.forward,
            self.backward,
            self.start,
            left,
            right,
            flags.join(" ")
        )
    }

    pub fn load_kz(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Не удалось прочитать задание: {e}"))?;
        Self::from_kz_str(&decode_actor_text(&bytes))
    }

    pub fn from_kz_str(text: &str) -> Result<Self, String> {
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
            .collect();
        let numbers = |line: &str| -> Vec<i64> {
            line.split_whitespace()
                .filter_map(|p| p.parse::<i64>().ok())
                .collect()
        };
        let steps = lines.first().map(|l| numbers(l)).unwrap_or_default();
        if steps.len() < 2 {
            return Err("В файле нет длин прыжков".to_string());
        }
        let start = lines
            .get(1)
            .and_then(|l| numbers(l).first().copied())
            .unwrap_or(0);
        let borders = lines.get(2).map(|l| numbers(l)).unwrap_or_default();
        let (mut left, mut right) = if borders.len() >= 2 {
            (borders[0], borders[1])
        } else {
            (-NO_BORDER, NO_BORDER)
        };
        let flags = lines.get(3).map(|l| numbers(l)).unwrap_or_default();
        let enabled = !(left < -1000 || right > 1000);
        if !enabled {
            left = -NO_BORDER;
            right = NO_BORDER;
        }
        let mut state = GrasshopperState::new();
        state.set_task(
            steps[0],
            steps[1],
            start,
            if enabled { Some((left, right)) } else { None },
            flags,
        );
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jumps_and_borders() {
        let mut g = GrasshopperState::new();
        g.set_task(3, 2, 0, Some((0, 5)), vec![3]);
        g.jump_forward(3).unwrap();
        assert_eq!(g.pos, 3);
        assert!(g.jump_forward(3).is_err()); // 6 is outside
        g.jump_backward(2).unwrap();
        assert_eq!(g.pos, 1);
        assert!(g.jump_forward(2).is_err()); // wrong step length
    }

    #[test]
    fn kz_roundtrip_and_unlimited_borders() {
        let mut g = GrasshopperState::new();
        g.set_task(2, 7, -5, None, vec![-5, 3]);
        let text = g.to_kz_string();
        let parsed = GrasshopperState::from_kz_str(&text).unwrap();
        assert_eq!(parsed.forward, 2);
        assert_eq!(parsed.backward, 7);
        assert_eq!(parsed.start, -5);
        assert_eq!(parsed.borders, None);
        assert_eq!(parsed.flags, vec![-5, 3]);
    }
}
