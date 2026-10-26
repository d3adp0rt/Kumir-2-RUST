//! The Vodoley actor (Водолей): three vials, pouring water.
//!
//! Task files use the original `.vod` layout:
//!
//! ```text
//! ;Capacity
//! 3 5 7
//! ;Fullness
//! 0 0 0
//! ;Need
//! 1 1 1
//! ```

use std::path::Path;

use super::robot::decode_actor_text;

pub const VIALS: usize = 3;
pub const VIAL_NAMES: [&str; VIALS] = ["A", "B", "C"];

#[derive(Clone, Debug)]
pub struct VodoleyState {
    /// Capacity of every vial, in litres.
    pub capacity: [i64; VIALS],
    /// Water at the beginning of the task.
    pub initial: [i64; VIALS],
    /// Current water level.
    pub fill: [i64; VIALS],
    /// The amount to be measured.
    pub target: i64,
    /// Number of commands executed (shown in the panel).
    pub moves: u64,
}

impl Default for VodoleyState {
    fn default() -> Self {
        Self::new()
    }
}

impl VodoleyState {
    pub fn new() -> Self {
        Self {
            capacity: [3, 5, 7],
            initial: [0, 0, 0],
            fill: [0, 0, 0],
            target: 1,
            moves: 0,
        }
    }

    pub fn reset(&mut self) {
        self.fill = self.initial;
        self.moves = 0;
    }

    pub fn set_task(&mut self, capacity: [i64; 3], initial: [i64; 3], target: i64) {
        self.capacity = capacity.map(|c| c.max(0));
        self.initial = [0, 1, 2].map(|i| initial[i].clamp(0, self.capacity[i]));
        self.fill = self.initial;
        self.target = target.max(0);
        self.moves = 0;
    }

    pub fn fill_vial(&mut self, i: usize) {
        self.fill[i] = self.capacity[i];
        self.moves += 1;
    }

    pub fn empty_vial(&mut self, i: usize) {
        self.fill[i] = 0;
        self.moves += 1;
    }

    /// Pour `from` into `to` until the source is empty or the target is full.
    pub fn pour(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let free = (self.capacity[to] - self.fill[to]).max(0);
        let amount = self.fill[from].min(free);
        self.fill[from] -= amount;
        self.fill[to] += amount;
        self.moves += 1;
    }

    /// `@решено`: the required amount is in one of the vials.
    pub fn solved(&self) -> bool {
        self.fill.contains(&self.target)
    }

    pub fn to_vod_string(&self) -> String {
        format!(
            ";Capacity\n{} {} {}\n;Fullness\n{} {} {}\n;Need\n{} {} {}\n",
            self.capacity[0],
            self.capacity[1],
            self.capacity[2],
            self.initial[0],
            self.initial[1],
            self.initial[2],
            self.target,
            self.target,
            self.target
        )
    }

    pub fn load_vod(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Не удалось прочитать задание: {e}"))?;
        Self::from_vod_str(&decode_actor_text(&bytes))
    }

    pub fn from_vod_str(text: &str) -> Result<Self, String> {
        let mut rows: Vec<Vec<i64>> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let mut row = Vec::new();
            for part in line.split_whitespace() {
                let value: i64 = part
                    .replace(',', ".")
                    .parse::<f64>()
                    .map(|v| v.round() as i64)
                    .map_err(|_| format!("Неверное число в задании: «{part}»"))?;
                row.push(value);
            }
            if row.len() >= VIALS {
                rows.push(row);
            }
        }
        let capacity = rows
            .first()
            .ok_or("В файле нет строки с объёмами сосудов")?;
        let initial = rows.get(1).cloned().unwrap_or_else(|| vec![0; VIALS]);
        let need = rows.get(2).cloned().unwrap_or_else(|| vec![1; VIALS]);
        let target = need.iter().copied().find(|&v| v > 0).unwrap_or(need[0]);
        let mut state = VodoleyState::new();
        state.set_task(
            [capacity[0], capacity[1], capacity[2]],
            [initial[0], initial[1], initial[2]],
            target,
        );
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pouring_matches_the_manual_example() {
        let mut state = VodoleyState::new();
        state.set_task([10, 6, 0], [7, 2, 0], 3);
        state.pour(1, 0); // B -> A
        assert_eq!(state.fill, [9, 0, 0]);
        state.pour(0, 1); // A -> B
        assert_eq!(state.fill, [3, 6, 0]);
        assert!(state.solved());
    }

    #[test]
    fn vod_roundtrip() {
        let mut state = VodoleyState::new();
        state.set_task([8, 5, 3], [8, 0, 0], 4);
        let text = state.to_vod_string();
        let parsed = VodoleyState::from_vod_str(&text).unwrap();
        assert_eq!(parsed.capacity, [8, 5, 3]);
        assert_eq!(parsed.initial, [8, 0, 0]);
        assert_eq!(parsed.target, 4);
    }
}
