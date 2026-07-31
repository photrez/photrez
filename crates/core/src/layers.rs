// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::transform::Transform;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PixelFormat {
    RGBA8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitmapData {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    #[serde(skip)]
    pub pixel_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub locked: bool,
    pub blend_mode: String,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub bitmap_ref: BitmapData,
    pub transform: Transform,
}

impl Layer {
    pub fn new(id: String, name: String, width: u32, height: u32) -> Self {
        let pixel_data = vec![255; (width * height * 4) as usize];
        Self {
            id,
            name,
            opacity: 1.0,
            visible: true,
            locked: false,
            blend_mode: "normal".to_string(),
            x: 0.0,
            y: 0.0,
            width,
            height,
            bitmap_ref: BitmapData {
                width,
                height,
                format: PixelFormat::RGBA8,
                pixel_data,
            },
            transform: Transform::new(),
        }
    }

    pub fn draw_brush_stroke(
        &mut self,
        path: &[(f32, f32)],
        settings: &crate::brush::BrushSettings,
        is_eraser: bool,
    ) {
        if path.is_empty() || self.locked {
            return;
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let r = settings.size / 2.0;

        let mut paint_point = |cx: f32, cy: f32| {
            let x_start = ((cx - r).floor() as i32).max(0).min(w - 1);
            let x_end = ((cx + r).ceil() as i32).max(0).min(w - 1);
            let y_start = ((cy - r).floor() as i32).max(0).min(h - 1);
            let y_end = ((cy + r).ceil() as i32).max(0).min(h - 1);

            for py in y_start..=y_end {
                for px in x_start..=x_end {
                    let dx = px as f32 - cx;
                    let dy = py as f32 - cy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist <= r {
                        let idx = ((py * w + px) * 4) as usize;
                        let pixel = &mut self.bitmap_ref.pixel_data[idx..idx + 4];

                        if is_eraser {
                            let hardness_r = r * settings.hardness;
                            let alpha_factor = if dist <= hardness_r {
                                1.0
                            } else if r > hardness_r {
                                1.0 - (dist - hardness_r) / (r - hardness_r)
                            } else {
                                1.0
                            };
                            let erase_amount = settings.color[3] * alpha_factor;
                            let dest_a = pixel[3] as f32;
                            pixel[3] = (dest_a * (1.0 - erase_amount)) as u8;
                        } else {
                            settings.paint_pixel(pixel, dist);
                        }
                    }
                }
            }
        };

        let (mut x0, mut y0) = path[0];
        paint_point(x0, y0);

        for &(x1, y1) in path.iter().skip(1) {
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            let step_size = (settings.size * 0.1).max(0.1);
            let steps = (len / step_size).ceil() as u32;

            if steps > 1 {
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    let cx = x0 + dx * t;
                    let cy = y0 + dy * t;
                    paint_point(cx, cy);
                }
            } else {
                paint_point(x1, y1);
            }
            x0 = x1;
            y0 = y1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_layer() {
        let layer = Layer::new("layer-1".to_string(), "Background".to_string(), 800, 600);
        assert_eq!(layer.id, "layer-1");
        assert_eq!(layer.name, "Background");
        assert_eq!(layer.opacity, 1.0);
        assert!(layer.visible);
        assert!(!layer.locked);
        assert_eq!(layer.blend_mode, "normal");
        assert_eq!(layer.x, 0.0);
        assert_eq!(layer.y, 0.0);
        assert_eq!(layer.width, 800);
        assert_eq!(layer.height, 600);

        // Verify bitmap initialization
        assert_eq!(layer.bitmap_ref.width, 800);
        assert_eq!(layer.bitmap_ref.height, 600);
        assert_eq!(layer.bitmap_ref.format, PixelFormat::RGBA8);
        assert_eq!(layer.bitmap_ref.pixel_data.len(), 800 * 600 * 4);
        assert_eq!(layer.bitmap_ref.pixel_data[0], 255); // Opaque white default
    }

    #[test]
    fn test_brush_stroke_drawing() {
        use crate::brush::BrushSettings;
        let mut layer = Layer::new("l-1".to_string(), "Layer 1".to_string(), 10, 10);
        let settings = BrushSettings::new(2.0, 1.0, [1.0, 0.0, 0.0, 1.0]); // Fully red opaque

        // Paint single dot at center
        layer.draw_brush_stroke(&[(5.0, 5.0)], &settings, false);

        let idx = ((5 * 10 + 5) * 4) as usize;
        assert_eq!(layer.bitmap_ref.pixel_data[idx], 255); // Red
        assert_eq!(layer.bitmap_ref.pixel_data[idx + 1], 0); // Green
        assert_eq!(layer.bitmap_ref.pixel_data[idx + 2], 0); // Blue
        assert_eq!(layer.bitmap_ref.pixel_data[idx + 3], 255); // Alpha
    }

    #[test]
    fn test_eraser_stroke_drawing() {
        use crate::brush::BrushSettings;
        let mut layer = Layer::new("l-1".to_string(), "Layer 1".to_string(), 10, 10);
        let settings = BrushSettings::new(2.0, 1.0, [0.0, 0.0, 0.0, 1.0]);

        // Erase center point
        layer.draw_brush_stroke(&[(5.0, 5.0)], &settings, true);

        let idx = ((5 * 10 + 5) * 4) as usize;
        assert_eq!(layer.bitmap_ref.pixel_data[idx + 3], 0); // Erased Alpha channel
    }
}
