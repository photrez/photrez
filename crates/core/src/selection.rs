// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SelectionRect {
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    pub fn translate(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }

    pub fn clamp_to_canvas(&self, canvas_w: u32, canvas_h: u32) -> Self {
        let cw = canvas_w as f32;
        let ch = canvas_h as f32;
        let w = self.width.min(cw).max(1.0);
        let h = self.height.min(ch).max(1.0);
        let x = self.x.clamp(0.0, cw - w);
        let y = self.y.clamp(0.0, ch - h);
        Self {
            x,
            y,
            width: w,
            height: h,
        }
    }

    pub fn normalize(&self) -> Self {
        let x = if self.width < 0.0 {
            self.x + self.width
        } else {
            self.x
        };
        let y = if self.height < 0.0 {
            self.y + self.height
        } else {
            self.y
        };
        Self {
            x,
            y,
            width: self.width.abs(),
            height: self.height.abs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectionState {
    pub current_selection: Option<SelectionRect>,
}

impl SelectionState {
    pub fn new() -> Self {
        Self {
            current_selection: None,
        }
    }
    pub fn set_selection(&mut self, rect: SelectionRect) {
        self.current_selection = Some(rect);
    }

    pub fn clear_selection(&mut self) {
        self.current_selection = None;
    }
}

impl Default for SelectionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection() {
        let mut state = SelectionState::new();
        assert!(state.current_selection.is_none());
        state.set_selection(SelectionRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        });
        assert!(state.current_selection.is_some());
        assert_eq!(state.current_selection.as_ref().unwrap().width, 100.0);
    }

    #[test]
    fn test_contains_point() {
        let rect = SelectionRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        assert!(rect.contains_point(50.0, 40.0));
        assert!(!rect.contains_point(5.0, 40.0));
        assert!(!rect.contains_point(50.0, 10.0));
        assert!(rect.contains_point(10.0, 20.0));
        assert!(rect.contains_point(110.0, 70.0));
    }

    #[test]
    fn test_translate() {
        let rect = SelectionRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        let moved = rect.translate(5.0, -10.0);
        assert_eq!(moved.x, 15.0);
        assert_eq!(moved.y, 10.0);
        assert_eq!(moved.width, 100.0);
    }

    #[test]
    fn test_clamp_to_canvas() {
        let rect = SelectionRect {
            x: -10.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let clamped = rect.clamp_to_canvas(100, 100);
        assert!(clamped.x >= 0.0);
        assert!(clamped.x + clamped.width <= 100.0);
    }

    #[test]
    fn test_normalize() {
        let rect = SelectionRect {
            x: 100.0,
            y: 50.0,
            width: -50.0,
            height: -25.0,
        };
        let norm = rect.normalize();
        assert_eq!(norm.x, 50.0);
        assert_eq!(norm.y, 25.0);
        assert_eq!(norm.width, 50.0);
        assert_eq!(norm.height, 25.0);
    }
}
