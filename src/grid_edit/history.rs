// SPDX-License-Identifier: GPL-3.0-or-later
use super::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Undo,
    Redo,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl History {
    pub fn target(&self, direction: Direction) -> Option<&Snapshot> {
        match direction {
            Direction::Undo => self.undo.last(),
            Direction::Redo => self.redo.last(),
        }
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    pub fn record(&mut self, before: Snapshot) {
        self.redo.clear();
        self.undo.push(before);
        self.trim();
    }
    pub fn restored(&mut self, direction: Direction, before: Snapshot) {
        let (from, to) = match direction {
            Direction::Undo => (&mut self.undo, &mut self.redo),
            Direction::Redo => (&mut self.redo, &mut self.undo),
        };
        from.pop();
        to.push(before);
        self.trim();
    }
    fn trim(&mut self) {
        // Bound the history on a Pi, including unusually dense imported grids.
        while self.undo.len() + self.redo.len() > 64
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|s| {
                    s.grid.beats().len() * 24
                        + s.raw.as_ref().map_or(0, |r| r.beats_ms.len() * 8)
                        + 1024
                })
                .sum::<usize>()
                > 16 * 1024 * 1024
        {
            if !self.undo.is_empty() {
                self.undo.remove(0);
            } else {
                self.redo.remove(0);
            }
        }
    }
}
