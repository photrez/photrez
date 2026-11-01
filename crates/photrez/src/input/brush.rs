//! Brush/eraser stroke construction for the native pointer path.

use std::rc::Rc;

use photrez_core::brush::engine::BrushStrokeEngine;
use photrez_core::brush::tip::{build_brush_tip_data, BrushTip, TipSpec};
use photrez_core::paint::raster::RasterDab;

use super::{BrushStroke, BrushTarget};

/// Build the same data-resolution tip the reference brush chain builds for one
/// stroke. The diameter-space diagnostic buffers are not produced: the native
/// rasterizer reads only the data-resolution mask.
pub fn build_tip(size: f64, hardness: f64, color: [u8; 4]) -> BrushTip {
    build_brush_tip_data(&TipSpec::new(
        size,
        hardness,
        [color[0], color[1], color[2]],
    ))
}

/// Start a stroke with its first dab at the pointer-down point. The tip is
/// supplied by the caller so a settings-keyed cache can own the build.
pub fn begin_stroke(
    target: BrushTarget,
    x: f64,
    y: f64,
    size: f64,
    eraser: bool,
    dab_alpha: f64,
    tip: Rc<BrushTip>,
) -> BrushStroke {
    let mut engine = BrushStrokeEngine::new(size);
    engine.begin(x, y);
    BrushStroke {
        target,
        engine,
        dabs: vec![RasterDab {
            x,
            y,
            alpha: dab_alpha,
        }],
        tip,
        eraser,
        size,
        dab_alpha,
        start: (x, y),
        last: (x, y),
    }
}
