// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};
use crate::selection::SelectionRect;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Transform {
    pub fn new() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }

    pub fn rotate(&mut self, degrees: f32) {
        self.rotation = (self.rotation + degrees) % 360.0;
    }

    pub fn set_rotation(&mut self, degrees: f32) {
        self.rotation = degrees % 360.0;
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.scale_x = sx;
        self.scale_y = sy;
    }

    pub fn set_flip_h(&mut self, flip: bool) {
        self.flip_h = flip;
    }

    pub fn set_flip_v(&mut self, flip: bool) {
        self.flip_v = flip;
    }

    pub fn apply_to_rect(&self, rect: &SelectionRect) -> SelectionRect {
        let abs_sx = self.scale_x.abs();
        let abs_sy = self.scale_y.abs();
        let w = rect.width * abs_sx;
        let h = rect.height * abs_sy;
        let cx = rect.x + rect.width / 2.0;
        let cy = rect.y + rect.height / 2.0;

        if self.rotation == 0.0 {
            return SelectionRect {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                width: w,
                height: h,
            };
        }

        let rad = self.rotation.to_radians();
        let cos = rad.cos().abs();
        let sin = rad.sin().abs();
        let bw = w * cos + h * sin;
        let bh = w * sin + h * cos;

        SelectionRect {
            x: cx - bw / 2.0,
            y: cy - bh / 2.0,
            width: bw,
            height: bh,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_default() {
        let t = Transform::default();
        assert_eq!(t.scale_x, 1.0);
        assert_eq!(t.scale_y, 1.0);
        assert_eq!(t.rotation, 0.0);
        assert!(!t.flip_h);
        assert!(!t.flip_v);
    }

    #[test]
    fn test_rotate() {
        let mut t = Transform::new();
        t.rotate(45.0);
        assert_eq!(t.rotation, 45.0);
        t.rotate(360.0);
        assert_eq!(t.rotation, 45.0);
    }

    #[test]
    fn test_scale() {
        let mut t = Transform::new();
        t.scale(2.0, 1.5);
        assert_eq!(t.scale_x, 2.0);
        assert_eq!(t.scale_y, 1.5);
    }

    #[test]
    fn test_flip() {
        let mut t = Transform::new();
        t.set_flip_h(true);
        t.set_flip_v(true);
        assert!(t.flip_h);
        assert!(t.flip_v);
    }

    #[test]
    fn test_apply_to_rect_no_transform() {
        let t = Transform::new();
        let rect = SelectionRect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 };
        let result = t.apply_to_rect(&rect);
        assert_eq!(result.width, 100.0);
        assert_eq!(result.height, 50.0);
    }

    #[test]
    fn test_apply_to_rect_scaled() {
        let mut t = Transform::new();
        t.scale(2.0, 0.5);
        let rect = SelectionRect { x: 50.0, y: 50.0, width: 100.0, height: 100.0 };
        let result = t.apply_to_rect(&rect);
        assert!((result.width - 200.0).abs() < 0.001);
        assert!((result.height - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_apply_to_rect_rotated() {
        let mut t = Transform::new();
        t.set_rotation(90.0);
        let rect = SelectionRect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 };
        let result = t.apply_to_rect(&rect);
        // After 90Â° rotation, width and height should swap
        assert!((result.width - 50.0).abs() < 0.001);
        assert!((result.height - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_set_rotation() {
        let mut t = Transform::new();
        t.set_rotation(45.0);
        assert_eq!(t.rotation, 45.0);
        t.set_rotation(400.0); // should wrap to 40.0
        assert_eq!(t.rotation, 40.0);
    }
}
